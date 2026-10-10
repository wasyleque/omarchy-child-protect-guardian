#!/usr/bin/env bash
# Host-side screen-time UNLOCK hook. guardiand runs this when a parent grant puts the child back
# inside their budget/window, via [schedule].unlock_command = /usr/lib/guardian/guardian-unlock.
# It thaws the frozen user slice; the child then dismisses the lock screen with their own password.
set -u

uid=""
if [ -n "${GUARDIAN_CHILD:-}" ]; then uid="$(id -u "$GUARDIAN_CHILD" 2>/dev/null || true)"; fi
if [ -z "$uid" ]; then
  uid="$(loginctl list-sessions --no-legend 2>/dev/null | awk '$3!="root"{print $2; exit}')"
fi
[ -n "$uid" ] || exit 0

logger -t guardian-unlock "thawing screen-time for uid $uid"
systemctl thaw "user-$uid.slice" 2>/dev/null || true
exit 0
