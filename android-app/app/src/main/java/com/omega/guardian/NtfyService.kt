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

    private var ws: WebSocket? = null
    private var closing = false
    private var backoffMs = 1000L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannels(this)
        startForeground(FG_ID, foregroundNotification())
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        connect()
        return START_STICKY   // re-create if the system kills us
    }

    override fun onDestroy() {
        closing = true
        ws?.cancel()
        super.onDestroy()
    }

    private fun connect() {
        val topic = Prefs.topic(this) ?: return
        val base = Prefs.server(this).trimEnd('/')
        val wsUrl = base.replaceFirst("https://", "wss://").replaceFirst("http://", "ws://") +
            "/" + topic + "/ws"
        ws?.cancel()
        ws = client.newWebSocket(Request.Builder().url(wsUrl).build(), listener)
    }

    private val listener = object : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            backoffMs = 1000L
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            handleEvent(text)
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            scheduleReconnect()
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            scheduleReconnect()
        }
    }

    private fun scheduleReconnect() {
        if (closing) return
        val delay = backoffMs
        backoffMs = (backoffMs * 2).coerceAtMost(60_000L)
        android.os.Handler(mainLooper).postDelayed({ if (!closing) connect() }, delay)
    }

    private fun handleEvent(text: String) {
        val ev = try { JSONObject(text) } catch (e: Exception) { return }
        if (ev.optString("event") != "message") return   // skip open/keepalive/poll_request
        val msgStr = ev.optString("message", "")
        if (msgStr.isBlank()) return
        val msg = try { JSONObject(msgStr) } catch (e: Exception) { return }
        val dpub = Prefs.dpub(this) ?: return

        // Security alert (signed). Show only if the signature verifies against the paired daemon key.
        if (msg.optString("kind") == "alert") {
            val textA = msg.optString("text", "Guardian alert")
            val ts = msg.optLong("ts", 0L)
            val asig = msg.optString("asig", "")
            if (Crypto.verify(dpub, Crypto.alertCanonical(textA, ts), asig)) {
                postAlert(textA)
            }
            return
        }

        // Install challenge. Reject anything the real daemon didn't sign.
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

        Prefs.setLastRespondTo(this, respondTo)             // remember where grants go
        postRequest(id, source, pkg, nonce, reason, respondTo)
    }

    // ---- notifications ----

    private fun foregroundNotification(): Notification =
        NotificationCompat.Builder(this, CH_FG)
            .setContentTitle("Guardian is watching")
            .setContentText("Listening for install requests")
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()

    private fun postRequest(id: String, source: String, pkg: String, nonce: String, reason: String, respondTo: String) {
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
        // Collapsed: short summary. Expanded: the exact command the child ran (advanced review) —
        // `reason` carries it and is covered by the daemon's signature, so it can't be faked.
        val summary = "$source install of \"$pkg\""
        val expanded = if (reason.isNotBlank()) "$summary\n\nCommand:\n$reason" else summary
        val n = NotificationCompat.Builder(this, CH_REQ)
            .setContentTitle("Approve install: $pkg")
            .setContentText(summary)
            .setStyle(NotificationCompat.BigTextStyle().bigText(expanded))
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setAutoCancel(true)
            .addAction(action("deny", "Deny"))
            .addAction(action("allow", "Allow"))
            .build()
        nm.notify(notifId, n)
    }

    private fun postAlert(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val n = NotificationCompat.Builder(this, CH_REQ)
            .setContentTitle("⚠ Guardian alert")
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

        fun ensureChannels(ctx: Context) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
            val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            nm.createNotificationChannel(
                NotificationChannel(CH_FG, "Background listener", NotificationManager.IMPORTANCE_MIN)
            )
            nm.createNotificationChannel(
                NotificationChannel(CH_REQ, "Install requests", NotificationManager.IMPORTANCE_HIGH)
            )
        }

        fun start(ctx: Context) {
            val i = Intent(ctx, NtfyService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) ctx.startForegroundService(i)
            else ctx.startService(i)
        }
    }
}
