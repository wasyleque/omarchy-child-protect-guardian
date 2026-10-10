# Screen-time enforcement (host-side lock hook)

The daemon tracks the window + daily budget (`[schedule]`), exposes status via `guardian-ctl schedule`,
and applies parent grants (`+X min`) arriving over the signed ntfy channel. When the child transitions
to **blocked** (outside the window or budget used up) the daemon runs `lock_command`; when a grant puts
them back inside, it runs `unlock_command`. These two hooks are the only host-specific piece — they need
a live Hyprland/logind session, so they are verified on the host, not in a sandbox.

## Install

```bash
sudo install -Dm755 guardian-lock.sh   /usr/lib/guardian/guardian-lock
sudo install -Dm755 guardian-unlock.sh /usr/lib/guardian/guardian-unlock
```

Then in `/etc/guardian/policy.toml`:

```toml
[schedule]
enabled = true
daily_budget_minutes = 120
window_start_min = 420     # 07:00
window_end_min   = 1230    # 20:30
lock_command   = "GUARDIAN_CHILD=kid /usr/lib/guardian/guardian-lock"
unlock_command = "GUARDIAN_CHILD=kid /usr/lib/guardian/guardian-unlock"
# tick_secs = 30           # lower only for testing
```

`GUARDIAN_CHILD` names the child account; if omitted the hook targets the first non-root login session.

## What the hooks do

- **lock**: `notify-send` a warning, `loginctl lock-sessions` (hyprlock on Wayland), then
  `systemctl freeze user-<uid>.slice` as a hard stop (processes paused in RAM, no data loss).
- **unlock**: `systemctl thaw user-<uid>.slice`; the child dismisses the lock screen with their password.

## Test (on host, as the child is logged in graphically)

```bash
guardian-ctl schedule                         # allowed (N min remaining today)
# let the budget run out (or set a tiny daily_budget_minutes) → session locks + freezes
# from the parent app, grant +X min → guardiand runs the unlock hook → thaw
```

A grant is a signed command (`guardian-sign`/the app); the daemon verifies it (Ed25519 + one-time
nonce) before unlocking — see docs/ARCHITECTURE and REVIEW-RESPONSE.

## Caveats
- `notify-send` into the child's session is best-effort (needs their D-Bus/`XDG_RUNTIME_DIR`).
- `systemctl freeze` is the reliable stop; the lock screen alone can be worked around less, but freeze
  pauses everything. Pair with the account/boot hardening so the child can't reach another TTY or user.
