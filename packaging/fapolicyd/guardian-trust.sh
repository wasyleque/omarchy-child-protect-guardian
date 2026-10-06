#!/usr/bin/env bash
# guardian-trust — PostTransaction pacman hook helper.
# Install to /usr/lib/guardian/guardian-trust (chmod 755).
#
# Rationale: the PreTransaction hook (guardian-hook) already blocked every install the parent did
# NOT approve, so any package that reaches a PostTransaction *Install* was approved. We add exactly
# those packages' files to the fapolicyd trust database, so the approved software can execute — and
# nothing the child downloads on their own can. This keeps the execution allowlist (L2) in sync with
# approvals automatically, with no daemon involvement. Package names arrive on stdin (NeedsTargets).

set -u

if ! command -v fapolicyd-cli >/dev/null 2>&1; then
  echo "guardian-trust: fapolicyd not installed — nothing to do" >&2
  exit 0
fi

added=0
while IFS= read -r pkg; do
  [ -n "$pkg" ] || continue
  # `pacman -Ql <pkg>` prints lines "<pkg> <path>"; trust the regular files that now exist.
  while read -r _name path; do
    [ -f "$path" ] || continue
    if fapolicyd-cli --file add "$path" >/dev/null 2>&1; then
      added=$((added + 1))
    fi
  done < <(pacman -Ql "$pkg" 2>/dev/null)
done

if [ "$added" -gt 0 ]; then
  fapolicyd-cli --update >/dev/null 2>&1 || true
  echo "guardian-trust: trusted $added file(s) from approved install(s)" >&2
fi
exit 0
