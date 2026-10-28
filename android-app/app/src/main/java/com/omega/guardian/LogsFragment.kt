package com.omega.guardian

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.text.method.ScrollingMovementMethod
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.Fragment
import androidx.recyclerview.widget.LinearLayoutManager
import com.omega.guardian.databinding.FragmentLogsBinding
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class LogsFragment : Fragment() {

    private var _b: FragmentLogsBinding? = null
    private val b get() = _b!!

    private lateinit var alertsAdapter: AlertsAdapter

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _b = FragmentLogsBinding.inflate(inflater, container, false)
        return b.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        val ctx = requireContext()

        alertsAdapter = AlertsAdapter()
        b.rvAlerts.layoutManager = LinearLayoutManager(ctx)
        b.rvAlerts.adapter = alertsAdapter
        b.rvAlerts.itemAnimator = null

        b.tvHistory.movementMethod = ScrollingMovementMethod()

        b.btnCopyDebug.setOnClickListener {
            val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val lines = Prefs.debugLogs(ctx).take(MAX_LOG_LINES).joinToString("\n")
            cm.setPrimaryClip(ClipData.newPlainText("guardian debug log", lines))
            (activity as? MainActivity)?.toast(getString(R.string.debug_copied))
        }
        b.btnClearDebug.setOnClickListener {
            Prefs.clearDebugLogs(ctx)
            renderLogs()
        }
    }

    override fun onResume() {
        super.onResume()
        renderLogs()
    }

    fun renderLogs() {
        val binding = _b ?: return
        val ctx = context ?: return
        if (!isAdded) return

        val al = Prefs.alerts(ctx)
        val alertItems = ArrayList<AlertItem>(al.length())
        for (i in 0 until al.length()) {
            val o = al.getJSONObject(i)
            alertItems.add(
                AlertItem(
                    child = o.optString("child"),
                    title = o.optString("title"),
                    text = o.optString("text"),
                    ts = o.optLong("ts")
                )
            )
        }
        binding.tvAlertsEmpty.visibility = if (alertItems.isEmpty()) View.VISIBLE else View.GONE
        alertsAdapter.submitList(alertItems)

        val h = Prefs.history(ctx)
        val fmt = SimpleDateFormat("MMM d HH:mm", Locale.getDefault())
        binding.tvHistory.text = if (h.length() == 0) getString(R.string.history_empty) else buildString {
            for (i in 0 until h.length()) {
                val o = h.getJSONObject(i)
                append(fmt.format(Date(o.optLong("ts") * 1000)))
                append("  ").append(o.optString("decision").uppercase())
                append("  ").append(o.optString("package").ifBlank { o.optString("pkg") })
                append("  (").append(o.optString("source")).append(")\n")
            }
        }.trimEnd()

        val dbg = Prefs.debugLogs(ctx).take(MAX_LOG_LINES)
        binding.tvDebugLog.text = if (dbg.isEmpty()) getString(R.string.debug_empty) else dbg.joinToString("\n")
    }

    override fun onDestroyView() {
        _b = null
        super.onDestroyView()
    }

    companion object {
        const val MAX_LOG_LINES = 200
    }
}
