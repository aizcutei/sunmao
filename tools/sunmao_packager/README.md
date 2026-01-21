# SunMao Unified Packager

A unified command-line tool for packaging audio plugins (AudioUnit, VST3, CLAP) across macOS, Windows, and Linux. This tool consolidates previous separate packagers into a single, robust utility.

## Features

- **Cross-Platform Support**: Handles platform-specific bundle structures for macOS, Windows, and Linux.
- **Unified Interface**: Single binary to package all formats.
- **AU Support (macOS)**: Generates `.component` bundles, creates `Info.plist` with required keys, and supports code signing.
- **VST3 Support**: Creates `.vst3` bundles with correct architecture folders (`MacOS`, `x86_64-win`, `x86_64-linux`).
- **CLAP Support**: Creates `.clap` bundles on macOS and single-file/folder structures on other platforms according to spec.
- **Code Signing**: Built-in support for macOS code signing (`codesign`).

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

- **macOS**: All formats (AU, VST3, CLAP) are packaged as Bundles (`.component`, `.vst3`, `.clap`) containing `Contents/MacOS`, `Resources`, `Info.plist`, etc.
- **Windows**: VST3 is packaged as `MyPlugin.vst3/Contents/x86_64-win/MyPlugin.vst3`. CLAP is typically a single `.clap` file (DLL).
- **Linux**: VST3 is packaged as `MyPlugin.vst3/Contents/x86_64-linux/MyPlugin.so`. CLAP is typically a single `.clap` file (SO).
