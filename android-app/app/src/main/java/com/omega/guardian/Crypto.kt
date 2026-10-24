package com.omega.guardian

import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer
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
}
