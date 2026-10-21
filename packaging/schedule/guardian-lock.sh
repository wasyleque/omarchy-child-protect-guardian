#!/usr/bin/env bash
# Host-side screen-time LOCK hook. guardiand runs this (as root) when the child's time is up, via
# [schedule].lock_command = "GUARDIAN_CHILD=<child> /usr/lib/guardian/guardian-lock". Locks ONLY the
# child's sessions (never the parent/greeter — review M6) and freezes the child's user slice as a hard
# stop. Needs a live graphical session — verify on the host (no-op in a headless sandbox).
set -u

# GUARDIAN_CHILD is REQUIRED: we never guess, to avoid locking the parent or a greeter. It may be a
# SPACE-SEPARATED LIST of children (setup-child.sh maintains it) — we lock each named account only.
if [ -z "${GUARDIAN_CHILD:-}" ]; then
  logger -t guardian-lock "GUARDIAN_CHILD not set — refusing to guess which user to lock"
  exit 0
fi

for child in $GUARDIAN_CHILD; do
  uid="$(id -u "$child" 2>/dev/null || true)"
  [ -n "$uid" ] || { logger -t guardian-lock "unknown user $child"; continue; }

  logger -t guardian-lock "locking screen-time for $child (uid $uid)"
  # Lock only this child's own sessions.
  loginctl list-sessions --no-legend 2>/dev/null | awk -v u="$uid" '$2==u{print $1}' | while read -r sid; do
    loginctl lock-session "$sid" 2>/dev/null || true
  done
  # Warn in the child's session (best-effort).
  sudo -u "#$uid" --preserve-env=XDG_RUNTIME_DIR notify-send \
    "Czas przed komputerem minął" "Poproś rodzica o więcej czasu." 2>/dev/null || true
  # Hard stop: freeze the child's processes (idempotent; the unlock hook thaws them after a grant).
  systemctl freeze "user-$uid.slice" 2>/dev/null || true
done
exit 0
