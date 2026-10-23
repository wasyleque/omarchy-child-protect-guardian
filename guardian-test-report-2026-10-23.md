# Guardian on-host test report (8a6e8ae)

Oct 23, 2026 · on-host run following `docs/TESTING.md`

## Verdict

The pacman loop is solid: hold → allow / deny / timeout, daemon-down fail-closed, anti-flood, control-socket
refusal and the audit chain all behave as documented, and B1 and N2 from the 21.10 report are fixed. fapolicyd
was run end-to-end for the first time, and that is where the serious problems are:

- **the shipped memfd sysctl makes the next boot fail** (dbus-broker can't start; C0) — repaired offline;
- **a child can run any downloaded binary despite fapolicyd** by bind-mounting it over a trusted path in an
  unprivileged user namespace (`bwrap` / `unshare`). The same gap lets Flatpak `--user` apps run, so B3 is not
  mitigated by fapolicyd as the docs claim. One config line closes it (verified);
- **following the fapolicyd README can stop systemd from starting any service**: the trust DB silently loads only
  part of the seed file;
- re-running `install.sh` (e.g. `--enforce`) fails after the watchdog has restored a file, leaving the host in
  TEST mode with **all installs ungated**;
- on Omarchy the child gets around the whole setup at the session level: SDDM **autologins the parent** (F12)
  and **text-console logins on tty3–6** escape the screen-time lock (F13);
- B4 (DoH to unlisted resolvers) is still open.

After removing the memfd sysctl, a reboot with fapolicyd enforcing works (see below).

**Scope.** Commit `8a6e8ae`, Omarchy (Arch), kernel 7.2.8, fapolicyd 2.0.1 (AUR). Parent `wasyl` (uid 1000),
child `dziecko` (uid 1001), child actions run via `sudo -u dziecko`. §9/§13 GUI parts were run by the tester in the child's Hyprland
session. Not run: §12 physical boot tests, snap/nix (not installed).

## Results by section

| § | Area | Result |
|---|---|---|
| 0 | Build + unit tests | ✅ 20/20 |
| 1 | Install (test mode) | ✅ daemon starts, both sockets, audit genesis (B1 fixed) · ⚠️ `guardian.sock` is 0666, docs say 0660 (F6) |
| 2 | pacman hold → allow / deny / timeout | ✅ ✅ ✅ |
| 3 | Child on control socket | ✅ refused, logged, `control_blocked` in audit (N2 fixed; ntfy alert not checked) |
| 4 | Anti-flood | ✅ 6th request → `too many pending requests` |
| 5 | ntfy / phone (signed mode) | ✅ pairing + key fingerprint check · ✅ allow (`via: ntfy_signed`, auto-trusted) · ✅ deny · ✅ no answer → DENY after 181 s · ✅ forged sig → rejected, audited, "FORGED approval blocked" on phone, request stays pending · ✅ unsigned mode refused when no key paired · ⚠️ terminal QR didn't scan, PNG did · ❌ F8 (QR without `dpub`) · ⚠️ F9 |
| 5+ | Replay / tampering of real signed decision | ✅ verbatim replay → stale nonce · ✅ real sig on new id/nonce → bad signature · ✅ package swapped → mismatch; request stays pending in all cases |
| 5+ | Off-box audit mirror (`audit_topic`) | ✅ entries + heartbeat published, Ed25519 sigs verify with `daemon.pub`, identical to local log, modified entry fails verification |
| 6 | Flatpak wrapper / polkit | ✅ wrapper gates, ✅ child `--system` install refused · ❌ B3 still open |
| 7 | fapolicyd | ✅ copy-in-$HOME, /tmp, /dev/shm, ld.so trick, memfd, shebang script blocked · ✅ auto-trust after approval · ❌ namespace bypass (C1) · ❌ README rollout bricks services (F1, F2) |
| 8 | nftables egress | ✅ DNS/DoT/listed-DoH/UDP/arbitrary ports blocked, web OK · ❌ B4 still open |
| 10 | Watchdog | ✅ restores hook + daemon binary, ✅ restarts daemon, ✅ detects audit edit · ❌ restart alert lost (F5) · ❌ breaks `install.sh` re-run (F4) |
| 11 | Audit chain | ✅ edit → `content tampered at entry 4`, truncation → `log truncated … anchor expects …` |
| 12 | harden-audit (software part) | 8 pass / 6 warn / 1 fail (Limine snapshots) · ⚠️ false nft WARN as non-root (F7) |
| 13 | Child terminal attempts | ✅ no sudo / su, can't kill or stop guardiand; daemon down → install denied |
| 14 | `--enforce` | ✅ approved install held, released and auto-trusted · ✅ deny · ✅ reinstall/upgrade not gated · ❌ first attempt failed (F4) |

## Bugs

### Reboot with fapolicyd enforcing (without the memfd sysctl) — ✅
Booted normally from `@`: no failed units, userspace 3.6 s, trust DB loaded fully (279 113), guardiand in SIGNED
mode, audit chain OK, child still can't run `~/evil2`, approved `figlet` still runs. Precondition found with a
pre-reboot scan of enabled units: `/usr/local/bin/ollama` (not pacman-owned, runs as user `ollama`) had to be added
to trust or it fails at boot (F2).

### C0 — `99-guardian-memfd.conf` (`vm.memfd_noexec=2`) makes the host unbootable (critical)
Installed as `packaging/fapolicyd/README.md` / `50-guardian.rules` instruct and applied with `sysctl --system`.
The running session is unaffected, so nothing shows during testing. On the next boot dbus-broker cannot start:
`dbus-broker: memfd_create() requires MFD_NOEXEC_SEAL with vm.memfd_noexec=2` →
`systemd: Failed to set up API bus: Transport endpoint is not connected`; the system hung after login and two
further boots failed. Repaired offline from another Linux (LUKS unlocked, `@` mounted, sysctl file and
fapolicyd/guardian enable links removed). **Fix:** do not ship `vm.memfd_noexec=2` system-wide; at most `1`, or
block memfd exec for the child another way, and say "reboot-test before relying on it" in the README. The same
boots also showed `ollama.service: … Operation not permitted` (fapolicyd, F2). A later reboot with fapolicyd
enforcing and without this sysctl was clean (section above).

### C1 — fapolicyd bypass through user-namespace bind mounts (critical)
fapolicyd marks mounts, so a mount created in a new user namespace is not watched. As the child, with a modified
binary that is certainly not in the trust DB:

```
~/evil2                                                        → Operation not permitted
bwrap --dev-bind / / --bind ~/evil2 /usr/bin/true /usr/bin/true → RUNS
unshare -rm sh -c 'mount --bind ~/evil2 /usr/bin/true && /usr/bin/true' → RUNS
/usr/bin/flatpak run --command=gnome-calculator org.gnome.Calculator     → RUNS (B3 app)
```

`integrity = sha256` does not help. **Fix (verified):** `allow_filesystem_mark = 1` in
`/etc/fapolicyd/fapolicyd.conf` — all four become `Operation not permitted`, normal child programs (ls, python3,
git, curl, nvim) still work. Ship it in the README/installer. The alternative is
`kernel.unprivileged_userns_clone=0`, but that breaks browser sandboxes.

### B3 — Flatpak bypass via `/usr/bin/flatpak --user` (still open)
The child installed `org.gnome.Calculator` and its runtimes with no request reaching guardiand and nothing in the
audit log. With C1 fixed, the app can no longer run, so the impact is reduced to disk use and an unaudited install.
The wrapper/README text "fapolicyd … so even a --user app the child sneaks in can't execute" is only true after the
C1 fix.

### B4 — DoH to resolvers not on the IP list (still open)
As the child, with the egress table loaded, these returned real DNS answers: `https://1.1.1.2/dns-query`
(Cloudflare Family) and `https://208.67.222.222/dns-query` (OpenDNS). `dns.google` and listed IPs are blocked. The
browser policies only cover the browsers' built-in DoH.

### F1 — fapolicyd README trust seeding fails; enforce can stop all services (high)
- `fapolicyd-cli --file add --from-file` does not exist (rc=2). The fallback calls `fapolicyd-cli` once per
  file — 278 930 calls.
- The README writes a bare path list into `/etc/fapolicyd/trust.d/`, but the file backend expects
  `path size sha256`.
- Even with a correct file, fapolicyd **stops loading at the first path with a backslash**
  (`/usr/lib/systemd/system/system-systemd\x2dcryptsetup.slice`, line 245 486) with no error. The remaining
  33 444 entries were missing, including `systemd-executor` and `libz`. Under enforce, systemd could not start
  any unit (`guardiand.service: Failed to spawn executor: Operation not permitted`, also `systemd-timesyncd`).
  Rolled back with `systemctl stop fapolicyd`.

Working seed: generate `path size sha256` and drop paths containing `\` (and spaces), then compare
`fapolicyd-cli --dump-db | wc -l` against the file's line count before enforcing.

### F2 — Upstream default rules restrict every user, not just the child
`50-guardian.rules` ends with "everyone else: allow", but the upstream `42-trusted-elf` / `43-known-elf` rules
come first and deny untrusted ELF executables for **all** uids. In permissive mode the parent's
`~/.local/share/mise/.../node`, `claude`, and `/usr/bin/guardian-ctl` / `guardiand` (not pacman-owned) were all
"would deny". Either install the Guardian rules ahead of the defaults (e.g. `05-guardian.rules`) or document it.
For this test, `01-test-parent.rules` (`allow perm=any uid=1000 : all`) was added.

### F3 — Arch/AUR fapolicyd needs extra steps the README doesn't mention
Not Guardian code, but each one blocks the documented rollout: the `fapolicyd` user isn't created on install
(run `systemd-sysusers`); the default `trust = rpmdb,file` must become `trust = file`; `22-buildroot.rules`
references the RPM user `mock` and aborts `fagenrules`; `43-known-elf.rules` contains an unsubstituted
`%ld_so_path%`; rules must be compiled with `fagenrules --load`; inline `#` comments break a rule line.

### F4 — `install.sh` cannot re-run after the watchdog restored a file (high)
The watchdog re-applies `chattr +i` when it restores a file; `install.sh` never removes it, so
`install.sh --enforce` dies at the first step with `install: cannot remove '/usr/bin/guardiand': Operation not
permitted` (rc=1). The host stays in TEST mode — only `sl` is gated, `pacman -S figlet` installed without any
prompt. **Fix:** `chattr -i` the managed files at the start of `install.sh`, as `uninstall.sh` does.

### F5 — Watchdog "guardiand was not running" alert never reaches the parent
4/4 runs: the alert went only to local syslog (`logger` fallback); nothing in the audit log, so no ntfy push.
`guardian-ctl alert` runs right after `systemctl restart` before the socket is up. **Fix:** wait for the socket
(retry loop) before alerting.

### F6 — `guardian.sock` created 0666 (low)
TESTING.md §1 says 0660. SO_PEERCRED still refuses non-root (§3 passes), so this is defense in depth only.

### F7 — `harden-audit.sh` false WARN when run as a normal user (low)
The script says to run as the normal user, but `nft list table` needs root, so it reports "nftables guardian
egress policy not loaded" while the table is loaded. With sudo it reports PASS.

### F8 — `guardian-ctl pair` without sudo silently drops the daemon key from the QR
TESTING.md §5a runs `guardian-ctl pair --topic …` as the normal user. `/var/lib/guardian` is root-only, so the
payload is `{pair_to, server, topic}` with no `dpub`, and no warning is printed. With sudo the payload includes
`dpub`. Without it the app cannot verify the daemon's signatures on pushed requests. **Fix:** fail loudly when
`daemon.pub` is unreadable, or make `daemon.pub` world-readable (it is public).

### F9 — TESTING.md §5b forgery example doesn't reach the signature check (low)
The documented `curl` payload has no `source`/`package`, so the daemon rejects it as "unknown decision format
(old app?)" and alerts "update the parent app" — the signature path is never exercised. With `source` and `package`
added it is rejected as `bad signature (forged)` as intended. Also: the "FORGED" alert says "The install was
denied", but the request correctly stays pending (a forger must not be able to deny either) — reword.

### F10 — No documented recovery path when guardiand is down (design)
In enforce mode the pacman hook denies every install when guardiand is unreachable — correct against the child, but
it also blocks the parent from repairing a broken system with pacman. Document a root-only break-glass (e.g. remove
`/etc/pacman.d/hooks/50-guardian.hook` or `uninstall.sh`) next to the hardware baseline.

### F11 — Off-box audit mirror drops entries at boot and never retries (medium)
After a reboot the mirror topic was missing `started` (seq 34, 36) and the boot heartbeat (seq 37); the local chain
had them. Causes: `started` is recorded before `audit.set_mirror()` is called (`bin/guardiand.rs:127-135`), and
`audit_mirror_loop` only logs `audit mirror publish failed` (DNS not up yet — `guardiand.service` is
`After=local-fs.target` only) with no retry. Since the heartbeat is daily, a lost boot heartbeat means a full day
with no dead-man's-switch signal. **Fix:** attach the mirror before the first record, retry with backoff (bounded
queue), and order the unit `After=network-online.target` / `Wants=network-online.target`.

### F12 — Omarchy's default SDDM autologin into the parent account is not detected or removed (high)
`/etc/sddm.conf.d/autologin.conf` has `[Autologin] User=wasyl` (Omarchy default). A child who powers the machine on
lands in the parent's session with sudo — every other layer is moot. THREAT_MODEL/ROADMAP list "no autologin" as
planned, but `harden-audit.sh` doesn't check it and `setup-child.sh` doesn't change it. **Fix:** harden-audit FAIL
when autologin targets any non-child account; setup-child offers to remove it.
Related: Omarchy's SDDM theme (`/usr/share/sddm/themes/omarchy/Main.qml`) only logs in `userModel.lastUser` — there
is no user picker, so the child **cannot log in at all** without switching to a stock theme (e.g. `maldives`).
setup-child should handle the greeter for multi-user use.

### F13 — Text-console logins on tty3–tty6; harden-audit reports a false PASS (high)
From the child's GUI session, Ctrl+Alt+F5/F6 shows a login prompt: `getty@tty3..6` are spawned on demand by logind
(`NAutoVTs=6` default), so `harden-audit.sh`'s "no spare getty on tty2+ active" check passes whenever nobody has
switched VT yet. A console login escapes the screen-time freeze — confirmed live, see F18. **Fix:** set `NAutoVTs=0` (and
`ReserveVT=0`) in a `logind.conf.d` drop-in, mask `getty@tty2..6`/`autovt@`, and have harden-audit check
`NAutoVTs` instead of currently-running units.

### F14 — With fapolicyd enforcing, the child can't run Guardian's own wrappers (medium)
In the child's GUI session: `flatpak install flathub org.videolan.VLC` → `bash: /usr/local/bin/flatpak:
/usr/bin/env: bad interpreter: Operation not permitted`. The wrappers (`/usr/local/bin/flatpak`, `snap`,
`nix-env`) and `/usr/bin/guardian-ctl` are installed by `install.sh`, not pacman, so they are not in fapolicyd
trust. Result: no Flatpak command works for the child at all, no request reaches guardiand, no card on the phone.
**Fix (verified):** add every file `install.sh` installs to `/etc/fapolicyd/trust.d/guardian`
(`fapolicyd-cli --file add <f> --trust-file guardian`) — afterwards the child's `flatpak install` is held, the
request reaches guardiand and the push goes out; deny → `not approved — aborting`.

### §9 / §13 in the child's GUI session (Chromium, Hyprland)
| Test | Result |
|---|---|
| `nmcli connection add` Wi-Fi / WireGuard | ✅ polkit asks for the parent's password / `Insufficient privileges` |
| Chromium Secure DNS | ✅ off and locked; `chrome://policy` shows `DnsOverHttpsMode = off` |
| downloaded `rg` release binary | ✅ `Operation not permitted` |
| Ctrl+Alt+F5/F6 | ❌ console login prompt (F13) |
| `sudo -v`, `systemctl stop guardiand` | ✅ no sudo; polkit requires the parent's password |
| `flatpak install …` | ❌ wrapper itself blocked (F14) |

### Screen-time (`[schedule]`) — live test with the child logged in graphically
Budget 3 min, window 07:00–23:00, `tick_secs = 10`, lock/unlock hooks for `dziecko`.

| Check | Result |
|---|---|
| Countdown → `budget_exhausted` | ✅ 17:02:28 start → blocked 17:05:19, `schedule_blocked` in audit |
| Hard stop | ✅ `user-1001.slice` frozen; mouse/keyboard dead in the child session |
| Lock screen | ❌ never shown (`LockedHint=no`): the freeze lands before the lock screen renders, and after the thaw the desktop is open with no password (F17) |
| Parent's phone grant (+15 min) | ❌ first attempt lost (F15) · ✅ after a fresh request card: `grant of 15 min accepted` → `UNBLOCKED` → slice thawed, `grant_applied` audited |
| Time accounting | ❌ budget keeps draining while frozen and with nobody logged in (F16): 3 + 15 min − 6.3 min wall clock → "11 min remaining" |

### F15 — Phone grants are lost after any daemon restart (high)
The response topic is random per daemon run (`guardian-resp-<random>`, `ntfy.rs:153`) and the app only learns it
from the last request card. After guardiand restarted (here: to load `[schedule]`; equally after every reboot),
the phone posted a signed `grant_time` (+15 min) to the **previous** run's topic at 17:07:03; the daemon never saw
it, the parent got no error, and the child stayed frozen. It only worked after a new install request reached the
phone. **Fix:** derive the response topic deterministically from pairing (e.g. `request_topic` + `-resp`, or a
value in the pairing QR), or have the daemon publish a signed "respond_to changed" notice on start.

### F16 — Screen-time budget is charged by wall clock, not child activity (high)
`bin/guardiand.rs:183`: `on_tick(day, minute, true, tick_secs)` — "active=true is a stub until the logind idle hook
lands". The engine's `idle_does_not_consume_budget` test passes, but the daemon never passes `false`. Time is
charged inside the window whether or not the child is logged in, and also while the child is frozen. With the
example policy (07:00–20:30, 120 min) the day's budget is gone by 09:00 even if the child never touches the PC.
**Fix:** feed `active` from logind (child has an active, unlocked, non-idle session; slice not frozen).

### F17 — Lock hook: no lock screen, no VT switching, notification spam (medium)
- `loginctl lock-session` + immediate `systemctl freeze` → the lock screen never renders; after the thaw the
  child's desktop is unlocked (README says the child must re-enter their password — not true).
- Freezing the slice freezes Hyprland, which handles Ctrl+Alt+Fn, so **the parent can't switch to another VT at the
  console either** — only root over SSH (`chvt`) could. Document a local recovery path.
- The hook re-runs every tick while blocked: `sudo -u dziecko notify-send …` logs 3 journal lines every 10 s into a
  frozen session. Notify once per transition.

### F18 — Console login escapes the screen-time freeze (critical, confirmed)
While `user-1001.slice` was frozen (budget exhausted, 17:17), root switched the display to tty3 (`chvt 3`; the child
could equally press Ctrl+Alt+F3 before the freeze or from any non-frozen moment) and the child logged in as
`dziecko`: shell works, `curl -I https://example.com` works. The child's bash runs in
`/system.slice/system-getty.slice/getty@tty3.service` — **not** in `user-1001.slice`: systemd could not start
`session-10.scope` inside the frozen slice (`ControlGroup=` empty), so the process stayed in the getty cgroup.
The lock hook re-runs every tick but only freezes `user-1001.slice`, so this session is never stopped.
**Fix:** F13 (no consoles) is the primary fix; additionally the lock hook should stop every process of the child's
uid regardless of cgroup (e.g. `loginctl terminate-session` for non-graphical sessions, or `pkill -STOP -u <uid>`
as a backstop), and deny new child logins while blocked (`pam_exec`/`pam_access` check of `guardian-ctl schedule`).

## Environment notes
- systemd-resolved forwards to `8.8.8.8` among others, and the child may query the stub on `lo`, so on this host
  DNS filtering depends on resolved's upstreams (harden-audit warns about this).
- `pip install --user` console scripts are blocked; `python3 -m <pkg>` still runs (documented gap).
