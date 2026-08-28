# SunMao fuzzing

Unbounded fuzzing of the paths that parse bytes SunMao did not produce.

This crate is **local-only and never part of CI**. It is excluded from the
workspace in the root `Cargo.toml`, so `cargo test`, `cargo build` and the
blocking gate neither build nor run it.

## What is fuzzed, and why

Saved state. It arrives from a project file or a preset that a user may have
edited, truncated, or corrupted, it is attacker-influenced in the sense that
sharing project files is normal, and it is decoded behind a C ABI where a panic
is undefined behaviour rather than a clean error. Both formats are covered:

| target | path exercised |
|---|---|
| `fuzz_clap_state_load` | arbitrary bytes → real `clap.state` `load` |
| `fuzz_vst3_state_load` | arbitrary bytes → real `IComponent::setState` |

Both drive the real plugin ABI rather than the internal decoder, so the
wrapper's guards are exercised along with the parser.

The property is simply: **any byte sequence is either rejected or applied, and
never panics or reads out of bounds.**

## Running it (stable, no extra toolchain)

```sh
cd fuzz
cargo run --release                      # runs until Ctrl-C
cargo run --release -- --iterations 100000
cargo run --release -- --seed 12345      # reproduce a specific run
```

The driver prints its seed on startup; passing that seed back reproduces the
same sequence, so a crash found here is reproducible without a corpus file.

This driver is **not coverage-guided**. It is the always-available baseline;
libFuzzer below reaches cases it will not.

## Running it with cargo-fuzz (coverage-guided, needs nightly)

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run clap_state_load
cargo +nightly fuzz run vst3_state_load
```

The `fuzz_targets/` wrappers call the same functions as the stable driver, so
the two cannot drift apart.

Note: `cargo-fuzz` was not installed in the environment where this scaffold was
written, so the libFuzzer wrappers themselves have not been executed — the fuzz
bodies they call have been, through the stable driver. Anyone with `cargo-fuzz`
installed should expect the wrappers to need nothing beyond the commands above.

## Adding a target

Put the body in `src/lib.rs` as a `pub fn name(data: &[u8])`, call it from the
loop in `src/main.rs`, and add a three-line wrapper in `fuzz_targets/`. Keeping
the body in the library is what lets both drivers share it.
