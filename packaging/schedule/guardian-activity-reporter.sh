#!/usr/bin/env bash
# guardian-activity-reporter — logs the child's foreground app so the daily AI digest can report
# "what apps/games were used". Runs INSIDE the child's Hyprland session (needs hyprctl + the child's
# WAYLAND_DISPLAY / HYPRLAND_INSTANCE_SIGNATURE), so install it as a systemd --user service for the
# child or a Hyprland `exec-once`. It only records the active window class + title; no keystrokes, no
# content. The log is world-readable-by-root so guardiand (root) can summarise it.
#
#   Env: GUARDIAN_ACTIVITY_LOG   (default ~/.local/share/guardian/activity.log)
#        GUARDIAN_ACTIVITY_INTERVAL seconds (default 30)
set -u

LOG="${GUARDIAN_ACTIVITY_LOG:-$HOME/.local/share/guardian/activity.log}"
INTERVAL="${GUARDIAN_ACTIVITY_INTERVAL:-30}"
mkdir -p "$(dirname "$LOG")" 2>/dev/null || true

command -v hyprctl >/dev/null 2>&1 || { echo "guardian-activity-reporter: hyprctl not found" >&2; exit 1; }

while :; do
  out="$(hyprctl activewindow 2>/dev/null || true)"
  # hyprctl prints indented "class: foo" / "title: bar" lines; grep the first of each.
  class="$(printf '%s\n' "$out" | sed -n 's/^[[:space:]]*class:[[:space:]]*//p' | head -1)"
  title="$(printf '%s\n' "$out" | sed -n 's/^[[:space:]]*title:[[:space:]]*//p' | head -1)"
  if [ -n "$class" ]; then
    # epoch <TAB> class <TAB> title (tabs in the title flattened to spaces)
    printf '%s\t%s\t%s\n' "$(date +%s)" "$class" "$(printf '%s' "$title" | tr '\t' ' ')" >> "$LOG"
  fi
  sleep "$INTERVAL"
done
