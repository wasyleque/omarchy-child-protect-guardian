package com.omega.guardian

import android.annotation.SuppressLint
import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.omega.guardian.databinding.ItemAlertBinding
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

data class AlertItem(
    val child: String,
    val title: String,
    val text: String,
    val ts: Long
)

class AlertsAdapter : ListAdapter<AlertItem, AlertsAdapter.VH>(DIFF) {

    class VH(val b: ItemAlertBinding) : RecyclerView.ViewHolder(b.root)

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VH {
        val b = ItemAlertBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return VH(b)
    }

    @SuppressLint("SetTextI18n")
    override fun onBindViewHolder(holder: VH, position: Int) {
        val item = getItem(position)
        val fmt = SimpleDateFormat("MMM d HH:mm", Locale.getDefault())
        val dateStr = fmt.format(Date(item.ts * 1000))
        val who = if (item.child.isNotBlank()) " [${item.child}]" else ""
        holder.b.tvAlertHeader.text = "$dateStr$who ${item.title}:"
        holder.b.tvAlertBody.text = item.text
    }

    companion object {
        private val DIFF = object : DiffUtil.ItemCallback<AlertItem>() {
            override fun areItemsTheSame(oldItem: AlertItem, newItem: AlertItem): Boolean =
                oldItem.ts == newItem.ts && oldItem.title == newItem.title && oldItem.child == newItem.child

            override fun areContentsTheSame(oldItem: AlertItem, newItem: AlertItem): Boolean =
                oldItem == newItem
        }
    }
}
