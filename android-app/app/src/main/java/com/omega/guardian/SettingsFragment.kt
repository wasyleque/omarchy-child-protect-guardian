package com.omega.guardian

import android.app.TimePickerDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Typeface
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.Spinner
import android.widget.TextView
import androidx.fragment.app.Fragment
import androidx.lifecycle.lifecycleScope
import com.omega.guardian.databinding.FragmentSettingsBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.util.Locale
import java.util.concurrent.TimeUnit

class SettingsFragment : Fragment() {

    private var _b: FragmentSettingsBinding? = null
    private val b get() = _b!!

    // Pre-created schedule rows (7 rows created once; visibility toggled instead of removeAllViews/addView).
    private val rowContainers = mutableListOf<LinearLayout>()
    private val rowLabels = mutableListOf<TextView>()
    private val rowSpinners = mutableListOf<Spinner>()
    private val rowHours = mutableListOf<Button>()

    // Budget dropdown positions → minutes. null = "No limit" (inherit default), 0 = "Blocked".
    private val presetMinutes = arrayOf<Int?>(null, 30, 60, 90, 120, 150, 180, 240, 0)

    // ---- AI / Ollama config editor ----
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
    private var lastLoadedChild: String? = null

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _b = FragmentSettingsBinding.inflate(inflater, container, false)
        return b.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        val ctx = requireContext()

        b.btnScan.setOnClickListener { (activity as? MainActivity)?.launchQrScan() }
        b.btnSendKey.setOnClickListener { (activity as? MainActivity)?.sendKeyToPc() }
        b.btnCopy.setOnClickListener {
            val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            cm.setPrimaryClip(ClipData.newPlainText("guardian pubkey", Prefs.publicKeyB64(ctx)))
            (activity as? MainActivity)?.toast(getString(R.string.pubkey_copied))
        }
        b.btnBattery.setOnClickListener { (activity as? MainActivity)?.requestIgnoreBatteryOptimizations() }
        b.btnSendSchedule.setOnClickListener { sendSchedule() }
        b.btnSendAi.setOnClickListener { sendAiConfig() }

        initScheduleRows()
        b.cbSameEveryDay.setOnCheckedChangeListener { _, _ -> updateScheduleRowsVisibility() }

        setupAiControls()
        renderHeaderOnly()
        loadAiConfigToForm()
    }

    override fun onResume() {
        super.onResume()
        renderHeaderOnly()
        val ctx = context ?: return
        val curChild = Prefs.activeChildName(ctx)
        if (curChild != lastLoadedChild) {
            loadAiConfigToForm()
        }
    }

    fun onActiveChildChanged() {
        if (_b == null || !isAdded) return
        renderHeaderOnly()
        loadAiConfigToForm()
    }

    fun renderHeaderOnly() {
        val binding = _b ?: return
        val ctx = context ?: return
        val n = Prefs.children(ctx).length()
        val activeName = Prefs.activeChildName(ctx)
        binding.lblScheduleFor.text =
            if (n > 0) getString(R.string.sched_for_fmt, activeName) else getString(R.string.hdr_schedule)
        binding.lblAiFor.text =
            if (n > 0) getString(R.string.ai_for_fmt, activeName) else getString(R.string.hdr_ai)
        binding.tvPubkey.text = Prefs.publicKeyB64(ctx)
    }

    /** Create the 7 schedule rows once; checkbox toggles visibility of rows 1..6 and label of row 0. */
    private fun initScheduleRows() {
        val ctx = requireContext()
        rowContainers.clear(); rowLabels.clear(); rowSpinners.clear(); rowHours.clear()
        b.scheduleRows.removeAllViews()
        val dayNames = resources.getStringArray(R.array.weekday_names)
        for (d in 0..6) {
            val row = LinearLayout(ctx).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, 10, 0, 10)
            }
            val lbl = TextView(ctx).apply {
                text = dayNames[d]
                setTypeface(typeface, Typeface.BOLD)
            }
            row.addView(lbl)
            val controls = LinearLayout(ctx).apply { orientation = LinearLayout.HORIZONTAL }
            val sp = Spinner(ctx).apply {
                adapter = ArrayAdapter.createFromResource(
                    ctx, R.array.budget_presets, android.R.layout.simple_spinner_dropdown_item
                )
                setSelection(4) // default 2 h
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            val hours = Button(ctx).apply {
                text = getString(R.string.hours_any)
                tag = null
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                setOnClickListener { pickHours(this) }
                setOnLongClickListener { tag = null; text = getString(R.string.hours_any); true }
            }
            controls.addView(sp); controls.addView(hours)
            row.addView(controls)
            b.scheduleRows.addView(row)
            rowContainers.add(row); rowLabels.add(lbl); rowSpinners.add(sp); rowHours.add(hours)
        }
        updateScheduleRowsVisibility()
    }

    private fun updateScheduleRowsVisibility() {
        val binding = _b ?: return
        if (rowContainers.size < 7) return
        val same = binding.cbSameEveryDay.isChecked
        val dayNames = resources.getStringArray(R.array.weekday_names)
        rowLabels[0].text = if (same) getString(R.string.lbl_all_days) else dayNames[0]
        for (i in 1..6) {
            rowContainers[i].visibility = if (same) View.GONE else View.VISIBLE
        }
    }

    private fun pickHours(btn: Button) {
        val ctx = context ?: return
        val cur = btn.tag as? IntArray
        val s0 = cur?.getOrNull(0) ?: 420     // default 07:00
        TimePickerDialog(ctx, { _, h1, m1 ->
            val start = h1 * 60 + m1
            val e0 = cur?.getOrNull(1) ?: 1260 // default 21:00
            TimePickerDialog(ctx, { _, h2, m2 ->
                val end = h2 * 60 + m2
                btn.tag = intArrayOf(start, end)
                btn.text = getString(R.string.hours_fmt, fmtMin(start), fmtMin(end))
            }, e0 / 60, e0 % 60, true).show()
        }, s0 / 60, s0 % 60, true).show()
    }

    private fun fmtMin(min: Int): String = "%02d:%02d".format(min / 60, min % 60)

    private fun sendSchedule() {
        val act = activity as? MainActivity ?: return
        val ctx = requireContext()
        val respondTo = Prefs.lastRespondTo(ctx); val dpub = Prefs.dpub(ctx)
        if (respondTo == null || dpub == null) { act.toast(getString(R.string.lock_needs_pair)); return }

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
            for (i in 0..6) days.put(keys[i], dayJson(i))
            sched.put("days", days)
        }

        val child = Prefs.activeChildName(ctx)
        val schedJson = sched.toString()
        val appCtx = ctx.applicationContext
        viewLifecycleOwner.lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = act.randomHex(16)
                    val sig = Crypto.signSchedule(Prefs.seed(appCtx), dpub, nonce, child, ts, schedJson)
                    val payload = JSONObject()
                        .put("kind", "set_schedule").put("daemon_pub", dpub).put("nonce", nonce)
                        .put("child", child).put("ts", ts).put("sched", schedJson).put("sig", sig)
                    val req = Request.Builder().url(respondTo).post(payload.toString().toRequestBody()).build()
                    act.http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) { false }
            }
            if (isAdded) {
                act.toast(if (ok) getString(R.string.schedule_sent) else getString(R.string.send_failed))
            }
        }
    }

    private fun defaultDigestLangForUi(): String = when (Locale.getDefault().language.lowercase()) {
        "pl" -> "Polish"
        "es" -> "Spanish"
        "de" -> "German"
        "fr" -> "French"
        "zh" -> "Chinese"
        else -> "English"
    }

    private fun aiActivityOptions(ctx: Context): Array<String?> {
        val child = Prefs.activeChildName(ctx).trim().lowercase().ifEmpty { "dziecko" }
        val list = linkedSetOf<String?>(
            null,
            "/home/$child/.local/share/guardian/activity.log",
            "/home/dziecko/.local/share/guardian/activity.log"
        )
        return list.toTypedArray()
    }

    private fun setupAiControls() {
        val ctx = requireContext()
        b.spAiProvider.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, aiProviders)
        b.spAiEndpoint.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, aiEndpoints)
        updateAiModelSpinner(defaultAiModels, null)
        b.spAiModel.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) {
                val binding = _b ?: return
                if (!suppressModelSpinnerSync && pos in currentAiModels.indices) {
                    binding.etAiModel.setText(currentAiModels[pos])
                }
            }
            override fun onNothingSelected(p: AdapterView<*>?) {}
        }
        b.spAiEndpoint.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) {
                val binding = _b ?: return
                if (pos in aiEndpoints.indices) {
                    fetchInstalledOllamaModels(
                        aiEndpoints[pos],
                        binding.etAiModel.text.toString().trim().ifEmpty { "llama3.2:latest" }
                    )
                }
            }
            override fun onNothingSelected(p: AdapterView<*>?) {}
        }
        b.spAiTimeout.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, aiTimeouts.map { "${it} s" })
        b.spAiDigestHour.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, aiHours.map { "%02d:00".format(it) })
        b.spAiDigestLang.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, aiLangs)
        val dnsLabels = listOf(getString(R.string.ai_opt_none), "/var/log/guardian/dns-queries.log")
        b.spAiDnsLog.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, dnsLabels)
        refreshAiActivitySpinner()
    }

    private fun updateAiModelSpinner(models: List<String>, selectedModel: String?) {
        val binding = _b ?: return
        val ctx = context ?: return
        currentAiModels = models
        suppressModelSpinnerSync = true
        binding.spAiModel.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, models)
        val idx = if (selectedModel != null) models.indexOf(selectedModel) else 0
        if (idx >= 0) {
            binding.spAiModel.setSelection(idx)
            binding.etAiModel.setText(models[idx])
        } else if (selectedModel != null && selectedModel.isNotBlank()) {
            binding.etAiModel.setText(selectedModel)
        } else if (models.isNotEmpty()) {
            binding.spAiModel.setSelection(0)
            binding.etAiModel.setText(models[0])
        }
        binding.spAiModel.post { suppressModelSpinnerSync = false }
    }

    private fun fetchInstalledOllamaModels(endpointUrl: String, preferredModel: String) {
        val act = activity as? MainActivity ?: return
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
        viewLifecycleOwner.lifecycleScope.launch {
            val installed = withContext(Dispatchers.IO) {
                val fastClient = act.http.newBuilder()
                    .connectTimeout(2, TimeUnit.SECONDS)
                    .readTimeout(3, TimeUnit.SECONDS)
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
            if (_b == null || !isAdded) return@launch
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
        val binding = _b ?: return
        val ctx = context ?: return
        val opts = aiActivityOptions(ctx)
        val labels = opts.map { it ?: getString(R.string.ai_opt_none) }
        binding.spAiActivityLog.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, labels)
    }

    fun loadAiConfigToForm() {
        val binding = _b ?: return
        val ctx = context ?: return
        lastLoadedChild = Prefs.activeChildName(ctx)
        refreshAiActivitySpinner()
        val saved = Prefs.aiConfig(ctx)
        binding.cbAiEnabled.isChecked = saved?.optBoolean("enabled", true) ?: true
        val prov = saved?.optString("provider", "ollama") ?: "ollama"
        binding.spAiProvider.setSelection(aiProviders.indexOf(prov).coerceAtLeast(0))
        val ep = saved?.optString("endpoint", "http://127.0.0.1:11434/api/generate") ?: "http://127.0.0.1:11434/api/generate"
        binding.spAiEndpoint.setSelection(aiEndpoints.indexOf(ep).coerceAtLeast(0))
        val rawModel = saved?.optString("model", "llama3.2:latest") ?: "llama3.2:latest"
        val model = if (rawModel == "qooba/bielik-4.5b-v3.0-instruct") "qooba/bielik-4.5b-v3.0-instruct:Q8_0" else rawModel
        updateAiModelSpinner(currentAiModels, model)
        fetchInstalledOllamaModels(ep, model)
        val tmo = saved?.optInt("timeout_secs", 12) ?: 12
        binding.spAiTimeout.setSelection(aiTimeouts.indexOf(tmo).takeIf { it >= 0 } ?: 1)
        binding.cbAiDigestEnabled.isChecked = saved?.optBoolean("digest_enabled", false) ?: false
        val hr = (saved?.optInt("digest_hour", 20) ?: 20).coerceIn(0, 23)
        binding.spAiDigestHour.setSelection(hr)
        val defLang = defaultDigestLangForUi()
        val lang = saved?.optString("digest_lang", defLang) ?: defLang
        binding.spAiDigestLang.setSelection(aiLangs.indexOf(lang).takeIf { it >= 0 } ?: aiLangs.indexOf(defLang).coerceAtLeast(0))
        val dns = saved?.optString("dns_query_log", "")?.ifBlank { null }
        binding.spAiDnsLog.setSelection(if (dns != null) 1 else 0)
        val act = saved?.optString("activity_log", "")?.ifBlank { null }
        val actOpts = aiActivityOptions(ctx)
        binding.spAiActivityLog.setSelection(actOpts.indexOf(act).coerceAtLeast(0))
    }

    private fun sendAiConfig() {
        val act = activity as? MainActivity ?: return
        val ctx = requireContext()
        val respondTo = Prefs.lastRespondTo(ctx); val dpub = Prefs.dpub(ctx)
        if (respondTo == null || dpub == null) { act.toast(getString(R.string.lock_needs_pair)); return }

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
        val actOpts = aiActivityOptions(ctx)
        actOpts.getOrNull(b.spAiActivityLog.selectedItemPosition)?.let {
            cfg.put("activity_log", it)
        }

        val cfgJson = cfg.toString()
        Prefs.setAiConfig(ctx, cfgJson)
        val appCtx = ctx.applicationContext
        viewLifecycleOwner.lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                try {
                    val ts = System.currentTimeMillis() / 1000
                    val nonce = act.randomHex(16)
                    val sig = Crypto.signAiCfg(Prefs.seed(appCtx), dpub, nonce, ts, cfgJson)
                    val payload = JSONObject()
                        .put("kind", "set_ai").put("daemon_pub", dpub).put("nonce", nonce)
                        .put("ts", ts).put("cfg", cfgJson).put("sig", sig)
                    val req = Request.Builder().url(respondTo).post(payload.toString().toRequestBody()).build()
                    act.http.newCall(req).execute().use { it.isSuccessful }
                } catch (e: Exception) { false }
            }
            if (isAdded) {
                act.toast(if (ok) getString(R.string.ai_sent) else getString(R.string.send_failed))
            }
        }
    }

    override fun onDestroyView() {
        rowContainers.clear(); rowLabels.clear(); rowSpinners.clear(); rowHours.clear()
        _b = null
        super.onDestroyView()
    }
}
