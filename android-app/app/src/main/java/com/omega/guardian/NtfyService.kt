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

    /** Open one WebSocket per paired child (each daemon has its own topic + key). */
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
            val wsUrl = server.replaceFirst("https://", "wss://").replaceFirst("http://", "ws://") + "/$topic/ws"
            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) { backoffMs = 1000L }
                override fun onMessage(webSocket: WebSocket, text: String) { handleEvent(text, name, dpub) }
                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) { scheduleReconnect() }
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) { scheduleReconnect() }
            }
            sockets.add(client.newWebSocket(Request.Builder().url(wsUrl).build(), listener))
        }
    }

    private fun scheduleReconnect() {
        if (closing) return
        val delay = backoffMs
        backoffMs = (backoffMs * 2).coerceAtMost(60_000L)
        android.os.Handler(mainLooper).postDelayed({ if (!closing) connectAll() }, delay)
    }

    private fun handleEvent(text: String, child: String, dpub: String) {
        if (dpub.isBlank()) return
        val ev = try { JSONObject(text) } catch (e: Exception) { return }
        if (ev.optString("event") != "message") return   // skip open/keepalive/poll_request
        val msgStr = ev.optString("message", "")
        if (msgStr.isBlank()) return
        val msg = try { JSONObject(msgStr) } catch (e: Exception) { return }

        // Security alert (signed). Show only if the signature verifies against this child's daemon key.
        if (msg.optString("kind") == "alert") {
            val textA = msg.optString("text", getString(R.string.alert_default))
            val ts = msg.optLong("ts", 0L)
            val asig = msg.optString("asig", "")
            if (Crypto.verify(dpub, Crypto.alertCanonical(textA, ts), asig)) {
                postAlert("$child: $textA")
            }
            return
        }

        // Install challenge. Reject anything this child's daemon didn't sign.
        val id = msg.optString("id", "")
        val nonce = msg.optString("nonce", "")
        val respondTo = msg.optString("respond_to", "")
        if (id.isBlank() || nonce.isBlank() || respondTo.isBlank()) return
        val source = msg.optString("source", "")
        val pkg = msg.optString("package", "")
        val reason = msg.optString("reason", "")
        val csig = msg.optString("csig", "")
        val canonical = Crypto.challengeCanonical(id, source, pkg, nonce, reason, respondTo)
        if (!Crypto.verify(dpub, canonical, csig)) return   // forged / injected card → ignore silently

        // Record it so the in-app list can Allow/Deny it too (not only the notification).
        Prefs.addPending(this, id, child, source, pkg, nonce, respondTo, reason, System.currentTimeMillis() / 1000)
        sendBroadcast(Intent(ACTION_PENDING_CHANGED).setPackage(packageName))
        postRequest(id, child, source, pkg, nonce, reason, respondTo)
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

    private fun postRequest(id: String, child: String, source: String, pkg: String, nonce: String, reason: String, respondTo: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val notifId = id.hashCode()
        fun action(decision: String, label: String): NotificationCompat.Action {
            val i = Intent(this, DecisionReceiver::class.java).apply {
                this.action = "com.omega.guardian.$decision"
                putExtra("decision", decision)
                putExtra("id", id); putExtra("nonce", nonce); putExtra("source", source)
                putExtra("package", pkg); putExtra("respond_to", respondTo)
                putExtra("notifId", notifId)
            }
            val pi = PendingIntent.getBroadcast(
                this, (id + decision).hashCode(), i,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            )
            return NotificationCompat.Action(0, label, pi)
        }
        // Collapsed: risk light + short summary. Expanded: full signed details (the exact command the
        // child ran + the AI risk note). `reason` carries it and is covered by the daemon's signature,
        // so neither the command nor the AI verdict can be faked by the broker.
        val risk = riskLightFor(reason)
        val who = if (child.isNotBlank()) "$child · " else ""
        val summary = who + (if (risk.isNotEmpty()) "$risk " else "") + getString(R.string.req_summary, source, pkg)
        val expanded = if (reason.isNotBlank()) "$summary\n\n${getString(R.string.details_label)}\n$reason" else summary
        val n = NotificationCompat.Builder(this, CH_REQ)
            .setContentTitle(getString(R.string.approve_title, pkg))
            .setContentText(summary)
            .setStyle(NotificationCompat.BigTextStyle().bigText(expanded))
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setAutoCancel(true)
            .addAction(action("deny", getString(R.string.action_deny)))
            .addAction(action("allow", getString(R.string.action_allow)))
            .build()
        nm.notify(notifId, n)
    }

    private fun postAlert(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val n = NotificationCompat.Builder(this, CH_REQ)
            .setContentTitle(getString(R.string.alert_title))
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setSmallIcon(android.R.drawable.stat_notify_error)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .build()
        nm.notify(("alert" + text).hashCode(), n)
    }

    companion object {
        const val CH_FG = "guardian_fg"
        const val CH_REQ = "guardian_requests"
        const val FG_ID = 1001
        /** Broadcast when the pending-requests set changes, so an open MainActivity re-renders. */
        const val ACTION_PENDING_CHANGED = "com.omega.guardian.PENDING_CHANGED"

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
