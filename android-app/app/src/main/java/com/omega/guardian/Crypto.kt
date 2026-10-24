package com.omega.guardian

import com.goterl.lazysodium.LazySodiumAndroid
import com.goterl.lazysodium.SodiumAndroid
import com.goterl.lazysodium.interfaces.Box
import org.bouncycastle.crypto.digests.Blake2bDigest
import org.bouncycastle.crypto.engines.XSalsa20Engine
import org.bouncycastle.crypto.macs.Poly1305
import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters
import org.bouncycastle.crypto.params.KeyParameter
import org.bouncycastle.crypto.params.ParametersWithIV
import org.bouncycastle.crypto.params.X25519PrivateKeyParameters
import org.bouncycastle.crypto.params.X25519PublicKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64

/**
 * Raw Ed25519 (RFC 8032) via BouncyCastle — byte-for-byte compatible with the Rust daemon
 * (ed25519-dalek `verify_strict`) and the PWA (@noble/ed25519). The canonical messages MUST match
 * [daemon/src/crypto.rs] exactly; CryptoTest pins them against a shared cross-impl vector.
 *
 * We deliberately use the low-level BC classes (not AndroidKeyStore) because they operate on raw
 * 32-byte seeds / 32-byte public keys / 64-byte signatures with no DER/PKCS#8 wrapping. The seed is
 * stored encrypted at rest by [Prefs] (EncryptedSharedPreferences), never leaving the device.
 */
object Crypto {
    private const val DECISION = "OCPG-v2"
    private const val CHALLENGE = "OCPG-CH-v2"
    private const val GRANT = "OCPG-GRANT-v3"
    private const val LOCK = "OCPG-LOCK-v1"
    private const val SCHED = "OCPG-SCHED-v1"
    private const val AICFG = "OCPG-AICFG-v1"

    private fun b64e(b: ByteArray): String = Base64.getEncoder().encodeToString(b)
    private fun b64d(s: String): ByteArray = Base64.getDecoder().decode(s)

    /** A fresh 32-byte private seed. */
    fun newSeed(): ByteArray = ByteArray(32).also { SecureRandom().nextBytes(it) }

    /** 32-byte public key for a seed. */
    fun publicKey(seed: ByteArray): ByteArray =
        Ed25519PrivateKeyParameters(seed, 0).generatePublicKey().encoded

    fun publicKeyB64(seed: ByteArray): String = b64e(publicKey(seed))

    private fun signRaw(seed: ByteArray, msg: String): ByteArray {
        val s = Ed25519Signer()
        s.init(true, Ed25519PrivateKeyParameters(seed, 0))
        val bytes = msg.toByteArray(Charsets.UTF_8)
        s.update(bytes, 0, bytes.size)
        return s.generateSignature()
    }

    // ---- canonical messages (keep identical to the daemon) ----

    fun decisionCanonical(id: String, decision: String, nonce: String, ts: Long, source: String, pkg: String) =
        "$DECISION|$id|$decision|$nonce|$ts|$source|$pkg"

    fun grantCanonical(daemonPubB64: String, nonce: String, child: String, minutes: Int, ts: Long) =
        "$GRANT|$daemonPubB64|$nonce|$child|$minutes|$ts"

    fun challengeCanonical(id: String, source: String, pkg: String, nonce: String, reason: String, respondTo: String) =
        "$CHALLENGE|$id|$source|$pkg|$nonce|$reason|$respondTo"

    fun activityCanonical(child: String, app: String, ts: Long) = "OCPG-ACT-v1|$child|$app|$ts"

    /** Sign a decision; returns base64 of the 64-byte signature. */
    fun signDecision(seed: ByteArray, id: String, decision: String, nonce: String, ts: Long, source: String, pkg: String): String =
        b64e(signRaw(seed, decisionCanonical(id, decision, nonce, ts, source, pkg)))

    /** Sign a screen-time grant for `child` ("" = all/single). */
    fun signGrant(seed: ByteArray, daemonPubB64: String, nonce: String, child: String, minutes: Int, ts: Long): String =
        b64e(signRaw(seed, grantCanonical(daemonPubB64, nonce, child, minutes, ts)))

    fun lockCanonical(daemonPubB64: String, nonce: String, action: String, ts: Long) =
        "$LOCK|$daemonPubB64|$nonce|$action|$ts"

    /** Sign an instant lock/unlock command (action = "lock" | "unlock"). */
    fun signLock(seed: ByteArray, daemonPubB64: String, nonce: String, action: String, ts: Long): String =
        b64e(signRaw(seed, lockCanonical(daemonPubB64, nonce, action, ts)))

    fun scheduleCanonical(daemonPubB64: String, nonce: String, child: String, ts: Long, schedJson: String) =
        "$SCHED|$daemonPubB64|$nonce|$child|$ts|$schedJson"

    /** Sign a pushed schedule. `schedJson` must be sent to the daemon verbatim (it verifies over it). */
    fun signSchedule(seed: ByteArray, daemonPubB64: String, nonce: String, child: String, ts: Long, schedJson: String): String =
        b64e(signRaw(seed, scheduleCanonical(daemonPubB64, nonce, child, ts, schedJson)))

    fun aiCfgCanonical(daemonPubB64: String, nonce: String, ts: Long, cfgJson: String) =
        "$AICFG|$daemonPubB64|$nonce|$ts|$cfgJson"

    /** Sign a pushed AI/Ollama config. `cfgJson` must be sent to the daemon verbatim as the `cfg` field. */
    fun signAiCfg(seed: ByteArray, daemonPubB64: String, nonce: String, ts: Long, cfgJson: String): String =
        b64e(signRaw(seed, aiCfgCanonical(daemonPubB64, nonce, ts, cfgJson)))

    /** Verify a daemon-signed challenge/alert message with the paired daemon public key (base64). */
    fun verify(daemonPubB64: String, msg: String, sigB64: String): Boolean = try {
        val v = Ed25519Signer()
        v.init(false, Ed25519PublicKeyParameters(b64d(daemonPubB64), 0))
        val bytes = msg.toByteArray(Charsets.UTF_8)
        v.update(bytes, 0, bytes.size)
        v.verifySignature(b64d(sigB64))
    } catch (e: Exception) {
        false
    }

    fun alertCanonical(text: String, ts: Long) = "OCPG-ALERT-v1|$text|$ts"

    // ---- M4 payload encryption (libsodium crypto_box_seal to parent X25519 key) ----

    private val lazySodium: LazySodiumAndroid? by lazy {
        try { LazySodiumAndroid(SodiumAndroid()) } catch (t: Throwable) { null }
    }

    /**
     * Derive the 32-byte X25519 secret scalar from a 32-byte Ed25519 seed:
     * `clamp(SHA-512(seed)[0..32])` (`crypto_sign_ed25519_sk_to_curve25519`).
     */
    fun ed25519SeedToX25519Secret(seed: ByteArray): ByteArray {
        val h = MessageDigest.getInstance("SHA-512").digest(seed)
        val s = h.copyOfRange(0, 32)
        s[0] = (s[0].toInt() and 248).toByte()
        s[31] = (s[31].toInt() and 127).toByte()
        s[31] = (s[31].toInt() or 64).toByte()
        return s
    }

    /** Derive the 32-byte X25519 public key corresponding to an Ed25519 seed. */
    fun ed25519SeedToX25519Public(seed: ByteArray): ByteArray =
        X25519PrivateKeyParameters(ed25519SeedToX25519Secret(seed), 0).generatePublicKey().encoded

    /**
     * Open a base64 `crypto_box_seal` ciphertext (`ephemeral_pk(32) || tag(16) || ct`) encrypted to
     * the parent's X25519 public key (derived from `seed`). Uses `lazysodium-android` (`cryptoBoxSealOpen`)
     * on Android and falls back to an exact BouncyCastle implementation of `crypto_box_seal_open`
     * (X25519 + HSalsa20 + XSalsa20-Poly1305 + Blake2b-192 nonce) on host JVM unit tests where
     * Android native `.so` libraries aren't loaded.
     */
    fun sealOpen(encB64: String, seed: ByteArray): String? {
        return try {
            val ct = b64d(encB64.trim())
            if (ct.size < Box.SEALBYTES) return null
            val xSec = ed25519SeedToX25519Secret(seed)
            val xPub = X25519PrivateKeyParameters(xSec, 0).generatePublicKey().encoded
            val ptLen = ct.size - Box.SEALBYTES
            val ls = lazySodium
            if (ls != null) {
                val out = ByteArray(ptLen)
                if (ls.cryptoBoxSealOpen(out, ct, ct.size.toLong(), xPub, xSec)) {
                    return String(out, Charsets.UTF_8)
                }
            }
            sealOpenJvm(ct, xPub, xSec)?.let { String(it, Charsets.UTF_8) }
        } catch (e: Exception) {
            null
        }
    }

    /** Exact JVM implementation of libsodium `crypto_box_seal_open` (for host unit tests + fallback). */
    private fun sealOpenJvm(ct: ByteArray, xPub: ByteArray, xSec: ByteArray): ByteArray? {
        if (ct.size < 48) return null
        val ephPub = ct.copyOfRange(0, 32)
        val boxed = ct.copyOfRange(32, ct.size) // 16-byte Poly1305 tag || ciphertext

        // 1) nonce = Blake2b-192(ephemeral_pk || recipient_pk)
        val blake = Blake2bDigest(null, 24, null, null)
        blake.update(ephPub, 0, 32)
        blake.update(xPub, 0, 32)
        val nonce = ByteArray(24)
        blake.doFinal(nonce, 0)

        // 2) shared secret = X25519(xSec, ephPub), rejected if all-zero
        val shared = ByteArray(32)
        X25519PrivateKeyParameters(xSec, 0).generateSecret(X25519PublicKeyParameters(ephPub, 0), shared, 0)
        var nonZero = 0
        for (b in shared) nonZero = nonZero or b.toInt()
        if (nonZero == 0) return null

        // 3) k = HSalsa20(shared, 0^16) (crypto_box_beforenm)
        val subkey = hsalsa20Zero(shared)

        // 4) XSalsa20 stream: first 32 bytes = Poly1305 one-time key, rest = keystream for plaintext
        val xsalsa = XSalsa20Engine()
        xsalsa.init(false, ParametersWithIV(KeyParameter(subkey), nonce))
        val polyKey = ByteArray(32)
        xsalsa.processBytes(polyKey, 0, 32, polyKey, 0)

        val tag = boxed.copyOfRange(0, 16)
        val cBody = boxed.copyOfRange(16, boxed.size)

        val poly = Poly1305()
        poly.init(KeyParameter(polyKey))
        poly.update(cBody, 0, cBody.size)
        val calcTag = ByteArray(16)
        poly.doFinal(calcTag, 0)
        if (!MessageDigest.isEqual(tag, calcTag)) return null

        val pt = ByteArray(cBody.size)
        xsalsa.processBytes(cBody, 0, cBody.size, pt, 0)
        return pt
    }

    /** HSalsa20 with an all-zero 16-byte input and standard Salsa20 sigma ("expand 32-byte k"). */
    private fun hsalsa20Zero(key: ByteArray): ByteArray {
        fun le32(b: ByteArray, off: Int): Int =
            (b[off].toInt() and 0xff) or
            ((b[off + 1].toInt() and 0xff) shl 8) or
            ((b[off + 2].toInt() and 0xff) shl 16) or
            ((b[off + 3].toInt() and 0xff) shl 24)
        fun putLe32(out: ByteArray, off: Int, v: Int) {
            out[off] = v.toByte()
            out[off + 1] = (v ushr 8).toByte()
            out[off + 2] = (v ushr 16).toByte()
            out[off + 3] = (v ushr 24).toByte()
        }
        fun rotl(v: Int, c: Int): Int = (v shl c) or (v ushr (32 - c))

        var x0 = 0x61707865
        var x1 = le32(key, 0); var x2 = le32(key, 4); var x3 = le32(key, 8); var x4 = le32(key, 12)
        var x5 = 0x3320646e
        var x6 = 0; var x7 = 0; var x8 = 0; var x9 = 0
        var x10 = 0x79622d32
        var x11 = le32(key, 16); var x12 = le32(key, 20); var x13 = le32(key, 24); var x14 = le32(key, 28)
        var x15 = 0x6b206574

        repeat(10) {
            x4 = x4 xor rotl(x0 + x12, 7);  x8 = x8 xor rotl(x4 + x0, 9)
            x12 = x12 xor rotl(x8 + x4, 13); x0 = x0 xor rotl(x12 + x8, 18)
            x9 = x9 xor rotl(x5 + x1, 7);   x13 = x13 xor rotl(x9 + x5, 9)
            x1 = x1 xor rotl(x13 + x9, 13);  x5 = x5 xor rotl(x1 + x13, 18)
            x14 = x14 xor rotl(x10 + x6, 7); x2 = x2 xor rotl(x14 + x10, 9)
            x6 = x6 xor rotl(x2 + x14, 13);  x10 = x10 xor rotl(x6 + x2, 18)
            x3 = x3 xor rotl(x15 + x11, 7); x7 = x7 xor rotl(x3 + x15, 9)
            x11 = x11 xor rotl(x7 + x3, 13); x15 = x15 xor rotl(x11 + x7, 18)

            x1 = x1 xor rotl(x0 + x3, 7);   x2 = x2 xor rotl(x1 + x0, 9)
            x3 = x3 xor rotl(x2 + x1, 13);  x0 = x0 xor rotl(x3 + x2, 18)
            x6 = x6 xor rotl(x5 + x4, 7);   x7 = x7 xor rotl(x6 + x5, 9)
            x4 = x4 xor rotl(x7 + x6, 13);  x5 = x5 xor rotl(x4 + x7, 18)
            x11 = x11 xor rotl(x10 + x9, 7); x8 = x8 xor rotl(x11 + x10, 9)
            x9 = x9 xor rotl(x8 + x11, 13);  x10 = x10 xor rotl(x9 + x8, 18)
            x12 = x12 xor rotl(x15 + x14, 7); x13 = x13 xor rotl(x12 + x15, 9)
            x14 = x14 xor rotl(x13 + x12, 13); x15 = x15 xor rotl(x14 + x13, 18)
        }

        val out = ByteArray(32)
        putLe32(out, 0, x0);  putLe32(out, 4, x5);  putLe32(out, 8, x10); putLe32(out, 12, x15)
        putLe32(out, 16, x6); putLe32(out, 20, x7); putLe32(out, 24, x8); putLe32(out, 28, x9)
        return out
    }
}
