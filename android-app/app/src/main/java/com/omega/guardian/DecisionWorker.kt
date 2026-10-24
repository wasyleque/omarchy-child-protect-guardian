package com.omega.guardian

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import java.util.concurrent.TimeUnit

/**
 * D4 fix: Sends a signed Allow/Deny decision via WorkManager with a CONNECTED network constraint,
 * so tapping Allow/Deny from a notification while the app is in the background isn't silently dropped
 * by Android background network restrictions or `goAsync()` timeout.
 *
 * If the immediate attempt fails, re-posts a high-priority notification ("Sending failed — tap to retry")
 * and lets WorkManager retry when network is available.
 */
class DecisionWorker(appContext: Context, params: WorkerParameters) : CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result {
        val decision = inputData.getString("decision") ?: return Result.failure()
        val id = inputData.getString("id") ?: return Result.failure()
        val nonce = inputData.getString("nonce") ?: return Result.failure()
        val source = inputData.getString("source") ?: ""
        val pkg = inputData.getString("package") ?: ""
        val respondTo = inputData.getString("respond_to") ?: return Result.failure()
        val notifId = inputData.getInt("notifId", id.hashCode())

        val ok = Decisions.send(applicationContext, decision, id, nonce, source, pkg, respondTo)
        return if (ok) {
            // Cancel any retry notification for this request
            val nm = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            nm.cancel(notifId)
            applicationContext.sendBroadcast(Intent(NtfyService.ACTION_PENDING_CHANGED).setPackage(applicationContext.packageName))
            Result.success()
        } else {
            postRetryNotification(applicationContext, decision, id, nonce, source, pkg, respondTo, notifId)
            if (runAttemptCount < 3) Result.retry() else Result.failure()
        }
    }

    companion object {
        fun enqueue(
            ctx: Context, decision: String, id: String, nonce: String,
            source: String, pkg: String, respondTo: String, notifId: Int
        ) {
            val data = workDataOf(
                "decision" to decision,
                "id" to id,
                "nonce" to nonce,
                "source" to source,
                "package" to pkg,
                "respond_to" to respondTo,
                "notifId" to notifId
            )
            val req = OneTimeWorkRequestBuilder<DecisionWorker>()
                .setInputData(data)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 10, TimeUnit.SECONDS)
                .build()
            WorkManager.getInstance(ctx).enqueueUniqueWork("decision_$id", ExistingWorkPolicy.REPLACE, req)
        }

        fun postRetryNotification(
            ctx: Context, decision: String, id: String, nonce: String,
            source: String, pkg: String, respondTo: String, notifId: Int
        ) {
            NtfyService.ensureChannels(ctx)
            val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val i = Intent(ctx, DecisionReceiver::class.java).apply {
                action = "com.omega.guardian.$decision.retry"
                putExtra("decision", decision)
                putExtra("id", id); putExtra("nonce", nonce); putExtra("source", source)
                putExtra("package", pkg); putExtra("respond_to", respondTo)
                putExtra("notifId", notifId)
            }
            val pi = PendingIntent.getBroadcast(
                ctx, (id + decision + "retry").hashCode(), i,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            )
            val n = NotificationCompat.Builder(ctx, NtfyService.CH_REQ)
                .setContentTitle(ctx.getString(R.string.approve_title, pkg))
                .setContentText(ctx.getString(R.string.decision_retry_prompt, decision.uppercase()))
                .setSmallIcon(android.R.drawable.stat_notify_error)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(pi)
                .addAction(0, ctx.getString(R.string.decision_retry_btn), pi)
                .build()
            nm.notify(notifId, n)
        }
    }
}
