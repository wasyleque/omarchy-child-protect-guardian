package com.omega.guardian

import android.Manifest
import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import com.omega.guardian.databinding.ActivityMainBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.security.SecureRandom
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class MainActivity : AppCompatActivity() {

    private lateinit var b: ActivityMainBinding
    private val http = OkHttpClient()

    // Re-render the pending list when the service records a new (verified) request while we're open.
    private val pendingReceiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context?, i: Intent?) = render()
    }

    private val scan = registerForActivityResult(ScanContract()) { result ->
        val contents = result.contents
        if (contents == null) {
            toast(getString(R.string.scan_cancelled))
        } else if (Prefs.applyPairing(this, contents)) {
            toast(getString(R.string.paired_ok))
            NtfyService.start(this)
            sendKeyToPc()   // auto-return our public key so the PC finishes pairing without copy/paste
            render()
        } else {
            toast(getString(R.string.qr_not_guardian))
        }
    }

    private val notifPerm = registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        b = ActivityMainBinding.inflate(layoutInflater)
        setContentView(b.root)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            notifPerm.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        b.btnScan.setOnClickListener {
            scan.launch(ScanOptions().setPrompt(getString(R.string.scan_prompt)).setBeepEnabled(false))
        }
        b.btnCopy.setOnClickListener {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            cm.setPrimaryClip(ClipData.newPlainText("guardian pubkey", Prefs.publicKeyB64(this)))
            toast(getString(R.string.pubkey_copied))
        }
        b.btnSendKey.setOnClickListener { sendKeyToPc() }
        b.btnBattery.setOnClickListener { requestIgnoreBatteryOptimizations() }
        b.btnGrant15.setOnClickListener { sendGrant(15) }
        b.btnGrant30.setOnClickListener { sendGrant(30) }
        b.btnLockNow.setOnClickListener { toggleLock() }
        b.btnSendSchedule.setOnClickListener { sendSchedule() }

        if (Prefs.isPaired(this)) NtfyService.start(this)
        render()
    }

    override fun onResume() {
        super.onResume()
        val filter = IntentFilter(NtfyService.ACTION_PENDING_CHANGED)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            registerReceiver(pendingReceiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag") registerReceiver(pendingReceiver, filter)
        }
        render()
    }

    override fun onPause() {
        super.onPause()
        try { unregisterReceiver(pendingReceiver) } catch (e: Exception) { }
    }

    @SuppressLint("SetTextI18n")
    private fun render() {
        b.tvStatus.text = getString(if (Prefs.isPaired(this)) R.string.status_paired else R.string.status_not_paired)
        b.tvPubkey.text = Prefs.publicKeyB64(this)
        val h = Prefs.history(this)
        val fmt = SimpleDateFormat("MMM d HH:mm", Locale.getDefault())
        b.tvHistory.text = if (h.length() == 0) getString(R.string.history_empty) else buildString {
            for (i in 0 until h.length()) {
                val o = h.getJSONObject(i)
                append(fmt.format(Date(o.optLong("ts") * 1000)))
                append("  ").append(o.optString("decision").uppercase())
                append("  ").append(o.optString("package"))
                append("  (").append(o.optString("source")).append(")\n")
            }
        }
        b.btnLockNow.setText(if (Prefs.lockRequested(this)) R.string.btn_unlock_now else R.string.btn_lock_now)
        renderPending()
    }

    /** Build an Allow/Deny card per pending request, so you can decide in-app (not only the push). */
    private fun renderPending() {
        val pend = Prefs.pending(this)
        b.pendingContainer.removeAllViews()
        b.tvPendingEmpty.visibility = if (pend.length() == 0) View.VISIBLE else View.GONE
        for (i in 0 until pend.length()) {
            val o = pend.getJSONObject(i)
            val id = o.optString("id"); val source = o.optString("source"); val pkg = o.optString("package")
            val nonce = o.optString("nonce"); val respondTo = o.optString("respond_to"); val reason = o.optString("reason")

            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, 12, 0, 12)
            }
            val light = NtfyService.riskLightFor(reason)
            val label = TextView(this).apply {
                text = (if (light.isNotEmpty()) "$light " else "") +
                    getString(R.string.req_summary, source, pkg) +
                    (if (reason.isNotBlank()) "\n$reason" else "")
                textSize = 13f
            }
            val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; setPadding(0, 6, 0, 0) }
            val deny = Button(this).apply {
                text = getString(R.string.action_deny)
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                setOnClickListener { decideInApp("deny", id, nonce, source, pkg, respondTo) }
            }
            val allow = Button(this).apply {
                text = getString(R.string.action_allow)
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                setOnClickListener { decideInApp("allow", id, nonce, source, pkg, respondTo) }
            }
            row.addView(deny); row.addView(allow)
            card.addView(label); card.addView(row)
            b.pendingContainer.addView(card)
        }
    }

    private fun decideInApp(decision: String, id: String, nonce: String, source: String, pkg: String, respondTo: String) {
        Prefs.removePending(this, id)   // optimistic: drop it from the list immediately
        renderPending()
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) { Decisions.send(this@MainActivity, decision, id, nonce, source, pkg, respondTo) }
            toast(if (ok) getString(R.string.sent_ok) else getString(R.string.send_failed))
            render()
        }
    }

    /** Toggle an immediate lock / unlock of the child's computer (signed OCPG-LOCK command). */
    private fun toggleLock() {
        val respondTo = Prefs.lastRespondTo(this)
        val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) {
            toast(getString(R.string.lock_needs_pair)); return
        }
        val lock = !Prefs.lockRequested(this)
        val action = if (lock) "lock" else "unlock"
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = randomHex(16)
                    val sig = Crypto.signLock(Prefs.seed(this@MainActivity), dpub, nonce, action, ts)
                    val payload = JSONObject()
                        .put("kind", "lock_now").put("daemon_pub", dpub)
                        .put("nonce", nonce).put("action", action).put("ts", ts).put("sig", sig)
                    val req = Request.Builder().url(respondTo).post(payload.toString().toRequestBody()).build()
                    http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) { false }
            }
            if (ok) Prefs.setLockRequested(this@MainActivity, lock)
            toast(if (ok) getString(if (lock) R.string.lock_sent else R.string.unlock_sent) else getString(R.string.send_failed))
            render()
        }
    }

    private fun sendGrant(minutes: Int) {
        val respondTo = Prefs.lastRespondTo(this)
        val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) {
            toast(getString(R.string.grant_need_request))
            return
        }
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = randomHex(16)
                    val sig = Crypto.signGrant(Prefs.seed(this@MainActivity), dpub, nonce, minutes, ts)
                    val payload = JSONObject()
                        .put("kind", "grant_time").put("daemon_pub", dpub)
                        .put("nonce", nonce).put("minutes", minutes).put("ts", ts).put("sig", sig)
                    val req = Request.Builder().url(respondTo)
                        .post(payload.toString().toRequestBody()).build()
                    http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) {
                    false
                }
            }
            toast(if (ok) getString(R.string.grant_ok, minutes) else getString(R.string.grant_failed))
        }
    }

    /** POST our public key to the pairing-return topic so the PC captures it (packaging/pair.sh). */
    private fun sendKeyToPc() {
        val pairTo = Prefs.pairTo(this)
        if (pairTo == null) {
            toast(getString(R.string.rescan_no_channel))
            return
        }
        val server = Prefs.server(this).trimEnd('/')
        val url = "$server/$pairTo"
        val pub = Prefs.publicKeyB64(this)
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val payload = JSONObject().put("kind", "parent_key").put("pubkey", pub)
                    val req = Request.Builder().url(url).post(payload.toString().toRequestBody()).build()
                    http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) {
                    false
                }
            }
            toast(if (ok) getString(R.string.key_sent_ok) else getString(R.string.key_sent_failed))
        }
    }

    /** Parse "HH:MM" to minutes-from-midnight; null if blank; throws on malformed input. */
    private fun hhmmToMin(s: String): Int? {
        val t = s.trim()
        if (t.isEmpty()) return null
        val parts = t.split(":")
        require(parts.size == 2)
        val h = parts[0].toInt(); val m = parts[1].toInt()
        require(h in 0..23 && m in 0..59)
        return h * 60 + m
    }

    /** Build the child's schedule from the form, sign it (OCPG-SCHED) and POST it to the daemon. */
    private fun sendSchedule() {
        val respondTo = Prefs.lastRespondTo(this)
        val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) { toast(getString(R.string.lock_needs_pair)); return }

        val sched = JSONObject()
        try {
            b.etSchedBudget.text.toString().trim().takeIf { it.isNotEmpty() }?.let {
                sched.put("daily_budget_minutes", it.toInt())
            }
            hhmmToMin(b.etWinStart.text.toString())?.let { sched.put("window_start_min", it) }
            hhmmToMin(b.etWinEnd.text.toString())?.let { sched.put("window_end_min", it) }
            hhmmToMin(b.etQuietStart.text.toString())?.let { sched.put("quiet_start_min", it) }
            hhmmToMin(b.etQuietEnd.text.toString())?.let { sched.put("quiet_end_min", it) }
            b.etWeekend.text.toString().trim().takeIf { it.isNotEmpty() }?.let {
                val wknd = it.toInt()
                val days = JSONObject()
                days.put("sat", JSONObject().put("budget_minutes", wknd))
                days.put("sun", JSONObject().put("budget_minutes", wknd))
                sched.put("days", days)
            }
        } catch (e: Exception) {
            toast(getString(R.string.schedule_bad_input)); return
        }

        val child = b.etSchedChild.text.toString().trim()
        val schedJson = sched.toString()
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = randomHex(16)
                    val sig = Crypto.signSchedule(Prefs.seed(this@MainActivity), dpub, nonce, child, ts, schedJson)
                    val payload = JSONObject()
                        .put("kind", "set_schedule").put("daemon_pub", dpub).put("nonce", nonce)
                        .put("child", child).put("ts", ts).put("sched", schedJson).put("sig", sig)
                    val req = Request.Builder().url(respondTo).post(payload.toString().toRequestBody()).build()
                    http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) { false }
            }
            toast(if (ok) getString(R.string.schedule_sent) else getString(R.string.send_failed))
        }
    }

    private fun requestIgnoreBatteryOptimizations() {
        try {
            startActivity(
                Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName"))
            )
        } catch (e: Exception) {
            toast(getString(R.string.battery_fallback))
        }
    }

    private fun randomHex(nBytes: Int): String {
        val b = ByteArray(nBytes); SecureRandom().nextBytes(b)
        return b.joinToString("") { "%02x".format(it) }
    }

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_SHORT).show()
}
