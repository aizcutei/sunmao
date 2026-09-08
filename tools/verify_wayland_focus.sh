#!/usr/bin/env bash
# Real xdg-activation round trips on a compositor implementing the extension.
set -euo pipefail
: "${XDG_RUNTIME_DIR:?A private runtime directory is required}"
log_dir="${RUNNER_TEMP:-$XDG_RUNTIME_DIR}"
export SUNMAO_FOCUS_DISPLAY_FILE="$log_dir/sunmao-focus-display-$$"
cat > "$log_dir/sway-focus.conf" <<'CONFIG'
xwayland disable
focus_follows_mouse no
focus_on_window_activation none
output * mode 640x480
seat seat0 fallback true
exec sh -c 'printenv WAYLAND_DISPLAY > "$SUNMAO_FOCUS_DISPLAY_FILE"'
CONFIG
env -u DISPLAY WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
  sway --unsupported-gpu -c "$log_dir/sway-focus.conf" >"$log_dir/sway-focus.log" 2>&1 &
sway_pid=$!
cleanup() {
  if [ -n "${keyboard_pid:-}" ]; then
    kill "$keyboard_pid" 2>/dev/null || true
    wait "$keyboard_pid" 2>/dev/null || true
  fi
  kill "$sway_pid" 2>/dev/null || true
  wait "$sway_pid" 2>/dev/null || true
  cat "$log_dir/sway-focus.log"
}
trap cleanup EXIT
# The compositor exports its chosen socket to its own child process.
for _ in $(seq 1 100); do
  kill -0 "$sway_pid"
  [ -s "$SUNMAO_FOCUS_DISPLAY_FILE" ] && break
  sleep 0.1
done
[ -s "$SUNMAO_FOCUS_DISPLAY_FILE" ]
WAYLAND_DISPLAY=$(cat "$SUNMAO_FOCUS_DISPLAY_FILE")
export WAYLAND_DISPLAY
# Keep keyboard capability present between short-lived input injections.
wtype -s 600000 &
keyboard_pid=$!
export LIBGL_ALWAYS_SOFTWARE=1
env -u DISPLAY WAYLAND_DEBUG=client SUNMAO_FOCUS_TEST=1 \
  timeout 180s cargo test --locked -p baseview --features wayland,opengl \
  --test wayland_focus -- --nocapture 2>&1 | tee "$log_dir/wayland-focus.log"
grep -q 'WAYLAND FOCUS VERIFIED' "$log_dir/wayland-focus.log"
grep -Eq 'xdg_activation_v1@.*get_activation_token' "$log_dir/wayland-focus.log"
grep -Eq 'xdg_activation_token_v1@.*set_serial' "$log_dir/wayland-focus.log"
grep -Eq 'xdg_activation_token_v1@.*done' "$log_dir/wayland-focus.log"
[ "$(grep -Ec 'xdg_activation_v1@.*activate\(' "$log_dir/wayland-focus.log")" -eq 2 ]
