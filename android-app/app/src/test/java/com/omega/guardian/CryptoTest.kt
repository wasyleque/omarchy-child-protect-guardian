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
}
