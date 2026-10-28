package com.omega.guardian

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import org.json.JSONArray
import org.json.JSONObject
import java.util.Base64

/**
 * All at-rest state. ONE parent Ed25519 seed (shared across all children) + a LIST of paired children,
 * each {name, server, topic, dpub, pair_to} — so one phone controls several kids on several computers.
 * The seed and the child list live in EncryptedSharedPreferences (AES256-GCM, key in the hardware-backed
 * AndroidKeyStore); the non-sensitive history/pending live in a plain prefs file.
 */
object Prefs {
    private const val SECURE_FILE = "guardian_secure"
    private const val PLAIN_FILE = "guardian_plain"
    private const val K_SEED = "seed_b64"
    private const val K_CHILDREN = "children"      // JSON array of {name, server, topic, dpub, pair_to}
    private const val K_ACTIVE = "active_child"     // index into K_CHILDREN
    private const val K_HISTORY = "history"
    private const val MAX_HISTORY = 30
    // legacy single-pairing keys (migrated into K_CHILDREN on first use)
    private const val K_TOPIC = "topic"
    private const val K_SERVER = "server"
    private const val K_DPUB = "dpub"
    private const val K_PAIR_TO = "pair_to"

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

    // ---- children ----

    /** The paired children, migrating a legacy single pairing into the list on first read. */
    fun children(ctx: Context): JSONArray {
        val p = secure(ctx)
        val raw = p.getString(K_CHILDREN, null)
        if (raw != null) return try { JSONArray(raw) } catch (e: Exception) { JSONArray() }
        // migrate the old single pairing, if any
        val oldTopic = p.getString(K_TOPIC, null)
        val arr = JSONArray()
        if (oldTopic != null) {
            arr.put(JSONObject()
                .put("name", "Dziecko")
                .put("server", p.getString(K_SERVER, "https://ntfy.sh"))
                .put("topic", oldTopic)
                .put("dpub", p.getString(K_DPUB, "") ?: "")
                .put("pair_to", p.getString(K_PAIR_TO, "") ?: ""))
            p.edit().putString(K_CHILDREN, arr.toString()).putInt(K_ACTIVE, 0).apply()
        }
        return arr
    }

    fun childNames(ctx: Context): List<String> {
        val c = children(ctx)
        return (0 until c.length()).map { c.getJSONObject(it).optString("name", "Dziecko ${it + 1}") }
    }

    fun activeIndex(ctx: Context): Int {
        val n = children(ctx).length()
        if (n == 0) return -1
        return secure(ctx).getInt(K_ACTIVE, 0).coerceIn(0, n - 1)
    }

    fun setActiveIndex(ctx: Context, i: Int) {
        secure(ctx).edit().putInt(K_ACTIVE, i).apply()
    }

    fun activeChild(ctx: Context): JSONObject? {
        val i = activeIndex(ctx)
        if (i < 0) return null
        return children(ctx).optJSONObject(i)
    }

    /** Add a paired child and make it active. Returns the new index. */
    fun addChild(ctx: Context, name: String, server: String, topic: String, dpub: String, pairTo: String): Int {
        val arr = children(ctx)
        arr.put(JSONObject()
            .put("name", name).put("server", server).put("topic", topic)
            .put("dpub", dpub).put("pair_to", pairTo))
        val idx = arr.length() - 1
        secure(ctx).edit().putString(K_CHILDREN, arr.toString()).putInt(K_ACTIVE, idx).apply()
        return idx
    }

    fun removeActiveChild(ctx: Context) {
        val i = activeIndex(ctx); if (i < 0) return
        val arr = children(ctx); val out = JSONArray()
        for (j in 0 until arr.length()) if (j != i) out.put(arr.get(j))
        secure(ctx).edit().putString(K_CHILDREN, out.toString()).putInt(K_ACTIVE, 0).apply()
    }

    // active-child-backed accessors (used by the grant/lock/schedule actions + pairing return)
    fun server(ctx: Context): String = activeChild(ctx)?.optString("server") ?: "https://ntfy.sh"
    fun topic(ctx: Context): String? = activeChild(ctx)?.optString("topic")?.ifBlank { null }
    fun dpub(ctx: Context): String? = activeChild(ctx)?.optString("dpub")?.ifBlank { null }
    fun pairTo(ctx: Context): String? = activeChild(ctx)?.optString("pair_to")?.ifBlank { null }
    fun activeChildName(ctx: Context): String = activeChild(ctx)?.optString("name") ?: ""

    /** Where a grant/lock/schedule command for the active child is POSTed (deterministic, review F15). */
    fun lastRespondTo(ctx: Context): String? {
        val t = topic(ctx) ?: return null
        return "${server(ctx).trimEnd('/')}/$t-resp"
    }

    fun isPaired(ctx: Context): Boolean = children(ctx).length() > 0

    /** Live "current app" per child (from signed activity pings). Stored as "ts\tapp". */
    fun setCurrentApp(ctx: Context, child: String, app: String, ts: Long) {
        plain(ctx).edit().putString("cur_$child", "$ts\t$app").apply()
    }
    /** The child's current app if seen in the last 3 minutes, else null (stale → "offline"). */
    fun currentApp(ctx: Context, child: String): String? {
        val v = plain(ctx).getString("cur_$child", null) ?: return null
        val parts = v.split("\t", limit = 2)
        if (parts.size < 2) return null
        val ts = parts[0].toLongOrNull() ?: return null
        if (System.currentTimeMillis() / 1000 - ts > 180) return null
        return parts[1]
    }

    /** Optimistic local "lock now" toggle, per active child. */
    fun lockRequested(ctx: Context): Boolean = plain(ctx).getBoolean("lock_${activeChildName(ctx)}", false)
    fun setLockRequested(ctx: Context, v: Boolean) {
        plain(ctx).edit().putBoolean("lock_${activeChildName(ctx)}", v).apply()
    }

    /** Last pushed AI config JSON per child (so the form stays populated when switching children). */
    fun aiConfig(ctx: Context, child: String = activeChildName(ctx)): JSONObject? {
        val raw = plain(ctx).getString("ai_$child", null) ?: return null
        return try { JSONObject(raw) } catch (e: Exception) { null }
    }
    fun setAiConfig(ctx: Context, cfgJson: String, child: String = activeChildName(ctx)) {
        plain(ctx).edit().putString("ai_$child", cfgJson).apply()
    }

    /** Last pushed autonomy mode per child ("ask" | "quiet" | "auto"), default "ask". */
    fun autonomyMode(ctx: Context, child: String = activeChildName(ctx)): String =
        plain(ctx).getString("autonomy_$child", "ask") ?: "ask"

    fun setAutonomyMode(ctx: Context, mode: String, child: String = activeChildName(ctx)) {
        plain(ctx).edit().putString("autonomy_$child", mode).apply()
    }

    /**
     * Parse a scanned QR / link (`#pair=` base64url of `{server, topic, dpub, pair_to}`). Returns the
     * fields as a JSONObject (does NOT store — the caller names the child, then calls [addChild]).
     */
    fun parsePairing(scanned: String): JSONObject? {
        val marker = "#pair="
        val i = scanned.indexOf(marker)
        val b64url = if (i >= 0) scanned.substring(i + marker.length) else scanned
        return try {
            val json = String(Base64.getUrlDecoder().decode(b64url.trim()), Charsets.UTF_8)
            val o = JSONObject(json)
            if (o.optString("topic", "").isBlank()) null else o
        } catch (e: Exception) {
            null
        }
    }

    // ---- recent request history (non-sensitive) ----

    fun addHistory(ctx: Context, pkg: String, source: String, decision: String, ts: Long) {
        val p = plain(ctx)
        val arr = try { JSONArray(p.getString(K_HISTORY, "[]")) } catch (e: Exception) { JSONArray() }
        val entry = JSONObject().put("package", pkg).put("pkg", pkg).put("source", source).put("decision", decision).put("ts", ts)
        val out = JSONArray().put(entry)
        var n = 0
        while (n < arr.length() && out.length() < MAX_HISTORY) { out.put(arr.get(n)); n++ }
        p.edit().putString(K_HISTORY, out.toString()).apply()
    }

    fun history(ctx: Context): JSONArray = try {
        JSONArray(plain(ctx).getString(K_HISTORY, "[]"))
    } catch (e: Exception) {
        JSONArray()
    }

    // ---- pending requests (shown in-app so you can Allow/Deny without the notification) ----
    private const val K_PENDING = "pending"
    private const val PENDING_TTL_SEC = 15 * 60   // the daemon times out long before this

    /** Record a verified challenge so MainActivity can list it. De-duped by id, newest first. */
    fun addPending(
        ctx: Context, id: String, child: String, source: String, pkg: String,
        nonce: String, respondTo: String, reason: String, ts: Long
    ) {
        val cur = pending(ctx)
        val out = JSONArray().put(
            JSONObject().put("id", id).put("child", child).put("source", source).put("package", pkg)
                .put("nonce", nonce).put("respond_to", respondTo).put("reason", reason).put("ts", ts)
        )
        for (i in 0 until cur.length()) {
            val o = cur.getJSONObject(i)
            if (o.optString("id") != id) out.put(o)
        }
        plain(ctx).edit().putString(K_PENDING, out.toString()).apply()
    }

    fun removePending(ctx: Context, id: String) {
        val cur = pending(ctx)
        val out = JSONArray()
        for (i in 0 until cur.length()) {
            val o = cur.getJSONObject(i)
            if (o.optString("id") != id) out.put(o)
        }
        plain(ctx).edit().putString(K_PENDING, out.toString()).apply()
    }

    /** Current pending requests, dropping any older than the TTL (expired cards the daemon denied). */
    fun pending(ctx: Context): JSONArray {
        val raw = try { JSONArray(plain(ctx).getString(K_PENDING, "[]")) } catch (e: Exception) { JSONArray() }
        val now = System.currentTimeMillis() / 1000
        val out = JSONArray()
        for (i in 0 until raw.length()) {
            val o = raw.getJSONObject(i)
            if (now - o.optLong("ts") <= PENDING_TTL_SEC) out.put(o)
        }
        if (out.length() != raw.length()) plain(ctx).edit().putString(K_PENDING, out.toString()).apply()
        return out
    }

    // ---- D5: per-topic last seen ntfy message id/timestamp + seen-id dedupe ----

    fun lastSeenId(ctx: Context, topic: String): String? =
        plain(ctx).getString("since_id_$topic", null)?.ifBlank { null }

    fun lastSeenTs(ctx: Context, topic: String): Long =
        plain(ctx).getLong("since_ts_$topic", 0L)

    fun recordSeenEvent(ctx: Context, topic: String, eventId: String, eventTs: Long): Boolean {
        val p = plain(ctx)
        val seenKey = "seen_ids_$topic"
        val raw = p.getString(seenKey, "") ?: ""
        val ids = if (raw.isEmpty()) mutableListOf() else raw.split(",").toMutableList()
        val already = eventId.isNotBlank() && ids.contains(eventId)
        if (eventId.isNotBlank() && !already) {
            ids.add(0, eventId)
            while (ids.size > 100) ids.removeAt(ids.lastIndex)
        }
        val ed = p.edit()
        if (eventId.isNotBlank()) {
            ed.putString("since_id_$topic", eventId)
            ed.putString(seenKey, ids.joinToString(","))
        }
        if (eventTs > 0L) ed.putLong("since_ts_$topic", eventTs)
        ed.apply()
        return !already
    }

    // ---- D5: connection status per child ----
    fun setConnected(ctx: Context, child: String, connected: Boolean) {
        plain(ctx).edit().putBoolean("conn_$child", connected).apply()
    }
    fun isConnected(ctx: Context, child: String = activeChildName(ctx)): Boolean =
        plain(ctx).getBoolean("conn_$child", true)

    // ---- D6: recent verified alerts (shown in-app alongside notifications) ----
    private const val K_ALERTS = "alerts"
    private const val MAX_ALERTS = 15

    fun addAlert(ctx: Context, child: String, title: String, text: String, ts: Long) {
        val p = plain(ctx)
        val arr = try { JSONArray(p.getString(K_ALERTS, "[]")) } catch (e: Exception) { JSONArray() }
        val entry = JSONObject().put("child", child).put("title", title).put("text", text).put("ts", ts)
        val out = JSONArray().put(entry)
        var n = 0
        while (n < arr.length() && out.length() < MAX_ALERTS) { out.put(arr.get(n)); n++ }
        p.edit().putString(K_ALERTS, out.toString()).apply()
    }

    fun alerts(ctx: Context): JSONArray = try {
        JSONArray(plain(ctx).getString(K_ALERTS, "[]"))
    } catch (e: Exception) {
        JSONArray()
    }

    // ---- Debug log (last 200 ws/crypto/network events for D4/D5/D6/M4 diagnostics) ----
    private const val K_DEBUG_LOG = "debug_log"
    private const val MAX_DEBUG = 200
    private val debugLock = Any()

    fun addDebugLog(ctx: Context, msg: String) {
        synchronized(debugLock) {
            val p = plain(ctx)
            val arr = try { JSONArray(p.getString(K_DEBUG_LOG, "[]")) } catch (e: Exception) { JSONArray() }
            val fmt = java.text.SimpleDateFormat("HH:mm:ss.SSS", java.util.Locale.US)
            val line = "${fmt.format(java.util.Date())} $msg"
            val out = JSONArray().put(line)
            var i = 0
            while (i < arr.length() && out.length() < MAX_DEBUG) {
                out.put(arr.getString(i))
                i++
            }
            p.edit().putString(K_DEBUG_LOG, out.toString()).apply()
        }
    }

    fun debugLogs(ctx: Context): List<String> {
        val arr = try { JSONArray(plain(ctx).getString(K_DEBUG_LOG, "[]")) } catch (e: Exception) { JSONArray() }
        val limit = arr.length().coerceAtMost(MAX_DEBUG)
        return (0 until limit).map { arr.optString(it, "") }
    }

    fun clearDebugLogs(ctx: Context) {
        synchronized(debugLock) {
            plain(ctx).edit().putString(K_DEBUG_LOG, "[]").apply()
        }
    }
}
