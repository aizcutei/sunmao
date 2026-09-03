#!/usr/bin/env bash
#
# package_examples.sh — Build and package the unified SunMao reference examples
# as Phase-1 VST3, CLAP, and standalone artifacts, then optionally exercise them.
# AU is an explicit, separate opt-in build because AU's Cocoa runtime is not part
# of the Phase-1 artifact graph.
#
# This is the Phase-1 packaging pipeline. It replaces the ad-hoc /tmp/package_all.sh.
#
# Usage:
#   tools/package_examples.sh              # build + package VST3/CLAP/standalone artifacts
#   tools/package_examples.sh --au         # macOS-only AU experiment (not Phase-1)
#   tools/package_examples.sh --install    # macOS: also install the generated bundles
#   tools/package_examples.sh --test       # test plugins and standalone DSP/MIDI smoke
#   tools/package_examples.sh --gui-test   # exercise embedded and standalone GUIs
#   tools/package_examples.sh --codesign   # ad-hoc codesign the --au bundles
#   tools/package_examples.sh --release    release build (default); --debug for debug
#
# Each example declares its own CLAP id and GUI metadata, so we keep a table
# here that mirrors what the example's lib.rs encodes. AU remains an explicit,
# separate macOS-only experiment and is never part of the Phase-1 gate.

set -euo pipefail

cd "$(dirname "$0")/.."

MODE="release"
INSTALL=0
TEST=0
GUI_TEST=0
CODESIGN=0
AU=0

for arg in "$@"; do
  case "$arg" in
    --debug)    MODE="debug" ;;
    --release)  MODE="release" ;;
    --install)  INSTALL=1 ;;
    --test)     TEST=1 ;;
    --gui-test) GUI_TEST=1 ;;
    --codesign) CODESIGN=1 ;;
    --au)       AU=1 ;;
    -h|--help)
      sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown flag: $arg" >&2; exit 1 ;;
  esac
done

HOST_OS="$(uname -s)"
if [ "$AU" -eq 1 ] && [ "$HOST_OS" != "Darwin" ]; then
  echo "--au is only supported on macOS" >&2
  exit 1
fi
if [ "$CODESIGN" -eq 1 ] && [ "$AU" -eq 0 ]; then
  echo "--codesign requires --au" >&2
  exit 1
fi
if [ "$INSTALL" -eq 1 ] && [ "$HOST_OS" != "Darwin" ]; then
  echo "--install is only supported on macOS" >&2
  exit 1
fi
if [ "$AU" -eq 1 ] && { [ "$TEST" -eq 1 ] || [ "$GUI_TEST" -eq 1 ]; }; then
  echo "--test and --gui-test require the default Phase-1 build" >&2
  echo "AU host tests require an installed and registered component" >&2
  exit 1
fi

# Map: example-crate -> "display name|bundle-id|au-type|au-subtype|au-manufacturer"
# au-* fields are macOS-only and ignored elsewhere.
EXAMPLES=(
  "sunmao_fx_gain|SunMao Gain|com.sunmao.fx.gain|||"
  "sunmao_syn_sine|SunMao Sine Synth|com.sunmao.synth.sine|||"
  "sunmao_fx_gain_gui_gl|SunMao Gain GL|com.sunmao.fx.gain.gl|aufx|smgg|SunM"
  "sunmao_fx_gain_gui_wgpu|SunMao Gain WGPU|com.sunmao.fx.gain.wgpu|aufx|smgw|SunM"
  "sunmao_fx_gain_gui_webview|SunMao Gain WebView|com.sunmao.fx.gain.webview|aufx|smgv|SunM"
  "sunmao_fx_lpf_gui_gl|SunMao LPF GL|com.sunmao.fx.lpf.gl|aufx|slpf|SunM"
  "sunmao_daw_info_gui|SunMao DAW Info GUI|com.sunmao.daw.info.gui|aufx|smdi|SunM"
  "sunmao_syn_sine_gui_gl|SunMao Sine Synth GL|com.sunmao.synth.sine.gl|||"
  "sunmao_syn_sine_gui_wgpu|SunMao Sine Synth WGPU|com.sunmao.synth.sine.wgpu|||"
  "sunmao_syn_sine_gui_webview|SunMao Sine Synth WebView|com.sunmao.synth.sine.webview|||"
  # Phase 2 contract fixtures. Packaged so the runner's host-side assertions
  # (latency/tail queries, multi-bus topology, sidechain routing) run against
  # plugins that actually have those properties — every Phase 1 example above
  # reports zero latency, no tail and a single input bus, so the assertions
  # would otherwise only ever take their skip paths.
  # Bundle ids use hyphens: the packager rejects underscores in bundle
  # identifier components. This is the macOS bundle id, not the plugin's own
  # CLAP id, so the fixtures' `clap_info` ids are unaffected.
  "sunmao_fx_tempo_delay|SunMao Tempo Delay|com.sunmao.fx.tempo-delay|||"
  "sunmao_fx_sidechain_comp|SunMao Sidechain Comp|com.sunmao.fx.sidechain-comp|||"
  # Phase 3 M4 fixtures. OS Distortion reports a linear-phase group delay,
  # which is the only kind the runner's `latency_alignment` impulse measurement
  # can check against; Meter exercises the lock-free metering publication.
  "sunmao_fx_os_dist|SunMao OS Distortion|com.sunmao.fx.os-dist|||"
  "sunmao_fx_meter|SunMao Meter|com.sunmao.fx.meter|||"
)

supports_au() {
  case "$1" in
    sunmao_fx_gain_gui_gl|sunmao_fx_gain_gui_wgpu|sunmao_fx_gain_gui_webview|\
    sunmao_fx_lpf_gui_gl|sunmao_daw_info_gui)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

supports_standalone() {
  case "$1" in
    sunmao_fx_gain|sunmao_syn_sine|\
    sunmao_fx_gain_gui_gl|sunmao_fx_gain_gui_wgpu|sunmao_fx_gain_gui_webview|\
    sunmao_syn_sine_gui_gl|sunmao_syn_sine_gui_wgpu|sunmao_syn_sine_gui_webview)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

standalone_binary_path() {
  local crate="$1" suffix=""
  case "$HOST_OS" in
    Darwin|Linux) ;;
    MINGW*|MSYS*|CYGWIN*) suffix=".exe" ;;
    *) echo "unsupported host OS: $HOST_OS" >&2; return 1 ;;
  esac
  printf '%s/%s_standalone%s\n' "$TARGET_DIR" "$crate" "$suffix"
}

packaged_standalone_path() {
  local name="$1"
  case "$HOST_OS" in
    Darwin) printf '%s/%s.app/Contents/MacOS/%s\n' "$OUT_DIR" "$name" "$name" ;;
    Linux) printf '%s/%s\n' "$OUT_DIR" "$name" ;;
    MINGW*|MSYS*|CYGWIN*) printf '%s/%s.exe\n' "$OUT_DIR" "$name" ;;
    *) echo "unsupported host OS: $HOST_OS" >&2; return 1 ;;
  esac
}

if [ "$MODE" = "release" ]; then
  CARGO_FLAGS=(--release)
  PROFILE_DIR="release"
else
  CARGO_FLAGS=(--profile dev)
  PROFILE_DIR="debug"
fi

if [ "$AU" -eq 1 ]; then
  TARGET_ROOT="target/au"
  OUT_DIR="build_au"
else
  TARGET_ROOT="target"
  OUT_DIR="build_new"
fi
TARGET_DIR="$TARGET_ROOT/$PROFILE_DIR"
mkdir -p "$OUT_DIR"

# Keep the unresolved AU GUI path opt-in and limited to the five examples that
# already support it. The corrected Phase-1 default includes standalone for the
# eight primary gain/sine reference examples.
SELECTED_EXAMPLES=()
for row in "${EXAMPLES[@]}"; do
  crate="${row%%|*}"
  if [ "$AU" -eq 1 ] && ! supports_au "$crate"; then
    continue
  fi
  SELECTED_EXAMPLES+=("$row")
done

echo "==> Building examples ($MODE)..."
EXAMPLE_CARGO_ARGS=()
for row in "${SELECTED_EXAMPLES[@]}"; do
  EXAMPLE_CARGO_ARGS+=(-p "${row%%|*}")
done
if [ "$AU" -eq 1 ]; then
  cargo build --locked --target-dir "$TARGET_ROOT" "${CARGO_FLAGS[@]}" \
    --features au "${EXAMPLE_CARGO_ARGS[@]}"
else
  cargo build --locked --target-dir "$TARGET_ROOT" "${CARGO_FLAGS[@]}" \
    "${EXAMPLE_CARGO_ARGS[@]}"

  echo "==> Building standalone examples..."
  STANDALONE_CARGO_ARGS=()
  for row in "${SELECTED_EXAMPLES[@]}"; do
    crate="${row%%|*}"
    if supports_standalone "$crate"; then
      STANDALONE_CARGO_ARGS+=(-p "$crate")
    fi
  done
  cargo build --locked --target-dir "$TARGET_ROOT" "${CARGO_FLAGS[@]}" \
    --bins --features standalone "${STANDALONE_CARGO_ARGS[@]}"
fi

# Keep the default-build tools in a separate Cargo invocation so the host binary
# is built from its own dependency graph. AU packaging does not use the runner.
if [ "$AU" -eq 1 ]; then
  echo "==> Building sunmao_packager..."
  cargo build --locked --target-dir "$TARGET_ROOT" "${CARGO_FLAGS[@]}" \
    -p sunmao_packager
else
  echo "==> Building sunmao_packager + sunmao_unittest_runner..."
  cargo build --locked --target-dir "$TARGET_ROOT" "${CARGO_FLAGS[@]}" \
    -p sunmao_packager -p sunmao_unittest_runner
fi

PACKAGER="$TARGET_DIR/sunmao_packager"
RUNNER="$TARGET_DIR/sunmao_unittest_runner"
CODESIGN_ARG=""
[ "$CODESIGN" -eq 1 ] && CODESIGN_ARG="--codesign"

build_one() {
  local crate="$1" name="$2" bid="$3" au_type="$4" au_sub="$5" au_mfr="$6"
  local bin="" standalone_bin=""
  case "$HOST_OS" in
    Darwin) bin="$TARGET_DIR/lib${crate}.dylib" ;;
    Linux) bin="$TARGET_DIR/lib${crate}.so" ;;
    MINGW*|MSYS*|CYGWIN*) bin="$TARGET_DIR/${crate}.dll" ;;
    *) echo "  !! unsupported host OS: $HOST_OS" >&2; return 1 ;;
  esac
  if [ ! -f "$bin" ]; then echo "  !! binary for $crate not found: $bin" >&2; return 1; fi

  echo "  packaging $name ($bin)"
  if [ "$AU" -eq 0 ]; then
    "$PACKAGER" clap --binary "$bin" --out "$OUT_DIR/${name}" --name "$name" --bundle-id "$bid" --version 1.0.0
    "$PACKAGER" vst3 --binary "$bin" --out "$OUT_DIR/${name}" --name "$name" --bundle-id "$bid" --version 1.0.0
    if supports_standalone "$crate"; then
      standalone_bin="$(standalone_binary_path "$crate")"
      if [ ! -f "$standalone_bin" ]; then
        echo "  !! standalone binary for $crate not found: $standalone_bin" >&2
        return 1
      fi
      "$PACKAGER" standalone --binary "$standalone_bin" --out "$OUT_DIR/${name}" \
        --name "$name" --bundle-id "$bid" --version 1.0.0
    fi
  elif [ "$HOST_OS" = "Darwin" ]; then
    "$PACKAGER" au      --binary "$bin" --out "$OUT_DIR/${name}" --name "$name" --bundle-id "$bid" --version 1.0.0 \
      --au-type "$au_type" --au-subtype "$au_sub" --au-manufacturer "$au_mfr" --au-factory RustAUFactory \
      ${CODESIGN_ARG:+--codesign}
  fi
}

for row in "${SELECTED_EXAMPLES[@]}"; do
  IFS='|' read -r crate name bid au_type au_sub au_mfr <<< "$row"
  build_one "$crate" "$name" "$bid" "$au_type" "$au_sub" "$au_mfr"
done

# Remove only incompatible siblings after the complete build/package pass
# succeeds. A failed invocation therefore cannot destroy the last good output.
# Iterate the full manifest so an AU run also removes stale components for the
# synth examples, which are intentionally excluded from the AU build graph.
for row in "${EXAMPLES[@]}"; do
  IFS='|' read -r crate name _bid _au_type _au_sub _au_mfr <<< "$row"
  if [ "$AU" -eq 1 ]; then
    for stale in "$OUT_DIR/${name}.clap" "$OUT_DIR/${name}.vst3"; do
      if [ -e "$stale" ] || [ -L "$stale" ]; then rm -rf -- "$stale"; fi
    done
    if ! supports_au "$crate"; then
      stale="$OUT_DIR/${name}.component"
      if [ -e "$stale" ] || [ -L "$stale" ]; then rm -rf -- "$stale"; fi
    fi
  else
    stale="$OUT_DIR/${name}.component"
    if [ -e "$stale" ] || [ -L "$stale" ]; then rm -rf -- "$stale"; fi
  fi
done

echo "==> Done. Artifacts in $OUT_DIR/"

if [ "$TEST" -eq 1 ]; then
  echo "==> Running unit tests..."
  for row in "${SELECTED_EXAMPLES[@]}"; do
    IFS='|' read -r crate name _ _ _ _ <<< "$row"
    for ext in clap vst3; do
      local_bundle="$OUT_DIR/${name}.${ext}"
      if [ ! -e "$local_bundle" ]; then
        echo "  missing expected bundle: $local_bundle" >&2
        exit 1
      fi
      echo "  test $local_bundle"
      "$RUNNER" test "$local_bundle"
    done
    if supports_standalone "$crate"; then
      raw_standalone="$(standalone_binary_path "$crate")"
      packaged_standalone="$(packaged_standalone_path "$name")"
      if [ ! -f "$raw_standalone" ]; then
        echo "  missing expected raw standalone executable: $raw_standalone" >&2
        exit 1
      fi
      if [ ! -f "$packaged_standalone" ]; then
        echo "  missing expected packaged standalone executable: $packaged_standalone" >&2
        exit 1
      fi
      echo "  smoke raw $raw_standalone"
      "$raw_standalone" --smoke
      echo "  smoke packaged $packaged_standalone"
      "$packaged_standalone" --smoke
    fi
  done
fi

if [ "$GUI_TEST" -eq 1 ]; then
  echo "==> Running GUI lifecycle tests..."
  GUI_RUNNER=("$RUNNER")
  if [ "$(uname -s)" = "Linux" ] && command -v xvfb-run >/dev/null 2>&1; then
    GUI_RUNTIME_DIR="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}/sunmao-xdg}"
    mkdir -p "$GUI_RUNTIME_DIR"
    chmod 700 "$GUI_RUNTIME_DIR"
    GUI_RUNNER=(xvfb-run -a env \
      GDK_BACKEND=x11 \
      WGPU_BACKEND=gl \
      LIBGL_ALWAYS_SOFTWARE=1 \
      XDG_RUNTIME_DIR="$GUI_RUNTIME_DIR" \
      "$RUNNER")
  elif [ "$(uname -s)" = "Linux" ]; then
    echo "--gui-test on Linux requires xvfb-run" >&2
    exit 1
  fi
  STANDALONE_GUI_PREFIX=()
  if [ "$(uname -s)" = "Linux" ]; then
    STANDALONE_GUI_PREFIX=(xvfb-run -a env \
      GDK_BACKEND=x11 \
      WGPU_BACKEND=gl \
      LIBGL_ALWAYS_SOFTWARE=1 \
      XDG_RUNTIME_DIR="$GUI_RUNTIME_DIR")
  fi
  run_gui_test() {
    if "${GUI_RUNNER[@]}" "$@"; then
      if [ "$(uname -s)" = "Darwin" ]; then sleep 1; fi
      return 0
    fi
    if [ "$(uname -s)" != "Darwin" ]; then
      return 1
    fi
    echo "  macOS GUI host failed; retrying once after WindowServer cooldown" >&2
    sleep 5
    if "${GUI_RUNNER[@]}" "$@"; then
      sleep 1
      return 0
    fi
    return 1
  }
  run_standalone_gui_test() {
    local executable="$1"
    local status=0
    set +e
    if [ "$(uname -s)" = "Linux" ]; then
      "${STANDALONE_GUI_PREFIX[@]}" "$executable" --gui-smoke
      status=$?
    else
      "$executable" --gui-smoke
      status=$?
    fi
    set -e
    if [ "$status" -eq 0 ]; then
      if [ "$(uname -s)" = "Darwin" ]; then sleep 1; fi
      return 0
    fi
    if [ "$(uname -s)" != "Darwin" ]; then
      return 1
    fi
    echo "  macOS standalone GUI failed; retrying once after WindowServer cooldown" >&2
    sleep 5
    set +e
    "$executable" --gui-smoke
    status=$?
    set -e
    return "$status"
  }
  for row in "${SELECTED_EXAMPLES[@]}"; do
    IFS='|' read -r crate name _ _ _ _ <<< "$row"
    case "$crate" in
      *_gui_gl|*_gui_wgpu|*_gui_webview|sunmao_fx_lpf_gui_gl|sunmao_daw_info_gui) ;;
      *) continue ;;
    esac
    GUI_COMMAND=(gui-test --auto-close --verify-pixels)
    case "$crate" in
      sunmao_fx_gain_gui_gl|sunmao_fx_gain_gui_wgpu|\
      sunmao_syn_sine_gui_gl|sunmao_syn_sine_gui_wgpu)
        GUI_COMMAND+=(--verify-input --drag-from 64,110 --drag-to 456,110)
        ;;
      sunmao_fx_gain_gui_webview)
        GUI_COMMAND+=(--verify-input --drag-from 120,150 --drag-to 400,150)
        ;;
      sunmao_syn_sine_gui_webview)
        # The synth panel pins its line heights and slider box, so after the
        # standard 520x220 resize its range input is centered at y=138.
        GUI_COMMAND+=(--verify-input --drag-from 120,138 --drag-to 400,138)
        ;;
    esac
    for ext in clap vst3; do
      local_bundle="$OUT_DIR/${name}.${ext}"
      if [ ! -e "$local_bundle" ]; then
        echo "  missing expected bundle: $local_bundle" >&2
        exit 1
      fi
      echo "  gui-test $local_bundle"
      run_gui_test "${GUI_COMMAND[@]}" "$local_bundle"
    done
    if supports_standalone "$crate"; then
      raw_standalone="$(standalone_binary_path "$crate")"
      packaged_standalone="$(packaged_standalone_path "$name")"
      if [ ! -f "$raw_standalone" ]; then
        echo "  missing expected raw standalone executable: $raw_standalone" >&2
        exit 1
      fi
      if [ ! -f "$packaged_standalone" ]; then
        echo "  missing expected packaged standalone executable: $packaged_standalone" >&2
        exit 1
      fi
      echo "  gui-smoke raw $raw_standalone"
      run_standalone_gui_test "$raw_standalone"
      echo "  gui-smoke packaged $packaged_standalone"
      run_standalone_gui_test "$packaged_standalone"
    fi
  done
fi

if [ "$INSTALL" -eq 1 ]; then
  echo "==> Installing into system plugin dirs..."
  component_dir="$HOME/Library/Audio/Plug-Ins/Components"
  vst3_dir="$HOME/Library/Audio/Plug-Ins/VST3"
  clap_dir="$HOME/Library/Audio/Plug-Ins/CLAP"
  mkdir -p "$component_dir" "$vst3_dir" "$clap_dir"

  # Validate the complete generation before replacing any installed bundle.
  for row in "${SELECTED_EXAMPLES[@]}"; do
    IFS='|' read -r _crate name _bid _au_type _au_sub _au_mfr <<< "$row"
    if [ "$AU" -eq 1 ]; then
      expected=("$OUT_DIR/${name}.component")
    else
      expected=("$OUT_DIR/${name}.vst3" "$OUT_DIR/${name}.clap")
    fi
    for source_bundle in "${expected[@]}"; do
      if [ ! -e "$source_bundle" ]; then
        echo "  missing expected install bundle: $source_bundle" >&2
        exit 1
      fi
    done
  done

  install_bundle() {
    local source_bundle="$1" destination_dir="$2"
    local bundle_name destination_bundle staging_root staged_bundle backup_bundle
    bundle_name="$(basename "$source_bundle")"
    destination_bundle="$destination_dir/$bundle_name"
    staging_root="$(mktemp -d "$destination_dir/.${bundle_name}.sunmao-install.XXXXXX")"
    staged_bundle="$staging_root/new"
    backup_bundle="$staging_root/previous"

    # Copy completely before touching the installed bundle. Publishing and
    # rollback are same-filesystem renames inside the destination directory.
    if ! cp -R "$source_bundle" "$staged_bundle"; then
      rm -rf -- "$staging_root"
      return 1
    fi
    if [ -e "$destination_bundle" ] || [ -L "$destination_bundle" ]; then
      if ! mv "$destination_bundle" "$backup_bundle"; then
        rm -rf -- "$staging_root"
        return 1
      fi
    fi
    if mv "$staged_bundle" "$destination_bundle"; then
      rm -rf -- "$staging_root"
      return 0
    fi

    echo "  failed to publish $destination_bundle; restoring previous bundle" >&2
    if [ -e "$backup_bundle" ] || [ -L "$backup_bundle" ]; then
      if ! mv "$backup_bundle" "$destination_bundle"; then
        echo "  rollback failed; previous bundle remains at $backup_bundle" >&2
        return 1
      fi
    fi
    rm -rf -- "$staging_root"
    return 1
  }

  for row in "${SELECTED_EXAMPLES[@]}"; do
    IFS='|' read -r _crate name _bid _au_type _au_sub _au_mfr <<< "$row"
    if [ "$AU" -eq 1 ]; then
      install_bundle "$OUT_DIR/${name}.component" "$component_dir"
    else
      install_bundle "$OUT_DIR/${name}.vst3" "$vst3_dir"
      install_bundle "$OUT_DIR/${name}.clap" "$clap_dir"
    fi
  done

  if [ "$AU" -eq 1 ]; then
    echo "  installed. Refresh AU cache with:"
    echo "    killall -9 AudioComponentRegistrar 2>/dev/null || true"
  else
    echo "  installed."
  fi
fi
