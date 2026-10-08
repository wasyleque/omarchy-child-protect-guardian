#!/usr/bin/env bash
# Guardian Nix interceptor. Install as BOTH /usr/local/bin/nix and /usr/local/bin/nix-env (symlink one
# to the other), ahead of /usr/bin on PATH. Gates `nix-env -i…` and `nix profile install|add`; every
# other invocation passes straight through. Pair with fapolicyd (the backstop for anything that slips).
set -u

self="$(basename -- "$0")"
case "$self" in
  nix-env) REAL="/usr/bin/nix-env" ;;
  nix)     REAL="/usr/bin/nix" ;;
  *)       REAL="/usr/bin/$self" ;;
esac
[ -x "$REAL" ] || { echo "guardian-nix-wrapper: $REAL not executable" >&2; exit 1; }

is_install=0
label=""

if [ "$self" = "nix-env" ]; then
  for arg in "$@"; do
    case "$arg" in -i|--install|-iA|-irA) is_install=1 ;; esac
  done
  if [ "$is_install" = 1 ]; then
    for arg in "$@"; do
      case "$arg" in -*) ;; *) label="${label:+$label }$arg" ;; esac
    done
  fi
else
  # `nix [flags] profile (install|add) <refs…>`
  sub1=""; sub2=""
  for arg in "$@"; do
    case "$arg" in
      -*) ;;
      *) if [ -z "$sub1" ]; then sub1="$arg"; elif [ -z "$sub2" ]; then sub2="$arg"; fi ;;
    esac
  done
  if [ "$sub1" = "profile" ] && { [ "$sub2" = "install" ] || [ "$sub2" = "add" ]; }; then
    is_install=1
    seen=0
    for arg in "$@"; do
      if [ "$seen" = 0 ]; then [ "$arg" = "$sub2" ] && seen=1; continue; fi
      case "$arg" in -*) ;; *) label="${label:+$label }$arg" ;; esac
    done
  fi
fi

if [ "$is_install" = 1 ]; then
  [ -n "$label" ] || label="(unspecified)"
  decision="$(guardian-ctl request nix "nix: $label" 2>/dev/null)"
  if [ "$decision" != "ALLOW" ]; then
    echo "guardian: nix install of '$label' was not approved - aborting." >&2
    exit 1
  fi
fi

exec "$REAL" "$@"
