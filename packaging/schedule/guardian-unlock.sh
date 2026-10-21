#!/usr/bin/env bash
# Host-side screen-time UNLOCK hook (thaw the child's slice after a parent grant). Run by guardiand via
# [schedule].unlock_command = "GUARDIAN_CHILD=<child> /usr/lib/guardian/guardian-unlock".
set -u

if [ -z "${GUARDIAN_CHILD:-}" ]; then
  logger -t guardian-unlock "GUARDIAN_CHILD not set"
  exit 0
fi
# GUARDIAN_CHILD may be a space-separated list of children — thaw each.
for child in $GUARDIAN_CHILD; do
  uid="$(id -u "$child" 2>/dev/null || true)"
  [ -n "$uid" ] || continue
  logger -t guardian-unlock "thawing screen-time for $child (uid $uid)"
  systemctl thaw "user-$uid.slice" 2>/dev/null || true
done
exit 0
