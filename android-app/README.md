# Guardian Parent — native Android app

A native Android app for the parent, replacing the web PWA (which couldn't notify while closed, kept
serving stale code, and lost its key in private tabs — see the on-host test report, B8/N11). It does
the same cryptography the daemon already trusts, so **no daemon change is needed**.

Languages: **English · Polski · Español** (the app follows your phone's system language).

## Download & install (parent's phone)

**→ Get the APK from the [Releases page](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest).**
Direct link to the current build:
[`guardian-parent-0.1.6.apk`](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/download/v0.1.6/guardian-parent-0.1.6.apk).

1. On the phone, open that link and **download the `.apk`**.
2. Tap the downloaded file; when asked, allow your browser/file manager to **install unknown apps**.
3. Open **Guardian Parent** → **Pair — scan QR**, and scan the QR shown on the PC by
   `guardian-ctl pair --topic <topic>` (or `sudo packaging/pair.sh <topic>`).
4. Tap **Allow unrestricted background** so approvals arrive even while the app is closed.

The APK is **debug-signed** (the key is committed in this repo, so it is reproducible and non-secret).
Because the signing key is fixed, each new version **updates in place** — no uninstall, pairing kept.
A new APK is attached to every `v*` release automatically (see *Build → GitHub Actions* below).

## What it does

- Generates an **Ed25519 key on the phone** and keeps the private seed in `EncryptedSharedPreferences`
  (AES-256-GCM, key in the hardware-backed AndroidKeyStore). The private key never leaves the device.
- **Pairs by scanning the QR** printed by `guardian-ctl pair` (`#pair=` → `{server, topic, dpub}`).
- Keeps a **foreground service** with a long-lived ntfy WebSocket, so install requests arrive and can
  be approved **even when the app is closed** — no FCM/Firebase, no Google account.
- On each request it **verifies the daemon's signature** (`csig`, `OCPG-CH-v2`) with the paired daemon
  key and **ignores any card the daemon didn't sign** (a forged/injected card shows nothing).
- Shows a high-priority notification with **Allow / Deny**; the tap **signs on-device** (`OCPG-v2`)
  and POSTs the decision. Signed security **alerts** are verified and shown too.
- **Screen-time grant** buttons (+15 / +30 min) send a signed `OCPG-GRANT-v2` command.

## Crypto compatibility (proven)

`Crypto.kt` uses raw BouncyCastle Ed25519 — byte-identical to the daemon's `ed25519-dalek` and the
PWA's `@noble/ed25519`. `app/src/test/.../CryptoTest.kt` pins this against a shared cross-impl vector
and runs in CI on every push. (The same logic was also checked on the JVM here before shipping: the
public key, decision signature and grant signature match the vector, a genuine challenge verifies and
a tampered one is rejected.)

## Build

> ⚠ This project has **not been compiled on a device** here (no Android SDK in the dev sandbox). The
> crypto core is verified; the Android wiring is a first cut — build it in CI or Android Studio and
> report anything that doesn't compile, the same way we iterated on the daemon.

### GitHub Actions (recommended, like the yt-downloader flow)

`.github/workflows/android.yml` runs the crypto tests and builds a **debug APK** on every push
(downloadable as the `guardian-parent-apk` workflow artifact — handy for testing a branch, but it
expires and needs a GitHub login). **For a permanent public download, cut a release:**

```bash
# bump versionCode/versionName in app/build.gradle.kts first, then:
git tag v0.1.7 && git push origin v0.1.7
```

The tag triggers the workflow, which attaches the APK to a **GitHub Release** on the
[Releases page](https://github.com/wasyleque/omarchy-child-protect-guardian/releases) — a stable link
anyone can download with no login. The committed debug key is used unless you provide a release
keystore via repo secrets `KEYSTORE_B64`, `KS_PASS`, `KEY_ALIAS`, `KEY_PASS` (generate once with
`keytool -genkeypair -keystore guardian.jks -keyalg RSA -keysize 2048 -validity 10000`).

### Local (Android Studio)

Open `android-app/` in Android Studio and Run. The Gradle wrapper (Gradle 9.3.0) and AGP 8.13.2 are
committed, so Studio and CI build the **same** stack — no version drift. CLI: `./gradlew :app:assembleDebug`.

## On the phone

1. Install the APK (enable "install unknown apps" for your browser/file manager).
2. Open the app, tap **Pair — scan QR**, scan the QR from `guardian-ctl pair --topic <topic>`.
3. Tap **Allow unrestricted background** so the listener survives Doze.
4. The child's installs now pop up as notifications you approve with one tap.

## Stack

Kotlin · Views + ViewBinding (no Compose) · minSdk 26 / targetSdk 34 · BouncyCastle (Ed25519) ·
OkHttp (ntfy WebSocket) · ZXing (QR) · androidx.security (key at rest). No FCM, Room, Hilt or NDK.
