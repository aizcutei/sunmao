//! Plugin trait and related types

use std::sync::Arc;

/// Plugin information for registration
#[derive(Clone, Debug)]
pub struct PluginInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub url: &'static str,
    pub email: &'static str,
    pub version: &'static str,
    /// Plugin category: "Fx" for effects, "Instrument|Synth" for synths
    pub category: &'static str,
}

impl Default for PluginInfo {
    fn default() -> Self {
        Self {
            id: "com.example.plugin",
            name: "Example Plugin",
            vendor: "Example",
            url: "",
            email: "",
            version: "1.0.0",
            category: "Fx",
        }
    }
}

/// Audio port configuration
#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub inputs: Vec<PortConfig>,
    pub outputs: Vec<PortConfig>,
    /// Whether plugin accepts MIDI events
    pub accepts_midi: bool,
}

impl AudioConfig {
    /// Standard stereo effect (one stereo input, one stereo output)
    pub fn stereo_effect() -> Self {
        Self {
            inputs: vec![PortConfig::stereo("Input")],
            outputs: vec![PortConfig::stereo("Output")],
            accepts_midi: false,
        }
    }
    
    /// Stereo synth (no audio input, one stereo output, accepts MIDI)
    pub fn stereo_synth() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![PortConfig::stereo("Output")],
            accepts_midi: true,
        }
    }
}

/// Single audio port configuration
#[derive(Clone, Debug)]
pub struct PortConfig {
    pub name: &'static str,
    pub channels: u32,
    pub port_type: PortType,
}

impl PortConfig {
    pub fn stereo(name: &'static str) -> Self {
        Self { name, channels: 2, port_type: PortType::Main }
    }
    
    pub fn mono(name: &'static str) -> Self {
        Self { name, channels: 1, port_type: PortType::Main }
    }
}

/// Port type (main or auxiliary)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortType {
    Main,
    Aux,
}

/// Handle to communicate with the host
#[derive(Clone)]
pub struct HostHandle {
    // Placeholder for host communication
    _private: (),
}

impl HostHandle {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }
}

unsafe impl Send for HostHandle {}
unsafe impl Sync for HostHandle {}

/// Main plugin trait - implement this to create a VST3 plugin
pub trait Plugin: Sized + Send + Sync + 'static {
    /// Plugin information (ID, name, vendor, etc.)
    fn info() -> PluginInfo;
    
    /// Create a new plugin instance
    fn new(host: HostHandle) -> Self;
    
    // === Lifecycle ===
    
    /// Called after construction, before any processing
    fn init(&mut self) -> bool { true }
    
    /// Called when plugin is activated (before processing starts)
    fn activate(&mut self, _sample_rate: f64, _max_frames: u32) -> bool { true }
    
    /// Called when plugin is deactivated
    fn deactivate(&mut self) {}
    
    /// Reset plugin state (clear buffers, etc.)
    fn reset(&mut self) {}
    
    // === Configuration ===
    
    /// Audio port configuration
    fn audio_config() -> AudioConfig { AudioConfig::stereo_effect() }
    
    /// Latency in samples (processing delay)
    fn latency(&self) -> u32 { 0 }
    
    /// Tail in samples (reverb tail, etc.)
    fn tail(&self) -> u32 { 0 }
    
    // === Parameters ===
    
    /// Declare plugin parameters
    fn params() -> Vec<crate::ParamInfo> { vec![] }
    
    /// Get normalized parameter value (0.0 - 1.0)
    fn get_param(&self, id: u32) -> f64;
    
    /// Set normalized parameter value (0.0 - 1.0)
    fn set_param(&mut self, id: u32, value: f64);
    
    // === MIDI Events ===
    
    /// Called when a MIDI note on is received
    fn note_on(&mut self, _channel: i16, _pitch: i16, _velocity: f32) {}
    
    /// Called when a MIDI note off is received  
    fn note_off(&mut self, _channel: i16, _pitch: i16, _velocity: f32) {}
    
    // === Processing ===
    
    /// Process audio
    fn process(&mut self, ctx: &mut crate::ProcessContext);
}
