use crate::gui_window::PluginGuiWindow;
use crate::host::*;
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, PartialEq)]
enum TestStatus {
    NotRun,
    Running,
    Pass,
    Fail(String),
}

#[derive(Clone)]
struct TestItem {
    name: String,
    status: TestStatus,
    detail: String,
    elapsed_ms: f64,
}

struct PluginEntry {
    info: PluginInfo,
    tests: Vec<TestItem>,
    test_status: TestStatus,
    param_count: u32,
    params: Vec<ParamInfo>,
}

enum ScanState {
    Idle,
    Scanning,
    Done(usize),
}

struct GuiWindowEntry {
    _window: PluginGuiWindow,
    _plugin: Box<dyn HostPlugin>,
    close_requested: Arc<AtomicBool>,
    plugin_index: usize,
    plugin_name: String,
}

impl Drop for GuiWindowEntry {
    fn drop(&mut self) {
        self._plugin.close_gui();
        self._plugin.shutdown();
    }
}

pub struct SunmaoTestRunnerApp {
    plugins: Vec<PluginEntry>,
    selected: Option<usize>,
    scan_path: String,
    scan_state: ScanState,
    log: Vec<String>,
    pending_result: Arc<Mutex<Option<Vec<PluginInfo>>>>,
    pending_test_results: Arc<Mutex<Vec<TestRunResult>>>,
    gui_windows: Vec<GuiWindowEntry>,
    // Deferred GUI open requests (processed outside render pass)
    pending_gui_open: Vec<usize>,
}

struct TestRunResult {
    index: usize,
    tests: Vec<TestItem>,
    params: Vec<ParamInfo>,
    param_count: u32,
}

impl Default for SunmaoTestRunnerApp {
    fn default() -> Self {
        Self {
            plugins: Vec::new(),
            selected: None,
            scan_path: "build/".to_string(),
            scan_state: ScanState::Idle,
            log: Vec::new(),
            pending_result: Arc::new(Mutex::new(None)),
            pending_test_results: Arc::new(Mutex::new(Vec::new())),
            gui_windows: Vec::new(),
            pending_gui_open: Vec::new(),
        }
    }
}

impl SunmaoTestRunnerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn start_scan(&mut self) {
        let path = self.scan_path.clone();
        let result = self.pending_result.clone();
        self.scan_state = ScanState::Scanning;
        self.log.push(format!("Scanning {}...", path));

        thread::spawn(move || {
            let path_obj = PathBuf::from(&path);
            let mut plugins = if path_obj.exists() {
                let ext = crate::plugin_extension(&path_obj);
                match ext.as_str() {
                    "clap" => scanner::scan_clap(&path_obj),
                    "vst3" => scanner::scan_vst3(&path_obj),
                    "component" => {
                        #[cfg(all(target_os = "macos", feature = "au"))]
                        {
                            scanner::scan_au(&path_obj)
                        }
                        #[cfg(not(all(target_os = "macos", feature = "au")))]
                        {
                            Vec::new()
                        }
                    }
                    _ => scanner::scan_directory(&path_obj),
                }
            } else {
                Vec::new()
            };
            #[cfg(all(target_os = "macos", feature = "au"))]
            {
                let au = scanner::scan_au_system();
                plugins.extend(au);
            }
            *result.lock().unwrap() = Some(plugins);
        });
    }

    fn start_test(&mut self, index: usize) {
        let info = self.plugins[index].info.clone();
        let results = self.pending_test_results.clone();
        self.plugins[index].test_status = TestStatus::Running;
        self.plugins[index].tests.clear();
        self.log.push(format!("Testing {}...", info.name));

        thread::spawn(move || {
            let tests = run_tests(&info);
            let (param_count, params) = extract_param_info(&info);
            results.lock().unwrap().push(TestRunResult {
                index,
                tests,
                params,
                param_count,
            });
        });
    }

    fn open_plugin_gui(&mut self, index: usize, repaint_context: egui::Context) {
        if index >= self.plugins.len() {
            return;
        }

        // Check if GUI is already open for this plugin
        if self.gui_windows.iter().any(|e| e.plugin_index == index) {
            self.log.push(format!(
                "GUI already open for {}",
                self.plugins[index].info.name
            ));
            return;
        }

        let info = self.plugins[index].info.clone();
        let plugin_name = info.name.clone();
        self.log.push(format!("Opening GUI for {}...", plugin_name));

        // Load a fresh plugin instance for GUI
        let plugin = match load_plugin(&info) {
            Ok(p) => p,
            Err(e) => {
                self.log.push(format!("  Failed to load plugin: {}", e));
                return;
            }
        };

        let mut plugin = plugin;
        if let Err(e) = plugin.initialize(44100.0, 512) {
            self.log.push(format!("  Failed to initialize: {}", e));
            return;
        }

        // Create native window
        let title = format!("{} - SunMao Test Runner", plugin_name);
        let close_requested = Arc::new(AtomicBool::new(false));
        let close_requested_callback = Arc::clone(&close_requested);
        let window = match PluginGuiWindow::new(
            &title,
            400.0,
            300.0,
            Box::new(move || {
                close_requested_callback.store(true, Ordering::Release);
                repaint_context.request_repaint();
            }),
        ) {
            Ok(w) => {
                self.log.push(format!("  Window created (400x300)"));
                w
            }
            Err(e) => {
                self.log.push(format!("  Failed to create window: {}", e));
                plugin.shutdown();
                return;
            }
        };

        // Open plugin GUI in the window
        if let Err(e) = plugin.open_gui(&window) {
            self.log.push(format!("  Failed to open plugin GUI: {}", e));
            plugin.shutdown();
            return;
        }

        self.log.push(format!("  GUI opened for {}", plugin_name));

        self.gui_windows.push(GuiWindowEntry {
            _window: window,
            _plugin: plugin,
            close_requested,
            plugin_index: index,
            plugin_name: plugin_name.clone(),
        });
    }

    fn close_plugin_gui(&mut self, window_index: usize) {
        if window_index < self.gui_windows.len() {
            let name = self.gui_windows[window_index].plugin_name.clone();
            self.gui_windows.remove(window_index);
            self.log.push(format!("Closed GUI for {}", name));
            // entry dropped here -> GuiWindowEntry::Drop handles close_gui + shutdown
        }
    }

    fn close_all_guis(&mut self) {
        let count = self.gui_windows.len();
        self.gui_windows.clear();
        // entries dropped here -> GuiWindowEntry::Drop handles close_gui + shutdown
        if count > 0 {
            self.log.push(format!("Closed {} GUI window(s)", count));
        }
    }

    fn check_pending(&mut self) {
        if let Some(plugins) = self.pending_result.lock().unwrap().take() {
            let count = plugins.len();
            self.plugins = plugins
                .into_iter()
                .map(|info| PluginEntry {
                    info,
                    tests: Vec::new(),
                    test_status: TestStatus::NotRun,
                    param_count: 0,
                    params: Vec::new(),
                })
                .collect();
            self.scan_state = ScanState::Done(count);
            self.log.push(format!("Found {} plugins", count));
        }

        let mut results = self.pending_test_results.lock().unwrap();
        for result in results.drain(..) {
            if result.index < self.plugins.len() {
                let all_pass = result
                    .tests
                    .iter()
                    .all(|t| matches!(t.status, TestStatus::Pass));
                self.plugins[result.index].test_status = if all_pass {
                    TestStatus::Pass
                } else {
                    TestStatus::Fail("Some tests failed".into())
                };
                self.plugins[result.index].tests = result.tests;
                self.plugins[result.index].param_count = result.param_count;
                self.plugins[result.index].params = result.params;

                let name = self.plugins[result.index].info.name.clone();
                let status = if all_pass { "PASS" } else { "FAIL" };
                self.log.push(format!("{}: {}", name, status));
            }
        }
    }

    fn close_requested_guis(&mut self) {
        let requested: Vec<_> = self
            .gui_windows
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                entry
                    .close_requested
                    .load(Ordering::Acquire)
                    .then_some(index)
            })
            .collect();
        for index in requested.into_iter().rev() {
            self.close_plugin_gui(index);
        }
    }
}

impl eframe::App for SunmaoTestRunnerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_pending();
        let mut host_errors = Vec::new();
        for entry in &mut self.gui_windows {
            if let Err(error) = entry._plugin.service_host_requests() {
                host_errors.push(format!(
                    "{} host callback failed: {error}",
                    entry.plugin_name
                ));
            }
        }
        self.log.extend(host_errors);
        self.close_requested_guis();

        if matches!(self.scan_state, ScanState::Scanning)
            || !self.pending_test_results.lock().unwrap().is_empty()
        {
            ctx.request_repaint();
        }
        if !self.gui_windows.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(8));
        }

        // Top toolbar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("SunMao Plugin Test Runner");
                ui.separator();
                ui.label("Path:");
                ui.text_edit_singleline(&mut self.scan_path);
                if ui.button("Scan").clicked() {
                    self.start_scan();
                }
                #[cfg(all(target_os = "macos", feature = "au"))]
                if ui.button("Scan System AU").clicked() {
                    {
                        let result = self.pending_result.clone();
                        self.scan_state = ScanState::Scanning;
                        self.log.push("Scanning system AU plugins...".into());
                        thread::spawn(move || {
                            let plugins = scanner::scan_au_system();
                            *result.lock().unwrap() = Some(plugins);
                        });
                    }
                }
                ui.separator();
                if !self.gui_windows.is_empty() {
                    ui.label(format!("{} GUI:", self.gui_windows.len()));
                    let mut close_idx: Option<usize> = None;
                    for (i, entry) in self.gui_windows.iter().enumerate() {
                        let label = format!("[{}] x", entry.plugin_name);
                        if ui.small_button(&label).clicked() {
                            close_idx = Some(i);
                        }
                    }
                    if ui.button("Close All").clicked() {
                        self.close_all_guis();
                    }
                    if let Some(idx) = close_idx {
                        self.close_plugin_gui(idx);
                    }
                    ui.separator();
                }
                if ui.button("Test All").clicked() {
                    for i in 0..self.plugins.len() {
                        if !matches!(self.plugins[i].test_status, TestStatus::Running) {
                            self.start_test(i);
                        }
                    }
                }
            });
        });

        // Bottom log
        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(120.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Log:");
                    if ui.button("Clear").clicked() {
                        self.log.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for msg in &self.log {
                            ui.monospace(msg);
                        }
                    });
            });

        // Left panel - plugin list
        let mut pending_tests: Vec<usize> = Vec::new();
        let mut pending_guis: Vec<usize> = Vec::new();
        egui::SidePanel::left("plugins")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Plugins");
                ui.label(format!("{} found", self.plugins.len()));
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, plugin) in self.plugins.iter().enumerate() {
                        let selected = self.selected == Some(i);
                        let status_icon = match &plugin.test_status {
                            TestStatus::NotRun => "○",
                            TestStatus::Running => "◌",
                            TestStatus::Pass => "✓",
                            TestStatus::Fail(_) => "✗",
                        };
                        let fmt = match plugin.info.format {
                            PluginFormat::CLAP => "CLAP",
                            PluginFormat::VST3 => "VST3",
                            PluginFormat::AU => "AU",
                        };
                        let vendor = if plugin.info.vendor.is_empty() {
                            String::new()
                        } else {
                            format!("({})", plugin.info.vendor)
                        };

                        let color = match &plugin.test_status {
                            TestStatus::Pass => egui::Color32::from_rgb(100, 200, 100),
                            TestStatus::Fail(_) => egui::Color32::from_rgb(200, 100, 100),
                            _ => egui::Color32::from_rgb(200, 200, 200),
                        };

                        let text =
                            format!("{} {} [{}] {}", status_icon, plugin.info.name, fmt, vendor);
                        let response =
                            ui.selectable_label(selected, egui::RichText::new(text).color(color));

                        if response.clicked() {
                            self.selected = Some(i);
                        }

                        let mut ctx_clicked = false;
                        let mut gui_clicked = false;
                        response.context_menu(|ui| {
                            if ui.button("Run Tests").clicked() {
                                ctx_clicked = true;
                                ui.close_menu();
                            }
                            if ui.button("Show GUI").clicked() {
                                gui_clicked = true;
                                ui.close_menu();
                            }
                        });
                        if ctx_clicked {
                            pending_tests.push(i);
                        }
                        if gui_clicked {
                            pending_guis.push(i);
                        }
                    }
                });
            });
        for i in pending_tests {
            self.start_test(i);
        }
        // Defer GUI opens to after the render pass
        self.pending_gui_open.extend(pending_guis);

        // Central panel
        let mut central_pending_test: Option<usize> = None;
        let mut central_test_all = false;
        let mut central_pending_gui: Option<usize> = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected {
                if idx < self.plugins.len() {
                    let plugin = &self.plugins[idx];

                    ui.heading(&plugin.info.name);
                    ui.horizontal(|ui| {
                        ui.label(format!("Format: {}", plugin.info.format));
                        ui.separator();
                        ui.label(format!("ID: {}", plugin.info.id));
                    });
                    if !plugin.info.vendor.is_empty() {
                        ui.label(format!("Vendor: {}", plugin.info.vendor));
                    }
                    if !plugin.info.version.is_empty() {
                        ui.label(format!("Version: {}", plugin.info.version));
                    }
                    ui.label(format!(
                        "Audio: {} in / {} out ({})",
                        plugin.info.input_channels,
                        plugin.info.output_channels,
                        if plugin.info.is_synth {
                            "synth"
                        } else {
                            "effect"
                        }
                    ));
                    ui.label(format!("Path: {}", plugin.info.path));
                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("Run Tests").clicked() {
                            central_pending_test = Some(idx);
                        }
                        if ui.button("Test All").clicked() {
                            central_test_all = true;
                        }
                        if ui.button("Show GUI").clicked() {
                            central_pending_gui = Some(idx);
                        }
                    });
                    ui.separator();

                    if !plugin.tests.is_empty() {
                        ui.heading("Test Results");
                        let passed = plugin
                            .tests
                            .iter()
                            .filter(|t| matches!(t.status, TestStatus::Pass))
                            .count();
                        let failed = plugin
                            .tests
                            .iter()
                            .filter(|t| matches!(t.status, TestStatus::Fail(_)))
                            .count();
                        let total = plugin.tests.len();

                        ui.horizontal(|ui| {
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 200, 100),
                                format!("{} passed", passed),
                            );
                            if failed > 0 {
                                ui.colored_label(
                                    egui::Color32::from_rgb(200, 100, 100),
                                    format!("{} failed", failed),
                                );
                            }
                            ui.label(format!("/ {} total", total));
                        });
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for test in &plugin.tests {
                                let (icon, color) = match &test.status {
                                    TestStatus::Pass => {
                                        ("✓", egui::Color32::from_rgb(100, 200, 100))
                                    }
                                    TestStatus::Fail(_) => {
                                        ("✗", egui::Color32::from_rgb(200, 100, 100))
                                    }
                                    TestStatus::Running => {
                                        ("◌", egui::Color32::from_rgb(200, 200, 100))
                                    }
                                    TestStatus::NotRun => {
                                        ("○", egui::Color32::from_rgb(150, 150, 150))
                                    }
                                };

                                ui.horizontal(|ui| {
                                    ui.colored_label(color, icon);
                                    ui.label(&test.name);
                                    if test.elapsed_ms > 0.0 {
                                        ui.label(format!("({:.1}ms)", test.elapsed_ms));
                                    }
                                    if !test.detail.is_empty() {
                                        ui.label(format!("- {}", test.detail));
                                    }
                                });

                                if let TestStatus::Fail(err) = &test.status {
                                    ui.indent("err", |ui| {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(255, 100, 100),
                                            err,
                                        );
                                    });
                                }
                            }
                        });
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label("Click 'Run Tests' to test this plugin");
                        });
                    }

                    if !plugin.params.is_empty() {
                        ui.separator();
                        ui.heading("Parameters");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for param in &plugin.params {
                                ui.horizontal(|ui| {
                                    ui.label(format!("[{}]", param.id));
                                    ui.label(&param.name);
                                    ui.label(format!(
                                        "({:.2} .. {:.2}, default: {:.2})",
                                        param.min, param.max, param.default
                                    ));
                                    if param.is_stepped {
                                        ui.label("(stepped)");
                                    }
                                });
                            }
                        });
                    }
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("SunMao Plugin Test Runner");
                    ui.label("Select a plugin from the list, or scan a directory to find plugins.");
                    ui.add_space(20.0);
                    ui.label("Supported formats: CLAP, VST3, AU (macOS)");
                    ui.add_space(10.0);
                    ui.label("Right-click a plugin for more options");
                });
            }
        });
        if let Some(idx) = central_pending_test {
            self.start_test(idx);
        }
        if central_test_all {
            for i in 0..self.plugins.len() {
                if !matches!(self.plugins[i].test_status, TestStatus::Running) {
                    self.start_test(i);
                }
            }
        }
        if let Some(idx) = central_pending_gui {
            self.pending_gui_open.push(idx);
        }

        // Process deferred GUI opens (outside render pass to avoid GL context conflicts)
        let pending: Vec<usize> = self.pending_gui_open.drain(..).collect();
        for idx in pending {
            self.open_plugin_gui(idx, ctx.clone());
        }
    }
}

fn extract_param_info(info: &PluginInfo) -> (u32, Vec<ParamInfo>) {
    let Ok(mut plugin) = load_plugin(info) else {
        return (0, Vec::new());
    };
    if plugin.initialize(44100.0, 512).is_err() {
        return (0, Vec::new());
    }
    let count = plugin.param_count();
    let mut params = Vec::new();
    for i in 0..count {
        if let Some(p) = plugin.param_info(i) {
            params.push(p);
        }
    }
    plugin.shutdown();
    (count, params)
}

fn run_tests(info: &PluginInfo) -> Vec<TestItem> {
    let mut tests = Vec::new();

    let t0 = std::time::Instant::now();
    let plugin = match load_plugin(info) {
        Ok(p) => {
            tests.push(TestItem {
                name: "Load".into(),
                status: TestStatus::Pass,
                detail: format!("{:.1}ms", t0.elapsed().as_secs_f64() * 1000.0),
                elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
            });
            p
        }
        Err(e) => {
            tests.push(TestItem {
                name: "Load".into(),
                status: TestStatus::Fail(e),
                detail: String::new(),
                elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
            });
            return tests;
        }
    };

    let mut plugin = plugin;

    let t0 = std::time::Instant::now();
    if let Err(e) = plugin.initialize(44100.0, 512) {
        tests.push(TestItem {
            name: "Initialize".into(),
            status: TestStatus::Fail(e),
            detail: String::new(),
            elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        });
        plugin.shutdown();
        return tests;
    }
    tests.push(make_pass("Initialize", t0));

    let param_count = plugin.param_count();
    tests.push(TestItem {
        name: format!("Params ({} found)", param_count),
        status: TestStatus::Pass,
        detail: String::new(),
        elapsed_ms: 0.0,
    });

    let frames = 512;
    let runtime_info = plugin.info().clone();
    let input_channels = runtime_info.input_channels as usize;
    let output_channels = runtime_info.output_channels as usize;
    let input = vec![0.0f32; frames * input_channels];

    if runtime_info.is_synth {
        let t0 = std::time::Instant::now();
        if input_channels != 0 || output_channels == 0 {
            tests.push(make_fail(
                "Synth Bus Layout",
                &format!("{} in / {} out", input_channels, output_channels),
                t0,
            ));
        } else {
            tests.push(make_pass("Synth Bus Layout (0 inputs)", t0));
        }

        let t0 = std::time::Instant::now();
        let mut output = vec![0.0f32; frames * output_channels];
        let note = [HostEvent::NoteOn {
            sample_offset: 17,
            channel: 0,
            pitch: 60,
            velocity: 0.8,
        }];
        match plugin.process_with_events(&input, &mut output, &note) {
            Ok(()) if samples_are_finite(&output) && output_peak(&output) > 1.0e-6 => {
                tests.push(TestItem {
                    name: "Synth Note On".into(),
                    status: TestStatus::Pass,
                    detail: format!("peak: {:.6}", output_peak(&output)),
                    elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
            }
            Ok(()) => tests.push(make_fail(
                "Synth Note On",
                "silent or non-finite output",
                t0,
            )),
            Err(error) => tests.push(make_fail("Synth Note On", &error, t0)),
        }

        let t0 = std::time::Instant::now();
        let mut output = vec![0.0f32; frames * output_channels];
        let note = [HostEvent::NoteOff {
            sample_offset: 31,
            channel: 0,
            pitch: 60,
            velocity: 0.0,
        }];
        match plugin.process_with_events(&input, &mut output, &note) {
            Ok(()) if samples_are_finite(&output) => {
                tests.push(make_pass("Synth Note Off / Release", t0));
            }
            Ok(()) => tests.push(make_fail(
                "Synth Note Off / Release",
                "non-finite output",
                t0,
            )),
            Err(error) => tests.push(make_fail("Synth Note Off / Release", &error, t0)),
        }
    } else {
        // Silence
        let t0 = std::time::Instant::now();
        let mut output = vec![0.0f32; frames * output_channels];
        match plugin.process(&input, &mut output) {
            Ok(()) => {
                let peak = output_peak(&output);
                tests.push(TestItem {
                    name: "Process Silence".into(),
                    status: TestStatus::Pass,
                    detail: format!("peak: {:.6}", peak),
                    elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
            }
            Err(e) => tests.push(make_fail("Process Silence", &e, t0)),
        }

        // Impulse
        let t0 = std::time::Instant::now();
        let mut impulse = vec![0.0f32; frames * input_channels];
        impulse
            .iter_mut()
            .take(input_channels)
            .for_each(|sample| *sample = 1.0);
        let mut output2 = vec![0.0f32; frames * output_channels];
        match plugin.process(&impulse, &mut output2) {
            Ok(()) => {
                let peak = output_peak(&output2);
                tests.push(TestItem {
                    name: "Process Impulse".into(),
                    status: TestStatus::Pass,
                    detail: format!("peak: {:.6}", peak),
                    elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
            }
            Err(e) => tests.push(make_fail("Process Impulse", &e, t0)),
        }

        // Sine
        let t0 = std::time::Instant::now();
        let sine = make_sine(440.0, frames, 44100.0, 0.5, input_channels);
        let mut output3 = vec![0.0f32; frames * output_channels];
        match plugin.process(&sine, &mut output3) {
            Ok(()) => {
                let peak = output_peak(&output3);
                let rms = output_rms(&output3);
                tests.push(TestItem {
                    name: "Process Sine 440Hz".into(),
                    status: TestStatus::Pass,
                    detail: format!("peak: {:.4}, rms: {:.4}", peak, rms),
                    elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
            }
            Err(e) => tests.push(make_fail("Process Sine 440Hz", &e, t0)),
        }
    }

    // Stress
    let t0 = std::time::Instant::now();
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
    let mut stress_ok = true;
    for block in 0..100 {
        let mut out = vec![0.0f32; frames * output_channels];
        let events = if runtime_info.is_synth && block == 0 {
            &stress_note[..]
        } else {
            &[]
        };
        if plugin
            .process_with_events(&stress_input, &mut out, events)
            .is_err()
            || !samples_are_finite(&out)
        {
            stress_ok = false;
            break;
        }
    }
    tests.push(TestItem {
        name: "Stress (100 blocks)".into(),
        status: if stress_ok {
            TestStatus::Pass
        } else {
            TestStatus::Fail("process failed".into())
        },
        detail: format!("{:.1}ms", t0.elapsed().as_secs_f64() * 1000.0),
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
    });

    // State
    let t0 = std::time::Instant::now();
    match capture_parameter_snapshot(plugin.as_ref()) {
        Ok(snapshot) => match plugin.save_state() {
            Ok(state) => {
                tests.push(TestItem {
                    name: "State Save".into(),
                    status: TestStatus::Pass,
                    detail: format!("{} bytes", state.len()),
                    elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
                let t1 = std::time::Instant::now();
                let restore_result = overwrite_parameter_snapshot(plugin.as_mut(), &snapshot)
                    .and_then(|()| plugin.load_state(&state))
                    .and_then(|()| verify_parameter_snapshot(plugin.as_ref(), &snapshot));
                match restore_result {
                    Ok(()) => tests.push(TestItem {
                        name: "State Load".into(),
                        status: TestStatus::Pass,
                        detail: format!("verified {} parameter(s)", snapshot.len()),
                        elapsed_ms: t1.elapsed().as_secs_f64() * 1000.0,
                    }),
                    Err(e) => tests.push(make_fail("State Load", &e, t1)),
                }
            }
            Err(e) => {
                tests.push(TestItem {
                    name: "State Save".into(),
                    status: TestStatus::Pass,
                    detail: format!("skipped: {}", e),
                    elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
            }
        },
        Err(e) => {
            tests.push(make_fail("State Snapshot", &e, t0));
        }
    }

    plugin.shutdown();
    tests.push(TestItem {
        name: "Shutdown".into(),
        status: TestStatus::Pass,
        detail: String::new(),
        elapsed_ms: 0.0,
    });

    tests
}

fn make_pass(name: &str, t0: std::time::Instant) -> TestItem {
    TestItem {
        name: name.into(),
        status: TestStatus::Pass,
        detail: String::new(),
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
    }
}

fn make_fail(name: &str, err: &str, t0: std::time::Instant) -> TestItem {
    TestItem {
        name: name.into(),
        status: TestStatus::Fail(err.into()),
        detail: String::new(),
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
    }
}

fn load_plugin(info: &PluginInfo) -> Result<Box<dyn HostPlugin>, String> {
    match info.format {
        PluginFormat::CLAP => {
            let p = crate::host::clap_host::ClapHostPlugin::load(&info.path, &info.id)?;
            Ok(Box::new(p))
        }
        PluginFormat::VST3 => {
            let p = crate::host::vst3_host::Vst3HostPlugin::load(&info.path, info.class_index)?;
            Ok(Box::new(p))
        }
        PluginFormat::AU => {
            #[cfg(all(target_os = "macos", feature = "au"))]
            {
                // First try matching by type-subtype-manufacturer ID
                let components = crate::host::au_host::scan_au_components();
                for (component, desc, _name) in &components {
                    let id = format!(
                        "{}-{}-{}",
                        fourcc(desc.componentType),
                        fourcc(desc.componentSubType),
                        fourcc(desc.componentManufacturer)
                    );
                    if id == info.id {
                        let p = crate::host::au_host::AuHostPlugin::load(*component)?;
                        return Ok(Box::new(p));
                    }
                }

                // If ID is in type-subtype-manufacturer format, try direct lookup
                let parts: Vec<&str> = info.id.split('-').collect();
                if parts.len() == 3 {
                    let t = crate::host::scanner::plist_fourcc(parts[0]);
                    let s = crate::host::scanner::plist_fourcc(parts[1]);
                    let m = crate::host::scanner::plist_fourcc(parts[2]);
                    if let (Some(t), Some(s), Some(m)) = (t, s, m) {
                        if let Some(component) = crate::host::au_host::find_au_by_desc(t, s, m) {
                            let p = crate::host::au_host::AuHostPlugin::load(component)?;
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
                        if let Some((t, s, m)) =
                            crate::host::scanner::extract_au_description(&plist_data)
                        {
                            if let Some(component) = crate::host::au_host::find_au_by_desc(t, s, m)
                            {
                                let p = crate::host::au_host::AuHostPlugin::load(component)?;
                                return Ok(Box::new(p));
                            }
                        }
                    }
                }
                Err(format!("AU component not found: {}", info.id))
            }
            #[cfg(not(all(target_os = "macos", feature = "au")))]
            {
                Err("AU is only supported on macOS".into())
            }
        }
    }
}

#[cfg(all(target_os = "macos", feature = "au"))]
fn fourcc(v: u32) -> String {
    let bytes = [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8];
    String::from_utf8_lossy(&bytes).to_string()
}

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

pub fn run_gui() -> Result<(), String> {
    crate::gui_window::initialize_platform()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "SunMao Plugin Test Runner",
        options,
        Box::new(|cc| Ok(Box::new(SunmaoTestRunnerApp::new(cc)))),
    )
    .map_err(|error| error.to_string())
}
