//! An interactive standalone host.
//!
//! The other commands are fixed scripts: they start a process, run a sequence
//! nobody can vary, and exit. That is what CI needs and none of what a person
//! needs when a plugin misbehaves and the question is "what happens if I move
//! this parameter and *then* save?".
//!
//! This host reads commands a line at a time, so the same binary serves both:
//! a terminal for a person, and a piped script for CI. Keeping those two the
//! same surface is deliberate — a debugging aid that CI never executes rots,
//! and an assertion that only exists inside CI cannot be reproduced by hand.
//!
//! The session tracks failures and exits non-zero if any command failed, which
//! is what makes a piped script usable as a blocking gate step.

use crate::host::{HostPlugin, PluginFormat};
use crate::preset;
use std::io::{BufRead, IsTerminal, Write};

/// Default absolute tolerance for `expect`.
///
/// Deliberately a named constant rather than `==`: a parameter round-trips
/// through the format's own `double`/`float` conversions, and asserting bitwise
/// equality on that would be asserting something neither format promises.
/// Scripts that need a different bound pass one explicitly.
pub const DEFAULT_EXPECT_TOLERANCE: f64 = 1e-6;

const DEFAULT_SAMPLE_RATE: f64 = 44100.0;
const DEFAULT_BLOCK_SIZE: u32 = 512;

struct Session {
    plugin: Box<dyn HostPlugin>,
    sample_rate: f64,
    block_size: u32,
    /// Kept alive for as long as the editor is open; dropping it closes the
    /// window, so the plugin's `close_gui` has to run first.
    window: Option<crate::gui_window::PluginGuiWindow>,
    executed: usize,
    failed: usize,
}

/// How a script names a parameter.
///
/// Real parameter IDs are hashes — `458499838` for a gain knob — so an
/// interactive host that only accepts IDs is one nobody can drive by hand.
/// `#0` addresses the first parameter as `params` lists it; a bare number is
/// still an ID, because that is what a script generated from a plugin's own
/// output will contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamRef {
    Id(u32),
    Index(u32),
}

impl std::fmt::Display for ParamRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamRef::Id(id) => write!(f, "id {id}"),
            ParamRef::Index(index) => write!(f, "index #{index}"),
        }
    }
}

/// One line of the session language, already parsed.
///
/// Parsing is separated from execution so the grammar can be tested without a
/// plugin: every rejection message below is reachable from a unit test.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Help,
    Info,
    Params,
    Buses,
    Get {
        param: ParamRef,
    },
    Set {
        param: ParamRef,
        value: f64,
    },
    Expect {
        param: ParamRef,
        value: f64,
        tolerance: f64,
    },
    Process {
        frames: u32,
    },
    Reset,
    StateSave {
        path: String,
    },
    StateLoad {
        path: String,
    },
    PresetSave {
        path: String,
    },
    PresetLoad {
        path: String,
    },
    EditorOpen,
    EditorClose,
    Echo {
        text: String,
    },
    Quit,
}

/// Parse one line.
///
/// `Ok(None)` is a blank or comment line, which is not an error and does not
/// count as an executed command.
pub fn parse_command(line: &str) -> Result<Option<Command>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let mut words = line.split_whitespace();
    let head = words.next().expect("non-empty after trim");
    let rest: Vec<&str> = words.collect();

    let want_param = |text: Option<&&str>| -> Result<ParamRef, String> {
        let text = text.ok_or_else(|| format!("{head}: missing parameter id or #index"))?;
        match text.strip_prefix('#') {
            Some(index) => index
                .parse::<u32>()
                .map(ParamRef::Index)
                .map_err(|_| format!("{head}: '#{index}' is not a parameter index")),
            None => text
                .parse::<u32>()
                .map(ParamRef::Id)
                .map_err(|_| format!("{head}: '{text}' is neither a parameter id nor a #index")),
        }
    };
    let want_u32 = |what: &str, text: Option<&&str>| -> Result<u32, String> {
        let text = text.ok_or_else(|| format!("{head}: missing {what}"))?;
        text.parse::<u32>()
            .map_err(|_| format!("{head}: {what} '{text}' is not a non-negative integer"))
    };
    let want_f64 = |what: &str, text: Option<&&str>| -> Result<f64, String> {
        let text = text.ok_or_else(|| format!("{head}: missing {what}"))?;
        let value = text
            .parse::<f64>()
            .map_err(|_| format!("{head}: {what} '{text}' is not a number"))?;
        if !value.is_finite() {
            return Err(format!("{head}: {what} '{text}' is not finite"));
        }
        Ok(value)
    };

    let command = match head {
        "help" => Command::Help,
        "info" => Command::Info,
        "params" => Command::Params,
        "buses" => Command::Buses,
        "get" => Command::Get {
            param: want_param(rest.first())?,
        },
        "set" => Command::Set {
            param: want_param(rest.first())?,
            value: want_f64("value", rest.get(1))?,
        },
        "expect" => Command::Expect {
            param: want_param(rest.first())?,
            value: want_f64("value", rest.get(1))?,
            tolerance: match rest.get(2) {
                None => DEFAULT_EXPECT_TOLERANCE,
                Some(_) => {
                    let tolerance = want_f64("tolerance", rest.get(2))?;
                    if tolerance < 0.0 {
                        return Err(format!("expect: tolerance {tolerance} is negative"));
                    }
                    tolerance
                }
            },
        },
        "process" => Command::Process {
            frames: match rest.first() {
                None => 0,
                Some(_) => want_u32("frame count", rest.first())?,
            },
        },
        "reset" => Command::Reset,
        "state" | "preset" => {
            let verb = rest
                .first()
                .ok_or_else(|| format!("{head}: expected 'save' or 'load'"))?;
            let path = rest
                .get(1)
                .ok_or_else(|| format!("{head} {verb}: missing file path"))?
                .to_string();
            match (head, *verb) {
                ("state", "save") => Command::StateSave { path },
                ("state", "load") => Command::StateLoad { path },
                ("preset", "save") => Command::PresetSave { path },
                ("preset", "load") => Command::PresetLoad { path },
                _ => return Err(format!("{head}: unknown subcommand '{verb}'")),
            }
        }
        "editor" => match rest.first() {
            Some(&"open") => Command::EditorOpen,
            Some(&"close") => Command::EditorClose,
            Some(other) => return Err(format!("editor: unknown subcommand '{other}'")),
            None => return Err("editor: expected 'open' or 'close'".into()),
        },
        "echo" => Command::Echo {
            text: line["echo".len()..].trim().to_string(),
        },
        "quit" | "exit" => Command::Quit,
        other => return Err(format!("unknown command '{other}'")),
    };
    Ok(Some(command))
}

fn usage() {
    println!("Commands:");
    println!("  info                        plugin identity and channel counts");
    println!("  params                      enumerate parameters");
    println!("  buses                       enumerate audio buses");
    println!("  get <id|#index>             read one parameter");
    println!("  set <id|#index> <value>     write one parameter");
    println!(
        "  expect <id|#index> <v> [tol] assert a parameter, tolerance defaults to {DEFAULT_EXPECT_TOLERANCE:e}"
    );
    println!("  process [frames]            run one block (default: the block size)");
    println!("  reset                       reset the plugin");
    println!("  state save|load <path>      raw format state bytes");
    println!("  preset save|load <path>     format-native preset file");
    println!("  editor open|close           open or close the plugin editor");
    println!("  echo <text>                 print a line, for marking script sections");
    println!("  quit                        end the session");
}

impl Session {
    /// Turn a `ParamRef` into the plugin's own parameter ID.
    ///
    /// An index is resolved through `param_info`, so `#0` means whatever
    /// `params` printed as `[0]` and the two can never disagree.
    fn resolve(&self, param: ParamRef) -> Result<u32, String> {
        match param {
            ParamRef::Id(id) => Ok(id),
            ParamRef::Index(index) => {
                let count = self.plugin.param_count();
                if index >= count {
                    return Err(format!(
                        "parameter index #{index} is out of range; the plugin has {count}"
                    ));
                }
                self.plugin
                    .param_info(index)
                    .map(|info| info.id)
                    .ok_or_else(|| format!("parameter index #{index} has no info"))
            }
        }
    }

    fn read(&self, param: ParamRef) -> Result<(u32, f64), String> {
        let id = self.resolve(param)?;
        let value = self
            .plugin
            .param_get(id)
            .ok_or_else(|| format!("plugin has no parameter with {param}"))?;
        Ok((id, value))
    }

    fn run_command(&mut self, command: Command) -> Result<bool, String> {
        match command {
            Command::Help => usage(),
            Command::Info => self.print_info(),
            Command::Params => self.print_params()?,
            Command::Buses => self.print_buses()?,
            Command::Get { param } => {
                let (id, value) = self.read(param)?;
                println!("param {id} = {value}");
            }
            Command::Set { param, value } => {
                let id = self.resolve(param)?;
                self.plugin.param_set(id, value)?;
                println!("param {id} <- {value}");
            }
            Command::Expect {
                param,
                value,
                tolerance,
            } => {
                let (id, actual) = self.read(param)?;
                let difference = (actual - value).abs();
                if difference > tolerance {
                    return Err(format!(
                        "param {id} is {actual}, expected {value} within {tolerance} (off by {difference})"
                    ));
                }
                println!("param {id} == {value} (within {tolerance}, off by {difference})");
            }
            Command::Process { frames } => self.process(frames)?,
            Command::Reset => {
                self.plugin.reset()?;
                println!("reset");
            }
            Command::StateSave { path } => {
                let bytes = self.plugin.save_state()?;
                std::fs::write(&path, &bytes).map_err(|error| format!("{path}: {error}"))?;
                println!("state saved: {} bytes -> {path}", bytes.len());
            }
            Command::StateLoad { path } => {
                let bytes = std::fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
                self.plugin.load_state(&bytes)?;
                println!("state loaded: {} bytes <- {path}", bytes.len());
            }
            Command::PresetSave { path } => self.save_preset(&path)?,
            Command::PresetLoad { path } => self.load_preset(&path)?,
            Command::EditorOpen => self.open_editor()?,
            Command::EditorClose => self.close_editor(),
            Command::Echo { text } => println!("{text}"),
            Command::Quit => return Ok(false),
        }
        Ok(true)
    }

    fn print_info(&self) {
        let info = self.plugin.info();
        println!("name      {}", info.name);
        println!("format    {}", info.format);
        println!("id        {}", info.id);
        if let Some(class_id) = self.plugin.class_id() {
            println!(
                "class     {} ({} UID layout)",
                preset::fuid_to_string(&class_id, preset::host_uses_com_uid_layout()),
                if preset::host_uses_com_uid_layout() {
                    "COM"
                } else {
                    "native"
                }
            );
        }
        println!("vendor    {}", info.vendor);
        println!("version   {}", info.version);
        println!(
            "channels  {} in / {} out",
            info.input_channels, info.output_channels
        );
        println!(
            "kind      {}",
            if info.is_synth { "synth" } else { "effect" }
        );
        println!("latency   {:?}", self.plugin.reported_latency());
        println!("tail      {:?}", self.plugin.reported_tail());
        println!(
            "session   {} Hz, {} frames",
            self.sample_rate, self.block_size
        );
    }

    fn print_params(&self) -> Result<(), String> {
        let count = self.plugin.param_count();
        println!("parameters {count}");
        for index in 0..count {
            let info = self
                .plugin
                .param_info(index)
                .ok_or_else(|| format!("parameter index {index} has no info"))?;
            let value = self
                .plugin
                .param_get(info.id)
                .ok_or_else(|| format!("parameter id {} has no value", info.id))?;
            println!(
                "  [{index}] id={} name={:?} range=[{}, {}] default={} value={} stepped={} automatable={}",
                info.id,
                info.name,
                info.min,
                info.max,
                info.default,
                value,
                info.is_stepped,
                info.can_automate
            );
        }
        Ok(())
    }

    fn print_buses(&self) -> Result<(), String> {
        let buses = self
            .plugin
            .audio_buses()
            .ok_or_else(|| format!("{} does not expose bus topology", self.plugin.info().format))?;
        println!("buses {}", buses.len());
        for bus in &buses {
            println!(
                "  {} {:?} channels={} {}",
                if bus.is_input { "in " } else { "out" },
                bus.name,
                bus.channels,
                if bus.is_main { "main" } else { "aux" }
            );
        }
        Ok(())
    }

    fn process(&mut self, frames: u32) -> Result<(), String> {
        let frames = if frames == 0 { self.block_size } else { frames };
        if frames > self.block_size {
            return Err(format!(
                "process: {frames} frames exceeds the session block size {}",
                self.block_size
            ));
        }
        let info = self.plugin.info().clone();
        let input = vec![0.0f32; frames as usize * info.input_channels as usize];
        let mut output = vec![0.0f32; frames as usize * info.output_channels as usize];
        self.plugin.process(&input, &mut output)?;
        let peak = output
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        if !output.iter().all(|sample| sample.is_finite()) {
            return Err("process: output contains non-finite samples".into());
        }
        println!("processed {frames} frames, peak {peak:.6}");
        Ok(())
    }

    fn save_preset(&mut self, path: &str) -> Result<(), String> {
        let state = self.plugin.save_state()?;
        let format = self.plugin.info().format;
        let bytes = match format {
            PluginFormat::VST3 => {
                let class_id = self
                    .plugin
                    .class_id()
                    .ok_or("VST3 plugin did not report a class ID")?;
                preset::write_vst3_preset(&class_id, &state)
            }
            PluginFormat::CLAP => preset::clap_preset_bytes(&state).to_vec(),
            PluginFormat::AU => {
                return Err(
                    "AU preset files are out of scope; AU is not in the default build".into(),
                );
            }
        };
        std::fs::write(path, &bytes).map_err(|error| format!("{path}: {error}"))?;
        println!(
            "preset saved: {format} container, {} state bytes in {} file bytes -> {path}",
            state.len(),
            bytes.len()
        );
        Ok(())
    }

    fn load_preset(&mut self, path: &str) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
        let format = self.plugin.info().format;
        let state = match format {
            PluginFormat::VST3 => {
                let parsed = preset::read_vst3_preset(&bytes)?;
                let class_id = self
                    .plugin
                    .class_id()
                    .ok_or("VST3 plugin did not report a class ID")?;
                let expected =
                    preset::fuid_to_string(&class_id, preset::host_uses_com_uid_layout());
                if parsed.class_string != expected {
                    return Err(format!(
                        "preset belongs to class {}, this plugin is {expected}",
                        parsed.class_string
                    ));
                }
                parsed.component_state
            }
            PluginFormat::CLAP => preset::read_clap_preset(&bytes)?.to_vec(),
            PluginFormat::AU => {
                return Err(
                    "AU preset files are out of scope; AU is not in the default build".into(),
                );
            }
        };
        self.plugin.load_state(&state)?;
        println!(
            "preset loaded: {format} container, {} state bytes <- {path}",
            state.len()
        );
        Ok(())
    }

    fn open_editor(&mut self) -> Result<(), String> {
        if self.window.is_some() {
            return Err("editor is already open".into());
        }
        crate::gui_window::initialize_platform()?;
        let window = crate::gui_window::PluginGuiWindow::new(
            &format!("SunMao host - {}", self.plugin.info().name),
            600.0,
            400.0,
            Box::new(|| {}),
        )?;
        // Open before storing the window: if the plugin refuses, the window has
        // to go away rather than linger as a session with a dead editor.
        self.plugin.open_gui(&window)?;
        self.window = Some(window);
        println!("editor opened");
        Ok(())
    }

    fn close_editor(&mut self) {
        if self.window.is_none() {
            println!("editor was not open");
            return;
        }
        self.shutdown_editor();
        println!("editor closed");
    }

    /// Close the editor without narrating it, for end-of-session cleanup.
    ///
    /// The plugin goes first and the window second: the editor is parented
    /// into that window, so destroying the window under a live editor leaves
    /// the plugin holding a handle to nothing.
    fn shutdown_editor(&mut self) {
        if self.window.is_some() {
            self.plugin.close_gui();
            self.window = None;
        }
    }
}

/// Run the interactive host. Returns whether the session finished clean.
pub fn cmd_host(args: &[String]) -> bool {
    let mut sample_rate = DEFAULT_SAMPLE_RATE;
    let mut block_size = DEFAULT_BLOCK_SIZE;
    let mut path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--sample-rate" => {
                index += 1;
                match args.get(index).and_then(|value| value.parse::<f64>().ok()) {
                    Some(value) if value > 0.0 => sample_rate = value,
                    _ => {
                        eprintln!("--sample-rate needs a positive number");
                        return false;
                    }
                }
            }
            "--block-size" => {
                index += 1;
                match args.get(index).and_then(|value| value.parse::<u32>().ok()) {
                    Some(value) if value > 0 => block_size = value,
                    _ => {
                        eprintln!("--block-size needs a positive integer");
                        return false;
                    }
                }
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
            "Usage: sunmao_unittest_runner host [--sample-rate HZ] [--block-size N] <plugin_path>"
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
    if let Err(error) = plugin.initialize(sample_rate, block_size) {
        eprintln!("Failed to initialize: {error}");
        plugin.shutdown();
        return false;
    }

    let mut session = Session {
        plugin,
        sample_rate,
        block_size,
        window: None,
        executed: 0,
        failed: 0,
    };

    println!(
        "SunMao interactive host: {} ({}) at {} Hz, {} frames",
        session.plugin.info().name,
        session.plugin.info().format,
        sample_rate,
        block_size
    );
    println!("Type 'help' for commands, 'quit' to end the session.");

    let stdin = std::io::stdin();
    // Prompt a person, stay silent for a pipe. A prompt written to stderr still
    // lands in the log when a CI step merges the two streams, and it would then
    // sit in front of every line an assertion is anchored to.
    let interactive = stdin.is_terminal();
    let mut line = String::new();
    loop {
        line.clear();
        if interactive {
            eprint!("> ");
            let _ = std::io::stderr().flush();
        }
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("stdin: {error}");
                session.failed += 1;
                break;
            }
        }
        match parse_command(&line) {
            Ok(None) => continue,
            Ok(Some(command)) => {
                session.executed += 1;
                match session.run_command(command) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        session.failed += 1;
                        println!("ERROR: {error}");
                        eprintln!("ERROR: {error}");
                    }
                }
            }
            Err(error) => {
                session.executed += 1;
                session.failed += 1;
                println!("ERROR: {error}");
                eprintln!("ERROR: {error}");
            }
        }
    }

    session.shutdown_editor();
    session.plugin.shutdown();

    let Session {
        executed, failed, ..
    } = session;
    if failed == 0 {
        println!("HOST SESSION VERIFIED: {executed} commands, 0 failed");
        true
    } else {
        println!("HOST SESSION FAILED: {executed} commands, {failed} failed");
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_comment_lines_are_not_commands() {
        assert_eq!(parse_command("").expect("ok"), None);
        assert_eq!(parse_command("   ").expect("ok"), None);
        assert_eq!(parse_command("# a note").expect("ok"), None);
    }

    #[test]
    fn parameter_commands_parse() {
        assert_eq!(
            parse_command("set 3 0.25").expect("ok"),
            Some(Command::Set {
                param: ParamRef::Id(3),
                value: 0.25
            })
        );
        assert_eq!(
            parse_command("get 3").expect("ok"),
            Some(Command::Get {
                param: ParamRef::Id(3)
            })
        );
    }

    /// `#n` is a different thing from `n`, and conflating them would make a
    /// script silently address the wrong parameter on any plugin whose IDs are
    /// hashes — which is every plugin the framework generates.
    #[test]
    fn a_hashed_index_addresses_a_position_not_an_id() {
        assert_eq!(
            parse_command("set #0 0.25").expect("ok"),
            Some(Command::Set {
                param: ParamRef::Index(0),
                value: 0.25
            })
        );
        assert_eq!(
            parse_command("expect #2 1").expect("ok"),
            Some(Command::Expect {
                param: ParamRef::Index(2),
                value: 1.0,
                tolerance: DEFAULT_EXPECT_TOLERANCE
            })
        );
        assert_ne!(ParamRef::Index(3), ParamRef::Id(3));
        assert!(parse_command("get #x").is_err());
        assert!(parse_command("get #").is_err());
    }

    /// The tolerance is the point of `expect`: it defaults to a stated number
    /// rather than to exact equality, because a parameter that has been through
    /// the format's own conversions is not promised to come back bit-identical.
    #[test]
    fn expect_defaults_to_a_named_tolerance_and_accepts_an_explicit_one() {
        assert_eq!(
            parse_command("expect 1 0.5").expect("ok"),
            Some(Command::Expect {
                param: ParamRef::Id(1),
                value: 0.5,
                tolerance: DEFAULT_EXPECT_TOLERANCE
            })
        );
        assert_eq!(
            parse_command("expect 1 0.5 0.01").expect("ok"),
            Some(Command::Expect {
                param: ParamRef::Id(1),
                value: 0.5,
                tolerance: 0.01
            })
        );
        assert!(parse_command("expect 1 0.5 -0.01").is_err());
    }

    #[test]
    fn a_zero_tolerance_is_allowed_because_it_is_a_deliberate_choice() {
        assert_eq!(
            parse_command("expect 1 0.5 0").expect("ok"),
            Some(Command::Expect {
                param: ParamRef::Id(1),
                value: 0.5,
                tolerance: 0.0
            })
        );
    }

    #[test]
    fn file_commands_parse_both_verbs_for_both_kinds() {
        for (line, expected) in [
            (
                "state save a.bin",
                Command::StateSave {
                    path: "a.bin".into(),
                },
            ),
            (
                "state load a.bin",
                Command::StateLoad {
                    path: "a.bin".into(),
                },
            ),
            (
                "preset save a.vstpreset",
                Command::PresetSave {
                    path: "a.vstpreset".into(),
                },
            ),
            (
                "preset load a.vstpreset",
                Command::PresetLoad {
                    path: "a.vstpreset".into(),
                },
            ),
        ] {
            assert_eq!(parse_command(line).expect("ok"), Some(expected), "{line}");
        }
    }

    #[test]
    fn editor_commands_parse() {
        assert_eq!(
            parse_command("editor open").expect("ok"),
            Some(Command::EditorOpen)
        );
        assert_eq!(
            parse_command("editor close").expect("ok"),
            Some(Command::EditorClose)
        );
        assert!(parse_command("editor").is_err());
        assert!(parse_command("editor sideways").is_err());
    }

    #[test]
    fn process_defaults_to_the_session_block_size() {
        assert_eq!(
            parse_command("process").expect("ok"),
            Some(Command::Process { frames: 0 })
        );
        assert_eq!(
            parse_command("process 64").expect("ok"),
            Some(Command::Process { frames: 64 })
        );
    }

    #[test]
    fn echo_keeps_the_rest_of_the_line_intact() {
        assert_eq!(
            parse_command("echo   two words  ").expect("ok"),
            Some(Command::Echo {
                text: "two words".into()
            })
        );
    }

    /// Every one of these is a line a person could plausibly type. Each must
    /// come back as a message naming what was wrong, never as a silent no-op:
    /// a host that ignores a misspelled command is a host that lets a CI script
    /// pass while asserting nothing.
    #[test]
    fn malformed_lines_are_rejected_with_a_reason() {
        for line in [
            "set",
            "set 1",
            "set one 0.5",
            "set 1 not-a-number",
            "set 1 inf",
            "set -1 0.5",
            "get",
            "expect 1",
            "process sixty-four",
            "state",
            "state save",
            "state rename a.bin",
            "preset flip a.bin",
            "wiggle",
        ] {
            assert!(parse_command(line).is_err(), "{line:?} should be rejected");
        }
    }
}
