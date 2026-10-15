#!/usr/bin/env bash
# Refresh the Guardian DNS category blocklist, then reload the filtering dnsmasq. Keeps the old list
# on any failure (fail-safe). Run by guardian-blocklist-update.timer. Install at
# /usr/lib/guardian/guardian-blocklist-update.
set -euo pipefail

SRC="${1:-https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/porn/hosts}"
DEST="/etc/guardian/dns/blocked.hosts"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

if ! curl -fsSL --max-time 60 "$SRC" -o "$tmp"; then
  echo "guardian-blocklist-update: download failed from $SRC — keeping existing list" >&2
  exit 1
fi

count="$(grep -c '^0\.0\.0\.0 ' "$tmp" || true)"
if [ "${count:-0}" -lt 100 ]; then
  echo "guardian-blocklist-update: downloaded file looks wrong (only ${count:-0} sinkhole lines) — keeping existing list" >&2
  exit 1
fi

mkdir -p "$(dirname "$DEST")"
# Keep ONLY sinkhole lines (review L2): a compromised upstream line like "1.2.3.4 accounts.google.com"
# must never be installed as a DNS override.
clean="$(mktemp)"
grep -E '^(0\.0\.0\.0|127\.0\.0\.1)[[:space:]]' "$tmp" > "$clean" || true
installed="$(wc -l < "$clean")"
chmod 644 "$clean"
mv -f "$clean" "$DEST"
rm -f "$tmp"
trap - EXIT

# Reload ONLY the guardian dnsmasq instance (not every dnsmasq on the host).
systemctl reload guardian-dnsmasq 2>/dev/null \
  || systemctl kill -s HUP guardian-dnsmasq 2>/dev/null \
  || pkill -HUP -x dnsmasq 2>/dev/null || true

echo "guardian-blocklist-update: installed $installed sinkhole entries to $DEST"
