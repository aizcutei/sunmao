# SunMao Unified Packager

A unified command-line tool for packaging VST3 and CLAP plugins and standalone
applications across macOS, Windows, and Linux. An explicit macOS-only Audio
Unit path is retained for experiments, but is outside the Phase 1 acceptance
gate. This tool consolidates the previous separate packagers into one validation
and publication pipeline.

## Features

- **Cross-Platform Support**: Handles platform-specific bundle structures for macOS, Windows, and Linux.
- **Unified Interface**: Single binary to package all formats.
- **AU Support (macOS)**: Generates `.component` bundles, creates `Info.plist` with required keys, and supports code signing.
- **VST3 Support**: Creates `.vst3` bundles with the standard platform and architecture layout.
- **CLAP Support**: Creates `.clap` bundles on macOS and single-file modules on Windows and Linux.
- **Standalone Support**: Creates a macOS `.app`, Windows `.exe`, or extensionless Linux executable.
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

**Packaging a standalone application:**
```bash
cargo run -p sunmao_packager -- standalone \
    --binary target/release/myplugin_standalone \
    --out build/MyPlugin \
    --name "My Plugin" \
    --bundle-id com.example.myplugin \
    --version 1.0.0
```

## Platform Details

- **macOS**: Plug-in formats are bundles with an extensionless module under `Contents/MacOS`. Standalone produces `MyPlugin.app/Contents/MacOS/MyPlugin` with an `APPL` `Info.plist`.
- **Windows**: VST3 modules use `Contents/x86-win`, `Contents/x86_64-win`, or `Contents/arm_64-win` and are named `MyPlugin.vst3`. CLAP is a single `MyPlugin.clap` file and standalone is `MyPlugin.exe`.
- **Linux**: VST3 modules use `Contents/i386-linux`, `Contents/x86_64-linux`, `Contents/aarch64-linux`, or `Contents/riscv64-linux` and are named `MyPlugin.so`. CLAP is a single `MyPlugin.clap` file and standalone is an extensionless `MyPlugin` executable.

Input validation distinguishes modules from applications. macOS standalone
inputs must be Mach-O `MH_EXECUTE`; Windows inputs must be PE executables and
must not carry the DLL flag; Linux accepts ELF `ET_EXEC` or PIE `ET_DYN` with a
`PT_INTERP` program header and rejects ordinary shared objects. The copied
standalone retains executable permissions.

The packager validates a single native architecture but does not merge multiple binaries into a macOS universal binary or a multi-architecture VST3 bundle. Code signing remains macOS-only and uses the existing ad-hoc identity; Windows Authenticode, Linux package signing, notarization, and installer generation are outside its scope.

## Compatibility Commands

The historical `clap_packager`, `vst3_packager`, and `au_packager` binaries are compatibility frontends only. They delegate validation and staged publication to this crate, so existing scripts can migrate without retaining a second bundle implementation. New scripts should use `sunmao_packager` directly. AudioUnit installation remains available through `tools/package_examples.sh --au --install`; it is not part of the Phase 1 VST3/CLAP/standalone gate.

The default `sunmao_unittest_runner` build is deliberately AU-free for the
Phase 1 graph. Build it with `--features au` on macOS only when exercising the
experimental AudioUnit host.
