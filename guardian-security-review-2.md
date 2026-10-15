Guardian security review (e8de6b3)
Oct 14, 2026 · @Alan Alexis Roman Guerra
Verdict
Do not accept this as a kids' protection plugin yet. Five issues defeat the protections the project advertises:
• the shipped service crash-loops on a fresh install;
• a reboot defeats screen time;
• untrusted binaries can still run;
• Omarchy's own boot menu bypasses every layer;
• tamper alerts never reach the parent app.
The author's own README still says not to rely on it as a child's sole protection.
Scope. Repository wasyleque/omarchy-child-protect-guardian, commit e8de6b3 (Oct 11, 2026). I read all of it: the Rust daemon and clients, every packaging script, unit, hook and policy file, and the parent PWA. This is a source-only review. No repo code was built or executed.
It is not a plugin in the Omarchy 4 sense. Omarchy 4 plugins are bar widgets added from git as the user. This project instead uses install.sh as root to install:
• a root daemon and systemd units;
• pacman hooks and polkit rules;
• /usr/local/bin shims that shadow flatpak, snap and nix.
It should be reviewed and gated as a privileged system component.
Provenance. Per AGENTS.md, much of the code was written by AI agents (agy, and a local qwen3-coder:30b via aider), with on-host testing still pending. Treat its "verified e2e" claims as unverified until reproduced.
Findings at a glance
There are 18 findings: 5 high, 7 medium and 6 low or informational. Severity measures harm to the promised child-safety boundary. None of them gives an ordinary user root.
#
Severity
Finding
Where
H1
High
Daemon can't create its key under its own unit, so it crash-loops on fresh install
guardiand.service, crypto.rs:67-90
H2
High
Reboot resets the screen-time budget, and the lock never fires after login
schedule.rs, guardiand.rs:113-165, guardian-lock.sh
H3
High
fapolicyd rules still allow ld.so ./binary laundering; the comment wrongly says it's covered
fapolicyd/50-guardian.rules:18-28
H4
High
Omarchy's Limine menu boots older snapshots, and the child knows the disk passphrase
platform; harden-audit.sh
H5
High
Tamper alerts are plain text the parent app silently drops
ntfy.rs:198-207, index.html:160-173
M1
Medium
Child can hold all 64 connection slots, blocking parent control and watchdog alerts
ipc.rs:27-29,111-178,292-296
M2
Medium
Challenge signature omits reason and respond_to
ntfy.rs:274-286, index.html:178-184
M3
Medium
App accepts unsigned cards when no daemon key is paired; #pair= links overwrite config silently
index.html:166-171,239-247
M4
Medium
Requests, reasons and audit entries go to public ntfy.sh in plaintext
ntfy.rs:278-332
M5
Medium
DNS filter likely bypassed through systemd-resolved (nss-resolve)
dns/10-guardian-dns-redirect.nft
M6
Medium
Lock hook can freeze the wrong user; lock-sessions locks everyone
schedule/guardian-lock.sh:9-23
M7
Medium
Screen-time grants can be replayed after a restart or onto a sibling's PC
crypto.rs:32-63, ntfy.rs:148-169
L1
Low
Malformed policy silently falls back to defaults, turning screen time off
config.rs:89-114
L2
Low
Blocklist updater installs any line from upstream, not just sinkholes
dns/guardian-blocklist-update.sh
L3
Low
Wrappers advisory; nix run, nix shell, nix-shell -p and snap try not gated
packaging/{nix,snap}
L4
Low
Browser and NetworkManager policies leave gaps (proxy settings, saved Wi-Fi)
packaging/network/
L5
Low
Daemon key regenerated on any read error; written before chmod
crypto.rs:67-90
L6
Info
Hook children never reaped or checked; harden-audit.sh misses Limine and the docker group; PWA has no CSP
various
High-severity findings
H1. The shipped unit can't start the daemon on a fresh install
guardiand.service sets ProtectSystem=strict, which mounts /etc read-only. It grants writable RuntimeDirectory and LogsDirectory but no writable config or state path.
On first start, load_or_create_daemon_key must write /etc/guardian/daemon.key, and install.sh never pre-creates it. The write fails and main returns an error through ? (guardiand.rs:67). Restart=always with RestartSec=2 then crash-loops into systemd's start limit.
Knock-on effects:
• Installs fail closed, so the parent can't install anything.
• Screen time, ntfy and the audit log never run.
• Watchdog alerts fall back to logger, because they travel over the dead daemon's socket.
• daemon.pub is never written, so guardian-ctl pair emits a QR with no dpub. The app then accepts unsigned cards (see M3).
This regression came with the post-review daemon-key feature. Fix: use StateDirectory=guardian and move the key under /var/lib/guardian, or generate it in install.sh. Also fail loudly if the key can't be persisted.
H2. A reboot defeats screen time
Usage is kept only in memory (TimeEngine, schedule.rs:65-83), so every boot or daemon restart resets the budget to full.
The lock also fires only on the transition into blocked (guardiand.rs:146-160). At boot, the first tick runs before anyone logs in. guardian-lock.sh then finds no session (or, without GUARDIAN_CHILD, may pick a display-manager greeter), and freezing a not-yet-existing user-<uid>.slice does nothing. When the child logs in later, nothing re-locks them until the status changes again, which can be the next day.
Outside the allowed window, "reboot, then log in" is therefore a complete bypass. Mid-session, the power button gets the child to the same place.
Aggravating factors:
• install.sh doesn't install guardian-lock or guardian-unlock, and run_hook never checks the exit status. A missing script fails silently.
• The parent app has no grant-time UI. Today the only way to issue a grant is the Node CLI with a raw private key.
Fix:
• Persist usage per day.
• Re-assert the block on every tick while blocked, not only on transitions.
• Deny login while blocked, at the PAM/logind level.
• Run the lock from a logind session-new hook.
H3. fapolicyd still allows loader laundering
The rules say rule 2 stops ld-linux.so <untrusted-elf>. It doesn't:
• The kernel execs ld.so, which is trusted.
• ld.so then open()s and mmaps the target. fapolicyd sees perm=open, not perm=execute.
• A PIE or static executable isn't typed application/x-sharedlib, so rule 3 doesn't match.
• The final allow perm=any all : all lets it through.
Upstream fapolicyd ships a pattern=ld_so rule for exactly this case; it isn't included here. Executing from a memfd via a trusted interpreter (e.g. Python's os.memfd_create) is a second likely gap. Set vm.memfd_noexec=2 and test it.
This matters because the project calls L2 the backstop for every wrapper bypass. Confidence is medium-high from the rule semantics; it needs an on-host test.
H4. Omarchy's boot menu offers a pre-Guardian system
Omarchy 4 boots through Limine. Its boot menu lists Btrfs snapshots you can boot directly, and the user types the disk password on every boot (StarryHope, Omarchy 4.0.4).
A child who boots their own PC therefore knows the LUKS passphrase. They can pick any snapshot taken before Guardian, nftables or fapolicyd were installed, and get a session with none of them.
The threat model's L6 doesn't hold on stock Omarchy. LUKS stops a stolen disk, not the child. harden-audit.sh checks only systemd-boot and GRUB, and it passes on any crypt device.
Fix:
• Remove or prune pre-Guardian snapshot entries.
• Password-protect or hide the Limine menu and editor.
• Use TPM-bound unlock if the child mustn't know the passphrase.
• Add a Limine check to the audit script.
H5. Tamper alerts never reach the parent app
Alerts (forged approval, blocked control attempt, watchdog repair) are published to request_topic as plain ntfy text (ntfy.rs:198-207). The PWA only renders JSON challenges with id, nonce and respond_to, and silently returns on anything else (index.html:160-173).
A parent using only the app never sees an L7 alert. Nothing consumes the off-box audit heartbeat either. Alerts are also unsigned, so anyone holding the topic can forge or spam them. And one cheap probe uses up the 15-minute per-kind cooldown, muting a real alert of that kind behind it.
Fix:
• Sign alerts with the daemon key.
• Render them in the app.
• Add heartbeat-gap alarms.
• Rate-limit per source, not per kind.
Medium findings
M1. Connection starvation silences the parent. One 64-slot semaphore is shared by both sockets (ipc.rs:111). On the submit socket:
• a non-Submit line gets an error, but the loop continues (ipc.rs:292-296);
• the 30 s read timeout resets on every line;
• the 64 KiB per-connection cap allows thousands of short keep-alive lines.
An unprivileged child can therefore hold all 64 slots for hours. Once they're held, the parent's guardian-ctl and the watchdog's alert are dropped at accept (ipc.rs:147-152), and the watchdog falls back to syslog. Fix: give each socket its own semaphore, add a per-uid connection cap at accept, close after the first invalid message, and use a short first-line timeout.
M2. Signed cards leave reason and respond_to unsigned. The daemon signs id|source|package|nonce only (crypto.rs:28-30). A broker the threat model treats as untrusted can do two things to a genuine card:
• rewrite the child's "reason" text, e.g. "teacher says it's required";
• point respond_to at its own URL.
The parent's signed decision is still bound to the real package, so no swap is possible. But the attacker captures the decision and can drop it, and the app still shows "Allowed ✓". The earlier review asked to constrain responses to the paired server; that isn't done.
M3. Pairing is optional and unauthenticated.
• Without a stored dpub, the app shows any card from the topic (index.html:166-171). H1 makes this the default.
• Opening any #pair= link silently overwrites the server, topic and dpub, with no confirmation (index.html:239-247).
A child who gets a parent to tap a link can point the app at their own server, and real requests and alerts stop arriving. Fix: require dpub, and confirm before re-pairing.
M4. Child activity is published in plaintext to a public broker. Every request is posted to ntfy.sh unencrypted, including the free-text reason and uid. So is every audit entry (installs, schedule blocks, grants) when audit_topic is set. Signing isn't encryption, and anyone who learns a topic can read it.
This contradicts the README's "no shipping your child's data to a corporate cloud". Fix: default to self-hosted ntfy with auth and ACLs, and encrypt payloads to the parent's key.
M5. DNS filtering probably doesn't apply to most apps. The redirect only rewrites port-53 packets the child's own processes send. Arch's default nsswitch.conf resolves hosts through nss-resolve, so when systemd-resolved is running, lookups are made by the systemd-resolve user, not the child. Neither the redirect nor the egress DNS rule then applies, and browsers resolve unfiltered.
This is unconfirmed for Omarchy 4. Test as the child: getent hosts <a-blocked-domain> should return 0.0.0.0.
M6. The lock hook can hit the wrong person.
• Without GUARDIAN_CHILD, it freezes the first non-root session's slice, which may be the parent or a greeter.
• loginctl lock-sessions locks every session.
• The "lock" itself is the child's own screen locker, so the freeze is the only real stop.
M7. Grants can be replayed. The grant message is OCPG-GRANT-v1|nonce|minutes|ts. It isn't bound to the daemon or to a daemon-issued challenge, and the used-nonce set lives only in memory.
Anyone holding the topic can capture a grant from the public response topic. They can replay it within the 10-minute window after a daemon restart, or onto a sibling's PC paired with the same parent key. Fix: bind grants to the daemon pubkey plus a daemon-issued nonce, and persist used nonces.
Low and informational
• L1. An unreadable or invalid policy.toml silently falls back to defaults (config.rs:89-114). Remote approval, screen time and the audit mirror all turn off, with only a stderr line. There's no deny_unknown_fields, so a typoed key is ignored. The install gate stays fail-closed.
• L2. The blocklist updater only counts 0.0.0.0 lines and installs the whole file. If the upstream list were compromised, a line like 1.2.3.4 accounts.google.com would hijack the child's DNS. It should keep sinkhole lines only. pkill -HUP -x dnsmasq also signals every dnsmasq instance on the host.
• L3. The flatpak, snap and nix shims are bypassed by calling /usr/bin/* directly; the project already acknowledges this. They also miss nix run, nix shell, nix-shell -p and snap try, which run software without an "install".
• L4. The Firefox and Chromium policies lock DoH but not proxy settings, so a configured HTTPS proxy on 443 bypasses DNS filtering. Omarchy 4 moved to NetworkManager, so the NM polkit rule now applies. But it also blocks network-control, which a child needs to join saved Wi-Fi, contrary to its comment.
• L5. load_or_create_daemon_key treats any read error as "missing" and overwrites the key. It also writes the seed before chmod 0600.
• L6. Remaining informational items:
    ◦ run_hook never reaps or logs the exit status of sh -c children.
    ◦ harden-audit.sh doesn't detect Limine, and its group check covers only wheel and sudo (not docker or libvirt).
    ◦ The PWA has no CSP, and it's served from the author's GitHub Pages: any push changes the code holding the parent's signing key.
    ◦ The vendored noble-ed25519.js has no recorded version or hash.
Earlier review: what the fixes actually did
All seven fixes from the October 9 review are present in source. Two are incomplete and one introduced a regression.
Oct 9 finding
Claimed
In source today
1. AbortOnFail missing
Fixed
Present in 50- and 99- hooks; real pacman run still untested
2. fapolicyd subject vs object trust
Fixed
Object trust fixed; loader and memfd laundering still open (H3)
3. Token mode default; policy 0644
Fixed
Token mode opt-in; policy written 0600
4. Signature didn't bind package
Fixed
Binds source and package; challenge signed but omits reason and respond_to, and is optional (M2, M3)
5. Socket env honored; PATH lookup
Fixed
Release builds ignore env; wrappers use absolute paths
6. Audit log blocked; /run 0750
Fixed
LogsDirectory and 0755 added; the new daemon-key write now fails the same way (H1)
7. Unbounded connections and reads
Fixed
Caps added, but slots can be held indefinitely (M1)
Audit truncation
Mitigated
.head anchor and missing-log alert added; the off-box mirror has no consumer (H5)
Before any acceptance: tests on a real Omarchy 4 machine
Run these on a disposable machine with a separate child account, never on a family PC. Each one checks a finding above.
[ ] Fresh install.sh, then systemctl status guardiand: does it stay up, and does /etc/guardian/daemon.key exist? (H1)
[ ] Exhaust the budget, reboot, log in as the child: is the session locked? Repeat outside the window. (H2)
[ ] As the child, with fapolicyd enforcing, run /usr/lib/ld-linux-x86-64.so.2 ~/Downloads/<elf> and a memfd exec from Python: are both denied? (H3)
[ ] At the Limine menu, boot a snapshot older than the Guardian install: are nftables, fapolicyd and the hooks absent? (H4)
[ ] Trigger a forged approval: does the parent app show anything? (H5)
[ ] As the child, open 64 idle connections to submit.sock, then run sudo guardian-ctl list as the parent: is it dropped? (M1)
[ ] As the child, run getent hosts on a blocked domain and load it in the browser: is it sinkholed? (M5)
[ ] Run a real pacman -S with a denial, a timeout and the daemon down: does the transaction abort each time? (Oct 9 #1)
Method and limits
The repo was cloned read-only, shallow, at e8de6b3, and every file was read. Nothing from it was built, installed or run. A cargo test attempt failed at dependency download, before any code compiled, and was not retried by choice.
Findings are therefore source-level:
• Confirmed in code: H1, H2, H5, M1–M4, M6 and M7.
• Platform-dependent, need the tests above: H3, H4 and M5.
• Not checked: dependency advisories and the vendored crypto library's provenance.
Omarchy 4 details (Limine snapshot menu, disk password at boot, NetworkManager switch, plugin system) come from the sources below. They weren't verified on a machine.
Sources
• Repository, commit e8de6b3
• Omarchy 4.0.0 release notes (newreleases.io)
• Omarchy snapshots and Limine rollback (StarryHope, for 4.0.4)
• In-repo: guardian-security-review.md (Oct 9 review) and docs/REVIEW-RESPONSE.md