package com.omega.guardian

import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

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
                Decisions.send(context, decision, id, nonce, source, pkg, respondTo)
            } finally {
                pending.finish()
            }
        }
    }
}
