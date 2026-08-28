#![no_main]
//! Coverage-guided version of `sunmao_fuzz::fuzz_clap_state_load`.
//! Requires `cargo-fuzz`; see `fuzz/README.md`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    sunmao_fuzz::fuzz_clap_state_load(data);
});
