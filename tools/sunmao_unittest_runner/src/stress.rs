//! Repeated-lifecycle stress, for leaks that only show up on the Nth time.
//!
//! Plugins get loaded and unloaded, and editors get opened and closed, far more
//! often than a single test does either. A handle that is not released, a
//! window that is not destroyed, or a module that keeps a little state per
//! instantiation costs nothing once and takes a session down after an hour of
//! ordinary use. The only way to see that is to do it many times and watch what
//! the operating system says about the process.
//!
//! What this deliberately does **not** do is unload plugin modules.
//! `load_plugin_library` keeps them mapped on purpose — a plugin that has
//! initialised a GPU backend is not safely unloadable, and run #65 proved it by
//! passing every GUI assertion and then faulting on the way out. So the
//! instantiate/destroy loop measures repeated *instantiation*, which is the
//! part a host really does repeatedly, and the module stays resident exactly as
//! it does in a real host.

use crate::rss::{self, GrowthVerdict};

/// A scoped Cocoa autorelease pool.
///
/// Without one of these around each iteration, this whole module measures the
/// wrong thing on macOS. Cocoa hands back autoreleased objects — windows,
/// views, strings — that are only freed when the enclosing pool drains, which
/// in an app happens once per run-loop turn. A tight loop that never drains one
/// accumulates every temporary it creates and then reports the pile as a leak.
/// Draining per iteration is what makes "grew by N" a claim about ownership
/// rather than about pool timing.
#[cfg(target_os = "macos")]
struct AutoreleasePool(*mut std::ffi::c_void);

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn objc_autoreleasePoolPush() -> *mut std::ffi::c_void;
    fn objc_autoreleasePoolPop(pool: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
impl AutoreleasePool {
    fn new() -> Self {
        Self(unsafe { objc_autoreleasePoolPush() })
    }
}

#[cfg(target_os = "macos")]
impl Drop for AutoreleasePool {
    fn drop(&mut self) {
        unsafe { objc_autoreleasePoolPop(self.0) }
    }
}

/// Run one iteration inside whatever per-iteration cleanup the platform needs.
fn with_iteration_scope<R>(body: impl FnOnce() -> R) -> R {
    #[cfg(target_os = "macos")]
    {
        let _pool = AutoreleasePool::new();
        body()
    }
    #[cfg(not(target_os = "macos"))]
    {
        body()
    }
}

/// Iterations discarded before the baseline reading is taken.
///
/// The first pass loads libraries, builds caches and lets the allocator claim
/// arenas. Counting that as a leak would fail every plugin on its merits.
/// Event-dispatch turns given to the window system after each lifecycle.
///
/// One is usually enough; a few costs nothing and covers platforms that split
/// teardown across more than one turn.
pub const PUMPS_PER_ITERATION: u32 = 4;

pub const DEFAULT_WARMUP: u32 = 8;
/// Measured iterations after the warm-up.
pub const DEFAULT_ITERATIONS: u32 = 64;

/// Per-iteration growth budget for the instantiate/destroy loop.
///
/// Not zero, and not a guess dressed as precision: resident size is a page-
/// granular number that moves for reasons outside this process's control, so
/// the budget has to sit above the noise floor while staying far below what a
/// real per-instance leak would produce. A plugin that leaked even a single
/// small allocation per instantiation would, over 64 iterations, have to leak
/// less than one page per iteration to slip through.
pub const INSTANCE_BUDGET_BYTES: u64 = 64 * 1024;
/// Per-iteration budget for what the *editor* adds on top of the host's own
/// window lifecycle.
///
/// This is a differential, and it has to be: an absolute budget on the editor
/// loop measures the window system, not the plugin. Measured on macOS, the
/// host's own create-window/destroy-window loop grows by ~270 KiB per
/// iteration with no editor in it at all, and ~680 KiB per iteration once the
/// loop dispatches the events that actually complete the teardown. The
/// instrument moves the number by 2.5x depending on how it is held.
///
/// Running the same window lifecycle with and without the editor cancels all
/// of that, because both loops pay it identically. What survives the
/// subtraction is what the editor itself failed to give back.
pub const EDITOR_EXCESS_BUDGET_BYTES: u64 = 64 * 1024;

/// One completed stress measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    pub iterations: u32,
    pub baseline: Option<u64>,
    pub after: Option<u64>,
    pub budget_per_iteration: u64,
}

impl Measurement {
    pub fn verdict(&self) -> GrowthVerdict {
        rss::verdict(
            self.baseline,
            self.after,
            self.iterations,
            self.budget_per_iteration,
        )
    }

    pub fn growth(&self) -> Option<u64> {
        match (self.baseline, self.after) {
            (Some(baseline), Some(after)) => Some(after.saturating_sub(baseline)),
            _ => None,
        }
    }

    /// One line a person can read and a CI step can grep.
    ///
    /// The budget and verdict are only shown for a loop that is actually
    /// judged on an absolute budget. The two GUI loops are not — they exist to
    /// be subtracted from each other — and printing a sentinel budget beside
    /// them would read as a threshold nobody is checking.
    pub fn report(&self, label: &str) -> String {
        let growth = match self.growth() {
            Some(growth) => rss::human_bytes(growth),
            None => "unmeasured".to_string(),
        };
        let per_iteration = match self.growth() {
            Some(growth) if self.iterations > 0 => {
                rss::human_bytes(growth / u64::from(self.iterations))
            }
            _ => "unmeasured".to_string(),
        };
        if self.budget_per_iteration == u64::MAX {
            format!(
                "{label}: {} iterations, grew {growth} ({per_iteration}/iteration, not judged on its own)",
                self.iterations
            )
        } else {
            format!(
                "{label}: {} iterations, grew {growth} ({per_iteration}/iteration, budget {}/iteration) -> {:?}",
                self.iterations,
                rss::human_bytes(self.budget_per_iteration),
                self.verdict()
            )
        }
    }
}

/// What the editor cost beyond the window lifecycle, and whether that is
/// acceptable.
///
/// Separated from the loops so the arithmetic can be tested without a window
/// system: this is the number the CI step turns red on, and the one place a
/// sign error would quietly make the check unfailable.
pub fn editor_excess(
    window_only: &Measurement,
    with_editor: &Measurement,
    budget_per_iteration: u64,
) -> Option<(u64, bool)> {
    let (Some(without), Some(with)) = (window_only.growth(), with_editor.growth()) else {
        return None;
    };
    // Saturating: an editor run that grew *less* than the bare window run costs
    // nothing, and must not bank the difference as credit.
    let excess = with.saturating_sub(without);
    let iterations = with_editor.iterations;
    if iterations == 0 {
        return None;
    }
    let allowed = budget_per_iteration.saturating_mul(u64::from(iterations));
    Some((excess, excess <= allowed))
}

/// Run `body` `warmup + iterations` times, measuring only the tail.
///
/// The closure returns a `Result` so a lifecycle failure stops the run rather
/// than being averaged into a memory number.
pub fn measure(
    warmup: u32,
    iterations: u32,
    budget_per_iteration: u64,
    mut body: impl FnMut(u32) -> Result<(), String>,
) -> Result<Measurement, String> {
    for index in 0..warmup {
        with_iteration_scope(|| body(index))?;
    }
    let baseline = rss::resident_bytes();
    for index in 0..iterations {
        with_iteration_scope(|| body(warmup + index))?;
    }
    let after = rss::resident_bytes();
    Ok(Measurement {
        iterations,
        baseline,
        after,
        budget_per_iteration,
    })
}

/// Entry point for the `stress` subcommand.
pub fn cmd_stress(args: &[String]) -> bool {
    let mut warmup = DEFAULT_WARMUP;
    let mut iterations = DEFAULT_ITERATIONS;
    let mut editor = false;
    let mut window_only = false;
    let mut path = None;
    let mut instance_budget = INSTANCE_BUDGET_BYTES;
    // Absolute cap inside each GUI loop is deliberately generous; the
    // judgement is made on the differential below.
    let editor_budget = u64::MAX;
    let mut editor_excess_budget = EDITOR_EXCESS_BUDGET_BYTES;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--iterations" => {
                index += 1;
                match args.get(index).and_then(|value| value.parse::<u32>().ok()) {
                    Some(value) if value > 0 => iterations = value,
                    _ => {
                        eprintln!("--iterations needs a positive integer");
                        return false;
                    }
                }
            }
            "--warmup" => {
                index += 1;
                match args.get(index).and_then(|value| value.parse::<u32>().ok()) {
                    Some(value) => warmup = value,
                    None => {
                        eprintln!("--warmup needs a non-negative integer");
                        return false;
                    }
                }
            }
            "--instance-budget" => {
                index += 1;
                match args.get(index).and_then(|value| value.parse::<u64>().ok()) {
                    Some(value) => instance_budget = value,
                    None => {
                        eprintln!("--instance-budget needs a byte count");
                        return false;
                    }
                }
            }
            "--editor-excess-budget" => {
                index += 1;
                match args.get(index).and_then(|value| value.parse::<u64>().ok()) {
                    Some(value) => editor_excess_budget = value,
                    None => {
                        eprintln!("--editor-excess-budget needs a byte count");
                        return false;
                    }
                }
            }
            "--editor" => editor = true,
            // Attribution, not decoration: when the editor loop grows, the
            // first question is whether the plugin's editor or the host's own
            // window is responsible, and the only way to answer it is to run
            // the window lifecycle with no editor in it.
            "--window-only" => {
                editor = true;
                window_only = true;
            }
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
            "Usage: sunmao_unittest_runner stress [--iterations N] [--warmup N] [--editor]\n\
             \x20                                 [--instance-budget BYTES] [--editor-excess-budget BYTES] <plugin_path>"
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
    let info = plugins[0].clone();
    println!(
        "stress: {} ({}), {warmup} warm-up + {iterations} measured iterations",
        info.name, info.format
    );

    // Scan, instantiate, initialize, process one block, destroy.
    let scan_measurement = match measure(warmup, iterations, instance_budget, |_| {
        let found = crate::scan_plugin_path(&path).ok_or("scan failed")?;
        if found.is_empty() {
            return Err("scan found no plugins".into());
        }
        let mut plugin = crate::load_plugin(&found[0])?;
        plugin.initialize(48000.0, 256)?;
        let runtime = plugin.info().clone();
        let input = vec![0.0f32; 256 * runtime.input_channels as usize];
        let mut output = vec![0.0f32; 256 * runtime.output_channels as usize];
        plugin.process(&input, &mut output)?;
        plugin.shutdown();
        drop(plugin);
        Ok(())
    }) {
        Ok(measurement) => measurement,
        Err(error) => {
            eprintln!("scan/instantiate/destroy loop failed: {error}");
            return false;
        }
    };
    println!("{}", scan_measurement.report("scan-instantiate-destroy"));

    let mut gui_measurements: Option<(Measurement, Measurement)> = None;
    if editor {
        if let Err(error) = crate::gui_window::initialize_platform() {
            eprintln!("Failed to initialize the window system: {error}");
            return false;
        }
        let mut plugin = match crate::load_plugin(&info) {
            Ok(plugin) => plugin,
            Err(error) => {
                eprintln!("Failed to load plugin: {error}");
                return false;
            }
        };
        if let Err(error) = plugin.initialize(48000.0, 256) {
            eprintln!("Failed to initialize: {error}");
            plugin.shutdown();
            return false;
        }
        let title = format!("SunMao stress - {}", info.name);

        // Two loops, identical but for the editor. Each gets its own warm-up so
        // neither is charged for the other's caches.
        let mut run_loop = |open_editor: bool, plugin: &mut Box<dyn crate::host::HostPlugin>| {
            measure(warmup, iterations, editor_budget, |_| {
                let window =
                    crate::gui_window::PluginGuiWindow::new(&title, 400.0, 300.0, Box::new(|| {}))?;
                if open_editor {
                    plugin.open_gui(&window)?;
                    // The editor is parented into this window, so it has to be
                    // told to let go before the window goes away.
                    plugin.close_gui();
                }
                drop(window);
                // Let the window system finish the teardown it was just asked
                // for. Destroying a window is a request, not an act: AppKit,
                // X11 and Win32 all complete it while dispatching events, so a
                // loop that never dispatches any measures a queue of pending
                // destructions and calls the backlog a leak.
                for _ in 0..PUMPS_PER_ITERATION {
                    crate::gui_window::PluginGuiWindow::pump_events();
                }
                Ok(())
            })
        };

        let window_only_measurement = match run_loop(false, &mut plugin) {
            Ok(measurement) => measurement,
            Err(error) => {
                plugin.shutdown();
                eprintln!("window open/close loop failed: {error}");
                return false;
            }
        };
        println!("{}", window_only_measurement.report("window-open-close"));

        if window_only {
            plugin.shutdown();
            println!("STRESS BASELINE ONLY: window lifecycle measured without an editor");
            return true;
        }

        let editor_measurement = match run_loop(true, &mut plugin) {
            Ok(measurement) => measurement,
            Err(error) => {
                plugin.shutdown();
                eprintln!("editor open/close loop failed: {error}");
                return false;
            }
        };
        plugin.shutdown();
        println!("{}", editor_measurement.report("editor-open-close"));

        gui_measurements = Some((window_only_measurement, editor_measurement));
    }

    let mut ok = true;
    match scan_measurement.verdict() {
        GrowthVerdict::Stable => {}
        GrowthVerdict::Leaking => {
            println!("STRESS LEAK DETECTED: scan-instantiate-destroy");
            ok = false;
        }
        GrowthVerdict::Unmeasured => {
            println!(
                "STRESS UNMEASURED: scan-instantiate-destroy -- this platform did not report resident memory"
            );
            ok = false;
        }
    }

    if let Some((window_only_measurement, editor_measurement)) = gui_measurements {
        match editor_excess(
            &window_only_measurement,
            &editor_measurement,
            editor_excess_budget,
        ) {
            None => {
                println!(
                    "STRESS UNMEASURED: editor-excess -- this platform did not report resident memory"
                );
                ok = false;
            }
            Some((excess, within_budget)) => {
                println!(
                    "editor-excess: {} beyond the window lifecycle over {iterations} iterations ({}/iteration, budget {}/iteration)",
                    rss::human_bytes(excess),
                    rss::human_bytes(excess / u64::from(iterations.max(1))),
                    rss::human_bytes(editor_excess_budget)
                );
                if !within_budget {
                    println!("STRESS LEAK DETECTED: editor-excess");
                    ok = false;
                }
            }
        }
    }

    if ok {
        println!("STRESS VERIFIED: no growth beyond budget over {iterations} iterations");
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warmup_runs_before_the_baseline_is_taken() {
        let mut seen = Vec::new();
        let measurement = measure(3, 5, 0, |index| {
            seen.push(index);
            Ok(())
        })
        .expect("run");
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            measurement.iterations, 5,
            "only the tail is the measurement"
        );
    }

    #[test]
    fn a_failing_iteration_stops_the_run_rather_than_being_averaged_in() {
        let mut calls = 0;
        let error = measure(0, 10, 0, |index| {
            calls += 1;
            if index == 3 {
                Err("boom".into())
            } else {
                Ok(())
            }
        })
        .expect_err("must propagate");
        assert_eq!(error, "boom");
        assert_eq!(calls, 4, "iteration 4 onwards must not run");
    }

    #[test]
    fn a_failure_during_warmup_stops_the_run_too() {
        let error = measure(5, 10, 0, |index| {
            if index == 1 {
                Err("early".into())
            } else {
                Ok(())
            }
        })
        .expect_err("must propagate");
        assert_eq!(error, "early");
    }

    #[test]
    fn a_report_line_names_the_budget_and_the_verdict() {
        let measurement = Measurement {
            iterations: 64,
            baseline: Some(1_000_000),
            after: Some(1_000_000),
            budget_per_iteration: INSTANCE_BUDGET_BYTES,
        };
        let line = measurement.report("scan-instantiate-destroy");
        assert!(line.contains("64 iterations"), "{line}");
        assert!(line.contains("Stable"), "{line}");
        assert!(line.contains("64.00 KiB/iteration"), "{line}");
    }

    #[test]
    fn an_unmeasurable_run_says_so_in_its_report() {
        let measurement = Measurement {
            iterations: 64,
            baseline: None,
            after: None,
            budget_per_iteration: INSTANCE_BUDGET_BYTES,
        };
        assert_eq!(measurement.verdict(), GrowthVerdict::Unmeasured);
        assert!(measurement.report("x").contains("unmeasured"));
    }

    /// The budgets are a contract with the CI step, so a change to them should
    /// be a deliberate edit here rather than a silent drift.
    fn measurement(growth: u64, iterations: u32) -> Measurement {
        Measurement {
            iterations,
            baseline: Some(1_000_000),
            after: Some(1_000_000 + growth),
            budget_per_iteration: u64::MAX,
        }
    }

    /// The differential is the whole point: the platform noise both loops pay
    /// has to cancel, leaving only what the editor failed to give back.
    #[test]
    fn the_editor_is_charged_only_for_what_it_adds() {
        // 21 MiB of window-system growth in both loops, and nothing else.
        let window = measurement(21 * 1024 * 1024, 32);
        let editor = measurement(21 * 1024 * 1024, 32);
        assert_eq!(
            editor_excess(&window, &editor, EDITOR_EXCESS_BUDGET_BYTES),
            Some((0, true)),
            "identical loops must net to zero no matter how large the shared cost"
        );
    }

    /// And it must still be able to fail.
    #[test]
    fn an_editor_that_keeps_memory_is_caught_through_the_noise() {
        let window = measurement(21 * 1024 * 1024, 32);
        // 4 MiB more than the bare window loop over 32 iterations is 128 KiB
        // per iteration, twice the budget.
        let editor = measurement(21 * 1024 * 1024 + 4 * 1024 * 1024, 32);
        let (excess, within) =
            editor_excess(&window, &editor, EDITOR_EXCESS_BUDGET_BYTES).expect("measured");
        assert_eq!(excess, 4 * 1024 * 1024);
        assert!(
            !within,
            "128 KiB/iteration must not pass a 64 KiB/iteration budget"
        );
    }

    #[test]
    fn the_differential_budget_boundary_is_inclusive() {
        let window = measurement(0, 32);
        let exactly = measurement(EDITOR_EXCESS_BUDGET_BYTES * 32, 32);
        assert_eq!(
            editor_excess(&window, &exactly, EDITOR_EXCESS_BUDGET_BYTES).map(|r| r.1),
            Some(true)
        );
        let one_over = measurement(EDITOR_EXCESS_BUDGET_BYTES * 32 + 1, 32);
        assert_eq!(
            editor_excess(&window, &one_over, EDITOR_EXCESS_BUDGET_BYTES).map(|r| r.1),
            Some(false)
        );
    }

    /// An editor loop that grew less than the bare window loop has not earned
    /// credit it can spend later.
    #[test]
    fn an_editor_cheaper_than_the_bare_window_earns_no_credit() {
        let window = measurement(10 * 1024 * 1024, 32);
        let editor = measurement(1024 * 1024, 32);
        assert_eq!(editor_excess(&window, &editor, 0), Some((0, true)));
    }

    #[test]
    fn an_unmeasured_pair_concludes_nothing() {
        let measured = measurement(0, 32);
        let unmeasured = Measurement {
            iterations: 32,
            baseline: None,
            after: None,
            budget_per_iteration: u64::MAX,
        };
        assert_eq!(editor_excess(&unmeasured, &measured, 0), None);
        assert_eq!(editor_excess(&measured, &unmeasured, 0), None);
    }

    #[test]
    fn a_loop_that_is_not_judged_says_so_instead_of_printing_a_sentinel() {
        let line = measurement(1024, 32).report("window-open-close");
        assert!(line.contains("not judged on its own"), "{line}");
        assert!(
            !line.contains("GiB"),
            "a u64::MAX budget must never be printed: {line}"
        );
    }

    #[test]
    fn the_budgets_are_the_documented_ones() {
        assert_eq!(INSTANCE_BUDGET_BYTES, 64 * 1024);
        assert_eq!(EDITOR_EXCESS_BUDGET_BYTES, 64 * 1024);
    }
}
