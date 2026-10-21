#!/usr/bin/env bash
# pair.sh <request_topic> [server]
#
# One-step pairing: shows the QR, then AUTO-CAPTURES the phone's public key (the app POSTs it back
# after scanning) and finishes pairing — no copy/paste. The parent confirms the key fingerprint
# matches what the app shows, which defeats anyone who merely guessed the topic.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ "$(id -u)" -eq 0 ] || { echo "Run as root:  sudo $0 $*" >&2; exit 1; }

TOPIC="${1:-}"
SERVER="${2:-https://ntfy.sh}"
[ -n "$TOPIC" ] || { echo "usage: pair.sh <request_topic> [server]" >&2; exit 2; }
command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }
SERVER="${SERVER%/}"
PAIR_TO="$TOPIC-pair"

echo "==> Show this QR to the parent's phone (Guardian Parent → Pair — scan QR):"
guardian-ctl pair --topic "$TOPIC" --server "$SERVER" || true

echo
echo "==> Waiting for the phone to send its key (scan the QR; the app sends it automatically)…"
KEY=""
for _ in $(seq 1 40); do   # ~2 minutes
  KEY="$(curl -s --max-time 8 "$SERVER/$PAIR_TO/json?poll=1" 2>/dev/null | python3 -c '
import json,sys
key=""
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    try:
        ev=json.loads(line)
        if ev.get("event")!="message": continue
        m=json.loads(ev.get("message","") or "{}")
        if m.get("kind")=="parent_key" and m.get("pubkey"): key=m["pubkey"]
    except Exception: pass
print(key)
' 2>/dev/null || true)"
  [ -n "$KEY" ] && break
  sleep 3
done

if [ -z "$KEY" ]; then
  echo "No key received (timeout). You can still finish manually:" >&2
  echo "  in the app tap 'Copy public key', then:  sudo $HERE/set-parent-key.sh <key>" >&2
  exit 1
fi

FP="$(printf '%s' "$KEY" | sha256sum | cut -c1-16)"
echo
echo "==> Received a parent key:"
echo "    $KEY"
echo "    fingerprint: $FP"
echo "Confirm this is YOUR phone's key (compare with the app's 'Copy public key')."
read -r -p "Pair this key? [y/N] " ans
case "$ans" in
  y|Y|yes|YES) "$HERE/set-parent-key.sh" "$KEY" ;;
  *) echo "Aborted — nothing changed."; exit 1 ;;
esac
