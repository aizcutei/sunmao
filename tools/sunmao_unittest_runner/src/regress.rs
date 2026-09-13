//! Deterministic batch regression runs.
//!
//! The `test` command answers "does this plugin work". This one answers a
//! harder question: "does it still do exactly what it did last week, and does
//! it do the same thing on all three platforms?". That needs a run whose every
//! input is reproducible from a seed, and an output that two machines can
//! compare without either of them being present.
//!
//! Three things are pinned on purpose, because each has caught bugs elsewhere
//! in this repository:
//!
//! * **The seed**, so the signal and the automation are the same everywhere.
//! * **The maximum block size**, because a plugin sized for one block and
//!   handed another is a classic crash.
//! * **The block division**, which is deliberately *uneven*. A plugin that only
//!   ever sees 512-frame blocks can hide an off-by-one at a block boundary for
//!   years; real hosts split blocks wherever automation lands.
//!
//! Comparison is against a golden trace with an **explicitly stated tolerance**.
//! Exact equality would be the wrong contract: the same arithmetic compiled for
//! three targets may contract a multiply-add differently, and a regression
//! suite that forbids that is a suite that goes red for reasons nobody can act
//! on. The tolerance therefore lives in the trace file itself, so the file
//! documents the rule it is checked under.

use crate::host::{HostEvent, HostPlugin};
use std::fmt::Write as _;

/// Absolute tolerance used when a golden trace does not state one.
///
/// Chosen to sit above f32 rounding on unit-scale audio and below anything a
/// real DSP change would produce.
pub const DEFAULT_ABS_TOLERANCE: f64 = 1e-6;
/// Relative tolerance used when a golden trace does not state one.
pub const DEFAULT_REL_TOLERANCE: f64 = 1e-6;

/// Trace format version. Bumping it makes an old golden fail loudly rather
/// than be compared under rules it was not written for.
pub const TRACE_VERSION: u32 = 1;

/// xorshift64*, written out rather than pulled in.
///
/// A regression seed has to mean the same thing in five years; a dependency's
/// generator is free to change its stream in a minor release, and the first
/// sign of that would be every golden failing at once.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Mix the seed instead of merely forcing it off zero. `seed | 1` was
        // the obvious way to avoid xorshift's zero fixed point, and it is
        // wrong: it discards the low bit, so every seed and its neighbour
        // produce byte-identical runs. A regression suite where two different
        // seeds silently mean the same run is worse than having one seed.
        // SplitMix64's finalizer keeps all 64 bits and avalanches them.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `[-1, 1)`, derived from the top 24 bits so the value is
    /// exactly representable in f32 and survives the f32 audio path unchanged.
    pub fn next_bipolar(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32; // 24 bits
        (bits as f32 / 8_388_608.0) - 1.0
    }

    /// Uniform in `[low, high]`, inclusive, for `low <= high`.
    pub fn next_range(&mut self, low: u32, high: u32) -> u32 {
        debug_assert!(low <= high);
        let span = (high - low) as u64 + 1;
        low + (self.next_u64() % span) as u32
    }
}

/// How a run is configured. Everything here goes into the trace header, so a
/// golden can only be compared against a run that was set up the same way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunConfig {
    pub seed: u64,
    pub sample_rate: f64,
    pub max_block: u32,
    pub blocks: u32,
    pub abs_tolerance: f64,
    pub rel_tolerance: f64,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            seed: 0x5150_4D41_4F53_4D31,
            sample_rate: 48000.0,
            max_block: 512,
            blocks: 24,
            abs_tolerance: DEFAULT_ABS_TOLERANCE,
            rel_tolerance: DEFAULT_REL_TOLERANCE,
        }
    }
}

/// The uneven block division for a run.
///
/// Derived from the seed alone, so it is identical on every platform, and
/// always within `1..=max_block` so it can never violate the size the plugin
/// was initialized for.
pub fn block_sizes(config: &RunConfig) -> Vec<u32> {
    let mut rng = Rng::new(config.seed ^ 0x424C_4F43_4B53_4944);
    (0..config.blocks)
        .map(|index| {
            // Pin the first and last block to the extremes. Those two are the
            // ones a size-assuming plugin trips over, and leaving them to
            // chance means some seeds never test them at all.
            match index {
                0 => config.max_block,
                i if i + 1 == config.blocks => 1,
                _ => rng.next_range(1, config.max_block),
            }
        })
        .collect()
}

/// One scheduled parameter move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduledParam {
    pub block: u32,
    pub sample_offset: u32,
    pub id: u32,
    pub value: f64,
}

/// Build the automation schedule for a run.
///
/// Values are quantised to 1/1024 so the trace stays readable and the schedule
/// itself cannot become a source of float drift between platforms.
pub fn automation(
    config: &RunConfig,
    params: &[(u32, f64, f64)],
    sizes: &[u32],
) -> Vec<ScheduledParam> {
    if params.is_empty() {
        return Vec::new();
    }
    let mut rng = Rng::new(config.seed ^ 0x4155_544F_4D41_5445);
    let mut schedule = Vec::new();
    for (block, &frames) in sizes.iter().enumerate() {
        // Roughly every third block carries a move, so plenty of blocks run
        // with no automation at all.
        if rng.next_u64() % 3 != 0 {
            continue;
        }
        let (id, min, max) = params[(rng.next_u64() as usize) % params.len()];
        let step = rng.next_range(0, 1024) as f64 / 1024.0;
        schedule.push(ScheduledParam {
            block: block as u32,
            sample_offset: rng.next_range(0, frames.saturating_sub(1)),
            id,
            value: min + (max - min) * step,
        });
    }
    schedule
}

/// One block's worth of observed output.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockTrace {
    pub index: u32,
    pub frames: u32,
    pub peak: f64,
    pub rms: f64,
    /// A few samples read at fixed positions.
    ///
    /// Peak and RMS alone would miss a plugin that mangles the middle of a
    /// block while keeping its envelope; storing every sample would make the
    /// golden unreadable and enormous.
    pub probes: Vec<f64>,
}

/// Number of probe samples recorded per block.
pub const PROBES_PER_BLOCK: usize = 4;

fn probe_indices(frames: usize, channels: usize) -> Vec<usize> {
    if frames == 0 || channels == 0 {
        return Vec::new();
    }
    let total = frames * channels;
    (0..PROBES_PER_BLOCK)
        .map(|probe| (total - 1) * probe / PROBES_PER_BLOCK.max(1))
        .map(|index| index.min(total - 1))
        .collect()
}

/// A complete run, ready to be written out or compared.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pub plugin: String,
    pub format: String,
    pub config: RunConfig,
    pub input_channels: u32,
    pub output_channels: u32,
    pub blocks: Vec<BlockTrace>,
    pub schedule: Vec<ScheduledParam>,
    /// Parameter values read back at the end, so a run proves the automation
    /// landed rather than only that it was sent.
    pub final_params: Vec<(u32, f64)>,
}

/// Run one plugin through a deterministic batch and record what came out.
pub fn run(plugin: &mut dyn HostPlugin, config: &RunConfig) -> Result<Trace, String> {
    let info = plugin.info().clone();
    let sizes = block_sizes(config);

    let mut automatable = Vec::new();
    for index in 0..plugin.param_count() {
        if let Some(param) = plugin.param_info(index) {
            if param.can_automate && param.max > param.min {
                automatable.push((param.id, param.min, param.max));
            }
        }
    }
    let schedule = automation(config, &automatable, &sizes);

    let mut rng = Rng::new(config.seed);
    let mut blocks = Vec::with_capacity(sizes.len());
    let mut input = Vec::new();
    let mut output = Vec::new();
    let mut events = Vec::new();

    for (index, &frames) in sizes.iter().enumerate() {
        let frames = frames as usize;
        input.clear();
        input.resize(frames * info.input_channels as usize, 0.0);
        for sample in input.iter_mut() {
            *sample = rng.next_bipolar();
        }
        output.clear();
        output.resize(frames * info.output_channels as usize, 0.0);

        events.clear();
        for moved in schedule.iter().filter(|item| item.block == index as u32) {
            events.push(HostEvent::ParamValue {
                sample_offset: moved.sample_offset,
                id: moved.id,
                value: moved.value,
            });
        }
        // A synth has no input to react to, so give it a note to render.
        if info.is_synth && index == 0 {
            events.insert(
                0,
                HostEvent::NoteOn {
                    sample_offset: 0,
                    channel: 0,
                    pitch: 60,
                    velocity: 0.8,
                },
            );
        }
        events.sort_by_key(|event| event.sample_offset());

        plugin
            .process_with_events(&input, &mut output, &events)
            .map_err(|error| format!("block {index} ({frames} frames): {error}"))?;

        if let Some(position) = output.iter().position(|sample| !sample.is_finite()) {
            return Err(format!(
                "block {index} produced a non-finite sample at index {position}"
            ));
        }

        let peak = output
            .iter()
            .fold(0.0f64, |peak, &sample| peak.max(f64::from(sample).abs()));
        let rms = if output.is_empty() {
            0.0
        } else {
            (output
                .iter()
                .map(|&sample| f64::from(sample) * f64::from(sample))
                .sum::<f64>()
                / output.len() as f64)
                .sqrt()
        };
        let probes = probe_indices(frames, info.output_channels as usize)
            .into_iter()
            .map(|at| f64::from(output[at]))
            .collect();

        blocks.push(BlockTrace {
            index: index as u32,
            frames: frames as u32,
            peak,
            rms,
            probes,
        });
    }

    let mut final_params = Vec::new();
    for index in 0..plugin.param_count() {
        if let Some(param) = plugin.param_info(index) {
            if let Some(value) = plugin.param_get(param.id) {
                final_params.push((param.id, value));
            }
        }
    }

    Ok(Trace {
        plugin: info.name.clone(),
        format: info.format.to_string(),
        config: *config,
        input_channels: info.input_channels,
        output_channels: info.output_channels,
        blocks,
        schedule,
        final_params,
    })
}

/// Render a trace as the text that goes in a golden file.
///
/// Every float is written as `{:.17e}` — seventeen significant digits in
/// scientific notation, which round-trips an f64 exactly. The readable-looking
/// `{:.17}` is seventeen *decimal places*, not significant digits: it renders
/// `f32::MIN_POSITIVE` as `0.00000000000000000`, so a golden written that way
/// would quietly record zero for every small sample and then match anything.
/// Losslessness wins over looking nice.
pub fn render(trace: &Trace) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "sunmao-regression-trace {TRACE_VERSION}");
    let _ = writeln!(out, "plugin {}", trace.plugin);
    let _ = writeln!(out, "format {}", trace.format);
    let _ = writeln!(out, "seed {:#018x}", trace.config.seed);
    let _ = writeln!(out, "sample_rate {:.17e}", trace.config.sample_rate);
    let _ = writeln!(out, "max_block {}", trace.config.max_block);
    let _ = writeln!(out, "blocks {}", trace.config.blocks);
    let _ = writeln!(
        out,
        "tolerance abs {:.17e} rel {:.17e}",
        trace.config.abs_tolerance, trace.config.rel_tolerance
    );
    let _ = writeln!(
        out,
        "channels {} {}",
        trace.input_channels, trace.output_channels
    );
    for moved in &trace.schedule {
        let _ = writeln!(
            out,
            "automate {} {} {} {:.17e}",
            moved.block, moved.sample_offset, moved.id, moved.value
        );
    }
    for block in &trace.blocks {
        let _ = write!(
            out,
            "block {} {} {:.17e} {:.17e}",
            block.index, block.frames, block.peak, block.rms
        );
        for probe in &block.probes {
            let _ = write!(out, " {probe:.17e}");
        }
        let _ = writeln!(out);
    }
    for (id, value) in &trace.final_params {
        let _ = writeln!(out, "final {id} {value:.17e}");
    }
    out
}

/// One disagreement between two traces.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub what: String,
    pub expected: String,
    pub actual: String,
    /// `None` for fields that are not numbers, where any difference counts.
    pub deviation: Option<f64>,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: expected {}, got {}",
            self.what, self.expected, self.actual
        )?;
        if let Some(deviation) = self.deviation {
            write!(f, " (off by {deviation:e})")?;
        }
        Ok(())
    }
}

/// Whether two numbers agree under a stated tolerance.
///
/// Both bounds are checked and either one passing is enough: the absolute bound
/// carries values near zero, where a relative bound is meaningless, and the
/// relative bound carries large values, where an absolute one would be
/// unreasonably strict.
pub fn within(expected: f64, actual: f64, abs: f64, rel: f64) -> bool {
    if expected == actual {
        return true;
    }
    if !expected.is_finite() || !actual.is_finite() {
        return false;
    }
    let difference = (expected - actual).abs();
    difference <= abs || difference <= rel * expected.abs().max(actual.abs())
}

/// Compare a run against a golden trace.
///
/// Returns every difference found rather than the first, and reports the worst
/// deviation seen even when everything passed — a tolerance nobody can see the
/// margin on is a tolerance nobody can set correctly.
pub fn compare(golden: &Trace, actual: &Trace) -> (Vec<Difference>, f64) {
    let abs = golden.config.abs_tolerance;
    let rel = golden.config.rel_tolerance;
    let mut differences = Vec::new();
    let mut worst = 0.0f64;

    let mut exact =
        |what: &str, expected: String, got: String, differences: &mut Vec<Difference>| {
            if expected != got {
                differences.push(Difference {
                    what: what.to_string(),
                    expected,
                    actual: got,
                    deviation: None,
                });
            }
        };
    exact(
        "format",
        golden.format.clone(),
        actual.format.clone(),
        &mut differences,
    );
    exact(
        "seed",
        format!("{:#018x}", golden.config.seed),
        format!("{:#018x}", actual.config.seed),
        &mut differences,
    );
    exact(
        "max_block",
        golden.config.max_block.to_string(),
        actual.config.max_block.to_string(),
        &mut differences,
    );
    exact(
        "blocks",
        golden.config.blocks.to_string(),
        actual.config.blocks.to_string(),
        &mut differences,
    );
    exact(
        "channels",
        format!("{} {}", golden.input_channels, golden.output_channels),
        format!("{} {}", actual.input_channels, actual.output_channels),
        &mut differences,
    );
    exact(
        "automation schedule length",
        golden.schedule.len().to_string(),
        actual.schedule.len().to_string(),
        &mut differences,
    );
    exact(
        "block count",
        golden.blocks.len().to_string(),
        actual.blocks.len().to_string(),
        &mut differences,
    );

    let mut check = |what: String, expected: f64, got: f64, differences: &mut Vec<Difference>| {
        let deviation = (expected - got).abs();
        if deviation.is_finite() {
            worst = worst.max(deviation);
        }
        if !within(expected, got, abs, rel) {
            differences.push(Difference {
                what,
                expected: format!("{expected:.17e}"),
                actual: format!("{got:.17e}"),
                deviation: Some(deviation),
            });
        }
    };

    for (expected, got) in golden.blocks.iter().zip(&actual.blocks) {
        if expected.frames != got.frames {
            differences.push(Difference {
                what: format!("block {} frames", expected.index),
                expected: expected.frames.to_string(),
                actual: got.frames.to_string(),
                deviation: None,
            });
            continue;
        }
        check(
            format!("block {} peak", expected.index),
            expected.peak,
            got.peak,
            &mut differences,
        );
        check(
            format!("block {} rms", expected.index),
            expected.rms,
            got.rms,
            &mut differences,
        );
        for (probe, (expected_probe, got_probe)) in
            expected.probes.iter().zip(&got.probes).enumerate()
        {
            check(
                format!("block {} probe {probe}", expected.index),
                *expected_probe,
                *got_probe,
                &mut differences,
            );
        }
    }

    for (expected, got) in golden.final_params.iter().zip(&actual.final_params) {
        if expected.0 != got.0 {
            differences.push(Difference {
                what: "final parameter id".into(),
                expected: expected.0.to_string(),
                actual: got.0.to_string(),
                deviation: None,
            });
            continue;
        }
        check(
            format!("final parameter {}", expected.0),
            expected.1,
            got.1,
            &mut differences,
        );
    }

    (differences, worst)
}

/// Parse a golden trace back from its text form.
pub fn parse(text: &str) -> Result<Trace, String> {
    let mut config = RunConfig::default();
    let mut plugin = String::new();
    let mut format = String::new();
    let mut input_channels = 0;
    let mut output_channels = 0;
    let mut blocks = Vec::new();
    let mut schedule = Vec::new();
    let mut final_params = Vec::new();
    let mut saw_header = false;

    let number = |text: &str, what: &str| -> Result<f64, String> {
        text.parse::<f64>()
            .map_err(|_| format!("{what}: '{text}' is not a number"))
    };

    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let at = format!("line {}", line_number + 1);
        match words[0] {
            "sunmao-regression-trace" => {
                let version: u32 = words
                    .get(1)
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| format!("{at}: missing trace version"))?;
                if version != TRACE_VERSION {
                    return Err(format!(
                        "{at}: trace version {version}, this build writes {TRACE_VERSION}"
                    ));
                }
                saw_header = true;
            }
            "plugin" => plugin = words[1..].join(" "),
            "format" => format = words.get(1).unwrap_or(&"").to_string(),
            "seed" => {
                let text = words.get(1).ok_or_else(|| format!("{at}: missing seed"))?;
                let stripped = text.strip_prefix("0x").unwrap_or(text);
                config.seed = u64::from_str_radix(stripped, 16)
                    .map_err(|_| format!("{at}: '{text}' is not a hexadecimal seed"))?;
            }
            "sample_rate" => config.sample_rate = number(words[1], &at)?,
            "max_block" => config.max_block = number(words[1], &at)? as u32,
            "blocks" => config.blocks = number(words[1], &at)? as u32,
            "tolerance" => {
                // `tolerance abs <x> rel <y>`
                if words.len() != 5 || words[1] != "abs" || words[3] != "rel" {
                    return Err(format!("{at}: expected 'tolerance abs <x> rel <y>'"));
                }
                config.abs_tolerance = number(words[2], &at)?;
                config.rel_tolerance = number(words[4], &at)?;
                if config.abs_tolerance < 0.0 || config.rel_tolerance < 0.0 {
                    return Err(format!("{at}: a tolerance cannot be negative"));
                }
            }
            "channels" => {
                input_channels = number(words[1], &at)? as u32;
                output_channels = number(words[2], &at)? as u32;
            }
            "automate" => {
                if words.len() != 5 {
                    return Err(format!(
                        "{at}: expected 'automate <block> <offset> <id> <value>'"
                    ));
                }
                schedule.push(ScheduledParam {
                    block: number(words[1], &at)? as u32,
                    sample_offset: number(words[2], &at)? as u32,
                    id: number(words[3], &at)? as u32,
                    value: number(words[4], &at)?,
                });
            }
            "block" => {
                if words.len() < 5 {
                    return Err(format!(
                        "{at}: a block line needs index, frames, peak and rms"
                    ));
                }
                blocks.push(BlockTrace {
                    index: number(words[1], &at)? as u32,
                    frames: number(words[2], &at)? as u32,
                    peak: number(words[3], &at)?,
                    rms: number(words[4], &at)?,
                    probes: words[5..]
                        .iter()
                        .map(|probe| number(probe, &at))
                        .collect::<Result<Vec<f64>, String>>()?,
                });
            }
            "final" => {
                if words.len() != 3 {
                    return Err(format!("{at}: expected 'final <id> <value>'"));
                }
                final_params.push((number(words[1], &at)? as u32, number(words[2], &at)?));
            }
            other => return Err(format!("{at}: unknown trace record '{other}'")),
        }
    }

    if !saw_header {
        return Err("not a regression trace: missing the version line".into());
    }
    if blocks.is_empty() {
        return Err("regression trace has no block records".into());
    }

    Ok(Trace {
        plugin,
        format,
        config,
        input_channels,
        output_channels,
        blocks,
        schedule,
        final_params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trace() -> Trace {
        let config = RunConfig::default();
        Trace {
            plugin: "SunMao Gain".into(),
            format: "VST3".into(),
            config,
            input_channels: 2,
            output_channels: 2,
            blocks: vec![
                BlockTrace {
                    index: 0,
                    frames: 512,
                    peak: 0.5,
                    rms: 0.25,
                    probes: vec![0.1, -0.2, 0.3, -0.4],
                },
                BlockTrace {
                    index: 1,
                    frames: 7,
                    peak: 0.125,
                    rms: 0.0625,
                    probes: vec![0.0, 0.0, 0.0, 0.0],
                },
            ],
            schedule: vec![ScheduledParam {
                block: 1,
                sample_offset: 3,
                id: 42,
                value: 0.75,
            }],
            final_params: vec![(42, 0.75)],
        }
    }

    #[test]
    fn a_trace_survives_a_round_trip_through_its_text_form() {
        let trace = sample_trace();
        let parsed = parse(&render(&trace)).expect("round trip");
        assert_eq!(parsed, trace);
    }

    /// The seed is the whole contract. If the same seed ever produced a
    /// different division, every golden in the repository would be worthless.
    #[test]
    fn the_same_seed_always_divides_blocks_the_same_way() {
        let config = RunConfig::default();
        assert_eq!(block_sizes(&config), block_sizes(&config));
    }

    /// The other half of the contract, and the one that was briefly broken:
    /// seeding with `seed | 1` threw away the low bit, so neighbouring seeds
    /// produced byte-identical runs. Two seeds that silently mean the same run
    /// make a regression suite look broader than it is.
    #[test]
    fn neighbouring_seeds_produce_different_runs() {
        let base = RunConfig::default();
        let mut seen = std::collections::HashSet::new();
        for offset in 0..8u64 {
            let config = RunConfig {
                seed: base.seed ^ offset,
                ..base
            };
            assert!(
                seen.insert(block_sizes(&config)),
                "seed ^ {offset} collided with an earlier seed"
            );
        }
    }

    /// A block larger than the size the plugin was initialized for is an
    /// out-of-contract call, and a zero-frame block is not a test of anything.
    #[test]
    fn every_block_is_within_the_size_the_plugin_was_promised() {
        for seed in [0u64, 1, 7, 0x5150_4D41_4F53_4D31, u64::MAX] {
            let config = RunConfig {
                seed,
                ..RunConfig::default()
            };
            let sizes = block_sizes(&config);
            assert_eq!(sizes.len(), config.blocks as usize);
            assert!(sizes
                .iter()
                .all(|&size| size >= 1 && size <= config.max_block));
        }
    }

    /// Leaving the extremes to chance means some seeds never exercise them.
    #[test]
    fn the_division_always_covers_the_largest_and_smallest_block() {
        let config = RunConfig::default();
        let sizes = block_sizes(&config);
        assert_eq!(sizes.first().copied(), Some(config.max_block));
        assert_eq!(sizes.last().copied(), Some(1));
    }

    #[test]
    fn automation_lands_inside_the_block_it_is_scheduled_for() {
        let config = RunConfig::default();
        let sizes = block_sizes(&config);
        let schedule = automation(&config, &[(1, 0.0, 1.0), (2, -3.0, 3.0)], &sizes);
        assert!(!schedule.is_empty(), "a 24-block run should move something");
        for moved in &schedule {
            let frames = sizes[moved.block as usize];
            assert!(
                moved.sample_offset < frames,
                "offset {} is outside a {frames}-frame block",
                moved.sample_offset
            );
            let (min, max) = if moved.id == 1 {
                (0.0, 1.0)
            } else {
                (-3.0, 3.0)
            };
            assert!(moved.value >= min && moved.value <= max, "{moved:?}");
        }
    }

    #[test]
    fn a_plugin_with_no_automatable_parameters_gets_an_empty_schedule() {
        let config = RunConfig::default();
        assert!(automation(&config, &[], &block_sizes(&config)).is_empty());
    }

    /// Both bounds have to be live. An absolute-only rule is useless for large
    /// values and a relative-only rule is useless near zero.
    #[test]
    fn the_tolerance_uses_whichever_bound_is_meaningful() {
        assert!(within(0.0, 1e-9, 1e-6, 1e-6), "absolute bound carries zero");
        assert!(!within(0.0, 1e-3, 1e-6, 1e-6));
        assert!(
            within(1e6, 1e6 + 0.5, 1e-6, 1e-6),
            "relative bound carries large values"
        );
        assert!(!within(1e6, 1e6 + 5.0, 1e-6, 1e-6));
        assert!(
            within(1.0, 1.0, 0.0, 0.0),
            "equality needs no tolerance at all"
        );
    }

    #[test]
    fn a_non_finite_value_never_compares_equal() {
        assert!(!within(f64::NAN, f64::NAN, 1.0, 1.0));
        assert!(!within(1.0, f64::INFINITY, f64::MAX, f64::MAX));
    }

    #[test]
    fn an_identical_run_reports_no_differences() {
        let trace = sample_trace();
        let (differences, worst) = compare(&trace, &trace);
        assert!(differences.is_empty(), "{differences:?}");
        assert_eq!(worst, 0.0);
    }

    /// The guard has to be able to fail, and it has to say by how much.
    #[test]
    fn a_changed_sample_is_caught_and_the_margin_is_reported() {
        let golden = sample_trace();
        let mut actual = sample_trace();
        actual.blocks[0].probes[2] += 0.01;
        let (differences, worst) = compare(&golden, &actual);
        assert_eq!(differences.len(), 1, "{differences:?}");
        assert!(differences[0].what.contains("block 0 probe 2"));
        assert!((worst - 0.01).abs() < 1e-9, "worst was {worst}");
    }

    /// A drift smaller than the stated tolerance is not a regression, and the
    /// reported margin still has to show how close it came.
    #[test]
    fn a_drift_under_the_tolerance_passes_but_is_still_measured() {
        let golden = sample_trace();
        let mut actual = sample_trace();
        actual.blocks[0].peak += 1e-9;
        let (differences, worst) = compare(&golden, &actual);
        assert!(differences.is_empty(), "{differences:?}");
        assert!(
            worst > 0.0,
            "a real difference was reported as no margin at all"
        );
    }

    #[test]
    fn a_different_block_division_is_caught_before_any_sample_is_compared() {
        let golden = sample_trace();
        let mut actual = sample_trace();
        actual.blocks[1].frames = 8;
        let (differences, _) = compare(&golden, &actual);
        assert!(
            differences.iter().any(|d| d.what.contains("frames")),
            "{differences:?}"
        );
    }

    #[test]
    fn comparing_across_formats_or_seeds_is_refused() {
        let golden = sample_trace();
        let mut actual = sample_trace();
        actual.format = "CLAP".into();
        actual.config.seed ^= 1;
        let (differences, _) = compare(&golden, &actual);
        assert!(differences.iter().any(|d| d.what == "format"));
        assert!(differences.iter().any(|d| d.what == "seed"));
    }

    #[test]
    fn a_trace_from_a_future_version_is_refused_rather_than_misread() {
        let text = render(&sample_trace()).replace(
            &format!("sunmao-regression-trace {TRACE_VERSION}"),
            &format!("sunmao-regression-trace {}", TRACE_VERSION + 1),
        );
        let error = parse(&text).expect_err("must refuse");
        assert!(error.contains("trace version"), "{error}");
    }

    #[test]
    fn malformed_traces_are_refused_with_a_reason() {
        for (text, expected) in [
            ("", "missing the version line"),
            ("plugin x\nblock 0 1 0 0\n", "missing the version line"),
            (
                &format!("sunmao-regression-trace {TRACE_VERSION}\n"),
                "no block records",
            ),
            (
                &format!("sunmao-regression-trace {TRACE_VERSION}\nwiggle 1\n"),
                "unknown trace record",
            ),
            (
                &format!("sunmao-regression-trace {TRACE_VERSION}\ntolerance abs -1 rel 0\nblock 0 1 0 0\n"),
                "cannot be negative",
            ),
            (
                &format!("sunmao-regression-trace {TRACE_VERSION}\ntolerance 1e-6\nblock 0 1 0 0\n"),
                "expected 'tolerance abs",
            ),
        ] {
            let error = parse(text).expect_err(&format!("{text:?} should be refused"));
            assert!(error.contains(expected), "{text:?} gave {error}");
        }
    }

    /// Seventeen significant digits is what makes a golden lossless. At fewer,
    /// writing and re-reading a trace would introduce a difference the
    /// comparison would then blame on the plugin.
    #[test]
    fn the_text_form_preserves_every_bit_of_an_awkward_double() {
        let mut trace = sample_trace();
        trace.blocks[0].rms = 0.1 + 0.2;
        trace.blocks[0].probes[0] = f64::from(f32::MIN_POSITIVE);
        trace.blocks[0].probes[1] = -0.000_000_000_123_456_789;
        let parsed = parse(&render(&trace)).expect("round trip");
        assert_eq!(
            parsed.blocks[0].rms.to_bits(),
            trace.blocks[0].rms.to_bits()
        );
        for (a, b) in parsed.blocks[0].probes.iter().zip(&trace.blocks[0].probes) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn probe_positions_stay_inside_the_buffer_for_any_block_size() {
        for frames in [1usize, 2, 3, 7, 511, 512] {
            for channels in [1usize, 2] {
                for index in probe_indices(frames, channels) {
                    assert!(
                        index < frames * channels,
                        "{frames}x{channels} gave {index}"
                    );
                }
            }
        }
        assert!(probe_indices(0, 2).is_empty());
        assert!(probe_indices(64, 0).is_empty());
    }
}

/// Entry point for the `regress` subcommand.
pub fn cmd_regress(args: &[String]) -> bool {
    let mut config = RunConfig::default();
    let mut path = None;
    let mut write_to = None;
    let mut golden = None;
    let mut index = 0;

    macro_rules! take {
        ($what:literal, $parse:expr) => {{
            index += 1;
            match args.get(index).and_then($parse) {
                Some(value) => value,
                None => {
                    eprintln!("--{} needs a valid value", $what);
                    return false;
                }
            }
        }};
    }

    while index < args.len() {
        match args[index].as_str() {
            "--seed" => {
                config.seed = take!("seed", |value: &String| {
                    let stripped = value.strip_prefix("0x");
                    match stripped {
                        Some(hex) => u64::from_str_radix(hex, 16).ok(),
                        None => value.parse::<u64>().ok(),
                    }
                })
            }
            "--sample-rate" => {
                config.sample_rate = take!("sample-rate", |value: &String| value
                    .parse::<f64>()
                    .ok()
                    .filter(|rate| *rate > 0.0))
            }
            "--block-size" => {
                config.max_block = take!("block-size", |value: &String| value
                    .parse::<u32>()
                    .ok()
                    .filter(|size| *size > 0))
            }
            "--blocks" => {
                config.blocks = take!("blocks", |value: &String| value
                    .parse::<u32>()
                    .ok()
                    .filter(|count| *count > 0))
            }
            "--tolerance-abs" => {
                config.abs_tolerance = take!("tolerance-abs", |value: &String| value
                    .parse::<f64>()
                    .ok()
                    .filter(|bound| *bound >= 0.0))
            }
            "--tolerance-rel" => {
                config.rel_tolerance = take!("tolerance-rel", |value: &String| value
                    .parse::<f64>()
                    .ok()
                    .filter(|bound| *bound >= 0.0))
            }
            "--write" => write_to = Some(take!("write", |value: &String| Some(value.clone()))),
            "--golden" => golden = Some(take!("golden", |value: &String| Some(value.clone()))),
            other if other.starts_with("--") => {
                eprintln!("Unknown option: {other}");
                return false;
            }
            other => path = Some(other.to_string()),
        }
        index += 1;
    }

    let Some(path) = path else {
        eprintln!(
            "Usage: sunmao_unittest_runner regress [--seed N] [--sample-rate HZ] [--block-size N]\n\
             \x20                                  [--blocks N] [--tolerance-abs X] [--tolerance-rel X]\n\
             \x20                                  [--write TRACE] [--golden TRACE] <plugin_path>"
        );
        return false;
    };

    let plugins = match crate::scan_plugin_path(&path) {
        Some(plugins) => plugins,
        None => return false,
    };
    if plugins.is_empty() {
        eprintln!("No plugins found in {path}");
        return false;
    }

    let mut plugin = match crate::load_plugin(&plugins[0]) {
        Ok(plugin) => plugin,
        Err(error) => {
            eprintln!("Failed to load plugin: {error}");
            return false;
        }
    };
    if let Err(error) = plugin.initialize(config.sample_rate, config.max_block) {
        eprintln!("Failed to initialize: {error}");
        plugin.shutdown();
        return false;
    }

    let trace = run(plugin.as_mut(), &config);
    plugin.shutdown();
    let trace = match trace {
        Ok(trace) => trace,
        Err(error) => {
            eprintln!("Regression run failed: {error}");
            return false;
        }
    };

    println!(
        "regression run: {} ({}), seed {:#018x}, {} blocks, {} frames total",
        trace.plugin,
        trace.format,
        trace.config.seed,
        trace.blocks.len(),
        trace
            .blocks
            .iter()
            .map(|block| block.frames as u64)
            .sum::<u64>()
    );

    if let Some(target) = &write_to {
        if let Err(error) = std::fs::write(target, render(&trace)) {
            eprintln!("Failed to write {target}: {error}");
            return false;
        }
        println!("trace written: {target}");
    }

    let Some(golden_path) = golden else {
        if write_to.is_none() {
            print!("{}", render(&trace));
        }
        println!("REGRESSION RUN COMPLETE: no golden supplied, nothing compared");
        return true;
    };

    let text = match std::fs::read_to_string(&golden_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("Failed to read {golden_path}: {error}");
            return false;
        }
    };
    let golden_trace = match parse(&text) {
        Ok(trace) => trace,
        Err(error) => {
            eprintln!("{golden_path}: {error}");
            return false;
        }
    };

    let (differences, worst) = compare(&golden_trace, &trace);
    println!(
        "compared against {golden_path} under abs {:e} / rel {:e}; worst deviation {worst:e}",
        golden_trace.config.abs_tolerance, golden_trace.config.rel_tolerance
    );
    if differences.is_empty() {
        println!(
            "REGRESSION MATCHED GOLDEN: {} blocks, {} comparisons, worst deviation {worst:e}",
            trace.blocks.len(),
            trace.blocks.len() * (2 + PROBES_PER_BLOCK) + trace.final_params.len()
        );
        true
    } else {
        for difference in differences.iter().take(20) {
            println!("DIFF {difference}");
        }
        if differences.len() > 20 {
            println!("DIFF ... and {} more", differences.len() - 20);
        }
        println!(
            "REGRESSION DIFFERS FROM GOLDEN: {} differences, worst deviation {worst:e}",
            differences.len()
        );
        false
    }
}
