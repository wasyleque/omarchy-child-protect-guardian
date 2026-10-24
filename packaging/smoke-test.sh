#!/usr/bin/env bash
# Guardian smoke test — a NON-DESTRUCTIVE automated check of a running install. Run on the host:
#   sudo packaging/smoke-test.sh [child-username]
# It installs NOTHING and changes no policy; it exercises the local approval loop via guardian-ctl,
# checks the sockets/schedule/audit, and probes the network + AI layers. Pass/Warn/Fail summary at the
# end — paste the output back. Safe to run on the child's machine.
set -u
CHILD="${1:-}"
pass=0; warn=0; fail=0
ok()  { printf '  \033[32m[PASS]\033[0m %s\n' "$1"; pass=$((pass+1)); }
nok() { printf '  \033[31m[FAIL]\033[0m %s\n' "$1"; fail=$((fail+1)); }
meh() { printf '  \033[33m[WARN]\033[0m %s\n' "$1"; warn=$((warn+1)); }
hdr() { printf '\n== %s ==\n' "$1"; }

[ "$(id -u)" -eq 0 ] || echo "(not root — some checks will be limited; prefer: sudo $0 $*)"
CTL="$(command -v guardian-ctl || echo /usr/bin/guardian-ctl)"

hdr "Daemon + sockets"
if systemctl is-active --quiet guardiand 2>/dev/null; then ok "guardiand.service active"; else nok "guardiand.service not active"; fi
for s in /run/guardian/guardian.sock /run/guardian/submit.sock; do
  if [ -S "$s" ]; then ok "socket present: $s ($(stat -c '%a' "$s" 2>/dev/null))"; else nok "socket missing: $s"; fi
done
if [ -r /var/lib/guardian/daemon.pub ]; then ok "daemon.pub readable (F8)"; else meh "daemon.pub not readable here"; fi

hdr "Local approval loop (no phone, no install)"
if [ -x "$CTL" ]; then
  # ALLOW: submit a dummy request in the background, find it, allow it, expect ALLOW on stdout.
  "$CTL" request pacman guardian-smoke-allow >/tmp/gsmoke_a.out 2>&1 &
  RP=$!; sleep 1
  ID="$("$CTL" list 2>/dev/null | grep -iF guardian-smoke-allow | grep -oE '[0-9a-f-]{36}' | head -1)"
  if [ -n "$ID" ]; then
    "$CTL" allow "$ID" >/dev/null 2>&1
    wait "$RP" 2>/dev/null || true
    if grep -qx ALLOW /tmp/gsmoke_a.out 2>/dev/null; then ok "approval loop: submit → list → ALLOW → request returned ALLOW"; else nok "ALLOW path: request did not return ALLOW"; fi
  else nok "approval loop: submitted request not found via 'list'"; kill "$RP" 2>/dev/null; fi

  # DENY: expect the request to print DENY on stdout (fail-closed).
  "$CTL" request pacman guardian-smoke-deny >/tmp/gsmoke_d.out 2>&1 &
  RP=$!; sleep 1
  ID="$("$CTL" list 2>/dev/null | grep -iF guardian-smoke-deny | grep -oE '[0-9a-f-]{36}' | head -1)"
  if [ -n "$ID" ]; then
    "$CTL" deny "$ID" >/dev/null 2>&1
    wait "$RP" 2>/dev/null || true
    if grep -qx DENY /tmp/gsmoke_d.out 2>/dev/null; then ok "approval loop: DENY → request returned DENY (blocked)"; else nok "DENY path: request did not return DENY (should fail-closed)"; fi
  else meh "DENY path: request not found (skipped)"; kill "$RP" 2>/dev/null; fi
else
  nok "guardian-ctl not found"
fi

hdr "Screen-time"
if [ -x "$CTL" ]; then
  ST="$("$CTL" schedule 2>/dev/null || true)"
  if [ -n "$ST" ]; then ok "schedule status: $ST"; else meh "no schedule configured / no status"; fi
fi

hdr "Audit chain"
AUD=/var/log/guardian/audit.log
if [ -f "$AUD" ] && [ -x "$CTL" ]; then
  if "$CTL" audit-verify "$AUD" >/tmp/gsmoke_av.out 2>&1; then ok "audit chain verifies ($(wc -l <"$AUD") entries)"; else nok "audit-verify FAILED: $(tail -1 /tmp/gsmoke_av.out)"; fi
else meh "no audit log at $AUD"; fi

hdr "Network egress (nftables)"
if command -v nft >/dev/null 2>&1; then
  if nft list table inet guardian >/dev/null 2>&1; then ok "nftables 'guardian' table loaded"; else meh "nftables 'guardian' table not loaded"; fi
  if nft list table ip guardian_dns >/dev/null 2>&1; then ok "per-child DNS redirect (guardian_dns) loaded"; else meh "DNS redirect table not loaded"; fi
  if systemctl is-enabled --quiet guardian-nftables.service 2>/dev/null; then ok "guardian-nftables.service enabled (rules persist on reboot)"; else meh "guardian-nftables.service not enabled — nft rules are LOST on reboot"; fi
else meh "nft not installed"; fi

hdr "DNS content filter (porn/SafeSearch)"
if systemctl is-active --quiet guardian-dnsmasq 2>/dev/null; then ok "guardian-dnsmasq filter running (:5353)"; else meh "guardian-dnsmasq not running — category/SafeSearch filtering is OFF"; fi
# The real test: does a known adult domain resolve to a sinkhole FOR THE CHILD? (catches the M5
# systemd-resolved trap, where the filter runs but the child's lookups bypass it.)
if [ -n "$CHILD" ] && id "$CHILD" >/dev/null 2>&1 && command -v getent >/dev/null 2>&1; then
  RES="$(sudo -u "$CHILD" getent hosts pornhub.com 2>/dev/null | awk '{print $1; exit}')"
  case "$RES" in
    0.0.0.0|127.0.0.1|::|"") ok "adult domain sinkholed for $CHILD (pornhub.com → ${RES:-NXDOMAIN})" ;;
    *) nok "adult domain RESOLVES for $CHILD (pornhub.com → $RES) — DNS filter not effective (systemd-resolved/M5?)" ;;
  esac
else meh "DNS filter effectiveness: pass a child username to test (getent as the child)"; fi

hdr "Execution allowlist (fapolicyd)"
if command -v fapolicyd >/dev/null 2>&1; then
  if systemctl is-active --quiet fapolicyd 2>/dev/null; then
    ok "fapolicyd active (enforcing)"
    if [ -n "$CHILD" ] && id "$CHILD" >/dev/null 2>&1; then
      printf '#!/bin/sh\necho hi\n' > "/tmp/gsmoke_evil"; chmod +x /tmp/gsmoke_evil 2>/dev/null
      if sudo -u "$CHILD" /tmp/gsmoke_evil >/dev/null 2>&1; then nok "child could RUN an untrusted script (/tmp) — fapolicyd not blocking!"; else ok "child blocked from running an untrusted /tmp binary"; fi
      rm -f /tmp/gsmoke_evil
    else meh "pass the child username to test an untrusted-exec block"; fi
    grep -q 'allow_filesystem_mark *= *1' /etc/fapolicyd/fapolicyd.conf 2>/dev/null && ok "allow_filesystem_mark=1 (C1 closed)" || nok "allow_filesystem_mark not =1 (C1 bypass open!)"
  else meh "fapolicyd installed but not active (permissive/off)"; fi
else meh "fapolicyd not installed (REQUIRED for real enforcement)"; fi

hdr "AI / Ollama"
POLICY=/etc/guardian/policy.toml
EP="$(sed -nE 's/^[[:space:]]*endpoint[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$POLICY" 2>/dev/null | head -1)"
if [ -n "$EP" ]; then
  BASE="${EP%/api/*}"; BASE="${BASE%/v1/*}"
  if curl -s --max-time 6 "$BASE/api/tags" >/tmp/gsmoke_ai.out 2>&1; then
    MODELS="$(grep -oE '"name":"[^"]+"' /tmp/gsmoke_ai.out | sed 's/"name":"//;s/"//' | paste -sd, -)"
    ok "Ollama reachable at $BASE — models: ${MODELS:-?}"
  else meh "AI endpoint $BASE not reachable (digest/risk-summary will be skipped)"; fi
else meh "no [ai] endpoint in policy.toml"; fi

hdr "Activity reporter"
if [ -n "$CHILD" ]; then
  AL="$(getent passwd "$CHILD" | cut -d: -f6)/.local/share/guardian/activity.log"
  if [ -f "$AL" ]; then
    AGE=$(( $(date +%s) - $(stat -c %Y "$AL" 2>/dev/null || echo 0) ))
    [ "$AGE" -lt 300 ] && ok "activity log fresh ($AL, ${AGE}s old)" || meh "activity log stale ($AL, ${AGE}s old — reporter running?)"
  else meh "no activity log for $CHILD (reporter not running?)"; fi
fi

hdr "Summary"
printf '  %d pass, %d warn, %d fail\n' "$pass" "$warn" "$fail"
[ "$fail" -eq 0 ] && printf '  \033[32mNo hard failures.\033[0m\n' || printf '  \033[31m%d FAIL(s) — see above.\033[0m\n' "$fail"
