package com.omega.guardian

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Base64

/**
 * Cross-implementation vectors generated with @noble/ed25519 (which is proven byte-identical to the
 * Rust daemon's ed25519-dalek over the exact canonical messages). If this test passes, the Android
 * app's signatures are accepted by the daemon and its challenge verification matches the daemon's.
 * Seed is a FIXED non-secret test value (bytes 1..32).
 */
class CryptoTest {
    private val seed = Base64.getDecoder().decode("AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=")
    private val pub = "ebVWLo/mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ="

    @Test
    fun publicKeyMatchesVector() {
        assertEquals(pub, Crypto.publicKeyB64(seed))
    }

    @Test
    fun decisionSignatureMatchesVector() {
        val sig = Crypto.signDecision(
            seed, "11111111-1111-1111-1111-111111111111", "allow", "deadbeef", 1700000000L, "pacman", "firefox"
        )
        assertEquals("bZBULOA5RNnl1cthOwmL5WWBUVD4xSRjgoLzv/rdX+XhinPRcSX4KADR4+2NWQE1Mmto33uqgtctxK0RcjQxBg==", sig)
    }

    @Test
    fun grantSignatureMatchesVector() {
        // OCPG-GRANT-v3 binds the target child; signature computed by ed25519-dalek over the same msg.
        val sig = Crypto.signGrant(seed, pub, "abc123", "zosia", 30, 1700000001L)
        assertEquals("uQBGEZQyB0kaooLmVnnSU9e59bArZV/3IsaJg8A7Kgw/hs898k5pa6rGgLCitA20ZMHtxsJymrnb5ahukQgpDA==", sig)
    }

    @Test
    fun verifiesGenuineChallenge() {
        val msg = Crypto.challengeCanonical(
            "22222222-2222-2222-2222-222222222222", "pacman", "steam", "cafe01",
            "wants to install steam", "https://ntfy.sh/resp-topic"
        )
        val sig = "ofl7dDkyOeTx/9G+MdmoLzUEsRGZvDjedE2kl6+uJHdhMSKv2Y4c3LSe5qK7489cyrDWM9QNRZxwmycsiVUvCA=="
        assertTrue(Crypto.verify(pub, msg, sig))
    }

    @Test
    fun rejectsTamperedChallenge() {
        // Same signature, but the package was swapped steam -> firefox: must NOT verify.
        val tampered = Crypto.challengeCanonical(
            "22222222-2222-2222-2222-222222222222", "pacman", "firefox", "cafe01",
            "wants to install steam", "https://ntfy.sh/resp-topic"
        )
        val sig = "ofl7dDkyOeTx/9G+MdmoLzUEsRGZvDjedE2kl6+uJHdhMSKv2Y4c3LSe5qK7489cyrDWM9QNRZxwmycsiVUvCA=="
        assertFalse(Crypto.verify(pub, tampered, sig))
    }

    @Test
    fun aiCfgSignatureVerifiesAndTamperedFails() {
        val cfg = """{"enabled":true,"provider":"ollama","endpoint":"http://127.0.0.1:11434/api/generate","model":"llama3.2:latest","timeout_secs":12,"digest_enabled":true,"digest_hour":20,"digest_lang":"Polish"}"""
        val sig = Crypto.signAiCfg(seed, pub, "nonce99", 1700000002L, cfg)
        val msg = Crypto.aiCfgCanonical(pub, "nonce99", 1700000002L, cfg)
        assertEquals("OCPG-AICFG-v1|$pub|nonce99|1700000002|$cfg", msg)
        assertTrue(Crypto.verify(pub, msg, sig))
        val tampered = Crypto.aiCfgCanonical(pub, "nonce99", 1700000002L, cfg.replace("true", "false"))
        assertFalse(Crypto.verify(pub, tampered, sig))
    }

    @Test
    fun sealOpenMatchesDaemonVector() {
        val m4Seed = ByteArray(32) { 0x11.toByte() }
        assertEquals("0EqyMnQrtKs6E2i9RhXk5tAiSrcaAWuvhSCjMsl3hzc=", Crypto.publicKeyB64(m4Seed))
        val encB64 = "Jq9NajZHK9tUpmV++uwrq9oTbQbpNHt2DyyasJ33Mga+BCC+Hwdq+3s5ZQnMh4T5mTmTz53pKykbnTVXw9O660adP1dWzUPA2BfNxQCD2vCR7viROGajqeXmJOnKr/kbWoZ5gntXypwPWPqJVTOOE+NlQ9hB"
        val expected = """{"source":"pacman","package":"steam","reason":"sudo pacman -S steam"}"""
        val pt = Crypto.sealOpen(encB64, m4Seed)
        assertEquals(expected, pt)
        // Tampered ciphertext must return null (authentication failure)
        val tamperedBytes = Base64.getDecoder().decode(encB64).apply { this[size - 1] = (this[size - 1].toInt() xor 1).toByte() }
        assertEquals(null, Crypto.sealOpen(Base64.getEncoder().encodeToString(tamperedBytes), m4Seed))
    }

    @Test
    fun alertNonAsciiVectorD6() {
        val dpub = "cXuf/aiKe+z9dGMugopN89JjubmfqaZYvqJljAlHiwo="
        val text = "TEST2 Zażółć gęślą ⚠ jaźń"
        val ts = 1792847488L
        val asig = "l4wTcieIzdIBEWO/l5axxjSlL3/nZiLgt1K0+S0gQ91CxkPenIxp81jnzNkjCUIxpSSqMVuHo49W1XG4IRigCA=="
        val canonical = Crypto.alertCanonical(text, ts)
        assertEquals("OCPG-ALERT-v1|TEST2 Zażółć gęślą ⚠ jaźń|1792847488", canonical)
        assertTrue(Crypto.verify(dpub, canonical, asig))
    }

    @Test
    fun autonomySignatureVerifiesAndTamperedFails() {
        val sig = Crypto.signAutonomy(seed, pub, "nonce42", "", 1700000003L, "quiet", "")
        val msg = Crypto.autonomyCanonical(pub, "nonce42", "", 1700000003L, "quiet", "")
        assertEquals("OCPG-AUTONOMY-v1|$pub|nonce42||1700000003|quiet|", msg)
        assertTrue(Crypto.verify(pub, msg, sig))

        val escalated = Crypto.autonomyCanonical(pub, "nonce42", "", 1700000003L, "auto", "")
        assertFalse(Crypto.verify(pub, escalated, sig))

        val sigUndo = Crypto.signAutonomy(seed, pub, "nonce43", "", 1700000004L, "auto", "gimp")
        val msgUndo = Crypto.autonomyCanonical(pub, "nonce43", "", 1700000004L, "auto", "gimp")
        assertEquals("OCPG-AUTONOMY-v1|$pub|nonce43||1700000004|auto|gimp", msgUndo)
        assertTrue(Crypto.verify(pub, msgUndo, sigUndo))

        val tamperedPkg = Crypto.autonomyCanonical(pub, "nonce43", "", 1700000004L, "auto", "wine")
        assertFalse(Crypto.verify(pub, tamperedPkg, sigUndo))
    }
}
