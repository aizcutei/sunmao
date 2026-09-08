#!/usr/bin/env bash
# The observer reads Weston pixels; the editor has no X11 connection.
set -euo pipefail
: "${DISPLAY:?Run under xvfb-run}"
: "${XDG_RUNTIME_DIR:?A private runtime directory is required}"
log_dir="${RUNNER_TEMP:-$XDG_RUNTIME_DIR}"
export SUNMAO_INPUT_DISPLAY="$DISPLAY" LIBGL_ALWAYS_SOFTWARE=1 XCURSOR_THEME=Adwaita XCURSOR_SIZE=24
weston --no-config --backend=x11-backend.so --shell=kiosk-shell.so --width=640 --height=480 --scale=2 \
  --socket=wayland-cursor-scale --idle-time=0 >"$log_dir/weston-cursor-scale.log" 2>&1 &
weston_pid=$!
cleanup() {
  kill "$weston_pid" 2>/dev/null || true
  wait "$weston_pid" 2>/dev/null || true
  cat "$log_dir/weston-cursor-scale.log"
}
trap cleanup EXIT
for _ in $(seq 1 100); do
  kill -0 "$weston_pid"
  if [ -S "$XDG_RUNTIME_DIR/wayland-cursor-scale" ]; then
    SUNMAO_INPUT_WINDOW=$(xdotool search --onlyvisible --limit 1 --class '^Weston Compositor$') || true
    [ -n "${SUNMAO_INPUT_WINDOW:-}" ] && break
  fi
  sleep 0.1
done
: "${SUNMAO_INPUT_WINDOW:?Weston window did not appear}"
export SUNMAO_INPUT_WINDOW SUNMAO_CURSOR_CAPTURE="$PWD/tools/capture_wayland_cursor.py"
env -u DISPLAY WAYLAND_DEBUG=client WAYLAND_DISPLAY=wayland-cursor-scale \
  timeout 180s cargo test --locked -p baseview --features wayland,opengl \
  --test wayland_cursor -- --nocapture 2>&1 | tee "$log_dir/wayland-cursor-scale.log"
grep -q 'WAYLAND CURSOR VERIFIED' "$log_dir/wayland-cursor-scale.log"
grep -Eq 'wl_output@.*scale\(2\)' "$log_dir/wayland-cursor-scale.log"
grep -Eq 'wl_shm_pool@.*create_buffer\(.*48, 48, 192,' "$log_dir/wayland-cursor-scale.log"
grep -Eq 'wp_viewport@.*set_destination\(24, 24\)' "$log_dir/wayland-cursor-scale.log"
echo 'WAYLAND CURSOR SCALE VERIFIED: 48px theme images, 24-unit surfaces, compositor pixels'
