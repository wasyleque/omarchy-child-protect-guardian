package com.omega.guardian

import android.annotation.SuppressLint
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.AdapterView
import android.widget.ArrayAdapter
import androidx.fragment.app.Fragment
import androidx.recyclerview.widget.LinearLayoutManager
import com.omega.guardian.databinding.FragmentHomeBinding

class HomeFragment : Fragment() {

    private var _b: FragmentHomeBinding? = null
    private val b get() = _b!!

    private lateinit var pendingAdapter: PendingAdapter
    private var lastChildNames: List<String> = emptyList()

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _b = FragmentHomeBinding.inflate(inflater, container, false)
        return b.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)

        pendingAdapter = PendingAdapter { decision, item ->
            (activity as? MainActivity)?.decideInApp(
                decision, item.id, item.nonce, item.source, item.pkg, item.respondTo
            )
        }
        b.rvPending.layoutManager = LinearLayoutManager(requireContext())
        b.rvPending.adapter = pendingAdapter
        b.rvPending.itemAnimator = null

        b.btnGrant15.setOnClickListener { (activity as? MainActivity)?.sendGrant(15) }
        b.btnGrant30.setOnClickListener { (activity as? MainActivity)?.sendGrant(30) }
        b.btnLockNow.setOnClickListener { (activity as? MainActivity)?.toggleLock() }
        b.btnRefresh.setOnClickListener { (activity as? MainActivity)?.manualRefresh() }

        b.spChild.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) {
                val ctx = context ?: return
                if (pos != Prefs.activeIndex(ctx)) {
                    Prefs.setActiveIndex(ctx, pos)
                    renderHome()
                    (activity as? MainActivity)?.onActiveChildChanged()
                }
            }
            override fun onNothingSelected(p: AdapterView<*>?) {}
        }

        renderHome()
    }

    override fun onResume() {
        super.onResume()
        renderHome()
    }

    @SuppressLint("SetTextI18n")
    fun renderHome() {
        val binding = _b ?: return
        val ctx = context ?: return

        val n = Prefs.children(ctx).length()
        val connLost = n > 0 && !Prefs.isConnected(ctx)
        val base = when {
            n == 0 -> getString(R.string.status_not_paired)
            connLost -> getString(R.string.status_conn_lost_fmt, n)
            else -> getString(R.string.status_children_fmt, n)
        }
        val locked = n > 0 && Prefs.lockRequested(ctx)
        val now = if (n > 0) Prefs.currentApp(ctx, Prefs.activeChildName(ctx)) else null
        val statusText = buildString {
            append(base)
            if (locked) append("\n").append(getString(R.string.status_locked))
            if (now != null) append("\n").append(getString(R.string.now_using_fmt, now))
        }
        if (binding.tvStatus.text.toString() != statusText) {
            binding.tvStatus.text = statusText
        }

        val names = Prefs.childNames(ctx)
        if (names != lastChildNames) {
            lastChildNames = names
            binding.spChild.adapter = ArrayAdapter(ctx, android.R.layout.simple_spinner_dropdown_item, names)
        }
        val idx = Prefs.activeIndex(ctx)
        if (idx in names.indices && binding.spChild.selectedItemPosition != idx) {
            binding.spChild.setSelection(idx)
        }

        binding.btnLockNow.setText(if (locked) R.string.btn_unlock_now else R.string.btn_lock_now)

        val pendArr = Prefs.pending(ctx)
        val items = ArrayList<PendingItem>(pendArr.length())
        for (i in 0 until pendArr.length()) {
            val o = pendArr.getJSONObject(i)
            items.add(
                PendingItem(
                    id = o.optString("id"),
                    child = o.optString("child"),
                    source = o.optString("source"),
                    pkg = o.optString("package"),
                    nonce = o.optString("nonce"),
                    respondTo = o.optString("respond_to"),
                    reason = o.optString("reason"),
                    ts = o.optLong("ts")
                )
            )
        }
        binding.tvPendingEmpty.visibility = if (items.isEmpty()) View.VISIBLE else View.GONE
        pendingAdapter.submitList(items)
    }

    override fun onDestroyView() {
        lastChildNames = emptyList()
        _b = null
        super.onDestroyView()
    }
}
