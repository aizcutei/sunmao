#!/usr/bin/env bash
# Exercise preferred fractional density and the wl_output integer fallback.
set -euo pipefail
: "${XDG_RUNTIME_DIR:?A private runtime directory is required}"
log_dir="${RUNNER_TEMP:-$XDG_RUNTIME_DIR}"
export SUNMAO_SCALE_ENV_FILE="$log_dir/sunmao-scale-env-$$"
cat > "$log_dir/sway-scale.conf" <<'CONFIG'
xwayland disable
focus_follows_mouse no
output HEADLESS-1 mode 1280x960 position 0 0 scale 1
output HEADLESS-2 mode 1280x960 position 1280 0 scale 2
for_window [title="SunMao scale acceptance"] floating enable, border none
exec sh -c 'printf "%s\n%s\n" "$WAYLAND_DISPLAY" "$SWAYSOCK" > "$SUNMAO_SCALE_ENV_FILE"'
CONFIG
env -u DISPLAY WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=2 WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
  sway --unsupported-gpu -c "$log_dir/sway-scale.conf" >"$log_dir/sway-scale.log" 2>&1 &
compositor_pid=$!
cleanup() {
  kill "$compositor_pid" 2>/dev/null || true
  wait "$compositor_pid" 2>/dev/null || true
  cat "$log_dir/sway-scale.log"
  if [ -f "$log_dir/weston-scale.log" ]; then cat "$log_dir/weston-scale.log"; fi
}
trap cleanup EXIT
for _ in $(seq 1 100); do
  kill -0 "$compositor_pid"
  [ -s "$SUNMAO_SCALE_ENV_FILE" ] && break
  sleep 0.1
done
mapfile -t scale_env < "$SUNMAO_SCALE_ENV_FILE"
export WAYLAND_DISPLAY="${scale_env[0]}" SWAYSOCK="${scale_env[1]}" LIBGL_ALWAYS_SOFTWARE=1
# Bind the initial workspace to the 1x output.
swaymsg -r 'focus output HEADLESS-1'
env -u DISPLAY WAYLAND_DEBUG=client SUNMAO_SCALE_TEST=sway \
  timeout 180s cargo test --locked -p baseview --features wayland,opengl \
  --test wayland_scale -- --nocapture 2>&1 | tee "$log_dir/wayland-scale.log"
grep -q 'WAYLAND SCALE VERIFIED' "$log_dir/wayland-scale.log"
grep -Eq 'wp_fractional_scale_v1@.*preferred_scale\(180\)' "$log_dir/wayland-scale.log"
grep -Eq 'wp_viewport@.*set_destination\(160, 120\)' "$log_dir/wayland-scale.log"
kill "$compositor_pid"
wait "$compositor_pid" || true
unset SWAYSOCK
weston --backend=headless-backend.so --use-gl --width=1280 --height=960 --scale=2 \
  --socket=wayland-scale-core --idle-time=0 >"$log_dir/weston-scale.log" 2>&1 &
compositor_pid=$!
for _ in $(seq 1 100); do
  kill -0 "$compositor_pid"
  [ -S "$XDG_RUNTIME_DIR/wayland-scale-core" ] && break
  sleep 0.1
done
env -u DISPLAY WAYLAND_DEBUG=client WAYLAND_DISPLAY=wayland-scale-core SUNMAO_SCALE_TEST=core \
  timeout 180s cargo test --locked -p baseview --features wayland,opengl \
  --test wayland_scale -- --nocapture 2>&1 | tee "$log_dir/wayland-core-scale.log"
grep -q 'WAYLAND CORE SCALE VERIFIED' "$log_dir/wayland-core-scale.log"
grep -Eq 'wl_output@.*scale\(2\)' "$log_dir/wayland-core-scale.log"
