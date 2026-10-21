package com.omega.guardian

import android.Manifest
import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
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

    private val scan = registerForActivityResult(ScanContract()) { result ->
        val contents = result.contents
        if (contents == null) {
            toast("Scan cancelled")
        } else if (Prefs.applyPairing(this, contents)) {
            toast("Paired ✓")
            NtfyService.start(this)
            sendKeyToPc()   // auto-return our public key so the PC finishes pairing without copy/paste
            render()
        } else {
            toast("That QR is not a Guardian pairing code")
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
            scan.launch(ScanOptions().setPrompt("Scan the Guardian pairing QR").setBeepEnabled(false))
        }
        b.btnCopy.setOnClickListener {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            cm.setPrimaryClip(ClipData.newPlainText("guardian pubkey", Prefs.publicKeyB64(this)))
            toast("Public key copied")
        }
        b.btnSendKey.setOnClickListener { sendKeyToPc() }
        b.btnBattery.setOnClickListener { requestIgnoreBatteryOptimizations() }
        b.btnGrant15.setOnClickListener { sendGrant(15) }
        b.btnGrant30.setOnClickListener { sendGrant(30) }

        if (Prefs.isPaired(this)) NtfyService.start(this)
        render()
    }

    override fun onResume() {
        super.onResume()
        render()
    }

    @SuppressLint("SetTextI18n")
    private fun render() {
        b.tvStatus.text = if (Prefs.isPaired(this)) "Paired · listening" else "Not paired — scan the QR on the PC"
        b.tvPubkey.text = Prefs.publicKeyB64(this)
        val h = Prefs.history(this)
        val fmt = SimpleDateFormat("MMM d HH:mm", Locale.getDefault())
        b.tvHistory.text = if (h.length() == 0) "No requests yet." else buildString {
            for (i in 0 until h.length()) {
                val o = h.getJSONObject(i)
                append(fmt.format(Date(o.optLong("ts") * 1000)))
                append("  ").append(o.optString("decision").uppercase())
                append("  ").append(o.optString("package"))
                append("  (").append(o.optString("source")).append(")\n")
            }
        }
    }

    private fun sendGrant(minutes: Int) {
        val respondTo = Prefs.lastRespondTo(this)
        val dpub = Prefs.dpub(this)
        if (respondTo == null || dpub == null) {
            toast("Grants work after the first install request (so the app learns where to send)")
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
            toast(if (ok) "Granted +$minutes min" else "Grant failed — try again")
        }
    }

    /** POST our public key to the pairing-return topic so the PC captures it (packaging/pair.sh). */
    private fun sendKeyToPc() {
        val pairTo = Prefs.pairTo(this)
        if (pairTo == null) {
            toast("Re-scan the QR (this pairing has no return channel)")
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
            toast(if (ok) "Sent my key to the PC — confirm it there" else "Couldn't reach the PC; use Copy public key")
        }
    }

    private fun requestIgnoreBatteryOptimizations() {
        try {
            startActivity(
                Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName"))
            )
        } catch (e: Exception) {
            toast("Open Settings → Battery and allow unrestricted background use")
        }
    }

    private fun randomHex(nBytes: Int): String {
        val b = ByteArray(nBytes); SecureRandom().nextBytes(b)
        return b.joinToString("") { "%02x".format(it) }
    }

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_SHORT).show()
}
