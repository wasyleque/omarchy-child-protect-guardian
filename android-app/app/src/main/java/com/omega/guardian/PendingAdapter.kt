package com.omega.guardian

import android.annotation.SuppressLint
import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.omega.guardian.databinding.ItemPendingBinding

data class PendingItem(
    val id: String,
    val child: String,
    val source: String,
    val pkg: String,
    val nonce: String,
    val respondTo: String,
    val reason: String,
    val ts: Long
)

class PendingAdapter(
    private val onDecision: (decision: String, item: PendingItem) -> Unit
) : ListAdapter<PendingItem, PendingAdapter.VH>(DIFF) {

    class VH(val b: ItemPendingBinding) : RecyclerView.ViewHolder(b.root)

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VH {
        val b = ItemPendingBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return VH(b)
    }

    @SuppressLint("SetTextI18n")
    override fun onBindViewHolder(holder: VH, position: Int) {
        val item = getItem(position)
        val ctx = holder.itemView.context
        val light = NtfyService.riskLightFor(item.reason)
        holder.b.tvPendingSummary.text =
            (if (item.child.isNotBlank()) "${item.child} · " else "") +
            (if (light.isNotEmpty()) "$light " else "") +
            ctx.getString(R.string.req_summary, item.source, item.pkg) +
            (if (item.reason.isNotBlank()) "\n${item.reason}" else "")

        holder.b.btnDeny.setOnClickListener { onDecision("deny", item) }
        holder.b.btnAllow.setOnClickListener { onDecision("allow", item) }
    }

    companion object {
        private val DIFF = object : DiffUtil.ItemCallback<PendingItem>() {
            override fun areItemsTheSame(oldItem: PendingItem, newItem: PendingItem): Boolean =
                oldItem.id == newItem.id

            override fun areContentsTheSame(oldItem: PendingItem, newItem: PendingItem): Boolean =
                oldItem == newItem
        }
    }
}
