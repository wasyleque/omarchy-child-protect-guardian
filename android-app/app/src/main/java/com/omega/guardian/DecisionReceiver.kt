package com.omega.guardian

import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Handles an Allow / Deny tap from a request notification: dismisses the card immediately and enqueues
 * [DecisionWorker] (WorkManager with network constraint) which signs the decision on-device, POSTs it
 * to `respond_to`, and re-posts a "Sending failed — tap to retry" notification if the POST fails (D4 fix).
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

        (context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).cancel(notifId)
        Prefs.addDebugLog(context, "notif tap: $decision $pkg ($id)")
        DecisionWorker.enqueue(context, decision, id, nonce, source, pkg, respondTo, notifId)
    }
}
