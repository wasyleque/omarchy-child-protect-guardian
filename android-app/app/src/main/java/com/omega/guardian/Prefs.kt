package com.omega.guardian

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import org.json.JSONArray
import org.json.JSONObject
import java.util.Base64

/**
 * All at-rest state. The private seed, server, topic and daemon pubkey live in
 * EncryptedSharedPreferences (AES256-GCM, key in the hardware-backed AndroidKeyStore). The recent
 * request history is non-sensitive and kept in a plain prefs file.
 */
object Prefs {
    private const val SECURE_FILE = "guardian_secure"
    private const val PLAIN_FILE = "guardian_plain"
    private const val K_SEED = "seed_b64"
    private const val K_SERVER = "server"
    private const val K_TOPIC = "topic"
    private const val K_DPUB = "dpub"
    private const val K_RESPOND = "last_respond_to"
    private const val K_HISTORY = "history"
    private const val MAX_HISTORY = 30

    private fun secure(ctx: Context): SharedPreferences {
        val master = MasterKey.Builder(ctx)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        return EncryptedSharedPreferences.create(
            ctx,
            SECURE_FILE,
            master,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }

    private fun plain(ctx: Context): SharedPreferences =
        ctx.getSharedPreferences(PLAIN_FILE, Context.MODE_PRIVATE)

    /** The parent's Ed25519 seed, created once on first use and never leaving the device. */
    fun seed(ctx: Context): ByteArray {
        val p = secure(ctx)
        p.getString(K_SEED, null)?.let { return Base64.getDecoder().decode(it) }
        val s = Crypto.newSeed()
        p.edit().putString(K_SEED, Base64.getEncoder().encodeToString(s)).apply()
        return s
    }

    fun publicKeyB64(ctx: Context): String = Crypto.publicKeyB64(seed(ctx))

    fun server(ctx: Context): String = secure(ctx).getString(K_SERVER, "https://ntfy.sh")!!
    fun topic(ctx: Context): String? = secure(ctx).getString(K_TOPIC, null)
    fun dpub(ctx: Context): String? = secure(ctx).getString(K_DPUB, null)
    fun lastRespondTo(ctx: Context): String? = secure(ctx).getString(K_RESPOND, null)

    fun setLastRespondTo(ctx: Context, url: String) {
        secure(ctx).edit().putString(K_RESPOND, url).apply()
    }

    fun isPaired(ctx: Context): Boolean = topic(ctx) != null && dpub(ctx) != null

    /**
     * Apply a pairing payload from a scanned QR / link: the fragment after `#pair=` is base64url of
     * `{server, topic, dpub}`. Returns true if a valid pairing was stored.
     */
    fun applyPairing(ctx: Context, scanned: String): Boolean {
        val marker = "#pair="
        val i = scanned.indexOf(marker)
        val b64url = if (i >= 0) scanned.substring(i + marker.length) else scanned
        return try {
            val json = String(Base64.getUrlDecoder().decode(b64url.trim()), Charsets.UTF_8)
            val o = JSONObject(json)
            val topic = o.optString("topic", "")
            if (topic.isBlank()) return false
            secure(ctx).edit().apply {
                if (o.has("server")) putString(K_SERVER, o.getString("server"))
                putString(K_TOPIC, topic)
                if (o.has("dpub")) putString(K_DPUB, o.getString("dpub"))
            }.apply()
            true
        } catch (e: Exception) {
            false
        }
    }

    // ---- recent request history (non-sensitive) ----

    fun addHistory(ctx: Context, pkg: String, source: String, decision: String, ts: Long) {
        val p = plain(ctx)
        val arr = try {
            JSONArray(p.getString(K_HISTORY, "[]"))
        } catch (e: Exception) {
            JSONArray()
        }
        val entry = JSONObject()
            .put("pkg", pkg).put("source", source).put("decision", decision).put("ts", ts)
        // newest first, capped
        val out = JSONArray().put(entry)
        var n = 0
        while (n < arr.length() && out.length() < MAX_HISTORY) {
            out.put(arr.get(n)); n++
        }
        p.edit().putString(K_HISTORY, out.toString()).apply()
    }

    fun history(ctx: Context): JSONArray = try {
        JSONArray(plain(ctx).getString(K_HISTORY, "[]"))
    } catch (e: Exception) {
        JSONArray()
    }
}
