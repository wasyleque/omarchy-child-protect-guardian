#!/usr/bin/env bash
set -u

REAL="/usr/bin/snap"
if [ ! -x "$REAL" ]; then
    echo "guardian-snap-wrapper: $REAL is not executable" >&2
    exit 1
fi

# Find the subcommand (first argument that doesn't start with -)
subcommand=""
for arg in "$@"; do
    if [[ "$arg" != -* ]]; then
        subcommand="$arg"
        break
    fi
done

if [ "$subcommand" = "install" ]; then
    # Collect every non-option token after "install" (options anywhere are ignored, order-independent).
    seen=0; label=""
    for arg in "$@"; do
        if [ "$seen" = 0 ]; then
            [ "$arg" = "install" ] && seen=1
            continue
        fi
        case "$arg" in -*) ;; *) label="${label:+$label }$arg" ;; esac
    done
    [ -n "$label" ] || label="(unspecified)"

    decision="$(/usr/bin/guardian-ctl request snap "snap: $label" 2>/dev/null)"
    if [ "$decision" != "ALLOW" ]; then
        echo "guardian: snap install of '$label' was not approved - aborting." >&2
        exit 1
    fi
fi

exec "$REAL" "$@"
