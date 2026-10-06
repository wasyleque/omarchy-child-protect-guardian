#!/usr/bin/env bash
# Guardian Flatpak interceptor. Install as /usr/local/bin/flatpak (which must precede /usr/bin on
# PATH). It gates `flatpak install` through guardiand's submit socket (via guardian-ctl) and only
# proceeds on parent approval; every other flatpak subcommand passes straight through.
#
# Flatpak doesn't use libalpm, so the pacman hook can't see it — this wrapper covers the CLI path.
# Pair it with the polkit rule (49-guardian-flatpak.rules, blocks child system installs via any GUI)
# and fapolicyd (so even a --user app the child sneaks in can't execute). See README.md.
set -u

REAL="/usr/bin/flatpak"
[ -x "$REAL" ] || { echo "guardian-flatpak: $REAL not found" >&2; exit 1; }

# first non-option token = the subcommand
sub=""
for a in "$@"; do
  case "$a" in -*) ;; *) sub="$a"; break ;; esac
done

if [ "$sub" = "install" ]; then
  # human label from the non-option tokens after 'install' (so the parent sees what it is)
  seen=0; label=""
  for a in "$@"; do
    if [ "$seen" = 0 ]; then [ "$a" = "install" ] && seen=1; continue; fi
    case "$a" in -*) ;; *) label="${label:+$label }$a" ;; esac
  done
  [ -n "$label" ] || label="(unspecified)"

  decision="$(guardian-ctl request flatpak "flatpak: $label" 2>/dev/null)"
  if [ "$decision" != "ALLOW" ]; then
    echo "guardian: flatpak install of '$label' was not approved — aborting." >&2
    exit 1
  fi
fi

exec "$REAL" "$@"
