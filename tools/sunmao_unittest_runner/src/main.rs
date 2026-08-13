mod gui;
mod gui_window;
mod host;

use host::*;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

pub(crate) fn plugin_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::FAILURE;
    }

    match args[1].as_str() {
        #[cfg(target_os = "windows")]
        "__windows-uia-range-drag" => {
            return match gui_window::run_windows_ui_automation_helper(&args[2..]) {
                Ok(target) => {
                    println!("SUNMAO_UIA_TARGET={target:.17}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("Windows UI Automation helper failed: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        "scan" => {
            return if cmd_scan(&args[2..]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        "info" => {
            return if cmd_info(&args[2..]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        "test" => {
            return if cmd_test(&args[2..]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        "process" => {
            return if cmd_process(&args[2..]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        "gui" => {
            return if cmd_gui(&args[2..]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        "gui-test" => {
            return if cmd_gui_test(&args[2..]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn print_usage() {
    eprintln!("sunmao_unittest_runner - Audio plugin test runner");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  sunmao_unittest_runner scan <path>              Scan directory for plugins");
    eprintln!(
        "  sunmao_unittest_runner scan --system            Scan all installed AU plugins (macOS)"
    );
    eprintln!("  sunmao_unittest_runner info <plugin_path>       Show plugin information");
    eprintln!("  sunmao_unittest_runner test <plugin_path>       Run all tests on a plugin");
    eprintln!("  sunmao_unittest_runner process <plugin_path>    Process audio through plugin");
    eprintln!("  sunmao_unittest_runner gui                      Open GUI interface");
    eprintln!(
        "  sunmao_unittest_runner gui-test [--auto-close] [--verify-pixels] [--verify-input [--drag-from X,Y --drag-to X,Y]] <plugin_path>"
    );
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("  sunmao_unittest_runner scan ~/Library/Audio/Plug-Ins/Components");
    eprintln!("  sunmao_unittest_runner scan build/");
    eprintln!("  sunmao_unittest_runner scan --system");
    eprintln!("  sunmao_unittest_runner test build/SunMaoGain.clap");
    eprintln!("  sunmao_unittest_runner test build/SunMaoGain.vst3");
    eprintln!("  sunmao_unittest_runner test build/SunMaoGain.component");
    eprintln!("  sunmao_unittest_runner process build/SunMaoGain.clap");
    eprintln!("  sunmao_unittest_runner gui-test build/SunMaoGain.component");
}

// ---- Scan Command ----

fn cmd_scan(args: &[String]) -> bool {
    // Check for --system flag
    #[cfg(target_os = "macos")]
    if args.iter().any(|a| a == "--system") {
        println!("Scanning system AudioUnit plugins...");
        let plugins = scanner::scan_au_system();
        if plugins.is_empty() {
            eprintln!("No AU plugins found.");
            return false;
        }
        println!("Found {} AU plugin(s):", plugins.len());
        println!();
        for (i, p) in plugins.iter().enumerate() {
            println!("  [{}] {}", i + 1, p.name);
            println!("      ID: {}", p.id);
            println!();
        }
        return true;
    }

    if args.is_empty() {
        eprintln!("Usage: sunmao_unittest_runner scan <path|--system>");
        return false;
    }

    let path = Path::new(&args[0]);
    println!("Scanning: {}", path.display());

    let plugins = if path.is_dir() || path.is_file() {
        let ext = plugin_extension(path);
        match ext.as_str() {
            "clap" => scanner::scan_clap(path),
            "vst3" => scanner::scan_vst3(path),
            "component" => {
                #[cfg(target_os = "macos")]
                {
                    scanner::scan_au(path)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Vec::new()
                }
            }
            _ => scanner::scan_directory(path),
        }
    } else {
        scanner::scan_directory(path)
    };

    if plugins.is_empty() {
        eprintln!("No plugins found.");
        return false;
    }

    println!("Found {} plugin(s):", plugins.len());
    println!();
    for (i, p) in plugins.iter().enumerate() {
        println!("  [{}] {} ({})", i + 1, p.name, p.format);
        println!("      ID: {}", p.id);
        if !p.vendor.is_empty() {
            println!("      Vendor: {}", p.vendor);
        }
        if !p.version.is_empty() {
            println!("      Version: {}", p.version);
        }
        println!("      Path: {}", p.path);
        println!();
    }
    true
}

// ---- Info Command ----

fn cmd_info(args: &[String]) -> bool {
    if args.is_empty() {
        eprintln!("Usage: sunmao_unittest_runner info <plugin_path>");
        return false;
    }

    let path = &args[0];
    let ext = plugin_extension(Path::new(path));

    let plugins = match ext.as_str() {
        "clap" => scanner::scan_clap(Path::new(path)),
        "vst3" => scanner::scan_vst3(Path::new(path)),
        "component" => {
            #[cfg(target_os = "macos")]
            {
                scanner::scan_au(Path::new(path))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Vec::new()
            }
        }
        _ => {
            eprintln!("Unknown plugin format: {}", ext);
            return false;
        }
    };

    for p in &plugins {
        println_plugin_info(p);
    }
    if plugins.is_empty() {
        eprintln!("No plugins found in {}", path);
        return false;
    }
    true
}

fn println_plugin_info(p: &PluginInfo) {
    println!("Name:    {}", p.name);
    println!("Format:  {}", p.format);
    println!("ID:      {}", p.id);
    if !p.vendor.is_empty() {
        println!("Vendor:  {}", p.vendor);
    }
    if !p.version.is_empty() {
        println!("Version: {}", p.version);
    }
    println!(
        "Audio:   {} in / {} out",
        p.input_channels, p.output_channels
    );
    println!("Type:    {}", if p.is_synth { "Synth" } else { "Effect" });
    println!("Path:    {}", p.path);
}

// ---- Test Command ----

fn cmd_test(args: &[String]) -> bool {
    if args.is_empty() {
        eprintln!("Usage: sunmao_unittest_runner test <plugin_path>");
        return false;
    }

    let path = &args[0];
    let ext = plugin_extension(Path::new(path));

    let plugins = match ext.as_str() {
        "clap" => scanner::scan_clap(Path::new(path)),
        "vst3" => scanner::scan_vst3(Path::new(path)),
        "component" => {
            #[cfg(target_os = "macos")]
            {
                scanner::scan_au(Path::new(path))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Vec::new()
            }
        }
        _ => {
            eprintln!("Unknown plugin format: {}", ext);
            return false;
        }
    };

    if plugins.is_empty() {
        eprintln!("No plugins found in {}", path);
        return false;
    }

    let mut all_passed = true;
    for plugin_info in &plugins {
        let total_start = Instant::now();
        println!("Testing: {} ({})", plugin_info.name, plugin_info.format);
        println!("ID:      {}", plugin_info.id);
        println!("{}", "=".repeat(60));
        println!();

        let mut results = Vec::new();

        // === Test 1: Load ===
        println!("  [1/16] Loading plugin...");
        let t0 = Instant::now();
        let plugin = match load_plugin(plugin_info) {
            Ok(p) => {
                let elapsed = t0.elapsed();
                println!("         Loaded in {:.2}ms", elapsed.as_secs_f64() * 1000.0);
                results.push(TestResult::pass(&format!(
                    "load ({:.1}ms)",
                    elapsed.as_secs_f64() * 1000.0
                )));
                p
            }
            Err(e) => {
                println!("         FAILED: {}", e);
                results.push(TestResult::fail("load", &e));
                all_passed &= print_results(&results);
                continue;
            }
        };

        let mut plugin = plugin;

        // === Test 2: Initialize ===
        println!("  [2/16] Initializing (44100 Hz, 512 frames)...");
        let t0 = Instant::now();
        match plugin.initialize(44100.0, 512) {
            Ok(()) => {
                let elapsed = t0.elapsed();
                println!(
                    "         Initialized in {:.2}ms",
                    elapsed.as_secs_f64() * 1000.0
                );
                results.push(TestResult::pass("initialize"));
            }
            Err(e) => {
                println!("         FAILED: {}", e);
                results.push(TestResult::fail("initialize", &e));
                plugin.shutdown();
                all_passed &= print_results(&results);
                continue;
            }
        }

        // === Test 3: Parameter enumeration ===
        println!("  [3/16] Enumerating parameters...");
        let param_count = plugin.param_count();
        let mut parameter_metadata_ok = true;
        let mut parameter_infos = Vec::with_capacity(param_count as usize);
        if param_count > 0 {
            println!("         Found {} parameter(s):", param_count);
            for i in 0..param_count {
                if let Some(info) = plugin.param_info(i) {
                    println!(
                        "           [{}] {} (range: {:.3} .. {:.3}, default: {:.3}, stepped: {}, automatable: {})",
                        i,
                        info.name,
                        info.min,
                        info.max,
                        info.default,
                        info.is_stepped,
                        info.can_automate
                    );
                    if !info.min.is_finite()
                        || !info.max.is_finite()
                        || !info.default.is_finite()
                        || info.min >= info.max
                        || !(info.min..=info.max).contains(&info.default)
                    {
                        println!("               invalid parameter metadata");
                        parameter_metadata_ok = false;
                    }
                    if let Some(value) = plugin.param_get(info.id) {
                        println!("               current value: {:.3}", value);
                        if !value.is_finite() || !(info.min..=info.max).contains(&value) {
                            println!("               current value is outside the declared range");
                            parameter_metadata_ok = false;
                        }
                    } else {
                        println!("               failed to read current value");
                        parameter_metadata_ok = false;
                    }
                    parameter_infos.push(info);
                } else {
                    println!("           [{}] (failed to get info)", i);
                    parameter_metadata_ok = false;
                }
            }

            if runtime_plugin_requires_all_parameter_kinds(plugin_info) {
                parameter_metadata_ok &= validate_reference_parameter_kinds(&parameter_infos);
            }

            if parameter_metadata_ok {
                results.push(TestResult::pass(&format!("params ({} valid)", param_count)));
            } else {
                results.push(TestResult::fail(
                    "params",
                    "parameter metadata or current values are invalid",
                ));
            }
        } else {
            println!("         No parameters (OK for simple effects)");
            results.push(TestResult::pass("params (0 found)"));
        }

        let runtime_info = plugin.info().clone();
        let input_channels = runtime_info.input_channels as usize;
        let output_channels = runtime_info.output_channels as usize;
        let frames = 512;
        let input = vec![0.0f32; frames * input_channels];

        if runtime_info.is_synth {
            run_synth_processing_tests(plugin.as_mut(), &runtime_info, frames, &mut results);
        } else {
            // === Test 4: Process silence ===
            println!("  [4/16] Processing silence (512 frames)...");
            let mut output = vec![0.0f32; frames * output_channels];
            match plugin.process(&input, &mut output) {
                Ok(()) => {
                    let peak = output_peak(&output);
                    println!("         Output peak: {:.6} ({:.1} dB)", peak, to_db(peak));
                    results.push(TestResult::pass(&format!("silence (peak: {:.6})", peak)));
                }
                Err(e) => {
                    println!("         FAILED: {}", e);
                    results.push(TestResult::fail("process_silence", &e));
                }
            }

            // === Test 5: Process impulse ===
            println!("  [5/16] Processing impulse...");
            let mut impulse = vec![0.0f32; frames * input_channels];
            impulse
                .iter_mut()
                .take(input_channels)
                .for_each(|sample| *sample = 1.0);
            let mut output2 = vec![0.0f32; frames * output_channels];
            match plugin.process(&impulse, &mut output2) {
                Ok(()) => {
                    let peak = output_peak(&output2);
                    println!("         Output peak: {:.6} ({:.1} dB)", peak, to_db(peak));
                    results.push(TestResult::pass(&format!("impulse (peak: {:.6})", peak)));
                }
                Err(e) => {
                    println!("         FAILED: {}", e);
                    results.push(TestResult::fail("process_impulse", &e));
                }
            }

            // === Test 6: Process sine wave (440 Hz) ===
            println!("  [6/16] Processing 440 Hz sine wave...");
            let sine = make_sine(440.0, frames, 44100.0, 0.5, input_channels);
            let mut output3 = vec![0.0f32; frames * output_channels];
            match plugin.process(&sine, &mut output3) {
                Ok(()) => {
                    let peak = output_peak(&output3);
                    let rms = output_rms(&output3);
                    println!(
                        "         Peak: {:.6} ({:.1} dB), RMS: {:.6} ({:.1} dB)",
                        peak,
                        to_db(peak),
                        rms,
                        to_db(rms)
                    );
                    results.push(TestResult::pass(&format!(
                        "sine440 (peak: {:.4}, rms: {:.4})",
                        peak, rms
                    )));
                }
                Err(e) => {
                    println!("         FAILED: {}", e);
                    results.push(TestResult::fail("process_sine440", &e));
                }
            }

            // === Test 7: Process sine wave (1000 Hz) ===
            println!("  [7/16] Processing 1000 Hz sine wave...");
            let sine1k = make_sine(1000.0, frames, 44100.0, 0.5, input_channels);
            let mut output3b = vec![0.0f32; frames * output_channels];
            match plugin.process(&sine1k, &mut output3b) {
                Ok(()) => {
                    let peak = output_peak(&output3b);
                    println!("         Peak: {:.6} ({:.1} dB)", peak, to_db(peak));
                    results.push(TestResult::pass(&format!("sine1000 (peak: {:.4})", peak)));
                }
                Err(e) => {
                    println!("         FAILED: {}", e);
                    results.push(TestResult::fail("process_sine1000", &e));
                }
            }

            // === Test 8: DC offset test ===
            println!("  [8/16] Processing DC offset (value=0.5)...");
            let dc_input = vec![0.5f32; frames * input_channels];
            let mut output_dc = vec![0.0f32; frames * output_channels];
            match plugin.process(&dc_input, &mut output_dc) {
                Ok(()) => {
                    let peak = output_peak(&output_dc);
                    println!("         Output peak: {:.6}", peak);
                    results.push(TestResult::pass(&format!("dc_offset (peak: {:.4})", peak)));
                }
                Err(e) => {
                    println!("         FAILED: {}", e);
                    results.push(TestResult::fail("dc_offset", &e));
                }
            }
        }

        // === Test 9: Parameter set/get ===
        println!("  [9/16] Testing parameter set/get...");
        if param_count > 0 {
            let mut param_ok = true;
            for i in 0..param_count {
                if let Some(info) = plugin.param_info(i) {
                    // Try setting to min
                    if let Err(e) = plugin.param_set(info.id, info.min) {
                        println!("         Failed to set param {} to min: {}", info.name, e);
                        param_ok = false;
                    }
                    // Try setting to max
                    if let Err(e) = plugin.param_set(info.id, info.max) {
                        println!("         Failed to set param {} to max: {}", info.name, e);
                        param_ok = false;
                    }
                    // Try setting to default
                    if let Err(e) = plugin.param_set(info.id, info.default) {
                        println!(
                            "         Failed to set param {} to default: {}",
                            info.name, e
                        );
                        param_ok = false;
                    }
                }
            }
            if param_ok {
                println!("         All parameter set/get operations succeeded");
                results.push(TestResult::pass("param_set_get"));
            } else {
                results.push(TestResult::fail(
                    "param_set_get",
                    "some param operations failed",
                ));
            }
        } else {
            println!("         Skipped (no parameters)");
            results.push(TestResult::pass("param_set_get (skipped)"));
        }

        // === Test 10: Sample-accurate parameter automation ===
        println!("  [10/16] Testing sample-accurate parameter automation...");
        results.push(run_parameter_automation_test(
            plugin.as_mut(),
            &runtime_info,
            frames,
        ));

        // === Test 11: Reset ===
        println!("  [11/16] Resetting plugin...");
        match plugin.reset() {
            Ok(()) => {
                println!("         Reset complete");
                results.push(TestResult::pass("reset"));
            }
            Err(error) => {
                println!("         FAILED: {error}");
                results.push(TestResult::fail("reset", &error));
            }
        }

        // === Test 12: Process after reset ===
        println!("  [12/16] Processing after reset...");
        let mut output4 = vec![0.0f32; frames * output_channels];
        match plugin.process(&input, &mut output4) {
            Ok(())
                if samples_are_finite(&output4)
                    && (!runtime_info.is_synth || output_peak(&output4) <= 1.0e-6) =>
            {
                let peak = output_peak(&output4);
                println!("         Output peak: {:.6}", peak);
                results.push(TestResult::pass("process_after_reset"));
            }
            Ok(()) if !samples_are_finite(&output4) => {
                println!("         FAILED: output contains NaN or infinity");
                results.push(TestResult::fail(
                    "process_after_reset",
                    "output contains NaN or infinity",
                ));
            }
            Ok(()) => {
                let peak = output_peak(&output4);
                println!("         FAILED: synth remained active (peak: {peak:.6})");
                results.push(TestResult::fail(
                    "process_after_reset",
                    format!("synth reset left an active voice (peak: {peak:.6})"),
                ));
            }
            Err(e) => {
                println!("         FAILED: {}", e);
                results.push(TestResult::fail("process_after_reset", &e));
            }
        }

        // === Test 13: Continuous processing (stress test) ===
        println!("  [13/16] Stress test: 100 continuous blocks...");
        let t0 = Instant::now();
        let mut stress_ok = true;
        let mut stress_peak: f32 = 0.0;
        let stress_input = if runtime_info.is_synth {
            Vec::new()
        } else {
            make_sine(440.0, frames, 44100.0, 0.5, input_channels)
        };
        let stress_note = [HostEvent::NoteOn {
            sample_offset: 0,
            channel: 0,
            pitch: 60,
            velocity: 0.8,
        }];
        for block in 0..100 {
            let mut out_block = vec![0.0f32; frames * output_channels];
            let events = if runtime_info.is_synth && block == 0 {
                &stress_note[..]
            } else {
                &[]
            };
            if let Err(e) = plugin.process_with_events(&stress_input, &mut out_block, events) {
                println!("         FAILED at block {}: {}", block, e);
                stress_ok = false;
                break;
            }
            if !samples_are_finite(&out_block) {
                println!("         FAILED at block {}: non-finite output", block);
                stress_ok = false;
                break;
            }
            let block_peak = output_peak(&out_block);
            if block_peak > stress_peak {
                stress_peak = block_peak;
            }
        }
        let stress_elapsed = t0.elapsed();
        if stress_ok {
            println!(
                "         100 blocks in {:.2}ms, peak: {:.6}",
                stress_elapsed.as_secs_f64() * 1000.0,
                stress_peak
            );
            results.push(TestResult::pass(&format!(
                "stress_100blocks ({:.1}ms)",
                stress_elapsed.as_secs_f64() * 1000.0
            )));
        } else {
            results.push(TestResult::fail(
                "stress_100blocks",
                "process failed during stress test",
            ));
        }

        // === Test 14: Different sample rates ===
        println!("  [14/16] Testing different sample rates (re-loading plugin)...");
        let rates = [22050.0, 44100.0, 48000.0, 88200.0, 96000.0];
        let mut sr_results = Vec::new();
        let mut sample_rates_ok = true;
        plugin.shutdown();
        for &sr in &rates {
            // Re-load plugin for each sample rate
            match load_plugin(plugin_info) {
                Ok(mut sr_plugin) => {
                    match sr_plugin.initialize(sr, 512) {
                        Ok(()) => {
                            let sr_info = sr_plugin.info().clone();
                            let test_input = if sr_info.is_synth {
                                Vec::new()
                            } else {
                                make_sine(440.0, frames, sr, 0.5, sr_info.input_channels as usize)
                            };
                            let mut sr_out =
                                vec![0.0f32; frames * sr_info.output_channels as usize];
                            let note = [HostEvent::NoteOn {
                                sample_offset: 0,
                                channel: 0,
                                pitch: 60,
                                velocity: 0.8,
                            }];
                            let events = if sr_info.is_synth { &note[..] } else { &[] };
                            match sr_plugin.process_with_events(&test_input, &mut sr_out, events) {
                                Ok(())
                                    if samples_are_finite(&sr_out)
                                        && (!sr_info.is_synth || output_peak(&sr_out) > 1.0e-6) =>
                                {
                                    let peak = output_peak(&sr_out);
                                    sr_results
                                        .push(format!("{}Hz: OK (peak {:.4})", sr as u32, peak));
                                }
                                Ok(()) => {
                                    sr_results.push(format!(
                                        "{}Hz: FAIL (silent or non-finite output)",
                                        sr as u32
                                    ));
                                    sample_rates_ok = false;
                                }
                                Err(e) => {
                                    sr_results.push(format!("{}Hz: FAIL ({})", sr as u32, e));
                                    sample_rates_ok = false;
                                }
                            }
                        }
                        Err(e) => {
                            sr_results.push(format!("{}Hz: init FAIL ({})", sr as u32, e));
                            sample_rates_ok = false;
                        }
                    }
                    sr_plugin.shutdown();
                }
                Err(e) => {
                    sr_results.push(format!("{}Hz: load FAIL ({})", sr as u32, e));
                    sample_rates_ok = false;
                }
            }
        }
        for s in &sr_results {
            println!("         {}", s);
        }
        // Re-load plugin at 44100 for remaining tests
        plugin = match load_plugin(plugin_info) {
            Ok(mut p) => match p.initialize(44100.0, 512) {
                Ok(()) => p,
                Err(e) => {
                    p.shutdown();
                    results.push(TestResult::fail(
                        "sample_rates",
                        &format!("final initialize failed: {}", e),
                    ));
                    all_passed &= print_results(&results);
                    continue;
                }
            },
            Err(e) => {
                results.push(TestResult::fail(
                    "sample_rates",
                    &format!("re-load failed: {}", e),
                ));
                all_passed &= print_results(&results);
                continue;
            }
        };
        if sample_rates_ok {
            results.push(TestResult::pass(&format!(
                "sample_rates ({})",
                sr_results.join(", ")
            )));
        } else {
            results.push(TestResult::fail("sample_rates", &sr_results.join(", ")));
        }

        // === Test 15: State save/roundtrip ===
        println!("  [15/16] Testing state save/load...");
        match capture_parameter_snapshot(plugin.as_ref()) {
            Ok(snapshot) => match plugin.save_state() {
                Ok(state) => {
                    println!("         Saved {} bytes", state.len());
                    let restore_result = overwrite_parameter_snapshot(plugin.as_mut(), &snapshot)
                        .and_then(|()| plugin.load_state(&state))
                        .and_then(|()| verify_parameter_snapshot(plugin.as_ref(), &snapshot));
                    match restore_result {
                        Ok(()) => {
                            println!(
                                "         Restored and verified {} parameter(s)",
                                snapshot.len()
                            );
                            results.push(TestResult::pass(&format!(
                                "state_roundtrip ({} bytes, {} params)",
                                state.len(),
                                snapshot.len()
                            )));
                        }
                        Err(e) => {
                            println!("         State roundtrip FAILED: {}", e);
                            results.push(TestResult::fail("state_roundtrip", &e));
                        }
                    }
                }
                Err(e) => {
                    if plugin_info.format == PluginFormat::AU {
                        println!("         Skipped: {}", e);
                        results.push(TestResult::pass(&format!("save_state (skipped: {})", e)));
                    } else {
                        println!("         FAILED: {}", e);
                        results.push(TestResult::fail("save_state", &e));
                    }
                }
            },
            Err(e) => {
                println!("         Snapshot FAILED: {}", e);
                results.push(TestResult::fail("state_snapshot", &e));
            }
        }

        // === Test 16: Shutdown ===
        println!("  [16/16] Shutting down...");
        plugin.shutdown();
        println!("         Shutdown complete");
        results.push(TestResult::pass("shutdown"));

        // Summary
        println!();
        let total_elapsed = total_start.elapsed();
        all_passed &= print_results(&results);
        println!("Total time: {:.2}s", total_elapsed.as_secs_f64());
    }

    all_passed
}

fn runtime_plugin_requires_all_parameter_kinds(info: &PluginInfo) -> bool {
    info.name == "SunMao Gain" && info.format != PluginFormat::AU
}

fn validate_reference_parameter_kinds(parameters: &[ParamInfo]) -> bool {
    let expected = [("Gain", false), ("Polarity", true), ("Bypass", true)];
    let mut valid = true;
    for (name, should_be_stepped) in expected {
        match parameters.iter().find(|parameter| parameter.name == name) {
            Some(parameter)
                if parameter.is_stepped == should_be_stepped && parameter.can_automate => {}
            Some(parameter) => {
                println!(
                    "         {name} has invalid flags: stepped={}, automatable={}",
                    parameter.is_stepped, parameter.can_automate
                );
                valid = false;
            }
            None => {
                println!("         Missing required reference parameter: {name}");
                valid = false;
            }
        }
    }
    valid
}

fn run_parameter_automation_test(
    plugin: &mut dyn HostPlugin,
    info: &PluginInfo,
    frames: usize,
) -> TestResult {
    if info.format == PluginFormat::AU {
        println!("         Skipped for Audio Unit");
        return TestResult::pass("parameter_automation (AU skipped)");
    }

    let parameter = (0..plugin.param_count())
        .filter_map(|index| plugin.param_info(index))
        .find(|parameter| {
            parameter.can_automate
                && parameter.min.is_finite()
                && parameter.max.is_finite()
                && parameter.max > parameter.min
        });
    let Some(parameter) = parameter else {
        println!("         Skipped (no automatable parameter)");
        return TestResult::pass("parameter_automation (skipped)");
    };

    const FIRST_OFFSET: usize = 17;
    const SECOND_OFFSET: usize = 31;
    if frames <= SECOND_OFFSET {
        return TestResult::fail(
            "parameter_automation",
            format!(
                "{}-frame block is too short for automation assertions",
                frames
            ),
        );
    }

    let range = parameter.max - parameter.min;
    let (initial, first, overridden, final_value) = if parameter.is_stepped {
        (
            parameter.min,
            parameter.max,
            parameter.min,
            parameter.default.clamp(parameter.min, parameter.max),
        )
    } else {
        (
            parameter.min + range * 0.25,
            parameter.min + range * 0.75,
            parameter.max,
            parameter.min + range * 0.5,
        )
    };

    let result = (|| -> Result<String, String> {
        plugin.param_set(parameter.id, initial)?;

        let input = if info.is_synth {
            Vec::new()
        } else {
            vec![1.0; frames * info.input_channels as usize]
        };
        let mut output = vec![0.0; frames * info.output_channels as usize];
        let mut events = Vec::with_capacity(if info.is_synth { 4 } else { 3 });
        if info.is_synth {
            events.push(HostEvent::NoteOn {
                sample_offset: 0,
                channel: 0,
                pitch: 69,
                velocity: 1.0,
            });
        }
        events.extend([
            HostEvent::ParamValue {
                sample_offset: FIRST_OFFSET as u32,
                id: parameter.id,
                value: first,
            },
            HostEvent::ParamValue {
                sample_offset: SECOND_OFFSET as u32,
                id: parameter.id,
                value: overridden,
            },
            HostEvent::ParamValue {
                sample_offset: SECOND_OFFSET as u32,
                id: parameter.id,
                value: final_value,
            },
        ]);
        plugin.process_with_events(&input, &mut output, &events)?;
        if !samples_are_finite(&output) {
            return Err("automation output contains NaN or infinity".into());
        }
        if info.is_synth && output_peak(&output) <= 1.0e-6 {
            return Err("synth automation block remained silent after note-on".into());
        }

        let published = plugin
            .param_get(parameter.id)
            .ok_or_else(|| format!("failed to read automated parameter {}", parameter.name))?;
        if !values_nearly_equal(published, final_value) {
            return Err(format!(
                "final parameter value was not published: expected {}, got {}",
                final_value, published
            ));
        }

        if info.name == "SunMao Gain" {
            validate_sunmao_gain_automation(
                &output,
                info.output_channels as usize,
                initial,
                first,
                final_value,
                FIRST_OFFSET,
                SECOND_OFFSET,
            )?;
            Ok(format!(
                "{}: audio boundaries {}/{} and final value {:.3}",
                parameter.name, FIRST_OFFSET, SECOND_OFFSET, published
            ))
        } else if info.name.starts_with("SunMao Sine Synth") {
            validate_sunmao_sine_automation(
                &output,
                info.output_channels as usize,
                initial,
                first,
                final_value,
                FIRST_OFFSET,
                SECOND_OFFSET,
            )?;
            Ok(format!(
                "{}: synth audio boundaries {}/{} and final value {:.3}",
                parameter.name, FIRST_OFFSET, SECOND_OFFSET, published
            ))
        } else {
            Ok(format!(
                "{}: delivered 3 points and published {:.3}",
                parameter.name, published
            ))
        }
    })();

    let restore = plugin.param_set(parameter.id, parameter.default);
    match (result, restore) {
        (Ok(detail), Ok(())) => {
            println!("         {}", detail);
            TestResult::pass(format!("parameter_automation ({detail})"))
        }
        (Err(error), _) => {
            println!("         FAILED: {}", error);
            TestResult::fail("parameter_automation", error)
        }
        (Ok(_), Err(error)) => {
            let message = format!("automation passed but restoring default failed: {error}");
            println!("         FAILED: {}", message);
            TestResult::fail("parameter_automation", message)
        }
    }
}

fn validate_sunmao_gain_automation(
    output: &[f32],
    channels: usize,
    initial: f64,
    first: f64,
    final_value: f64,
    first_offset: usize,
    second_offset: usize,
) -> Result<(), String> {
    if channels == 0 || output.len() % channels != 0 {
        return Err("SunMao Gain returned an invalid output layout".into());
    }

    for (frame, samples) in output.chunks_exact(channels).enumerate() {
        let normalized = if frame < first_offset {
            initial
        } else if frame < second_offset {
            first
        } else {
            final_value
        };
        let expected = (normalized * 2.0) as f32;
        for (channel, actual) in samples.iter().copied().enumerate() {
            if (actual - expected).abs() > 1.0e-5 {
                return Err(format!(
                    "gain automation mismatch at frame {}, channel {}: expected {}, got {}",
                    frame, channel, expected, actual
                ));
            }
        }
    }
    Ok(())
}

fn validate_sunmao_sine_automation(
    output: &[f32],
    channels: usize,
    initial: f64,
    first: f64,
    final_value: f64,
    first_offset: usize,
    second_offset: usize,
) -> Result<(), String> {
    if channels == 0 || output.len() % channels != 0 {
        return Err("SunMao Sine Synth returned an invalid output layout".into());
    }

    let phase_increment = 440.0 / 44_100.0;
    let mut phase = 0.0_f64;
    for (frame, samples) in output.chunks_exact(channels).enumerate() {
        let volume = if frame < first_offset {
            initial
        } else if frame < second_offset {
            first
        } else {
            final_value
        };
        let expected = (phase * std::f64::consts::TAU).sin() as f32 * volume as f32;
        for (channel, actual) in samples.iter().copied().enumerate() {
            if (actual - expected).abs() > 1.0e-4 {
                return Err(format!(
                    "sine automation mismatch at frame {}, channel {}: expected {}, got {}",
                    frame, channel, expected, actual
                ));
            }
        }
        phase += phase_increment;
        if phase >= 1.0 {
            phase -= 1.0;
        }
    }
    Ok(())
}

fn values_nearly_equal(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= 1.0e-6 * scale
}

fn run_synth_processing_tests(
    plugin: &mut dyn HostPlugin,
    info: &PluginInfo,
    frames: usize,
    results: &mut Vec<TestResult>,
) {
    println!("  [4/16] Verifying synth bus layout and idle processing...");
    let input = Vec::<f32>::new();
    let output_samples = frames * info.output_channels as usize;
    if info.input_channels != 0 || info.output_channels == 0 {
        results.push(TestResult::fail(
            "synth_layout_and_idle",
            format!(
                "expected zero inputs and at least one output, got {} in / {} out",
                info.input_channels, info.output_channels
            ),
        ));
    } else {
        let mut idle_output = vec![0.0; output_samples];
        match plugin.process(&input, &mut idle_output) {
            Ok(()) if samples_are_finite(&idle_output) => {
                results.push(TestResult::pass(format!(
                    "synth_layout_and_idle (0 in / {} out)",
                    info.output_channels
                )));
            }
            Ok(()) => results.push(TestResult::fail(
                "synth_layout_and_idle",
                "idle output contains NaN or infinity",
            )),
            Err(error) => results.push(TestResult::fail("synth_layout_and_idle", error)),
        }
    }

    println!("  [5/16] Sending note-on at sample offset 17...");
    let note_on = [HostEvent::NoteOn {
        sample_offset: 17,
        channel: 0,
        pitch: 60,
        velocity: 0.8,
    }];
    let mut note_output = vec![0.0; output_samples];
    match plugin.process_with_events(&input, &mut note_output, &note_on) {
        Ok(()) if !samples_are_finite(&note_output) => results.push(TestResult::fail(
            "synth_note_on",
            "note-on output contains NaN or infinity",
        )),
        Ok(()) => {
            let peak = output_peak(&note_output);
            let note_offset_samples =
                note_on[0].sample_offset() as usize * info.output_channels as usize;
            let before_peak = output_peak(&note_output[..note_offset_samples]);
            let after_peak = output_peak(&note_output[note_offset_samples..]);
            if before_peak <= 1.0e-6 && after_peak > 1.0e-6 {
                results.push(TestResult::pass(format!(
                    "synth_note_on (peak: {:.6}, offset: {})",
                    peak,
                    note_on[0].sample_offset()
                )));
            } else {
                results.push(TestResult::fail(
                    "synth_note_on",
                    format!(
                        "note-on timing invalid (before peak: {:.8}, after peak: {:.8})",
                        before_peak, after_peak
                    ),
                ));
            }
        }
        Err(error) => results.push(TestResult::fail("synth_note_on", error)),
    }

    println!("  [6/16] Sending note-off and processing release blocks...");
    let note_off = [HostEvent::NoteOff {
        sample_offset: 31,
        channel: 0,
        pitch: 60,
        velocity: 0.0,
    }];
    let mut release_ok = true;
    let mut release_error = String::new();
    let mut note_off_timing_checked = false;
    for block in 0..9 {
        let mut release_output = vec![0.0; output_samples];
        let events = if block == 0 { &note_off[..] } else { &[] };
        match plugin.process_with_events(&input, &mut release_output, events) {
            Ok(()) if samples_are_finite(&release_output) => {
                if block == 0 {
                    let offset_samples =
                        note_off[0].sample_offset() as usize * info.output_channels as usize;
                    let before_peak = output_peak(&release_output[..offset_samples]);
                    let after_peak = output_peak(&release_output[offset_samples..]);
                    if before_peak <= 1.0e-6 || after_peak > 1.0e-6 {
                        release_ok = false;
                        release_error = format!(
                            "note-off timing invalid (before peak: {:.8}, after peak: {:.8})",
                            before_peak, after_peak
                        );
                        break;
                    }
                    note_off_timing_checked = true;
                } else if output_peak(&release_output) > 1.0e-6 {
                    release_ok = false;
                    release_error = format!("release block {} remained audible", block);
                    break;
                }
            }
            Ok(()) => {
                release_ok = false;
                release_error = format!("release block {} contains NaN or infinity", block);
                break;
            }
            Err(error) => {
                release_ok = false;
                release_error = format!("release block {} failed: {}", block, error);
                break;
            }
        }
    }
    if release_ok && note_off_timing_checked {
        results.push(TestResult::pass("synth_note_off_release"));
    } else {
        if release_error.is_empty() {
            release_error = "note-off timing was not checked".into();
        }
        results.push(TestResult::fail("synth_note_off_release", release_error));
    }

    println!("  [7/16] Re-triggering synth after release...");
    let retrigger = [HostEvent::NoteOn {
        sample_offset: 0,
        channel: 0,
        pitch: 67,
        velocity: 0.7,
    }];
    let mut retrigger_output = vec![0.0; output_samples];
    match plugin.process_with_events(&input, &mut retrigger_output, &retrigger) {
        Ok(())
            if samples_are_finite(&retrigger_output) && output_peak(&retrigger_output) > 1.0e-6 =>
        {
            results.push(TestResult::pass("synth_retrigger"));
        }
        Ok(()) => results.push(TestResult::fail(
            "synth_retrigger",
            "re-trigger output is silent or non-finite",
        )),
        Err(error) => results.push(TestResult::fail("synth_retrigger", error)),
    }

    println!("  [8/16] Releasing re-triggered note...");
    let final_off = [HostEvent::NoteOff {
        sample_offset: (frames - 1) as u32,
        channel: 0,
        pitch: 67,
        velocity: 0.0,
    }];
    let mut final_output = vec![0.0; output_samples];
    match plugin.process_with_events(&input, &mut final_output, &final_off) {
        Ok(()) if samples_are_finite(&final_output) => {
            results.push(TestResult::pass("synth_final_note_off"));
        }
        Ok(()) => results.push(TestResult::fail(
            "synth_final_note_off",
            "final release output contains NaN or infinity",
        )),
        Err(error) => results.push(TestResult::fail("synth_final_note_off", error)),
    }
}

// ---- Process Command ----

fn cmd_process(args: &[String]) -> bool {
    if args.is_empty() {
        eprintln!("Usage: sunmao_unittest_runner process <plugin_path>");
        return false;
    }

    let path = &args[0];
    let ext = plugin_extension(Path::new(path));

    let plugins = match ext.as_str() {
        "clap" => scanner::scan_clap(Path::new(path)),
        "vst3" => scanner::scan_vst3(Path::new(path)),
        "component" => {
            #[cfg(target_os = "macos")]
            {
                scanner::scan_au(Path::new(path))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Vec::new()
            }
        }
        _ => {
            eprintln!("Unknown plugin format: {}", ext);
            return false;
        }
    };

    if plugins.is_empty() {
        eprintln!("No plugins found in {}", path);
        return false;
    }

    let plugin_info = &plugins[0];
    let mut plugin = match load_plugin(plugin_info) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to load plugin: {}", e);
            return false;
        }
    };

    if let Err(e) = plugin.initialize(44100.0, 512) {
        eprintln!("Failed to initialize: {}", e);
        plugin.shutdown();
        return false;
    }

    let runtime_info = plugin.info().clone();

    // Generate one second of input for effects; synths are driven by note events.
    let sample_rate = 44100.0;
    let duration = 1.0;
    let num_frames = (sample_rate * duration) as usize;
    let input = if runtime_info.is_synth {
        Vec::new()
    } else {
        make_sine(
            440.0,
            num_frames,
            sample_rate,
            0.5,
            runtime_info.input_channels as usize,
        )
    };

    let mut output = vec![0.0f32; num_frames * runtime_info.output_channels as usize];

    // Process in chunks. Keep the error visible to the command caller rather
    // than printing it and reporting a successful process run.
    let chunk_size = 512;
    let mut process_ok = true;
    for chunk_start in (0..num_frames).step_by(chunk_size) {
        let chunk_end = (chunk_start + chunk_size).min(num_frames);
        let in_start = chunk_start * runtime_info.input_channels as usize;
        let in_end = chunk_end * runtime_info.input_channels as usize;
        let out_start = chunk_start * runtime_info.output_channels as usize;
        let out_end = chunk_end * runtime_info.output_channels as usize;
        let in_chunk = &input[in_start..in_end];
        let out_chunk = &mut output[out_start..out_end];
        let note = [HostEvent::NoteOn {
            sample_offset: 0,
            channel: 0,
            pitch: 60,
            velocity: 0.8,
        }];
        let events = if runtime_info.is_synth && chunk_start == 0 {
            &note[..]
        } else {
            &[]
        };
        if let Err(e) = plugin.process_with_events(in_chunk, out_chunk, events) {
            eprintln!("Process error at frame {}: {}", chunk_start, e);
            process_ok = false;
            break;
        }
    }

    plugin.shutdown();

    let peak = output_peak(&output);
    let rms = output_rms(&output);
    println!("Processed {} frames ({:.1}s)", num_frames, duration);
    println!("Output peak: {:.6} ({:.1} dB)", peak, to_db(peak));
    println!("Output RMS:  {:.6} ({:.1} dB)", rms, to_db(rms));
    process_ok
}

// ---- GUI Command ----

fn cmd_gui(_args: &[String]) -> bool {
    if let Err(e) = gui::run_gui() {
        eprintln!("GUI error: {}", e);
        return false;
    }
    true
}

// ---- GUI Test Command ----

fn env_duration_ms(name: &str, default_ms: u64) -> std::time::Duration {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or_else(|| std::time::Duration::from_millis(default_ms))
}

fn gui_test_render_delay(plugin: &mut dyn HostPlugin) -> Result<(), String> {
    let deadline = std::time::Instant::now() + env_duration_ms("SUNMAO_GUI_RENDER_MS", 500);
    while std::time::Instant::now() < deadline {
        plugin.service_host_requests()?;
        if !gui_window::PluginGuiWindow::pump_events() {
            return Err("native GUI event loop stopped".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    Ok(())
}

fn gui_test_verify_pixels(
    plugin: &mut dyn HostPlugin,
    window: &gui_window::PluginGuiWindow,
) -> Result<gui_window::PixelEvidence, String> {
    let deadline = std::time::Instant::now() + env_duration_ms("SUNMAO_GUI_PIXEL_TIMEOUT_MS", 3000);
    loop {
        if std::env::var_os("SUNMAO_GUI_PIXEL_PROBE").is_some() {
            if let Some(library) = plugin.plugin_library() {
                if let Ok(evidence) = gui_window::read_plugin_pixel_probe(library) {
                    println!("GUI pixels verified via in-process renderer probe");
                    return Ok(evidence);
                }
            }
        }
        let os_error = match window.verify_non_uniform_pixels() {
            Ok(evidence) => return Ok(evidence),
            Err(error) => error,
        };
        let probe_error = match plugin
            .plugin_library()
            .ok_or_else(|| "plugin module is unavailable for in-process pixel probe".to_string())
            .and_then(gui_window::read_plugin_pixel_probe)
        {
            Ok(evidence) => {
                println!("GUI pixels verified via in-process renderer probe");
                return Ok(evidence);
            }
            Err(error) => error,
        };
        if std::time::Instant::now() >= deadline {
            return Err(format!("{os_error}; {probe_error}"));
        }
        gui_test_render_delay(plugin)?;
    }
}

fn validate_gui_gesture_evidence(
    format: PluginFormat,
    before: Option<GuiGestureEvidence>,
    after: Option<GuiGestureEvidence>,
    parameter_id: u32,
    parameter_value: f64,
) -> Result<(usize, usize, usize), String> {
    if !matches!(format, PluginFormat::CLAP | PluginFormat::VST3) {
        return Ok((0, 0, 0));
    }
    let before = before.ok_or_else(|| format!("{format} host exposes no GUI gesture evidence"))?;
    let after = after.ok_or_else(|| format!("{format} host exposes no GUI gesture evidence"))?;
    let begin_count = after.begin_count.saturating_sub(before.begin_count);
    let value_count = after.value_count.saturating_sub(before.value_count);
    let end_count = after.end_count.saturating_sub(before.end_count);
    let completed_count = after.completed_count.saturating_sub(before.completed_count);
    if begin_count == 0 || value_count == 0 || end_count == 0 || completed_count == 0 {
        return Err(format!(
            "{format} host callbacks were incomplete (begin +{begin_count}, value +{value_count}, end +{end_count}, completed +{completed_count})"
        ));
    }
    if after.last_completed_param_id != parameter_id {
        return Err(format!(
            "{format} host completed a gesture for parameter {}, expected {parameter_id}",
            after.last_completed_param_id
        ));
    }
    if !after.last_completed_value.is_finite()
        || (after.last_completed_value - parameter_value).abs() > 1.0e-6
    {
        return Err(format!(
            "{format} host callback published {:.6}, parameter reports {parameter_value:.6}",
            after.last_completed_value
        ));
    }
    Ok((begin_count, value_count, end_count))
}

fn wait_for_gui_test_close(
    plugin: &mut dyn HostPlugin,
    window_closed: std::sync::mpsc::Receiver<()>,
) -> Result<(), String> {
    let (input_tx, input_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
        let _ = input_tx.send(());
    });

    loop {
        if input_rx.try_recv().is_ok() || window_closed.try_recv().is_ok() {
            break;
        }
        plugin.service_host_requests()?;
        if !gui_window::PluginGuiWindow::pump_events() {
            return Err("native GUI event loop stopped".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GuiPoint {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GuiDrag {
    from: GuiPoint,
    to: GuiPoint,
}

#[derive(Debug, PartialEq)]
struct GuiTestOptions<'a> {
    auto_close: bool,
    verify_pixels: bool,
    input_drag: Option<GuiDrag>,
    path: &'a str,
}

fn parse_gui_point(value: &str, option: &str) -> Result<GuiPoint, String> {
    let (x, y) = value
        .split_once(',')
        .ok_or_else(|| format!("{option} expects X,Y, got '{value}'"))?;
    let x = x
        .parse::<f64>()
        .map_err(|_| format!("{option} has invalid X coordinate '{x}'"))?;
    let y = y
        .parse::<f64>()
        .map_err(|_| format!("{option} has invalid Y coordinate '{y}'"))?;
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return Err(format!(
            "{option} coordinates must be finite and non-negative, got '{value}'"
        ));
    }
    Ok(GuiPoint { x, y })
}

fn parse_gui_test_args(args: &[String]) -> Result<GuiTestOptions<'_>, String> {
    let mut auto_close = false;
    let mut verify_pixels = false;
    let mut verify_input = false;
    let mut drag_from = None;
    let mut drag_to = None;
    let mut path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--auto-close" => auto_close = true,
            "--verify-pixels" => verify_pixels = true,
            "--verify-input" => verify_input = true,
            "--drag-from" | "--drag-to" => {
                let option = args[index].as_str();
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{option} requires an X,Y value"))?;
                let point = parse_gui_point(value, option)?;
                if option == "--drag-from" {
                    if drag_from.replace(point).is_some() {
                        return Err("--drag-from may only be specified once".into());
                    }
                } else if drag_to.replace(point).is_some() {
                    return Err("--drag-to may only be specified once".into());
                }
            }
            argument if argument.starts_with('-') => {
                return Err(format!("unknown gui-test option '{argument}'"));
            }
            argument => {
                if path.replace(argument).is_some() {
                    return Err("gui-test expects exactly one plugin path".into());
                }
            }
        }
        index += 1;
    }

    let path = path.ok_or_else(|| "gui-test requires a plugin path".to_string())?;
    let input_drag = match (verify_input, drag_from, drag_to) {
        (false, None, None) => None,
        (false, _, _) => return Err("--drag-from/--drag-to require --verify-input".into()),
        (true, Some(from), Some(to)) => Some(GuiDrag { from, to }),
        (true, None, None) => Some(GuiDrag {
            from: GuiPoint { x: 64.0, y: 110.0 },
            to: GuiPoint { x: 456.0, y: 110.0 },
        }),
        (true, _, _) => return Err("--verify-input requires both --drag-from and --drag-to".into()),
    };

    Ok(GuiTestOptions {
        auto_close,
        verify_pixels,
        input_drag,
        path,
    })
}

fn cmd_gui_test(args: &[String]) -> bool {
    let options = match parse_gui_test_args(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("Invalid gui-test arguments: {error}");
            eprintln!(
                "Usage: sunmao_unittest_runner gui-test [--auto-close] [--verify-pixels] [--verify-input [--drag-from X,Y --drag-to X,Y]] <plugin_path>"
            );
            return false;
        }
    };
    if options.verify_pixels {
        std::env::set_var("SUNMAO_GUI_PIXEL_PROBE", "1");
    }
    if let Err(error) = gui_window::initialize_platform() {
        eprintln!("Failed to initialize native GUI support: {error}");
        return false;
    }

    let path = options.path;
    let ext = plugin_extension(Path::new(path));

    let plugins = match ext.as_str() {
        "clap" => scanner::scan_clap(Path::new(path)),
        "vst3" => scanner::scan_vst3(Path::new(path)),
        "component" => {
            #[cfg(target_os = "macos")]
            {
                scanner::scan_au(Path::new(path))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Vec::new()
            }
        }
        _ => {
            eprintln!("Unknown plugin format: {}", ext);
            return false;
        }
    };

    if plugins.is_empty() {
        eprintln!("No plugins found in {}", path);
        return false;
    }

    let plugin_info = &plugins[0];
    println!(
        "Testing GUI for: {} ({})",
        plugin_info.name, plugin_info.format
    );
    println!("ID: {}", plugin_info.id);
    println!();

    // Load plugin
    let mut plugin = match load_plugin(plugin_info) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to load plugin: {}", e);
            return false;
        }
    };

    if let Err(e) = plugin.initialize(44100.0, 512) {
        eprintln!("Failed to initialize: {}", e);
        plugin.shutdown();
        return false;
    }

    // Create window
    let title = format!("{} - GUI Test", plugin_info.name);
    let (window_closed_tx, window_closed_rx) = std::sync::mpsc::channel();
    let window = match gui_window::PluginGuiWindow::new(
        &title,
        400.0,
        300.0,
        Box::new(move || {
            let _ = window_closed_tx.send(());
        }),
    ) {
        Ok(w) => {
            println!("Window created: 400x300");
            w
        }
        Err(e) => {
            eprintln!("Failed to create window: {}", e);
            plugin.shutdown();
            return false;
        }
    };

    // Open GUI
    match plugin.open_gui(&window) {
        Ok(()) => {
            println!("GUI opened successfully");
        }
        Err(e) => {
            eprintln!("Failed to open GUI: {}", e);
            plugin.shutdown();
            return false;
        }
    }

    if let Err(error) = gui_test_render_delay(plugin.as_mut()) {
        eprintln!("GUI render failed: {error}");
        plugin.close_gui();
        plugin.shutdown();
        return false;
    }

    let resized_size = match plugin.resize_gui(520, 220) {
        Ok(size) => size,
        Err(error) => {
            eprintln!("GUI resize negotiation failed: {error}");
            plugin.close_gui();
            plugin.shutdown();
            return false;
        }
    };
    if let Err(error) = gui_test_render_delay(plugin.as_mut()) {
        eprintln!("GUI resize failed: {error}");
        plugin.close_gui();
        plugin.shutdown();
        return false;
    }
    println!(
        "GUI resized successfully: {}x{}",
        resized_size.0, resized_size.1
    );

    if options.verify_pixels {
        match gui_test_verify_pixels(plugin.as_mut(), &window) {
            Ok(evidence) => println!(
                "GUI pixels verified: {}x{}, {} sampled pixels, {} colors, intensity range {}, std dev {:.2}",
                evidence.width,
                evidence.height,
                evidence.sampled_pixels,
                evidence.distinct_colors,
                evidence.intensity_range,
                evidence.intensity_std_dev
            ),
            Err(error) => {
                eprintln!("GUI pixel verification failed: {error}");
                plugin.close_gui();
                plugin.shutdown();
                return false;
            }
        }
    }

    if let Some(drag) = options.input_drag {
        let Some(parameter) = (0..plugin.param_count()).find_map(|index| plugin.param_info(index))
        else {
            eprintln!("GUI input verification requires at least one parameter");
            plugin.close_gui();
            plugin.shutdown();
            return false;
        };
        let before = plugin.param_get(parameter.id).unwrap_or(parameter.default);
        let gesture_before = plugin.gui_gesture_evidence();
        let format = plugin.info().format;
        let delivery = match window.drag_slider(drag.from.x, drag.from.y, drag.to.x, drag.to.y) {
            Ok(delivery) => delivery,
            Err(error) => {
                eprintln!("GUI input injection failed: {error}");
                plugin.close_gui();
                plugin.shutdown();
                return false;
            }
        };
        if let Err(error) = gui_test_render_delay(plugin.as_mut()) {
            eprintln!("GUI input servicing failed: {error}");
            plugin.close_gui();
            plugin.shutdown();
            return false;
        }
        let after = plugin.param_get(parameter.id).unwrap_or(before);
        if (after - before).abs() <= 1.0e-6 {
            eprintln!(
                "GUI input verification failed: parameter '{}' stayed at {:.6}",
                parameter.name, before
            );
            plugin.close_gui();
            plugin.shutdown();
            return false;
        }
        println!(
            "GUI input verified via {delivery}: parameter '{}' changed {:.6} -> {:.6}",
            parameter.name, before, after,
        );
        match validate_gui_gesture_evidence(
            format,
            gesture_before,
            plugin.gui_gesture_evidence(),
            parameter.id,
            after,
        ) {
            Ok((begin_count, value_count, end_count))
                if matches!(format, PluginFormat::CLAP | PluginFormat::VST3) =>
            {
                println!(
                    "GUI host gesture verified: begin +{begin_count}, value +{value_count}, end +{end_count}"
                );
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("GUI host gesture verification failed: {error}");
                plugin.close_gui();
                plugin.shutdown();
                return false;
            }
        }
    }

    println!();
    println!("Recreating GUI...");
    plugin.close_gui();
    match plugin.open_gui(&window) {
        Ok(()) => println!("GUI recreated successfully"),
        Err(error) => {
            eprintln!("Failed to recreate GUI: {error}");
            plugin.shutdown();
            return false;
        }
    }
    if let Err(error) = gui_test_render_delay(plugin.as_mut()) {
        eprintln!("GUI recreate render failed: {error}");
        plugin.close_gui();
        plugin.shutdown();
        return false;
    }
    if options.verify_pixels {
        match gui_test_verify_pixels(plugin.as_mut(), &window) {
            Ok(evidence) => println!(
                "GUI pixels verified after recreate: {}x{}, {} sampled pixels, {} colors, intensity range {}, std dev {:.2}",
                evidence.width,
                evidence.height,
                evidence.sampled_pixels,
                evidence.distinct_colors,
                evidence.intensity_range,
                evidence.intensity_std_dev
            ),
            Err(error) => {
                eprintln!("GUI pixel verification after recreate failed: {error}");
                plugin.close_gui();
                plugin.shutdown();
                return false;
            }
        }
    }

    println!();
    println!("GUI test complete.");
    println!("Sizes are logged to stderr (look for [AU GUI] lines).");
    let wait_result = if options.auto_close {
        Ok(())
    } else {
        println!("Window is open. Press Enter or close the window to finish...");
        wait_for_gui_test_close(plugin.as_mut(), window_closed_rx)
    };

    // Cleanup
    plugin.close_gui();
    plugin.shutdown();
    if let Err(error) = wait_result {
        eprintln!("GUI lifecycle failed: {error}");
        return false;
    }
    println!("Done.");
    true
}

// ---- Plugin Loading ----

fn load_plugin(info: &PluginInfo) -> Result<Box<dyn HostPlugin>, String> {
    match info.format {
        PluginFormat::CLAP => {
            let p = clap_host::ClapHostPlugin::load(&info.path, &info.id)?;
            Ok(Box::new(p))
        }
        PluginFormat::VST3 => {
            let p = vst3_host::Vst3HostPlugin::load(&info.path, info.class_index)?;
            Ok(Box::new(p))
        }
        PluginFormat::AU => {
            #[cfg(target_os = "macos")]
            {
                // First try matching by type-subtype-manufacturer ID
                let components = au_host::scan_au_components();
                for (component, desc, _name) in &components {
                    let id = format!(
                        "{}-{}-{}",
                        fourcc_str(desc.componentType),
                        fourcc_str(desc.componentSubType),
                        fourcc_str(desc.componentManufacturer)
                    );
                    if id == info.id {
                        let p = au_host::AuHostPlugin::load(*component)?;
                        return Ok(Box::new(p));
                    }
                }

                // If ID is in type-subtype-manufacturer format, try direct lookup
                let parts: Vec<&str> = info.id.split('-').collect();
                if parts.len() == 3 {
                    let t = scanner::plist_fourcc(parts[0]);
                    let s = scanner::plist_fourcc(parts[1]);
                    let m = scanner::plist_fourcc(parts[2]);
                    if let (Some(t), Some(s), Some(m)) = (t, s, m) {
                        if let Some(component) = au_host::find_au_by_desc(t, s, m) {
                            let p = au_host::AuHostPlugin::load(component)?;
                            return Ok(Box::new(p));
                        }
                    }
                }

                // If ID looks like a path, parse the plist to find the component
                if info.id.contains('/') || info.id.ends_with(".component") {
                    let plist_path = std::path::Path::new(&info.id)
                        .join("Contents")
                        .join("Info.plist");
                    if let Ok(plist_data) = std::fs::read_to_string(&plist_path) {
                        if let Some((t, s, m)) = scanner::extract_au_description(&plist_data) {
                            if let Some(component) = au_host::find_au_by_desc(t, s, m) {
                                let p = au_host::AuHostPlugin::load(component)?;
                                return Ok(Box::new(p));
                            }
                        }
                    }
                }
                Err(format!("AU component not found: {}", info.id))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err("AU is only supported on macOS".into())
            }
        }
    }
}

// ---- Helpers ----

fn output_peak(output: &[f32]) -> f32 {
    output.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

fn output_rms(output: &[f32]) -> f32 {
    if output.is_empty() {
        return 0.0;
    }
    let sum: f64 = output.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / output.len() as f64).sqrt() as f32
}

fn to_db(level: f32) -> f32 {
    20.0 * level.max(1e-10).log10()
}

fn make_sine(
    freq: f64,
    frames: usize,
    sample_rate: f64,
    amplitude: f32,
    channels: usize,
) -> Vec<f32> {
    let mut buf = vec![0.0f32; frames * channels];
    for i in 0..frames {
        let t = i as f64 / sample_rate;
        let val = (t * freq * 2.0 * std::f64::consts::PI).sin() as f32 * amplitude;
        for channel in 0..channels {
            buf[i * channels + channel] = val;
        }
    }
    buf
}

fn samples_are_finite(samples: &[f32]) -> bool {
    samples.iter().all(|sample| sample.is_finite())
}

#[cfg(target_os = "macos")]
fn fourcc_str(fourcc: u32) -> String {
    let bytes = [
        (fourcc >> 24) as u8,
        (fourcc >> 16) as u8,
        (fourcc >> 8) as u8,
        fourcc as u8,
    ];
    String::from_utf8_lossy(&bytes).to_string()
}

fn print_results(results: &[TestResult]) -> bool {
    println!();
    println!("Test Results:");
    println!("{}", "-".repeat(60));
    let mut passed = 0;
    let mut failed = 0;
    for r in results {
        let status = if r.passed {
            passed += 1;
            "PASS"
        } else {
            failed += 1;
            "FAIL"
        };
        if r.message.is_empty() {
            println!("  [{}] {}", status, r.name);
        } else {
            println!("  [{}] {} - {}", status, r.name, r.message);
        }
    }
    println!("{}", "-".repeat(60));
    println!(
        "Summary: {} passed, {} failed, {} total",
        passed,
        failed,
        passed + failed
    );
    failed == 0
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn plugin_dispatch_extensions_are_case_insensitive() {
        assert_eq!(plugin_extension(Path::new("Gain.CLAP")), "clap");
        assert_eq!(plugin_extension(Path::new("Gain.VST3")), "vst3");
        assert_eq!(plugin_extension(Path::new("Gain.Component")), "component");
    }

    #[test]
    fn process_command_rejects_missing_plugins() {
        let args = ["/definitely/missing.clap".to_string()];
        assert!(!cmd_process(&args));
    }

    #[test]
    fn discovery_commands_reject_missing_plugins() {
        let args = ["/definitely/missing.clap".to_string()];
        assert!(!cmd_scan(&args));
        assert!(!cmd_info(&args));
    }

    #[test]
    fn reference_parameter_kind_contract_requires_float_int_and_bool_metadata() {
        let info = PluginInfo {
            name: "SunMao Gain".into(),
            vendor: String::new(),
            version: String::new(),
            id: String::new(),
            path: String::new(),
            format: PluginFormat::CLAP,
            class_index: 0,
            input_channels: 2,
            output_channels: 2,
            is_synth: false,
        };
        assert!(runtime_plugin_requires_all_parameter_kinds(&info));
        let parameters = vec![
            ParamInfo {
                id: 1,
                name: "Gain".into(),
                min: 0.0,
                max: 1.0,
                default: 0.5,
                is_stepped: false,
                can_automate: true,
            },
            ParamInfo {
                id: 2,
                name: "Polarity".into(),
                min: 0.0,
                max: 1.0,
                default: 0.0,
                is_stepped: true,
                can_automate: true,
            },
            ParamInfo {
                id: 3,
                name: "Bypass".into(),
                min: 0.0,
                max: 1.0,
                default: 0.0,
                is_stepped: true,
                can_automate: true,
            },
        ];
        assert!(validate_reference_parameter_kinds(&parameters));

        let mut invalid = parameters;
        invalid[2].is_stepped = false;
        assert!(!validate_reference_parameter_kinds(&invalid));
    }

    #[test]
    fn reference_gain_automation_checks_exact_sample_boundaries() {
        let output = [0.5, 0.5, 1.5, 1.5, 1.5, 1.5, 1.0, 1.0];
        assert!(validate_sunmao_gain_automation(&output, 2, 0.25, 0.75, 0.5, 1, 3).is_ok());

        let mut wrong = output;
        wrong[2] = 0.5;
        assert!(validate_sunmao_gain_automation(&wrong, 2, 0.25, 0.75, 0.5, 1, 3).is_err());
    }

    #[test]
    fn gui_test_arguments_parse_explicit_drag_contract() {
        let args = [
            "--auto-close",
            "--verify-pixels",
            "--verify-input",
            "--drag-from",
            "120,150",
            "--drag-to",
            "400,150",
            "Gain.clap",
        ]
        .map(str::to_string);

        assert_eq!(
            parse_gui_test_args(&args).unwrap(),
            GuiTestOptions {
                auto_close: true,
                verify_pixels: true,
                input_drag: Some(GuiDrag {
                    from: GuiPoint { x: 120.0, y: 150.0 },
                    to: GuiPoint { x: 400.0, y: 150.0 },
                }),
                path: "Gain.clap",
            }
        );
    }

    #[test]
    fn gui_test_arguments_reject_partial_or_unowned_drag_contracts() {
        let partial = ["--verify-input", "--drag-from", "64,110", "Gain.vst3"].map(str::to_string);
        assert!(parse_gui_test_args(&partial)
            .unwrap_err()
            .contains("requires both"));

        let unowned =
            ["--drag-from", "64,110", "--drag-to", "456,110", "Gain.vst3"].map(str::to_string);
        assert!(parse_gui_test_args(&unowned)
            .unwrap_err()
            .contains("require --verify-input"));

        let malformed = ["--verify-input", "--drag-from", "64", "Gain.vst3"].map(str::to_string);
        assert!(parse_gui_test_args(&malformed)
            .unwrap_err()
            .contains("expects X,Y"));
    }

    #[test]
    fn gui_gesture_validation_requires_all_host_callbacks() {
        let before = GuiGestureEvidence {
            begin_count: 2,
            value_count: 5,
            end_count: 2,
            last_param_id: 7,
            last_value: 0.25,
            completed_count: 2,
            last_completed_param_id: 7,
            last_completed_value: 0.25,
        };
        let complete = GuiGestureEvidence {
            begin_count: 3,
            value_count: 8,
            end_count: 3,
            last_param_id: 7,
            last_value: 0.75,
            completed_count: 3,
            last_completed_param_id: 7,
            last_completed_value: 0.75,
        };
        assert_eq!(
            validate_gui_gesture_evidence(
                PluginFormat::CLAP,
                Some(before),
                Some(complete),
                7,
                0.75,
            )
            .unwrap(),
            (1, 3, 1)
        );

        let missing_end = GuiGestureEvidence {
            end_count: before.end_count,
            ..complete
        };
        assert!(validate_gui_gesture_evidence(
            PluginFormat::VST3,
            Some(before),
            Some(missing_end),
            7,
            0.75,
        )
        .is_err());

        let unordered = GuiGestureEvidence {
            completed_count: before.completed_count,
            ..complete
        };
        assert!(validate_gui_gesture_evidence(
            PluginFormat::CLAP,
            Some(before),
            Some(unordered),
            7,
            0.75,
        )
        .is_err());
    }
}
