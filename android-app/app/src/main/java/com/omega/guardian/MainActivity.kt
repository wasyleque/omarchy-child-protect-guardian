package com.omega.guardian

import android.Manifest
import android.annotation.SuppressLint
import android.app.AlertDialog
import android.app.TimePickerDialog
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
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.CheckBox
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Spinner
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

    // Schedule-editor row state (built in code so there's no free-text input to get wrong).
    private val rowSpinners = mutableListOf<Spinner>()  // budget preset per row
    private val rowHours = mutableListOf<Button>()      // hours button per row (tag = IntArray[start,end] or null)
    private val rowDay = mutableListOf<Int>()           // weekday 0=Mon..6=Sun, or -1 = "every day"

    // Re-render the pending list when the service records a new (verified) request while we're open.
    private val pendingReceiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context?, i: Intent?) = render()
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

    /** After a successful scan, ask for a display name (the child's account name for multi-kid PCs). */
    private fun promptChildName(fields: org.json.JSONObject) {
        val input = EditText(this).apply {
            hint = getString(R.string.add_child_name_hint)
            setText("Dziecko ${Prefs.children(this@MainActivity).length() + 1}")
        }
        AlertDialog.Builder(this)
            .setTitle(R.string.add_child_name_title)
            .setView(input)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val name = input.text.toString().trim().ifEmpty { "Dziecko" }
                Prefs.addChild(this, name,
                    fields.optString("server", "https://ntfy.sh"),
                    fields.optString("topic", ""),
                    fields.optString("dpub", ""),
                    fields.optString("pair_to", ""))
                toast(getString(R.string.child_added))
                NtfyService.start(this)   // (re)subscribe to all children incl. the new one
                sendKeyToPc()             // return our key so the PC finishes pairing
                render()
                loadAiConfigToForm()
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
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
        b.btnSendAi.setOnClickListener { sendAiConfig() }
        b.btnRefresh.setOnClickListener { manualRefresh() }
        b.btnCopyDebug.setOnClickListener {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            cm.setPrimaryClip(ClipData.newPlainText("guardian debug log", Prefs.debugLogs(this).joinToString("\n")))
            toast(getString(R.string.debug_copied))
        }
        b.btnClearDebug.setOnClickListener {
            Prefs.clearDebugLogs(this)
            render()
        }

        b.spChild.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) {
                // Only react to a real user change (ignore programmatic re-selection → no render loop).
                if (pos != Prefs.activeIndex(this@MainActivity)) {
                    Prefs.setActiveIndex(this@MainActivity, pos)
                    render()
                    loadAiConfigToForm()
                }
            }
            override fun onNothingSelected(p: AdapterView<*>?) {}
        }
        b.cbSameEveryDay.setOnCheckedChangeListener { _, _ -> buildScheduleRows() }

        if (Prefs.isPaired(this)) {
            NtfyService.start(this)
            PollWorker.schedule(this)
            lifecycleScope.launch(Dispatchers.IO) { NtfyService.pollAllBlocking(this@MainActivity, "startup") }
        }
        buildScheduleRows()
        setupAiControls()
        render()
        loadAiConfigToForm()
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
        val n = Prefs.children(this).length()
        val connLost = n > 0 && !Prefs.isConnected(this)
        val base = when {
            n == 0 -> getString(R.string.status_not_paired)
            connLost -> getString(R.string.status_conn_lost_fmt, n)
            else -> getString(R.string.status_children_fmt, n)
        }
        val now = if (n > 0) Prefs.currentApp(this, Prefs.activeChildName(this)) else null
        b.tvStatus.text = if (now != null) "$base\n" + getString(R.string.now_using_fmt, now) else base
        populateChildSpinner()
        b.lblScheduleFor.text = if (n > 0) getString(R.string.sched_for_fmt, Prefs.activeChildName(this)) else getString(R.string.hdr_schedule)
        b.lblAiFor.text = if (n > 0) getString(R.string.ai_for_fmt, Prefs.activeChildName(this)) else getString(R.string.hdr_ai)
        b.tvPubkey.text = Prefs.publicKeyB64(this)
        val h = Prefs.history(this)
        val fmt = SimpleDateFormat("MMM d HH:mm", Locale.getDefault())
        b.tvHistory.text = if (h.length() == 0) getString(R.string.history_empty) else buildString {
            for (i in 0 until h.length()) {
                val o = h.getJSONObject(i)
                append(fmt.format(Date(o.optLong("ts") * 1000)))
                append("  ").append(o.optString("decision").uppercase())
                append("  ").append(o.optString("package").ifBlank { o.optString("pkg") })
                append("  (").append(o.optString("source")).append(")\n")
            }
        }
        val al = Prefs.alerts(this)
        b.tvAlerts.text = if (al.length() == 0) getString(R.string.alerts_empty) else buildString {
            for (i in 0 until al.length()) {
                val o = al.getJSONObject(i)
                append(fmt.format(Date(o.optLong("ts") * 1000)))
                val who = o.optString("child")
                if (who.isNotBlank()) append(" [").append(who).append("]")
                append(" ").append(o.optString("title")).append(":\n")
                append(o.optString("text")).append("\n\n")
            }
        }.trimEnd()
        val dbg = Prefs.debugLogs(this)
        b.tvDebugLog.text = if (dbg.isEmpty()) getString(R.string.debug_empty) else dbg.joinToString("\n")
        b.btnLockNow.setText(if (Prefs.lockRequested(this)) R.string.btn_unlock_now else R.string.btn_lock_now)
        renderPending()
    }

    private fun manualRefresh() {
        NtfyService.start(this)
        lifecycleScope.launch {
            val found = withContext(Dispatchers.IO) { NtfyService.pollAllBlocking(this@MainActivity, "manual") }
            toast(getString(R.string.refresh_done_fmt, found))
            render()
        }
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
            val who = o.optString("child")

            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, 12, 0, 12)
            }
            val light = NtfyService.riskLightFor(reason)
            val label = TextView(this).apply {
                text = (if (who.isNotBlank()) "$who · " else "") +
                    (if (light.isNotEmpty()) "$light " else "") +
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

    private fun populateChildSpinner() {
        val names = Prefs.childNames(this)
        b.spChild.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, names)
        val idx = Prefs.activeIndex(this)
        if (idx in names.indices) b.spChild.setSelection(idx)
    }

    // Budget dropdown positions → minutes. null = "No limit" (inherit default), 0 = "Blocked".
    private val presetMinutes = arrayOf<Int?>(null, 30, 60, 90, 120, 150, 180, 240, 0)

    /** Build the schedule rows: one "Every day" row, or seven per-day rows — each a budget dropdown +
     * an hours button (time pickers). No free-text, so the parent can't enter anything invalid. */
    private fun buildScheduleRows() {
        rowSpinners.clear(); rowHours.clear(); rowDay.clear()
        b.scheduleRows.removeAllViews()
        val days = if (b.cbSameEveryDay.isChecked) listOf(-1) else (0..6).toList()
        val dayNames = resources.getStringArray(R.array.weekday_names)
        for (d in days) {
            val row = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(0, 10, 0, 10) }
            row.addView(TextView(this).apply {
                text = if (d < 0) getString(R.string.lbl_all_days) else dayNames[d]
                setTypeface(typeface, android.graphics.Typeface.BOLD)
            })
            val controls = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            val sp = Spinner(this).apply {
                adapter = ArrayAdapter.createFromResource(
                    this@MainActivity, R.array.budget_presets, android.R.layout.simple_spinner_dropdown_item)
                setSelection(4) // default 2 h
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            val hours = Button(this).apply {
                text = getString(R.string.hours_any)
                tag = null
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                setOnClickListener { pickHours(this) }
                setOnLongClickListener { tag = null; text = getString(R.string.hours_any); true }
            }
            controls.addView(sp); controls.addView(hours)
            row.addView(controls)
            b.scheduleRows.addView(row)
            rowSpinners.add(sp); rowHours.add(hours); rowDay.add(d)
        }
    }

    /** Two time pickers (from, then to) — no typing. Long-press the button clears to "Any time". */
    private fun pickHours(btn: Button) {
        val cur = btn.tag as? IntArray
        val s0 = cur?.getOrNull(0) ?: 420     // default 07:00
        TimePickerDialog(this, { _, h1, m1 ->
            val start = h1 * 60 + m1
            val e0 = cur?.getOrNull(1) ?: 1260 // default 21:00
            TimePickerDialog(this, { _, h2, m2 ->
                val end = h2 * 60 + m2
                btn.tag = intArrayOf(start, end)
                btn.text = getString(R.string.hours_fmt, fmtMin(start), fmtMin(end))
            }, e0 / 60, e0 % 60, true).show()
        }, s0 / 60, s0 % 60, true).show()
    }

    private fun fmtMin(min: Int): String = "%02d:%02d".format(min / 60, min % 60)

    /** Build the selected child's schedule from the pickers, sign it (OCPG-SCHED) and POST it. */
    private fun sendSchedule() {
        val respondTo = Prefs.lastRespondTo(this); val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) { toast(getString(R.string.lock_needs_pair)); return }

        fun dayJson(i: Int): JSONObject {
            val o = JSONObject()
            presetMinutes[rowSpinners[i].selectedItemPosition]?.let { o.put("budget_minutes", it) }
            (rowHours[i].tag as? IntArray)?.let { o.put("window_start_min", it[0]); o.put("window_end_min", it[1]) }
            return o
        }

        val sched = JSONObject()
        if (b.cbSameEveryDay.isChecked) {
            val o = dayJson(0)
            if (o.has("budget_minutes")) sched.put("daily_budget_minutes", o.get("budget_minutes"))
            if (o.has("window_start_min")) {
                sched.put("window_start_min", o.get("window_start_min"))
                sched.put("window_end_min", o.get("window_end_min"))
            }
        } else {
            val keys = arrayOf("mon", "tue", "wed", "thu", "fri", "sat", "sun")
            val days = JSONObject()
            for (i in rowDay.indices) days.put(keys[rowDay[i]], dayJson(i))
            sched.put("days", days)
        }

        val child = Prefs.activeChildName(this)
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

    // ---- AI / Ollama config editor (dropdowns/pickers + live /api/tags + editable model tag fallback) ----
    private val aiProviders = arrayOf("ollama", "openai")
    private val aiEndpoints = arrayOf(
        "http://127.0.0.1:11434/api/generate",
        "http://192.168.0.11:11434/api/generate",
        "http://192.168.0.12:11434/api/generate",
        "http://127.0.0.1:11434/v1/chat/completions",
        "http://192.168.0.11:11434/v1/chat/completions"
    )
    private val defaultAiModels = listOf(
        "llama3.2:latest",
        "qooba/bielik-4.5b-v3.0-instruct:Q8_0",
        "llama3.1:8b",
        "qwen3.8:latest",
        "qwen2.5:7b",
        "gemma2:9b",
        "mistral:latest"
    )
    private var currentAiModels: List<String> = defaultAiModels
    private var suppressModelSpinnerSync = false
    private val aiTimeouts = intArrayOf(8, 12, 20, 30, 60)
    private val aiHours = (0..23).toList()
    private val aiLangs = arrayOf("Polish", "English", "Spanish", "German", "French", "Chinese")

    private fun defaultDigestLangForUi(): String = when (Locale.getDefault().language.lowercase()) {
        "pl" -> "Polish"
        "es" -> "Spanish"
        "de" -> "German"
        "fr" -> "French"
        "zh" -> "Chinese"
        else -> "English"
    }

    private fun aiActivityOptions(): Array<String?> {
        val child = Prefs.activeChildName(this).trim().lowercase().ifEmpty { "dziecko" }
        val list = linkedSetOf<String?>(null, "/home/$child/.local/share/guardian/activity.log", "/home/dziecko/.local/share/guardian/activity.log")
        return list.toTypedArray()
    }

    private fun setupAiControls() {
        b.spAiProvider.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, aiProviders)
        b.spAiEndpoint.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, aiEndpoints)
        updateAiModelSpinner(defaultAiModels, null)
        b.spAiModel.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) {
                if (!suppressModelSpinnerSync && pos in currentAiModels.indices) {
                    b.etAiModel.setText(currentAiModels[pos])
                }
            }
            override fun onNothingSelected(p: AdapterView<*>?) {}
        }
        b.spAiEndpoint.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) {
                if (pos in aiEndpoints.indices) {
                    fetchInstalledOllamaModels(aiEndpoints[pos], b.etAiModel.text.toString().trim().ifEmpty { "llama3.2:latest" })
                }
            }
            override fun onNothingSelected(p: AdapterView<*>?) {}
        }
        b.spAiTimeout.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, aiTimeouts.map { "${it} s" })
        b.spAiDigestHour.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, aiHours.map { "%02d:00".format(it) })
        b.spAiDigestLang.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, aiLangs)
        val dnsLabels = listOf(getString(R.string.ai_opt_none), "/var/log/guardian/dns-queries.log")
        b.spAiDnsLog.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, dnsLabels)
        refreshAiActivitySpinner()
    }

    private fun updateAiModelSpinner(models: List<String>, selectedModel: String?) {
        currentAiModels = models
        suppressModelSpinnerSync = true
        b.spAiModel.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, models)
        val idx = if (selectedModel != null) models.indexOf(selectedModel) else 0
        if (idx >= 0) {
            b.spAiModel.setSelection(idx)
            b.etAiModel.setText(models[idx])
        } else if (selectedModel != null && selectedModel.isNotBlank()) {
            b.etAiModel.setText(selectedModel)
        } else if (models.isNotEmpty()) {
            b.spAiModel.setSelection(0)
            b.etAiModel.setText(models[0])
        }
        b.spAiModel.post { suppressModelSpinnerSync = false }
    }

    /**
     * D7 fix: Query `GET {endpoint-base}/api/tags` (and LAN fallback `.12` / `.11` when endpoint is `127.0.0.1`)
     * so the dropdown lists ONLY the actually installed Ollama models with their exact tags (e.g. `:Q8_0`, `:latest`).
     * If unreachable (e.g. phone on mobile data), keeps the editable `etAiModel` field with a tag hint.
     */
    private fun fetchInstalledOllamaModels(endpointUrl: String, preferredModel: String) {
        val base = endpointUrl
            .removeSuffix("/api/generate")
            .removeSuffix("/v1/chat/completions")
            .trimEnd('/')
        val candidateBases = linkedSetOf(base).apply {
            if (base.contains("127.0.0.1")) {
                add("http://192.168.0.12:11434")
                add("http://192.168.0.11:11434")
            }
        }
        lifecycleScope.launch {
            val installed = withContext(Dispatchers.IO) {
                val fastClient = http.newBuilder()
                    .connectTimeout(2, java.util.concurrent.TimeUnit.SECONDS)
                    .readTimeout(3, java.util.concurrent.TimeUnit.SECONDS)
                    .build()
                for (bUrl in candidateBases) {
                    try {
                        val req = Request.Builder().url("$bUrl/api/tags").get().build()
                        fastClient.newCall(req).execute().use { resp ->
                            if (resp.isSuccessful) {
                                val body = resp.body?.string() ?: ""
                                val arr = JSONObject(body).optJSONArray("models")
                                if (arr != null && arr.length() > 0) {
                                    val names = mutableListOf<String>()
                                    for (i in 0 until arr.length()) {
                                        val name = arr.getJSONObject(i).optString("name", "").trim()
                                        if (name.isNotEmpty()) names.add(name)
                                    }
                                    if (names.isNotEmpty()) return@withContext names
                                }
                            }
                        }
                    } catch (_: Exception) { }
                }
                null
            }
            if (installed != null) {
                updateAiModelSpinner(installed, preferredModel)
            } else {
                val fallback = if (preferredModel.isNotBlank() && !defaultAiModels.contains(preferredModel)) {
                    listOf(preferredModel) + defaultAiModels
                } else {
                    defaultAiModels
                }
                updateAiModelSpinner(fallback, preferredModel)
            }
        }
    }

    private fun refreshAiActivitySpinner() {
        val opts = aiActivityOptions()
        val labels = opts.map { it ?: getString(R.string.ai_opt_none) }
        b.spAiActivityLog.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, labels)
    }

    private fun loadAiConfigToForm() {
        refreshAiActivitySpinner()
        val saved = Prefs.aiConfig(this)
        b.cbAiEnabled.isChecked = saved?.optBoolean("enabled", true) ?: true
        val prov = saved?.optString("provider", "ollama") ?: "ollama"
        b.spAiProvider.setSelection(aiProviders.indexOf(prov).coerceAtLeast(0))
        val ep = saved?.optString("endpoint", "http://127.0.0.1:11434/api/generate") ?: "http://127.0.0.1:11434/api/generate"
        b.spAiEndpoint.setSelection(aiEndpoints.indexOf(ep).coerceAtLeast(0))
        val rawModel = saved?.optString("model", "llama3.2:latest") ?: "llama3.2:latest"
        // Upgrade legacy untagged Bielik entry if saved previously
        val model = if (rawModel == "qooba/bielik-4.5b-v3.0-instruct") "qooba/bielik-4.5b-v3.0-instruct:Q8_0" else rawModel
        updateAiModelSpinner(currentAiModels, model)
        fetchInstalledOllamaModels(ep, model)
        val tmo = saved?.optInt("timeout_secs", 12) ?: 12
        b.spAiTimeout.setSelection(aiTimeouts.indexOf(tmo).takeIf { it >= 0 } ?: 1)
        b.cbAiDigestEnabled.isChecked = saved?.optBoolean("digest_enabled", false) ?: false
        val hr = (saved?.optInt("digest_hour", 20) ?: 20).coerceIn(0, 23)
        b.spAiDigestHour.setSelection(hr)
        val defLang = defaultDigestLangForUi()
        val lang = saved?.optString("digest_lang", defLang) ?: defLang
        b.spAiDigestLang.setSelection(aiLangs.indexOf(lang).takeIf { it >= 0 } ?: aiLangs.indexOf(defLang).coerceAtLeast(0))
        val dns = saved?.optString("dns_query_log", "")?.ifBlank { null }
        b.spAiDnsLog.setSelection(if (dns != null) 1 else 0)
        val act = saved?.optString("activity_log", "")?.ifBlank { null }
        val actOpts = aiActivityOptions()
        b.spAiActivityLog.setSelection(actOpts.indexOf(act).coerceAtLeast(0))
    }

    /** Build the selected child's AI/Ollama config JSON (exact config::AiConfig fields), sign (OCPG-AICFG-v1) and POST. */
    private fun sendAiConfig() {
        val respondTo = Prefs.lastRespondTo(this); val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) { toast(getString(R.string.lock_needs_pair)); return }

        val typedModel = b.etAiModel.text.toString().trim()
        val rawModel = if (typedModel.isNotEmpty()) typedModel else currentAiModels.getOrElse(b.spAiModel.selectedItemPosition) { "llama3.2:latest" }
        val provider = aiProviders[b.spAiProvider.selectedItemPosition]
        val finalModel = if (provider == "ollama" && !rawModel.contains(":")) "$rawModel:latest" else rawModel
        b.etAiModel.setText(finalModel)

        val cfg = JSONObject()
            .put("enabled", b.cbAiEnabled.isChecked)
            .put("provider", provider)
            .put("endpoint", aiEndpoints[b.spAiEndpoint.selectedItemPosition])
            .put("model", finalModel)
            .put("timeout_secs", aiTimeouts[b.spAiTimeout.selectedItemPosition])
            .put("digest_enabled", b.cbAiDigestEnabled.isChecked)
            .put("digest_hour", aiHours[b.spAiDigestHour.selectedItemPosition])
            .put("digest_lang", aiLangs[b.spAiDigestLang.selectedItemPosition])
        if (b.spAiDnsLog.selectedItemPosition == 1) {
            cfg.put("dns_query_log", "/var/log/guardian/dns-queries.log")
        }
        val actOpts = aiActivityOptions()
        actOpts.getOrNull(b.spAiActivityLog.selectedItemPosition)?.let {
            cfg.put("activity_log", it)
        }

        val cfgJson = cfg.toString()
        Prefs.setAiConfig(this, cfgJson)
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = randomHex(16)
                    val sig = Crypto.signAiCfg(Prefs.seed(this@MainActivity), dpub, nonce, ts, cfgJson)
                    val payload = JSONObject()
                        .put("kind", "set_ai").put("daemon_pub", dpub).put("nonce", nonce)
                        .put("ts", ts).put("cfg", cfgJson).put("sig", sig)
                    val req = Request.Builder().url(respondTo).post(payload.toString().toRequestBody()).build()
                    http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) { false }
            }
            toast(if (ok) getString(R.string.ai_sent) else getString(R.string.send_failed))
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
