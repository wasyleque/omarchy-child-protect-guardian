package com.omega.guardian

import android.Manifest
import android.app.AlertDialog
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.widget.EditText
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.fragment.app.Fragment
import androidx.lifecycle.Lifecycle
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

class MainActivity : AppCompatActivity() {

    private lateinit var b: ActivityMainBinding
    val http = OkHttpClient()

    private var activeTabId: Int = R.id.nav_home

    // Re-render ONLY the currently visible fragment when the service records a new event while open.
    private val pendingReceiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context?, i: Intent?) = renderVisible()
    }

    private val scan = registerForActivityResult(ScanContract()) { result ->
        val contents = result.contents
        val fields = if (contents != null) Prefs.parsePairing(contents) else null
        when {
            contents == null -> toast(getString(R.string.scan_cancelled))
            fields == null -> toast(getString(R.string.qr_not_guardian))
            else -> promptChildName(fields)
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

        activeTabId = savedInstanceState?.getInt(KEY_ACTIVE_TAB, R.id.nav_home) ?: R.id.nav_home
        switchTab(activeTabId)

        b.bottomNav.selectedItemId = activeTabId
        b.bottomNav.setOnItemSelectedListener { item ->
            if (item.itemId != activeTabId) {
                switchTab(item.itemId)
            }
            true
        }

        if (Prefs.isPaired(this)) {
            NtfyService.start(this)
            PollWorker.schedule(this)
            lifecycleScope.launch(Dispatchers.IO) { NtfyService.pollAllBlocking(this@MainActivity, "startup") }
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putInt(KEY_ACTIVE_TAB, activeTabId)
    }

    override fun onResume() {
        super.onResume()
        val filter = IntentFilter(NtfyService.ACTION_PENDING_CHANGED)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            registerReceiver(pendingReceiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag") registerReceiver(pendingReceiver, filter)
        }
        renderVisible()
    }

    override fun onPause() {
        super.onPause()
        try { unregisterReceiver(pendingReceiver) } catch (e: Exception) { }
    }

    private fun switchTab(tabId: Int) {
        activeTabId = tabId
        val tag = tagForTab(tabId)
        val fm = supportFragmentManager
        val tx = fm.beginTransaction()

        for (existing in fm.fragments) {
            if (existing != null && existing.tag != tag && existing.isAdded) {
                tx.hide(existing)
                tx.setMaxLifecycle(existing, Lifecycle.State.STARTED)
            }
        }

        var target: Fragment? = fm.findFragmentByTag(tag)
        if (target == null) {
            target = when (tabId) {
                R.id.nav_settings -> SettingsFragment()
                R.id.nav_logs -> LogsFragment()
                else -> HomeFragment()
            }
            tx.add(R.id.fragmentContainer, target, tag)
        } else {
            tx.show(target)
        }
        tx.setMaxLifecycle(target, Lifecycle.State.RESUMED)
        tx.commitNowAllowingStateLoss()
    }

    private fun tagForTab(tabId: Int): String = when (tabId) {
        R.id.nav_settings -> TAG_SETTINGS
        R.id.nav_logs -> TAG_LOGS
        else -> TAG_HOME
    }

    /** Update only the currently visible fragment; hidden fragments (especially Logs) are untouched. */
    fun renderVisible() {
        val fm = supportFragmentManager
        when (activeTabId) {
            R.id.nav_home -> (fm.findFragmentByTag(TAG_HOME) as? HomeFragment)?.renderHome()
            R.id.nav_settings -> (fm.findFragmentByTag(TAG_SETTINGS) as? SettingsFragment)?.renderHeaderOnly()
            R.id.nav_logs -> (fm.findFragmentByTag(TAG_LOGS) as? LogsFragment)?.renderLogs()
        }
    }

    fun onActiveChildChanged() {
        val fm = supportFragmentManager
        (fm.findFragmentByTag(TAG_SETTINGS) as? SettingsFragment)?.onActiveChildChanged()
    }

    fun launchQrScan() {
        scan.launch(ScanOptions().setPrompt(getString(R.string.scan_prompt)).setBeepEnabled(false))
    }

    /** After a successful scan, ask for a display name (the child's account name for multi-kid PCs). */
    private fun promptChildName(fields: JSONObject) {
        val input = EditText(this).apply {
            hint = getString(R.string.add_child_name_hint)
            setText("Dziecko ${Prefs.children(this@MainActivity).length() + 1}")
        }
        AlertDialog.Builder(this)
            .setTitle(R.string.add_child_name_title)
            .setView(input)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val name = input.text.toString().trim().ifEmpty { "Dziecko" }
                Prefs.addChild(
                    this, name,
                    fields.optString("server", "https://ntfy.sh"),
                    fields.optString("topic", ""),
                    fields.optString("dpub", ""),
                    fields.optString("pair_to", "")
                )
                toast(getString(R.string.child_added))
                NtfyService.start(this)
                sendKeyToPc()
                onActiveChildChanged()
                renderVisible()
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    fun manualRefresh() {
        NtfyService.start(this)
        lifecycleScope.launch {
            val found = withContext(Dispatchers.IO) { NtfyService.pollAllBlocking(this@MainActivity, "manual") }
            toast(getString(R.string.refresh_done_fmt, found))
            renderVisible()
        }
    }

    fun decideInApp(decision: String, id: String, nonce: String, source: String, pkg: String, respondTo: String) {
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                Decisions.send(this@MainActivity, decision, id, nonce, source, pkg, respondTo)
            }
            toast(if (ok) getString(R.string.sent_ok) else getString(R.string.send_failed))
            renderVisible()
        }
    }

    /** Toggle an immediate lock / unlock of the child's computer (signed OCPG-LOCK command). */
    fun toggleLock() {
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
            renderVisible()
        }
    }

    fun sendGrant(minutes: Int) {
        val respondTo = Prefs.lastRespondTo(this)
        val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) {
            toast(getString(R.string.grant_need_request))
            return
        }
        val child = Prefs.activeChildName(this)
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = randomHex(16)
                    val sig = Crypto.signGrant(Prefs.seed(this@MainActivity), dpub, nonce, child, minutes, ts)
                    val payload = JSONObject()
                        .put("kind", "grant_time").put("daemon_pub", dpub).put("child", child)
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
    fun sendKeyToPc() {
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

    fun requestIgnoreBatteryOptimizations() {
        try {
            startActivity(
                Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName"))
            )
        } catch (e: Exception) {
            toast(getString(R.string.battery_fallback))
        }
    }

    fun randomHex(nBytes: Int): String {
        val bytes = ByteArray(nBytes); SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }

    fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_SHORT).show()

    companion object {
        private const val KEY_ACTIVE_TAB = "active_tab"
        private const val TAG_HOME = "tab_home"
        private const val TAG_SETTINGS = "tab_settings"
        private const val TAG_LOGS = "tab_logs"
    }
}
