#!/usr/bin/env bash
# Omarchy Child Protect Guardian — hardening audit (READ-ONLY).
# Reports whether the mandatory anti-bypass baseline is in place. Changes nothing.
# Run as the normal user; a few checks say "run with sudo for detail". See docs/THREAT_MODEL.md.

set -u
pass=0; warn=0; fail=0
ok()   { printf '  \033[32m[PASS]\033[0m %s\n' "$1"; pass=$((pass+1)); }
nok()  { printf '  \033[31m[FAIL]\033[0m %s\n' "$1"; fail=$((fail+1)); }
meh()  { printf '  \033[33m[WARN]\033[0m %s\n' "$1"; warn=$((warn+1)); }
hdr()  { printf '\n\033[1m%s\033[0m\n' "$1"; }

CHILD="${1:-}"   # optional: child username to check account hygiene for

hdr "Boot & firmware"
if [ -d /sys/firmware/efi ]; then ok "UEFI firmware (not legacy BIOS)"; else meh "Legacy BIOS boot — UEFI features (Secure Boot) unavailable"; fi

if command -v mokutil >/dev/null 2>&1; then
  sb="$(mokutil --sb-state 2>/dev/null || true)"
  case "$sb" in
    *enabled*) ok "Secure Boot enabled" ;;
    *disabled*) nok "Secure Boot DISABLED — a live USB / unsigned kernel can boot" ;;
    *) meh "Secure Boot state unknown ($sb)" ;;
  esac
else
  meh "mokutil not installed — can't read Secure Boot state (verify in firmware)"
fi

if command -v bootctl >/dev/null 2>&1 && bootctl status >/dev/null 2>&1; then
  ok "systemd-boot present — prefer a signed UKI with 'timeout 0' (no editable kernel cmdline)"
elif [ -f /boot/grub/grub.cfg ]; then
  if grep -qE 'password_pbkdf2|--users' /boot/grub/grub.cfg 2>/dev/null; then
    ok "GRUB appears password-protected"
  else
    nok "GRUB present with NO password — child can edit cmdline (init=/bin/sh) → root"
  fi
else
  meh "Bootloader not detected via common paths — verify the kernel cmdline can't be edited at boot"
fi

meh "UEFI administrator (supervisor) password + disabled external boot: VERIFY MANUALLY in firmware (OS can't read this)"

# Omarchy 4 boots via Limine with a snapshot menu; a child who knows the LUKS passphrase can boot a
# pre-Guardian Btrfs snapshot and get a session with NONE of the controls (review H4).
if ls /boot/limine* /boot/EFI/*/limine* >/dev/null 2>&1 || command -v limine >/dev/null 2>&1; then
  nok "Limine detected — prune/lock pre-Guardian Btrfs snapshots, password or hide the Limine menu+editor, and use TPM-bound unlock so the child can't boot an older snapshot"
fi

hdr "Disk encryption"
if lsblk -o TYPE 2>/dev/null | grep -q crypt; then
  ok "LUKS/crypt device present (root/home encrypted → pulled disk & live USB see ciphertext)"
else
  nok "No crypt device found — a pulled disk or live USB can read & edit everything"
fi

hdr "Guardian deployment"
if command -v guardiand >/dev/null 2>&1 || [ -x /usr/bin/guardiand ]; then ok "guardiand binary installed"; else meh "guardiand not in PATH/ /usr/bin (not deployed yet?)"; fi
if systemctl list-unit-files 2>/dev/null | grep -q '^guardiand\.service'; then
  state="$(systemctl is-enabled guardiand 2>/dev/null || true)"
  [ "$state" = enabled ] && ok "guardiand.service enabled" || meh "guardiand.service present but '$state'"
else
  meh "guardiand.service not installed"
fi
[ -f /etc/pacman.d/hooks/50-guardian.hook ] && ok "pacman interception hook installed" || meh "pacman hook (50-guardian.hook) not installed"
if command -v fapolicyd >/dev/null 2>&1; then
  systemctl is-active fapolicyd >/dev/null 2>&1 && ok "fapolicyd active (execution allowlist)" || meh "fapolicyd installed but not active"
else
  nok "fapolicyd not installed — child can run downloaded binaries (biggest practical bypass)"
fi
if command -v nft >/dev/null 2>&1 && nft list table inet guardian >/dev/null 2>&1; then
  ok "nftables 'guardian' egress table loaded"
else
  meh "nftables guardian egress policy not loaded (DoH/DoT/VPN escapes open)"
fi
if systemctl is-enabled --quiet guardian-nftables.service 2>/dev/null; then
  ok "guardian-nftables.service enabled (egress + DNS redirect persist across reboot)"
else
  meh "guardian-nftables.service not enabled — the nft rules are lost on reboot"
fi
# Content filter: the dnsmasq blocklist/SafeSearch resolver must be running AND actually reach the child.
if systemctl is-active --quiet guardian-dnsmasq 2>/dev/null; then
  ok "guardian-dnsmasq content filter running (:5354)"
else
  nok "guardian-dnsmasq not running — porn/category blocklist + SafeSearch are OFF (install.sh --enforce installs it)"
fi
# nss-resolve can make lookups come from the systemd-resolve user, bypassing the child's DNS redirect (M5).
if systemctl is-active --quiet systemd-resolved 2>/dev/null; then
  meh "systemd-resolved active — nss-resolve may bypass the DNS redirect; test 'getent hosts <blocked>' as the child, and disable resolved or set nsswitch hosts to 'dns' pointing at :5354"
fi
# The decisive check: does a known adult domain sinkhole FOR THE CHILD (catches the M5 trap)?
if [ -n "$CHILD" ] && id "$CHILD" >/dev/null 2>&1; then
  R="$(sudo -u "$CHILD" getent ahostsv4 pornhub.com 2>/dev/null | awk '{print $1; exit}')"  # ahostsv4: require the 0.0.0.0 sinkhole (review D12)
  case "$R" in
    0.0.0.0|127.0.0.1|"") ok "adult domain sinkholed for $CHILD (pornhub.com → ${R:-NXDOMAIN})" ;;
    *) nok "adult domain RESOLVES for $CHILD (pornhub.com → $R) — DNS content filter is NOT effective (run guardian-dns-mode.sh)" ;;
  esac
fi

hdr "Account hygiene"
if [ -n "$CHILD" ] && id "$CHILD" >/dev/null 2>&1; then
  groups_c="$(id -nG "$CHILD" 2>/dev/null)"
  echo "$groups_c" | grep -qwE 'wheel|sudo' && nok "child '$CHILD' is in wheel/sudo (can escalate!)" || ok "child '$CHILD' not in wheel/sudo"
  echo "$groups_c" | grep -qwE 'docker|libvirt|lxd|kvm' && nok "child '$CHILD' is in a root-equivalent group (docker/libvirt/lxd) — can escalate" || ok "child '$CHILD' not in docker/libvirt/lxd"
else
  meh "Pass the child's username as arg 1 to audit its groups (e.g. ./harden-audit.sh kid)"
fi
# Text consoles (review F13): check the CONFIGURED NAutoVTs, not currently-running units — getty@ttyN
# is spawned on demand when someone presses Ctrl+Alt+Fn, so "no getty running now" is a false PASS.
nautovts="$(systemctl show systemd-logind -p NAutoVTs --value 2>/dev/null)"
if [ "${nautovts:-6}" = "0" ]; then
  ok "text consoles disabled (NAutoVTs=0) — no tty login surface"
else
  nok "on-demand text consoles enabled (NAutoVTs=${nautovts:-?}) — a tty login (Ctrl+Alt+F3..F6) escapes the screen-time lock (F18); run setup-child.sh or set NAutoVTs=0"
fi

# Display-manager autologin into a non-child (review F12): hands a child that session on power-on.
autologin_bad=""
for conf in /etc/sddm.conf.d/*.conf /etc/sddm.conf /etc/gdm/custom.conf; do
  [ -f "$conf" ] || continue
  au="$(sed -nE 's/^[[:space:]]*(User|AutomaticLogin)[[:space:]]*=[[:space:]]*(.+)/\2/p' "$conf" | head -1)"
  if [ -n "$au" ] && grep -qiE '^\[Autologin\]|Automatic[Ll]ogin|^[[:space:]]*Session[[:space:]]*=' "$conf"; then
    [ -n "$CHILD" ] && [ "$au" = "$CHILD" ] || autologin_bad="$au ($conf)"
  fi
done
if [ -n "$autologin_bad" ]; then
  nok "autologin into a non-child account: $autologin_bad — bypasses every Guardian layer (F12); remove it"
else
  ok "no display-manager autologin into a non-child account"
fi

hdr "Summary"
printf '  %d pass, %d warn, %d fail\n' "$pass" "$warn" "$fail"
if [ "$fail" -gt 0 ]; then
  printf '  \033[31mBaseline NOT met — the software controls can be bypassed until the FAILs are fixed.\033[0m\n'
else
  printf '  \033[32mNo hard failures. Re-check WARN items (especially the firmware ones the OS cannot read).\033[0m\n'
fi
exit 0
