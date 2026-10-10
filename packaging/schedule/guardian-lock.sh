#!/usr/bin/env bash
# Host-side screen-time LOCK hook. guardiand runs this (as root) when the child's time is up, via
# [schedule].lock_command = /usr/lib/guardian/guardian-lock. Best-effort: warn, lock the session
# (hyprlock on Wayland), and freeze the user slice as a hard stop. Needs a live graphical session —
# verify on the host (it is a no-op in a headless sandbox).
set -u

# Which user to lock: $GUARDIAN_CHILD if set, else the first non-root login session.
uid=""
if [ -n "${GUARDIAN_CHILD:-}" ]; then uid="$(id -u "$GUARDIAN_CHILD" 2>/dev/null || true)"; fi
if [ -z "$uid" ]; then
  uid="$(loginctl list-sessions --no-legend 2>/dev/null | awk '$3!="root"{print $2; exit}')"
fi
[ -n "$uid" ] || { logger -t guardian-lock "no child session to lock"; exit 0; }

logger -t guardian-lock "locking screen-time for uid $uid"
# Warn in the child's session (best-effort; needs their bus — ignore failures).
sudo -u "#$uid" --preserve-env=XDG_RUNTIME_DIR notify-send \
  "Czas przed komputerem minął" "Poproś rodzica o więcej czasu." 2>/dev/null || true
# Lock the graphical session (Wayland → hyprlock).
loginctl lock-sessions 2>/dev/null || true
# Hard stop: freeze all the child's processes (resumed by the unlock hook after a grant).
systemctl freeze "user-$uid.slice" 2>/dev/null || true
exit 0
