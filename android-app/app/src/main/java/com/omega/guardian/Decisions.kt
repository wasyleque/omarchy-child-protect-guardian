package com.omega.guardian

import android.content.Context
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject

/**
 * Signs an Allow / Deny decision on-device and POSTs it to the request's `respond_to` topic. Shared by
 * the notification action ([DecisionReceiver]/[DecisionWorker]) and the in-app pending list ([MainActivity])
 * so both paths sign identically. The private seed never leaves the phone. Call off the main thread.
 */
object Decisions {
    private val client = OkHttpClient()

    fun send(
        ctx: Context, decision: String, id: String, nonce: String,
        source: String, pkg: String, respondTo: String
    ): Boolean {
        return try {
            val ts = System.currentTimeMillis() / 1000
            val sig = Crypto.signDecision(Prefs.seed(ctx), id, decision, nonce, ts, source, pkg)
            val payload = JSONObject()
                .put("id", id).put("decision", decision).put("nonce", nonce)
                .put("ts", ts).put("source", source).put("package", pkg).put("sig", sig)
            val req = Request.Builder().url(respondTo).post(payload.toString().toRequestBody()).build()
            val code = client.newCall(req).execute().use { it.code }
            val ok = code in 200..299
            Prefs.addDebugLog(ctx, "decision POST $decision $pkg ($id) -> HTTP $code ok=$ok")
            if (ok) {
                Prefs.addHistory(ctx, pkg, source, decision, ts)
                Prefs.removePending(ctx, id)
            }
            ok
        } catch (e: Exception) {
            Prefs.addDebugLog(ctx, "decision POST $decision $pkg ($id) -> EX ${e.javaClass.simpleName}: ${e.message}")
            false
        }
    }
}
