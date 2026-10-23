# Guardian — fix list from the 2026-10-23 on-host test (commit 8a6e8ae)

Ordered by priority. IDs match `guardian-test-report-2026-10-23.md`. "Verify" is the check that passed (or should
pass) on the test host.

## P0 — must fix before anyone relies on it

1. **C0 — stop shipping `vm.memfd_noexec=2`** (`packaging/fapolicyd/99-guardian-memfd.conf`, its README mention,
   the note in `50-guardian.rules`).
   It breaks dbus-broker on the next boot → host unbootable. Remove the file or use `1`; add "reboot after each
   layer" to the rollout docs.
   *Verify:* reboot; `systemctl --failed` empty; `journalctl -b | grep MFD_NOEXEC_SEAL` empty.

2. **C1 — set `allow_filesystem_mark = 1` in `/etc/fapolicyd/fapolicyd.conf`** (README step + installer).
   Without it, `bwrap`/`unshare -rm` bind-mounting a downloaded binary over a trusted path runs it, and Flatpak
   `--user` apps run.
   *Verify (as child):* `bwrap --dev-bind / / --bind ~/x /usr/bin/true /usr/bin/true` → `Operation not permitted`.

3. **F1 — replace the fapolicyd trust-seeding instructions** (`packaging/fapolicyd/README.md` §2).
   `--from-file` doesn't exist; `trust.d` needs `path size sha256`; paths with `\` (and spaces) silently stop the
   load → partial trust → systemd can't spawn any unit. Ship a script that writes the `path size sha256` file,
   drops `\`/space paths, and refuses to continue unless `fapolicyd-cli --dump-db | wc -l` ≈ line count.
   *Verify:* `journalctl -u fapolicyd | grep "without duplicates"` equals the file's line count;
   `systemctl restart systemd-timesyncd` works under enforce.

4. **F12 — handle SDDM autologin into the parent** (`harden-audit.sh`, `setup-child.sh`).
   Omarchy ships `/etc/sddm.conf.d/autologin.conf` with `User=<parent>`. harden-audit: FAIL if autologin targets a
   non-child account. setup-child: offer to remove it. Also: the Omarchy SDDM theme only logs in `lastUser` (no user
   picker, and it's the only Qt6 theme installed) → the child can't log in at all; setup-child needs a greeter plan.
   *Verify:* fresh boot shows a login prompt; the child can log in.

5. **F13 — disable on-demand text consoles** (new `logind.conf.d` drop-in from `setup-child.sh`; `harden-audit.sh`).
   `NAutoVTs=0`, `ReserveVT=0`, mask `getty@tty2..6` / `autovt@`. harden-audit must check `NAutoVTs`, not running
   units (it currently reports a false PASS).
   *Verify:* Ctrl+Alt+F3..F6 → no login prompt.

5a. **F18 — console login escapes the screen-time freeze** (`packaging/schedule/guardian-lock.sh`). A child's tty
   login during a block lands in `getty@ttyN.service` (systemd can't create a session scope inside the frozen
   slice) and runs freely with network. Fix F13 first; also stop all processes of the child's uid regardless of
   cgroup and refuse new child logins while blocked (pam check).
   *Verify:* budget exhausted → child login on any tty is refused or immediately stopped.

6. **F4 — `install.sh` must `chattr -i` managed files first** (same list as `uninstall.sh`).
   After a watchdog restore, re-running `install.sh --enforce` dies at step 1 and the host stays in TEST mode
   (all installs ungated).
   *Verify:* delete hook → run watchdog → `install.sh --enforce` rc=0 and `50-guardian.hook` present.

6a. **F15 — phone grants lost after a daemon restart/reboot** (`daemon/src/ntfy.rs:153`, random
   `guardian-resp-<random>` per run). The app posts `grant_time` to the old topic; nothing arrives, no error, the
   child stays frozen. Make the response topic stable (derived from pairing) or announce the new one on start.
   *Verify:* restart guardiand → grant +X min from the phone without any new request card → `UNBLOCKED`.

6b. **F16 — screen-time is charged by wall clock** (`daemon/src/bin/guardiand.rs:183`, `active=true` stub). Feed
   `active` from logind (child session active, unlocked, not idle, slice not frozen).
   *Verify:* child logged out for 5 min inside the window → remaining minutes unchanged.

## P1 — important

6c. **F17 — lock hook** (`packaging/schedule/guardian-lock.sh`): wait until the session reports `LockedHint=yes`
   (or a few seconds) before `systemctl freeze`; notify once per transition, not every tick; document that a
   frozen Hyprland blocks Ctrl+Alt+Fn for the parent too (recovery: SSH + `systemctl thaw user-<uid>.slice`).
   *Verify:* budget runs out → lock screen visible; after a grant the child must enter their password.

7. **F14 — add Guardian's own files to fapolicyd trust** (`install.sh` or fapolicyd step):
   `/usr/local/bin/{flatpak,snap,nix-env}`, `/usr/bin/guardian-{ctl,sign}`, `/usr/bin/guardiand`,
   `/usr/lib/guardian/*` → `fapolicyd-cli --file add <f> --trust-file guardian`.
   *Verify (as child):* `flatpak install flathub org.videolan.VLC` is held and a card reaches the phone.

8. **F2 — Guardian rules vs. upstream defaults.** Upstream `42/43-*.rules` deny untrusted ELF for **all** uids
   before `50-guardian.rules`. Either install as `05-guardian.rules` (so "everyone else: allow" works) or document
   that the parent and every service need trust entries (e.g. `/usr/local/bin/ollama` failed at boot). Add a
   pre-enforce check that lists enabled units whose ExecStart isn't trusted.

9. **B3 — Flatpak `--user` install bypass** (`/usr/bin/flatpak --user install …` skips the wrapper).
   With C1 fixed the app can't run, but the install is unaudited. Options: polkit/permissions on the user
   installation, or detect new `~/.local/share/flatpak/app/*` and alert.

10. **B4 — DoH to unlisted resolvers** (`packaging/nftables/guardian-egress.nft`).
    `1.1.1.2` (Cloudflare Family) and `208.67.222.222` (OpenDNS) answered DoH. A static IP list won't keep up; consider
    a maintained DoH-resolver list updated by the blocklist timer, or SNI-based blocking of `dns-query` hosts.

11. **F11 — audit mirror loses entries** (`daemon/src/bin/guardiand.rs:127-135`, `ntfy.rs::audit_mirror_loop`,
    `guardiand.service`). Attach the mirror before the first `record()`, retry with backoff (bounded queue), add
    `After=/Wants=network-online.target`.
    *Verify:* reboot → mirror topic contains `started` + heartbeat with the same seq as the local log.

12. **F5 — watchdog restart alert is lost** (`packaging/guardian-watchdog.sh:39-41`). Wait for
    `/run/guardian/guardian.sock` (retry ~10 s) before `guardian-ctl alert`.
    *Verify:* `systemctl stop guardiand; systemctl start guardian-watchdog` → `watchdog_alert … not running` in audit.

13. **F8 — pairing QR without `dpub`** (`guardian-ctl pair`). As non-root it can't read `/var/lib/guardian/daemon.pub`
    and silently omits it. Make `daemon.pub` 0644 (dir 0755) or fail loudly.
    *Verify:* decoded `#pair=` payload contains `dpub` when run as the normal user.

## P2 — docs / polish

14. **F3 — Arch/AUR fapolicyd prerequisites** in the README: `systemd-sysusers`; `trust = file`; remove
    `22-buildroot.rules` (user `mock`); substitute `%ld_so_path%`; `fagenrules --load`; no inline `#` comments in rules.
15. **F10 — break-glass procedure**: how the parent repairs a host when guardiand is down (pacman hook fail-closed):
    remove `/etc/pacman.d/hooks/50-guardian.hook` or `uninstall.sh`; boot a Limine snapshot.
16. **F9 — TESTING.md §5b** forgery `curl` needs `source` + `package` fields, otherwise it never hits the signature
    check. Reword the FORGED alert ("install was denied" → "approval ignored; request still pending").
17. **F7 — `harden-audit.sh` nft check** needs root; say "run with sudo" or detect and skip with a note.
18. **F6 — `guardian.sock` is 0666**; docs say 0660. Align (peercred check already blocks non-root).
19. **Terminal QR** didn't scan on the parent's phone; the PNG did. Offer `guardian-ctl pair --png <file>` (qrencode).

## Already verified OK (no action)
pacman hold/allow/deny/timeout · daemon-down → deny · anti-flood · control socket refusal + audit (N2 fixed) ·
B1 fixed · signed ntfy allow/deny/timeout · forged/replayed/package-swapped decisions rejected · audit chain
edit/truncation detection · signed off-box mirror · watchdog restore/restart · fapolicyd blocks $HOME/tmp/shm/ld.so/
memfd/shebang · auto-trust after approval · NM polkit · Chromium DoH policy · reboot with fapolicyd enforcing.
