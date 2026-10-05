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

hdr "Account hygiene"
if [ -n "$CHILD" ] && id "$CHILD" >/dev/null 2>&1; then
  groups_c="$(id -nG "$CHILD" 2>/dev/null)"
  echo "$groups_c" | grep -qwE 'wheel|sudo' && nok "child '$CHILD' is in wheel/sudo (can escalate!)" || ok "child '$CHILD' not in wheel/sudo"
else
  meh "Pass the child's username as arg 1 to audit its groups (e.g. ./harden-audit.sh kid)"
fi
for t in tty2 tty3 tty4 tty5 tty6; do :; done
act_getty="$(systemctl list-units --type=service 2>/dev/null | grep -c 'getty@tty[2-9]')"
[ "${act_getty:-0}" -gt 0 ] && meh "spare getty on tty2+ active — consider masking (VT login surface)" || ok "no spare getty on tty2+ active"

hdr "Summary"
printf '  %d pass, %d warn, %d fail\n' "$pass" "$warn" "$fail"
if [ "$fail" -gt 0 ]; then
  printf '  \033[31mBaseline NOT met — the software controls can be bypassed until the FAILs are fixed.\033[0m\n'
else
  printf '  \033[32mNo hard failures. Re-check WARN items (especially the firmware ones the OS cannot read).\033[0m\n'
fi
exit 0
