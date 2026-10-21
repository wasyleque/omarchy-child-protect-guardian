package com.omega.guardian

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Restart the listener after a reboot so approvals keep working without the parent reopening the app. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED && Prefs.isPaired(context)) {
            NtfyService.start(context)
        }
    }
}
