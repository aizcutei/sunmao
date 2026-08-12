# SunMao Unified Packager

A unified command-line tool for packaging audio plugins (AudioUnit, VST3, CLAP) across macOS, Windows, and Linux. This tool consolidates previous separate packagers into a single, robust utility.

## Features

- **Cross-Platform Support**: Handles platform-specific bundle structures for macOS, Windows, and Linux.
- **Unified Interface**: Single binary to package all formats.
- **AU Support (macOS)**: Generates `.component` bundles, creates `Info.plist` with required keys, and supports code signing.
- **VST3 Support**: Creates `.vst3` bundles with the standard platform and architecture layout.
- **CLAP Support**: Creates `.clap` bundles on macOS and single-file modules on Windows and Linux.
- **Code Signing**: Built-in support for macOS code signing (`codesign`).
- **Input Validation**: Rejects missing/non-file inputs, wrong native module formats or architectures, malformed metadata, and unusable output paths before replacing an existing plugin.
- **Staged Publication**: Builds and signs beside the destination, then publishes the completed plugin while retaining the previous output if staging fails.

## Usage

### General Syntax

```bash
cargo run -p sunmao_packager -- \
    <FORMAT> \
    --binary <PATH> \
    --out <PATH> \
    --name <NAME> \
    --bundle-id <ID> \
    --version <VERSION> \
    [OPTIONS]
```

### Examples

**Packaging a VST3 (Cross-platform):**
```bash
cargo run -p sunmao_packager -- vst3 \
    --binary target/release/libmyplugin.dylib \
    --out build/MyPlugin \
    --name "My Plugin" \
    --bundle-id com.example.myplugin \
    --version 1.0.0
```

**Packaging an Audio Unit (macOS):**
```bash
cargo run -p sunmao_packager -- au \
    --binary target/release/libmyplugin.dylib \
    --out build/MyPlugin \
    --name "My Plugin" \
    --bundle-id com.example.myplugin \
    --version 1.0.0 \
    --au-type aufx \
    --au-subtype gain \
    --au-manufacturer ACME \
    --codesign
```

**Packaging a CLAP:**
```bash
cargo run -p sunmao_packager -- clap \
    --binary target/release/libmyplugin.dylib \
    --out build/MyPlugin \
    --name "My Plugin" \
    --bundle-id com.example.myplugin \
    --version 1.0.0
```

## Platform Details

- **macOS**: All formats are bundles. The module is extensionless and named after the bundle, for example `MyPlugin.vst3/Contents/MacOS/MyPlugin`.
- **Windows**: VST3 modules use `Contents/x86-win`, `Contents/x86_64-win`, or `Contents/arm_64-win` and are named `MyPlugin.vst3`. CLAP is a single `MyPlugin.clap` file.
- **Linux**: VST3 modules use `Contents/i386-linux`, `Contents/x86_64-linux`, `Contents/aarch64-linux`, or `Contents/riscv64-linux` and are named `MyPlugin.so`. CLAP is a single `MyPlugin.clap` file.

The packager validates a single native architecture but does not merge multiple binaries into a macOS universal binary or a multi-architecture VST3 bundle. Code signing remains macOS-only and uses the existing ad-hoc identity; Windows Authenticode, Linux package signing, notarization, and installer generation are outside its scope.
