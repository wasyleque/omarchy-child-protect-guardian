package com.omega.guardian

import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject

/**
 * Handles an Allow / Deny tap from a request notification: signs the decision on-device and POSTs it
 * to the request's `respond_to` topic. The private seed never leaves the phone.
 */
class DecisionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val decision = intent.getStringExtra("decision") ?: return
        val id = intent.getStringExtra("id") ?: return
        val nonce = intent.getStringExtra("nonce") ?: return
        val source = intent.getStringExtra("source") ?: ""
        val pkg = intent.getStringExtra("package") ?: ""
        val respondTo = intent.getStringExtra("respond_to") ?: return
        val notifId = intent.getIntExtra("notifId", id.hashCode())

        // Dismiss the notification immediately so it can't be tapped twice.
        (context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).cancel(notifId)

        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                val ts = System.currentTimeMillis() / 1000
                val seed = Prefs.seed(context)
                val sig = Crypto.signDecision(seed, id, decision, nonce, ts, source, pkg)
                val payload = JSONObject()
                    .put("id", id).put("decision", decision).put("nonce", nonce)
                    .put("ts", ts).put("source", source).put("package", pkg).put("sig", sig)
                val req = Request.Builder()
                    .url(respondTo)
                    .post(payload.toString().toRequestBody())
                    .build()
                client.newCall(req).execute().use { /* resolve on daemon side */ }
                Prefs.addHistory(context, pkg, source, decision, ts)
            } catch (e: Exception) {
                // Best-effort: if the POST fails the daemon's timeout fails closed (deny).
            } finally {
                pending.finish()
            }
        }
    }

    companion object {
        private val client = OkHttpClient()
    }
}
