# Guardian on-host test report (c5b90de)

Oct 21, 2026 · @wasyleque

## Verdict

The core loop works on real hardware: pacman installs are held, approved or denied from the parent
app, timeouts fail closed, and a forged approval is rejected and alerted. But the build is **not yet
safe as a child's sole protection**. Four bugs were confirmed on the host:

- the shipped policy makes the daemon crash on a fresh install;
- a child can install Flatpaks by calling `/usr/bin/flatpak --user` directly;
- browser DoH to any resolver not on the IP list bypasses DNS filtering;
- the parent PWA never updates once installed, so a parent stays on an old, incompatible version.

**Scope.** Commit `c5b90de`, installed with `packaging/install.sh` on Omarchy (Arch) on a dedicated
test machine. Parent account `wasyl`, child account `dziecko` (uid 1001). This was a live on-host test
following `docs/TESTING.md`, not a source review. Sections 7 (fapolicyd), 9/13 (child GUI session),
12 (boot/LUKS) and 14 (final/upgrade) were **not run**.

## Results by section

| § | Area | Result |
|---|---|---|
| 0 | Build + unit tests | ✅ 18/18 |
| 1 | Install | ❌ daemon did not start (B1); ✅ after workaround |
| 2 | pacman hold → allow / deny / timeout | ✅ ✅ ✅ |
| 3 | Child access to control socket | ✅ blocked / ⚠️ not logged or alerted (N2) |
| 4 | Anti-flood (`max_pending_per_uid`) | ✅ |
| 5 | Remote approval over ntfy, signed mode | ✅ allow, deny, timeout, forgery; ❌ phone PWA stale (B8) |
| 6 | Flatpak wrapper | ✅ wrapper gates / ❌ bypass (B3) |
| 8 | nftables egress for the child uid | ✅ mostly / ❌ DoH bypass (B4) |
| 10 | Watchdog | ✅ restored hook, restarted daemon, detected log change |
| 11 | Audit chain + `audit-verify` | ✅ |
| 13 | Daemon stopped → install denied | ✅ (partial) |

## Bugs (confirmed)

### B1 — Shipped policy crashes the daemon on a fresh install
`packaging/policy.example.toml:20` sets `daemon_key_path = "/etc/guardian/daemon.key"`, but
`guardiand.service` runs with `ProtectSystem=strict` and only `/var/lib/guardian` writable
(`StateDirectory=guardian`). The daemon cannot create its key and fails to start. The built-in default
in `daemon/src/config.rs:49` is already `/var/lib/guardian/daemon.key`.
**Fix:** use `/var/lib/guardian/daemon.key` in the example policy. This is the same symptom as H1 in
the second review, and it is still present.

### B3 — Flatpak bypass through `/usr/bin/flatpak --user`
The wrapper lives in `/usr/local/bin` and only intercepts `flatpak` found via `PATH`. As the child,
`/usr/bin/flatpak --user remote-add flathub … && /usr/bin/flatpak --user install …` started
downloading (`Installing runtime/org.freedesktop.Platform.GL.default…`) with no request reaching
Guardian. The polkit rule only covers system installs. The documented backstop, fapolicyd, is only in
the AUR and is not installed by `install.sh`.
**Fix:** gate at a layer the child cannot sidestep, e.g. deny `--user` installations with
`/etc/flatpak/…` config or a seccomp/exec rule, or make fapolicyd a hard requirement.

### B4 — DoH to resolvers not on the IP list bypasses DNS filtering
With `guardian-egress.nft` loaded, plain DNS, DoT, QUIC and DoH to the listed IPs were blocked. As
the child, these still resolved:

| Endpoint | IP | Result |
|---|---|---|
| `dns.mullvad.net` | 194.242.2.2 | resolves |
| `dns.nextdns.io` | 38.175.123.217, 217.146.13.3 | resolves |
| `mozilla.cloudflare-dns.com` (Firefox default) | 172.64.41.4, 162.159.61.4 | resolves |

Firefox's built-in DoH therefore bypasses filtering without any configuration by the child.
**Fix:** treat the IP list as best-effort only. Ship the Firefox/Chromium DoH-off policies by default
in `install.sh`, block the `use-application-dns.net` canary on the resolver, and plan SNI filtering.

### B8 — The parent PWA never updates on a device that has opened it once
`parent-app/sw.js` serves cache-first under a fixed cache name (`ocpg-parent-v1`), and `sw.js` itself
never changes, so the browser never installs a new service worker. On the phone, the app kept
signing decisions in the old `OCPG-v1` format, without `source` and `package`.

This was verified cryptographically: the posted signature verified with the phone key over
`OCPG-v1|id|decision|nonce|ts` and not over the `OCPG-v2` message. The daemon silently dropped every
tap (see N9), so the parent saw "Allow" succeed while the install timed out. Reloading, "New key…"
and re-pairing did not help. Only a cache-busting URL (`…/parent-app/?v=2#pair=…`) loaded the
current code.
**Fix:** version the cache name on every release, or serve `index.html` network-first. Have the
daemon tell the parent app when it needs updating (see N9).

## Notes (lower severity)

| # | Finding |
|---|---|
| N2 | A child opening `guardian.sock` gets `EACCES` from file permissions (0660), so the daemon never sees the attempt. Nothing is written to the audit log and no parent alert is sent, although the docs promise `control_blocked` alerts. |
| N5 | A rejected forged approval (`bad signature`) is logged to the journal and alerted to the phone ("FORGED approval blocked"), but **not written to the audit log**. The audit only shows `submitted` → `decided via timeout`. |
| N6 | Only one `parent_pubkey` is supported (`config.rs:74`). There is no second parent or second device, and no documented key-rotation or lost-phone procedure. |
| N7 | The PWA never removes expired or already-decided cards, and an unpaired device (old key) keeps showing "connected" and receiving cards it can no longer approve. |
| N9 | `ntfy.rs:470-473`: a decision that fails to parse is dropped with `Err(_) => return`. There is no log, audit entry or alert. This hid B8 completely. It should be logged and alerted ("decision in unknown format — update the app"). |
| N10 | Entering server and topic by hand gives an app that is "connected" but shows no requests. The only hint is a "Not paired" banner when a request arrives. Setup should require the QR or offer a daemon-key field. |
| N11 | The parent app is a web page that has to be "installed" from the browser menu. There is no Android or iOS app, and it gets no notifications while closed, so the native ntfy app is needed as well. A private tab loses the key whenever the screen turns off. This is hard for a typical parent. |
| N12 | `index.html:263-276`: when a pairing link would replace an existing pairing, a `confirm()` is shown. If it is cancelled or suppressed, the daemon key is **not** saved, yet `paired = true` still shows "Paired — now share the public key". Everyone upgrading from the old version hits this dialog. |

## What worked well

- pacman hold, allow, deny and fail-closed timeout, with `AbortOnFail` aborting the transaction.
- Signed mode end to end: allow and deny from a paired device, and a forged signature rejected with a
  signed alert to the parent.
- Anti-flood cap, watchdog restore and restart, and the hash-chained audit with `audit-verify`.
- Child egress: plain DNS to other resolvers, DoT, QUIC/UDP, and arbitrary-port tunnels (22, 9001)
  were blocked, while the parent account was unaffected.

## Not tested

- §7 fapolicyd (AUR).
- §9/§13 child GUI session: browser, NetworkManager, TTY.
- §12 bootloader, live USB, LUKS, Secure Boot.
- §14 `-Syu` and reboot persistence.
- IPv6 egress, because the test network has no IPv6.
- Phone notification delivery while the app is closed.

## Test-machine state left behind

- `/etc/guardian/policy.toml`: `[ntfy]` enabled in signed mode with the phone key,
  `daemon_key_path` under `/var/lib/guardian`, `decision_timeout_secs = 300`. Backup:
  `policy.toml.pre-ntfy`.
- `inet guardian` nftables table loaded until reboot (`sudo nft delete table inet guardian` to remove).
- Test hook `99-guardian-test.hook` (gates only `sl`). Extra packages: `bind`, `flatpak`.
