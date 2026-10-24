package com.omega.guardian

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * D5 periodic poll fallback (~15 min): fetches missed ntfy messages (`?poll=1&since=<lastId|lastTs>`)
 * for all paired children when connected to the network, recovering cards/alerts/digests even if
 * Doze or a network switch interrupted the WebSocket stream.
 */
class PollWorker(appContext: Context, params: WorkerParameters) : CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result {
        NtfyService.pollAllBlocking(applicationContext, "periodic-worker")
        return Result.success()
    }

    companion object {
        private const val WORK_NAME = "guardian_periodic_poll"

        fun schedule(ctx: Context) {
            val req = PeriodicWorkRequestBuilder<PollWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(
                WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, req
            )
        }
    }
}
