# Response to the second external security review (2026-10-14, commit e8de6b3)

A second reviewer (18 findings: 5 high, 7 medium, 6 low/info) again concluded **don't accept this as a
kids' protection plugin yet**. We agree, and the findings were excellent. This tracks each one.

Status: **FIXED** (changed + re-verified here) · **MITIGATED** (materially improved, residual noted) ·
**HOST** (needs the on-host test to confirm) · **DOC** (documented limitation / ops guidance).

## High
| # | Finding | Status | What changed |
|---|---------|--------|--------------|
| H1 | Daemon key unwritable under its unit → crash-loop on fresh install | **FIXED** | Key moved under `StateDirectory=guardian` (`/var/lib/guardian`); unit declares it; `daemon.pub` + `guardian-ctl pair` read there. Verified the daemon starts + writes the key in the e2e. |
| H2 | Reboot resets the screen-time budget; lock only fires on transition | **FIXED (core)** | Usage persisted to `schedule_state_path`; the lock is re-asserted on **every tick** while blocked (not just the transition). Persistence verified e2e. HOST: a logind session-new/PAM login-deny is still recommended for the gap between login and the first tick. |
| H3 | fapolicyd still allows `ld.so ./binary` laundering | **FIXED (rule) + HOST** | Added the upstream `pattern=ld_so` deny for the child + a `vm.memfd_noexec=2` sysctl for the memfd path. Can't run fapolicyd in the sandbox — must be confirmed on-host (the review's test). |
| H4 | Omarchy 4's Limine menu boots pre-Guardian snapshots; child knows the LUKS passphrase | **DOC + audit** | `harden-audit.sh` now detects Limine and FAILs with guidance (prune/lock snapshots, password/hide the menu, TPM-bound unlock). This is a genuine platform limit — documented in THREAT_MODEL; software alone can't close it. |
| H5 | Tamper alerts are plain text the app silently drops | **FIXED** | Alerts are now **signed** by the daemon key and published as structured JSON; the app verifies and renders a red banner, dropping unsigned/spoofed ones. Heartbeat-gap alarm on the app side remains a follow-up. |

## Medium
| # | Finding | Status | What changed |
|---|---------|--------|--------------|
| M1 | Child can hold all 64 connection slots, starving parent/watchdog | **FIXED** | Per-socket semaphores, a per-uid connection cap (8), and the connection closes on the first malformed/wrong-socket message. |
| M2 | Challenge signature omits reason + respond_to | **FIXED** | `OCPG-CH-v2` now binds reason + respond_to; verified e2e (genuine verifies, reason swap fails). |
| M3 | App accepts unsigned cards when unpaired; `#pair=` overwrites silently | **FIXED** | The app now requires a paired `dpub` before acting on any card, and a `#pair=` link asks for confirmation before replacing an existing pairing. |
| M4 | Requests/reasons/audit go to public ntfy.sh in plaintext | **DOC + MITIGATED** | README privacy claim corrected; we now recommend a **self-hosted ntfy with auth/ACLs** as the default and document that signing ≠ encryption. Payload encryption to the parent key is the planned fix. |
| M5 | nss-resolve likely bypasses the DNS redirect | **DOC + audit + HOST** | `harden-audit.sh` warns when `systemd-resolved` is active; dns/README documents disabling it or pointing nsswitch `hosts` at `:5353`. Confirm with `getent hosts <blocked>` as the child. |
| M6 | Lock hook can hit the wrong user; `lock-sessions` locks everyone | **FIXED** | Hook requires `GUARDIAN_CHILD` (no guessing) and locks only the child's sessions, then freezes only the child slice. |
| M7 | Grants replayable after restart or onto a sibling PC | **FIXED** | `OCPG-GRANT-v2` binds the target daemon's pubkey, and used nonces are persisted to `grant_nonce_path`. Verified e2e (wrong-daemon grant rejected; nonce persisted). |

## Low / info
- **L1** malformed policy → silent defaults: **FIXED** — guardiand now exits on an unreadable/malformed
  policy, and `deny_unknown_fields` rejects typos.
- **L2** blocklist installs any line: **FIXED** — only sinkhole lines are installed; reload is scoped to the guardian dnsmasq.
- **L3** `nix run/shell`, `snap try` ungated: **FIXED** — wrappers now gate these too.
- **L4** NM blocks saved-Wi-Fi join; browser proxy unlocked: **FIXED** — NM rule no longer blocks
  `network-control`; Firefox/Chromium proxy locked to system.
- **L5** key regen on any read error, chmod after write: **FIXED** — create-only-on-NotFound, 0600 before write.
- **L6** hook children unreaped; audit misses Limine/docker; no CSP: **FIXED** — hooks reaped; audit
  detects Limine + docker/libvirt/lxd groups; PWA has a CSP. (noble-ed25519 version/hash recording still TODO.)

## Still host-only (the review's test list)
H3 (fapolicyd enforce + ld.so/memfd), H4 (boot a pre-Guardian snapshot), M5 (getent as child), and the
Oct-9 #1 real `pacman -S` abort — these need a disposable Omarchy 4 machine with a child account, per
`docs/TESTING.md`.
