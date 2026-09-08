#!/usr/bin/env bash
# Run under xvfb-run: only Weston and the input injector may use X11.
set -euo pipefail
: "${DISPLAY:?Run this script under xvfb-run}"
: "${XDG_RUNTIME_DIR:?A private runtime directory is required}"
log_dir="${RUNNER_TEMP:-$XDG_RUNTIME_DIR}"
export SUNMAO_INPUT_DISPLAY="$DISPLAY"
export LIBGL_ALWAYS_SOFTWARE=1
weston --backend=x11-backend.so --shell=kiosk-shell.so --width=640 --height=480 \
  --socket=wayland-pointer-ci --idle-time=0 >"$log_dir/weston-pointer.log" 2>&1 &
weston_pid=$!
cleanup() {
  kill "$weston_pid" 2>/dev/null || true
  wait "$weston_pid" 2>/dev/null || true
  cat "$log_dir/weston-pointer.log"
}
trap cleanup EXIT
for _ in $(seq 1 100); do
  kill -0 "$weston_pid"
  if [ -S "$XDG_RUNTIME_DIR/wayland-pointer-ci" ]; then
    SUNMAO_INPUT_WINDOW=$(xdotool search --onlyvisible --name 'Weston Compositor' | head -1) || true
    [ -n "${SUNMAO_INPUT_WINDOW:-}" ] && break
  fi
  sleep 0.1
done
: "${SUNMAO_INPUT_WINDOW:?Weston X11 window did not appear}"
export SUNMAO_INPUT_WINDOW
env -u DISPLAY WAYLAND_DISPLAY=wayland-pointer-ci SUNMAO_GUI_PIXEL_PROBE=1 \
  timeout 180s cargo test --locked -p sunmao_view_baseview --features wayland \
  native_wayland_pointer_changes_rendered_pixels -- --nocapture \
  2>&1 | tee "$log_dir/wayland-pointer.log"
grep -q 'WAYLAND POINTER VERIFIED' "$log_dir/wayland-pointer.log"
