# Response to the external security review (2026-10-09)

An external reviewer assessed commit `bdd62cf` and recommended **not relying on that version to protect
a child's everyday computer**. We agree with that assessment for that commit, and we thank the reviewer
— the findings were correct and sharp. This document tracks each finding and what changed. The full
review is in [`../guardian-security-review.md`](../guardian-security-review.md).

Status legend: **FIXED** (changed + re-verified here) · **MITIGATED** (materially improved, residual
noted) · **ACKNOWLEDGED** (real limitation, documented, not yet solved).

## Findings

| # | Sev | Finding | Status | What changed / note |
|---|-----|---------|--------|---------------------|
| 1 | Critical | PreTransaction hooks lacked `AbortOnFail`, so a denial/timeout did **not** abort the pacman transaction | **FIXED** | Added `AbortOnFail` to `50-guardian.hook` and `99-guardian-test.hook`. Must still be verified through real pacman on-host (the review's reproduction method). I had tested the hook *binary*, never the pacman wiring — that was the gap. |
| 2 | High | fapolicyd rule asserted trust on the **subject** (calling process), not the **object** (file run) | **FIXED** | Rewrote `fapolicyd/50-guardian.rules` to `deny_audit perm=execute uid=CHILD : all trust=0` + explicit shared-lib/loader denies. Cannot run fapolicyd here — **must be validated on-host in permissive mode** with real downloaded ELF/AppImage/scripts. |
| 3 | High | Token mode was the silent default and leaked the secret through the public broker; policy file 0644 | **FIXED** | Token mode now requires explicit `allow_insecure_token = true`; otherwise remote approval is **refused and disabled** (local-only) with a loud message. Installer writes `/etc/guardian/policy.toml` as **0600**. Verified e2e. |
| 4 | High | Signature omitted package/source; a broker could show a benign card yet get a valid allow for the real request | **FIXED** | Canonical message bumped to `OCPG-v2`, now binds `source` + `package`; the daemon rejects a decision whose signed source/package ≠ the held request (alert raised). Verified e2e: swapped package → `signed …/calculator ≠ requested …/steam` → DENY. **Residual now closed:** the daemon has its own Ed25519 key and **signs every challenge** (`OCPG-CH-v1`, field `csig`); the app verifies it with the daemon's public key (carried in the pairing QR as `dpub`) and shows ONLY challenges the real daemon issued — fake/injected cards are rejected. Verified e2e with the app's noble library. |
| 5 | High | `guardian-ctl request` honored `GUARDIAN_SUBMIT_SOCKET` in release; wrappers resolved `guardian-ctl` via PATH | **FIXED** | `guardian-ctl` ignores the socket env vars in release builds (debug-only, for tests); wrappers call the absolute `/usr/bin/guardian-ctl`. Verified: release ignores a planted fake socket, debug honors it. **Honest limit:** user-side wrappers for `--user` installs remain advisory; fapolicyd (finding 2) is the real backstop. |
| 6 | Medium | `ProtectSystem=strict` blocked the audit log; `/run/guardian` 0750 blocked the child reaching `submit.sock` | **FIXED** | Unit now declares `LogsDirectory=guardian` and `RuntimeDirectoryMode=0755` (control socket stays 0660 + SO_PEERCRED). Needs an on-host start to confirm. |
| 7 | Medium | Unbounded connections / line length / read time; unbounded broker buffer | **FIXED** | Caps added: 64 concurrent connections (semaphore), 64 KiB per connection (`take`), 30 s read timeout, 1 MiB ntfy stream line cap. |

## Child-safety gaps (reviewer) — our honest position

- **Firewall allows all 80/443; DoH blocklist is partial.** ACKNOWLEDGED. DNS is forced + DoT/known-DoH/UDP-VPN blocked, but an arbitrary HTTPS proxy/tunnel on 443 still needs **SNI/DPI allowlisting** (planned). Broad "bypass-proof" wording has been removed.
- **No filtering DNS resolver, category blocklists, SafeSearch, or screen-time schedules.** ACKNOWLEDGED — these are **concept/roadmap items, not implemented**. The README now marks them as planned, not delivered.
- **AUR/approved scriptlets run as root; immutability + root watchdog can't defeat arbitrary root code.** ACKNOWLEDGED. The watchdog *detects & alerts* (not prevents). Intent-Binding (approve a specific artifact digest, flag scriptlet-carrying packages) is planned.
- **Audit is unkeyed with no off-box chain head; truncation/rewrite can pass; watchdog skipped a missing log.** MITIGATED: added a `.head` anchor (catches naive truncation/emptying) and the watchdog now **alerts if the log is missing**. Still tamper-*evident*, not tamper-*proof* against root; real durability needs **off-box shipping / an HMAC key the attacker can't read** (planned). Against the child (non-root) the log is unwritable anyway (root-owned 0640).
- **Parent seed is extractable from IndexedDB; hosted app is in the trust boundary.** ACKNOWLEDGED (already in parent-app docs). OS-keystore / non-extractable key is future hardening.
- **Installer leaves major layers manual, starts in `sl`-only mode, no separate child account; audit script is superficial.** ACKNOWLEDGED by design: the installer is deliberately safe-by-default and prints host-specific steps; `harden-audit.sh` is a **checklist, not proof** (e.g. it only detects a crypt device). The child account must be created by the admin.

## Overall

This remains an **experimental prototype**. The high-severity enforcement bugs (1, 3, 4, 5) are fixed and
re-verified here; two (1, 2, 6) still require on-host validation that this sandbox can't perform
(no destructive pacman run, no fapolicyd, no systemd unit start). Do not use it as the sole protection
for a child's everyday machine yet. See the HP-laptop guidance at the end of the review.
