#!/usr/bin/env bash
# Host-side screen-time UNLOCK hook (thaw the child's slice after a parent grant). Run by guardiand via
# [schedule].unlock_command = "GUARDIAN_CHILD=<child> /usr/lib/guardian/guardian-unlock".
set -u

if [ -z "${GUARDIAN_CHILD:-}" ]; then
  logger -t guardian-unlock "GUARDIAN_CHILD not set"
  exit 0
fi
uid="$(id -u "$GUARDIAN_CHILD" 2>/dev/null || true)"
[ -n "$uid" ] || exit 0

logger -t guardian-unlock "thawing screen-time for $GUARDIAN_CHILD (uid $uid)"
systemctl thaw "user-$uid.slice" 2>/dev/null || true
exit 0
