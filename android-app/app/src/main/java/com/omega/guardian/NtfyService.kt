package com.omega.guardian

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.nio.charset.StandardCharsets
import java.util.concurrent.TimeUnit

/**
 * Keeps a long-lived ntfy WebSocket open so install requests arrive even when the app is closed.
 * Runs as a foreground service (persistent low-priority notification). On each request it verifies
 * the daemon's signature (csig) with the paired daemon key, then posts a high-priority notification
 * with Allow / Deny actions handled by [DecisionReceiver].
 */
class NtfyService : Service() {

    private val client = OkHttpClient.Builder()
        .pingInterval(45, TimeUnit.SECONDS)      // detect silent NAT/Doze kills, trigger onFailure
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .retryOnConnectionFailure(true)
        .build()

    private val sockets = mutableListOf<WebSocket>()
    private var closing = false
    private var backoffMs = 1000L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannels(this)
        startForeground(FG_ID, foregroundNotification())
        PollWorker.schedule(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        connectAll()
        return START_STICKY   // re-create if the system kills us
    }

    override fun onDestroy() {
        closing = true
        sockets.forEach { it.cancel() }
        sockets.clear()
        super.onDestroy()
    }

    /** Open one WebSocket per paired child (each daemon has its own topic + key), with `?since=` catch-up (D5). */
    private fun connectAll() {
        sockets.forEach { it.cancel() }
        sockets.clear()
        val base = Prefs.server(this).trimEnd('/')
        val children = Prefs.children(this)
        for (i in 0 until children.length()) {
            val c = children.getJSONObject(i)
            val topic = c.optString("topic", ""); if (topic.isBlank()) continue
            val name = c.optString("name", "Dziecko ${i + 1}")
            val dpub = c.optString("dpub", "")
            val server = c.optString("server", base).trimEnd('/')
            val since = sinceParamFor(this, topic)
            val wsUrl = server.replaceFirst("https://", "wss://").replaceFirst("http://", "ws://") + "/$topic/ws?since=$since"
            Prefs.addDebugLog(this, "ws connecting: child=$name since=$since")
            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    backoffMs = 1000L
                    Prefs.setConnected(this@NtfyService, name, true)
                    Prefs.addDebugLog(this@NtfyService, "ws open: child=$name")
                    sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(packageName))
                }
                override fun onMessage(webSocket: WebSocket, text: String) {
                    handleEventStatic(this@NtfyService, text, name, topic, dpub, "ws")
                }
                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    Prefs.setConnected(this@NtfyService, name, false)
                    Prefs.addDebugLog(this@NtfyService, "ws fail: child=$name err=${t.javaClass.simpleName}:${t.message}")
                    sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(packageName))
                    scheduleReconnect()
                }
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    Prefs.setConnected(this@NtfyService, name, false)
                    Prefs.addDebugLog(this@NtfyService, "ws closed: child=$name code=$code reason=$reason")
                    sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(packageName))
                    scheduleReconnect()
                }
            }
            sockets.add(client.newWebSocket(Request.Builder().url(wsUrl).build(), listener))
        }
    }

    private fun scheduleReconnect() {
        if (closing) return
        val delay = backoffMs
        backoffMs = (backoffMs * 2).coerceAtMost(60_000L)
        Prefs.addDebugLog(this, "ws schedule reconnect in ${delay}ms")
        android.os.Handler(mainLooper).postDelayed({ if (!closing) connectAll() }, delay)
    }

    // ---- notifications ----

    private fun foregroundNotification(): Notification =
        NotificationCompat.Builder(this, CH_FG)
            .setContentTitle(getString(R.string.fg_title))
            .setContentText(getString(R.string.fg_text))
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()

    companion object {
        const val CH_FG = "guardian_fg"
        const val CH_REQ = "guardian_requests"
        const val FG_ID = 1001
        /** Broadcast when the pending-requests set or debug/connection state changes. */
        const val ACTION_PENDING_CHANGED = "com.omega.guardian.PENDING_CHANGED"

        private val pollHttp = OkHttpClient.Builder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(15, TimeUnit.SECONDS)
            .build()

        /** Compute the `?since=` parameter for ntfy catch-up: last seen message ID, or last timestamp, or `12h`. */
        fun sinceParamFor(ctx: Context, topic: String): String {
            Prefs.lastSeenId(ctx, topic)?.let { return it }
            val ts = Prefs.lastSeenTs(ctx, topic)
            if (ts > 0L) return ts.toString()
            return "12h"
        }

        /**
         * Poll all paired children over HTTP (`/json?poll=1&since=<since>`) with explicit UTF-8 decoding.
         * Called by [PollWorker] (~15 min) and by the manual Refresh button in [MainActivity].
         */
        fun pollAllBlocking(ctx: Context, caller: String): Int {
            ensureChannels(ctx)
            val base = Prefs.server(ctx).trimEnd('/')
            val children = Prefs.children(ctx)
            var count = 0
            for (i in 0 until children.length()) {
                val c = children.getJSONObject(i)
                val topic = c.optString("topic", ""); if (topic.isBlank()) continue
                val name = c.optString("name", "Dziecko ${i + 1}")
                val dpub = c.optString("dpub", "")
                val server = c.optString("server", base).trimEnd('/')
                val since = sinceParamFor(ctx, topic)
                val url = "$server/$topic/json?poll=1&since=$since"
                try {
                    val req = Request.Builder().url(url).build()
                    pollHttp.newCall(req).execute().use { resp ->
                        if (!resp.isSuccessful) {
                            Prefs.addDebugLog(ctx, "poll($caller) child=$name HTTP ${resp.code}")
                            return@use
                        }
                        val bytes = resp.body?.bytes() ?: ByteArray(0)
                        val bodyStr = String(bytes, StandardCharsets.UTF_8)
                        var lines = 0
                        for (line in bodyStr.lineSequence()) {
                            val trimmed = line.trim()
                            if (trimmed.isNotEmpty()) {
                                lines++
                                if (handleEventStatic(ctx, trimmed, name, topic, dpub, "poll:$caller")) count++
                            }
                        }
                        Prefs.addDebugLog(ctx, "poll($caller) child=$name since=$since lines=$lines new=$count")
                    }
                } catch (e: Exception) {
                    Prefs.addDebugLog(ctx, "poll($caller) child=$name EX ${e.javaClass.simpleName}:${e.message}")
                }
            }
            ctx.sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(ctx.packageName))
            return count
        }

        /**
         * D6 diagnostic + recovery: if `Crypto.verify` fails on a string that underwent Mojibake
         * (UTF-8 bytes misinterpreted as ISO-8859-1) or Unicode normalization differences, test
         * normalized/re-decoded variants so valid daemon-signed non-ASCII alerts/cards are never lost.
         */
        private fun verifyAlertWithFallback(dpub: String, textA: String, ts: Long, asig: String): Pair<Boolean, String> {
            if (Crypto.verify(dpub, Crypto.alertCanonical(textA, ts), asig)) return true to textA
            // Try NFC / NFD normalization
            val nfc = java.text.Normalizer.normalize(textA, java.text.Normalizer.Form.NFC)
            if (nfc != textA && Crypto.verify(dpub, Crypto.alertCanonical(nfc, ts), asig)) return true to nfc
            // Try recovering UTF-8 from ISO-8859-1 Mojibake
            val reUtf8 = String(textA.toByteArray(StandardCharsets.ISO_8859_1), StandardCharsets.UTF_8)
            if (reUtf8 != textA && Crypto.verify(dpub, Crypto.alertCanonical(reUtf8, ts), asig)) return true to reUtf8
            return false to textA
        }

        private fun verifyChallengeWithFallback(
            dpub: String, id: String, source: String, pkg: String, nonce: String, reason: String, respondTo: String, csig: String
        ): Pair<Boolean, String> {
            if (Crypto.verify(dpub, Crypto.challengeCanonical(id, source, pkg, nonce, reason, respondTo), csig)) return true to reason
            val nfc = java.text.Normalizer.normalize(reason, java.text.Normalizer.Form.NFC)
            if (nfc != reason && Crypto.verify(dpub, Crypto.challengeCanonical(id, source, pkg, nonce, nfc, respondTo), csig)) return true to nfc
            val reUtf8 = String(reason.toByteArray(StandardCharsets.ISO_8859_1), StandardCharsets.UTF_8)
            if (reUtf8 != reason && Crypto.verify(dpub, Crypto.challengeCanonical(id, source, pkg, nonce, reUtf8, respondTo), csig)) return true to reUtf8
            return false to reason
        }

        /** Process a single ntfy JSON event line from either WebSocket or HTTP poll. Returns true if new & processed. */
        fun handleEventStatic(ctx: Context, text: String, child: String, topic: String, dpub: String, via: String): Boolean {
            if (dpub.isBlank()) return false
            val ev = try { JSONObject(text) } catch (e: Exception) {
                Prefs.addDebugLog(ctx, "$via rx unparseable outer JSON (${text.length}B)")
                return false
            }
            val evType = ev.optString("event")
            if (evType != "message") return false   // skip open/keepalive/poll_request
            val evId = ev.optString("id", "")
            val evTime = ev.optLong("time", 0L)
            val msgStr = ev.optString("message", "")
            if (msgStr.isBlank()) return false

            // Deduplicate across WS + HTTP poll + `?since=` catch-up
            if (!Prefs.recordSeenEvent(ctx, topic, evId, evTime)) {
                return false
            }

            val msg = try { JSONObject(msgStr) } catch (e: Exception) {
                Prefs.addDebugLog(ctx, "$via rx id=$evId inner JSON parse FAIL (${msgStr.length} chars)")
                return false
            }
            val kind = msg.optString("kind", "challenge")

            // Security / digest alert (signed). Show only if the signature verifies against this child's daemon key.
            if (kind == "alert") {
                val textA = msg.optString("text", ctx.getString(R.string.alert_default))
                val ts = msg.optLong("ts", 0L)
                val asig = msg.optString("asig", "")
                val (ok, verifiedText) = verifyAlertWithFallback(dpub, textA, ts, asig)
                val utf8Len = verifiedText.toByteArray(StandardCharsets.UTF_8).size
                Prefs.addDebugLog(ctx, "$via rx id=$evId kind=alert len=${verifiedText.length}/${utf8Len}B verify=${if (ok) "OK" else "FAIL"}")
                if (ok) {
                    val evTitle = ev.optString("title", "").ifBlank { ctx.getString(R.string.alert_title) }
                    val alertTs = if (ts > 0L) ts else (if (evTime > 0L) evTime else (System.currentTimeMillis() / 1000))
                    val ageSec = (System.currentTimeMillis() / 1000) - alertTs
                    Prefs.addAlert(ctx, child, evTitle, verifiedText, alertTs)
                    // D5 UX: only buzz for fresh alerts (<= 180s); older catch-up alerts are collapsed into 1 summary notification
                    if (ageSec <= 180L && !via.startsWith("poll:")) {
                        postAlertStatic(ctx, evId, evTitle, "$child: $verifiedText")
                    } else {
                        bumpCatchupSummary(ctx)
                    }
                    ctx.sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(ctx.packageName))
                }
                return ok
            }

            // Live "current app" ping (signed, low priority — no notification, just updates the UI).
            if (kind == "activity") {
                val app = msg.optString("app", "")
                val ts = msg.optLong("ts", 0L)
                val sig = msg.optString("sig", "")
                val who = msg.optString("child", child)
                val ok = app.isNotBlank() && Crypto.verify(dpub, Crypto.activityCanonical(who, app, ts), sig)
                Prefs.addDebugLog(ctx, "$via rx id=$evId kind=activity app=$app verify=${if (ok) "OK" else "FAIL"}")
                if (ok) {
                    Prefs.setCurrentApp(ctx, child, app, ts)
                    ctx.sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(ctx.packageName))
                }
                return ok
            }

            // Install challenge. Reject anything this child's daemon didn't sign.
            val id = msg.optString("id", "")
            val nonce = msg.optString("nonce", "")
            val respondTo = msg.optString("respond_to", "")
            if (id.isBlank() || nonce.isBlank() || respondTo.isBlank()) {
                Prefs.addDebugLog(ctx, "$via rx id=$evId kind=$kind missing challenge fields")
                return false
            }
            val enc = msg.optString("enc", "")
            val source: String
            val pkg: String
            val rawReason: String
            if (enc.isNotBlank()) {
                val pt = Crypto.sealOpen(enc, Prefs.seed(ctx))
                if (pt == null) {
                    Prefs.addDebugLog(ctx, "$via rx id=$evId kind=challenge(enc) decrypt=FAIL")
                    return false
                }
                val dec = try { JSONObject(pt) } catch (e: Exception) {
                    Prefs.addDebugLog(ctx, "$via rx id=$evId kind=challenge(enc) decrypt=OK json=FAIL")
                    return false
                }
                source = dec.optString("source", "")
                pkg = dec.optString("package", "")
                rawReason = dec.optString("reason", "")
                Prefs.addDebugLog(ctx, "$via rx id=$evId kind=challenge(enc) decrypt=OK pkg=$pkg")
            } else {
                source = msg.optString("source", "")
                pkg = msg.optString("package", "")
                rawReason = msg.optString("reason", "")
            }
            val csig = msg.optString("csig", "")
            val (ok, reason) = verifyChallengeWithFallback(dpub, id, source, pkg, nonce, rawReason, respondTo, csig)
            Prefs.addDebugLog(ctx, "$via rx id=$evId kind=challenge pkg=$pkg verify=${if (ok) "OK" else "FAIL"}")
            if (!ok) return false   // forged / injected card → ignore

            // D5 UX: don't notify for cards that are already expired/resolved (> 300s old = daemon decision_timeout_secs).
            val nowSec = System.currentTimeMillis() / 1000
            val cardTs = if (evTime > 0L) evTime else nowSec
            val ageSec = nowSec - cardTs
            if (ageSec > 300L) {
                Prefs.addDebugLog(ctx, "$via rx id=$evId kind=challenge pkg=$pkg expired(age=${ageSec}s) -> skip notif")
                bumpCatchupSummary(ctx)
                return true
            }

            // Active request: record in-app and post actionable Allow/Deny notification.
            Prefs.addPending(ctx, id, child, source, pkg, nonce, respondTo, reason, cardTs)
            ctx.sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(ctx.packageName))
            postRequestStatic(ctx, id, child, source, pkg, nonce, reason, respondTo)
            return true
        }

        private const val CATCHUP_NOTIF_ID = 1002
        private var catchupCount = 0
        private var catchupWindowStart = 0L

        /** Collapse a burst of catch-up events into a single summary notification instead of buzzing 10 times. */
        private fun bumpCatchupSummary(ctx: Context) {
            ensureChannels(ctx)
            val now = System.currentTimeMillis()
            if (now - catchupWindowStart > 30_000L) {
                catchupCount = 0
                catchupWindowStart = now
            }
            catchupCount++
            val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val openPi = PendingIntent.getActivity(
                ctx, CATCHUP_NOTIF_ID, Intent(ctx, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            )
            val body = ctx.getString(R.string.catchup_summary_fmt, catchupCount)
            val n = NotificationCompat.Builder(ctx, CH_REQ)
                .setContentTitle(ctx.getString(R.string.alert_default))
                .setContentText(body)
                .setStyle(NotificationCompat.BigTextStyle().bigText(body))
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .setOnlyAlertOnce(true)
                .setAutoCancel(true)
                .setContentIntent(openPi)
                .build()
            nm.notify(CATCHUP_NOTIF_ID, n)
        }

        private fun postRequestStatic(
            ctx: Context, id: String, child: String, source: String, pkg: String,
            nonce: String, reason: String, respondTo: String
        ) {
            ensureChannels(ctx)
            val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val notifId = id.hashCode()
            fun action(decision: String, label: String): NotificationCompat.Action {
                val i = Intent(ctx, DecisionReceiver::class.java).apply {
                    this.action = "com.omega.guardian.$decision"
                    putExtra("decision", decision)
                    putExtra("id", id); putExtra("nonce", nonce); putExtra("source", source)
                    putExtra("package", pkg); putExtra("respond_to", respondTo)
                    putExtra("notifId", notifId)
                }
                val pi = PendingIntent.getBroadcast(
                    ctx, (id + decision).hashCode(), i,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
                )
                return NotificationCompat.Action(0, label, pi)
            }
            val risk = riskLightFor(reason)
            val who = if (child.isNotBlank()) "$child · " else ""
            val summary = who + (if (risk.isNotEmpty()) "$risk " else "") + ctx.getString(R.string.req_summary, source, pkg)
            val expanded = if (reason.isNotBlank()) "$summary\n\n${ctx.getString(R.string.details_label)}\n$reason" else summary
            val n = NotificationCompat.Builder(ctx, CH_REQ)
                .setContentTitle(ctx.getString(R.string.approve_title, pkg))
                .setContentText(summary)
                .setStyle(NotificationCompat.BigTextStyle().bigText(expanded))
                .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setCategory(NotificationCompat.CATEGORY_CALL)
                .setAutoCancel(true)
                .addAction(action("deny", ctx.getString(R.string.action_deny)))
                .addAction(action("allow", ctx.getString(R.string.action_allow)))
                .build()
            nm.notify(notifId, n)
        }

        private fun postAlertStatic(ctx: Context, evId: String, title: String, text: String) {
            ensureChannels(ctx)
            val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val n = NotificationCompat.Builder(ctx, CH_REQ)
                .setContentTitle(title)
                .setContentText(text)
                .setStyle(NotificationCompat.BigTextStyle().bigText(text))
                .setSmallIcon(android.R.drawable.stat_notify_error)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .build()
            val notifId = ("alert:" + evId.ifBlank { text }).hashCode()
            nm.notify(notifId, n)
        }

        /** Traffic-light emoji from a "RISK: low|medium|high" line in the (signed) AI summary, if present. */
        fun riskLightFor(reason: String): String {
            val lower = reason.lowercase()
            val i = lower.lastIndexOf("risk:")
            if (i < 0) return ""
            val tail = lower.substring(i + 5)
            return when {
                tail.contains("high") -> "🔴"
                tail.contains("medium") -> "🟡"
                tail.contains("low") -> "🟢"
                else -> ""
            }
        }

        fun ensureChannels(ctx: Context) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
            val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            nm.createNotificationChannel(
                NotificationChannel(CH_FG, ctx.getString(R.string.ch_fg_name), NotificationManager.IMPORTANCE_MIN)
            )
            nm.createNotificationChannel(
                NotificationChannel(CH_REQ, ctx.getString(R.string.ch_req_name), NotificationManager.IMPORTANCE_HIGH)
            )
        }

        fun start(ctx: Context) {
            val i = Intent(ctx, NtfyService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) ctx.startForegroundService(i)
            else ctx.startService(i)
        }
    }
}
