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

mkdir -p /run/guardian 2>/dev/null || true

for child in $GUARDIAN_CHILD; do
  uid="$(id -u "$child" 2>/dev/null || true)"
  [ -n "$uid" ] || { logger -t guardian-lock "unknown user $child"; continue; }

  stamp="/run/guardian/locked-$uid"
  first=0; [ -e "$stamp" ] || { first=1; : > "$stamp"; }

  # Only log + notify on the FIRST tick of a lock episode — guardiand may call this every tick while
  # blocked, which otherwise spammed the journal and the (frozen) session every 10s (review F17).
  if [ "$first" -eq 1 ]; then
    logger -t guardian-lock "locking screen-time for $child (uid $uid)"
    loginctl list-sessions --no-legend 2>/dev/null | awk -v u="$uid" '$2==u{print $1}' | while read -r sid; do
      loginctl lock-session "$sid" 2>/dev/null || true
    done
    sudo -u "#$uid" --preserve-env=XDG_RUNTIME_DIR notify-send \
      "Czas przed komputerem minął" "Poproś rodzica o więcej czasu." 2>/dev/null || true
  fi

  # Hard stop (idempotent; the unlock hook reverses both after a grant):
  #  1) freeze the child's graphical user slice;
  #  2) STOP *every* process of the child's uid regardless of cgroup — a tty/console login lands in
  #     getty@ttyN.service, NOT user-<uid>.slice, so freezing the slice alone misses it and the child
  #     keeps a working shell with network (review F18, confirmed live). F13 (no consoles) is the
  #     primary fix; this is the backstop.
  systemctl freeze "user-$uid.slice" 2>/dev/null || true
  pkill -STOP -u "$uid" 2>/dev/null || true
done
exit 0
