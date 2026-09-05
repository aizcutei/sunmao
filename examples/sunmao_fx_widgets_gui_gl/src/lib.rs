//! SunMao Widgets — Phase 4 acceptance fixture.
//!
//! Exercises the four control archetypes Phase 4 has to deliver: a knob
//! (continuous), a dropdown (discrete), a toggle (boolean), and a spectrum
//! display (audio→GUI data, not a control).
//!
//! Only the knob is a framework widget today. The dropdown, the toggle and the
//! spectrum are **skeletons living in this crate**, exactly like the Phase 3
//! fixtures carried inline DSP before `sunmao/dsp` existed. M2 replaces the
//! dropdown and toggle with framework widgets and declarative layout; M4
//! replaces `SpectrumPublisher` with the lock-free `VizChannel` and a
//! `SpectrumAnalyzer` widget. **The acceptance rule is that these tests keep
//! passing unchanged across those swaps.**

use std::sync::atomic::{AtomicU32, Ordering};
use sunmao::prelude::*;

/// Number of spectrum bands published from the audio thread.
pub const SPECTRUM_BANDS: usize = 8;

/// Band centre frequencies, spaced roughly logarithmically over the audible range.
const BAND_CENTRES_HZ: [f64; SPECTRUM_BANDS] = [
    60.0, 150.0, 350.0, 800.0, 1_800.0, 4_000.0, 8_000.0, 14_000.0,
];

/// Discrete tone modes, selected by the dropdown.
pub const MODE_NAMES: [&str; 4] = ["Clean", "Warm", "Bright", "Crush"];

// ============ audio → GUI transport (M4 replaces with VizChannel) ============

/// Lock-free, allocation-free publisher for spectrum magnitudes.
///
/// One relaxed atomic store per band per block on the audio side; the GUI reads
/// whenever it paints. A torn read across bands is acceptable here — the values
/// are for display and the next block corrects it — which is precisely why this
/// is a skeleton rather than the real `VizChannel`.
#[derive(Debug)]
pub struct SpectrumPublisher {
    bands: [AtomicU32; SPECTRUM_BANDS],
}

impl Default for SpectrumPublisher {
    fn default() -> Self {
        Self {
            bands: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

impl SpectrumPublisher {
    /// Publish one band's magnitude. Called from the audio thread; no
    /// allocation, no locking.
    pub fn publish(&self, band: usize, magnitude: f32) {
        if let Some(slot) = self.bands.get(band) {
            slot.store(magnitude.to_bits(), Ordering::Relaxed);
        }
    }

    /// Read one band's magnitude. Called from the GUI thread.
    pub fn read(&self, band: usize) -> f32 {
        self.bands
            .get(band)
            .map(|slot| f32::from_bits(slot.load(Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    /// Read every band into a caller-owned array, so the GUI never allocates.
    pub fn snapshot(&self) -> [f32; SPECTRUM_BANDS] {
        std::array::from_fn(|band| self.read(band))
    }
}

// ============ Parameters ============

#[derive(Params)]
pub struct WidgetsParams {
    /// Continuous — driven by the knob.
    #[param(name = "Gain", default = 1.0, range = 0.0..=2.0, unit = "LinearGain")]
    pub gain: FloatParam,

    /// Discrete — driven by the dropdown. An `IntParam` because `EnumParam`
    /// does not exist yet (see `docs/design/target_syntax.md`).
    #[param(name = "Mode", default = 0, range = 0..=3)]
    pub mode: IntParam,

    /// Boolean — driven by the toggle.
    #[param(name = "Bypass", default = false)]
    pub bypass: BoolParam,
}

// ============ Plugin ============

pub struct WidgetsPlugin {
    params: Arc<WidgetsParams>,
    /// One bandpass per spectrum band, shared across channels.
    analysis: [Svf; SPECTRUM_BANDS],
    spectrum: Arc<SpectrumPublisher>,
    /// Per-band tone shaping for the `Crush`/`Warm`/`Bright` modes.
    tone: OnePole,
    sample_rate: f64,
}

impl Default for WidgetsPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(WidgetsParams::default()),
            analysis: std::array::from_fn(|_| Svf::new()),
            spectrum: Arc::new(SpectrumPublisher::default()),
            tone: OnePole::new(OnePoleKind::Lowpass),
            sample_rate: 48_000.0,
        }
    }
}

impl WidgetsPlugin {
    /// The publisher handle, obtainable before `initialize` so an editor opened
    /// ahead of activation still has something to read.
    pub fn spectrum(&self) -> Arc<SpectrumPublisher> {
        Arc::clone(&self.spectrum)
    }

    fn configure_analysis(&mut self) {
        for (filter, centre) in self.analysis.iter_mut().zip(BAND_CENTRES_HZ) {
            // Clamp above Nyquist so a low sample rate cannot produce an
            // unstable coefficient.
            let centre = centre.min(self.sample_rate * 0.45);
            filter.set_params(centre, 0.7, self.sample_rate);
        }
    }

    /// Tone cutoff for a mode index. Out-of-range indices fall back to `Clean`.
    fn mode_cutoff(mode: i32, sample_rate: f64) -> f64 {
        let hz: f64 = match mode {
            1 => 2_000.0,  // Warm
            2 => 12_000.0, // Bright
            3 => 800.0,    // Crush
            _ => 20_000.0, // Clean
        };
        hz.min(sample_rate * 0.45)
    }
}

impl SunmaoPlugin for WidgetsPlugin {
    const NAME: &'static str = "SunMao Widgets GL";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = WidgetsParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn input_channels(&self) -> u32 {
        2
    }
    fn output_channels(&self) -> u32 {
        2
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.sample_rate = sample_rate;
        self.configure_analysis();
        self.tone.set_cutoff(
            Self::mode_cutoff(self.params.mode.get(), sample_rate),
            sample_rate,
        );
    }

    fn reset(&mut self) {
        for filter in &mut self.analysis {
            filter.reset();
        }
        self.tone.reset();
        for band in 0..SPECTRUM_BANDS {
            self.spectrum.publish(band, 0.0);
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        let bypass = self.params.bypass.get();
        let gain = self.params.gain.get();
        let mode = self.params.mode.get();
        self.tone
            .set_cutoff(Self::mode_cutoff(mode, self.sample_rate), self.sample_rate);

        // Peak per band for this block. A fixed-size array: no allocation.
        let mut band_peaks = [0.0f32; SPECTRUM_BANDS];
        let channels = buffer.num_output_channels();
        let frames = buffer.num_samples();

        for index in 0..frames {
            // Analyse the pre-gain mono sum so the display reflects the input.
            let mut mono = 0.0f32;
            for channel in 0..channels {
                mono += buffer.output(channel)[index];
            }
            if channels > 1 {
                mono /= channels as f32;
            }

            for (band, filter) in self.analysis.iter_mut().enumerate() {
                let magnitude = filter.tick(mono).bandpass.abs();
                if magnitude > band_peaks[band] {
                    band_peaks[band] = magnitude;
                }
            }

            if bypass {
                continue;
            }
            let shaped = self.tone.process(mono);
            // `Clean` leaves the signal alone; the other modes blend the
            // filtered path in so the dropdown is audible.
            let blend = if mode == 0 { 0.0 } else { 1.0 };
            for channel in 0..channels {
                let sample = buffer.output(channel)[index];
                let voiced = sample * (1.0 - blend) + shaped * blend;
                buffer.output(channel)[index] = voiced * gain;
            }
        }

        for (band, peak) in band_peaks.iter().enumerate() {
            self.spectrum.publish(band, *peak);
        }

        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao Widgets".to_string(),
            width: 420,
            height: 260,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.10, 0.10, 0.15),
        };
        let spectrum = self.spectrum();
        // The builder may run more than once, so each call gets its own handle
        // onto the same publisher.
        let view = BaseviewView::new(config, move |context| {
            WidgetsViewState::new(context, Arc::clone(&spectrum), 420.0, 260.0)
        });
        Some(Box::new(view))
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.widgets.gl",
            features: &["audio-effect", "utility", "stereo"],
        }
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxWidgets!",
            categories: &["Fx"],
            ..Default::default()
        }
    }
}

// ============ Spectrum display (M4 replaces with VizChannel) ============

/// Spectrum display. M4 replaces this with `SpectrumAnalyzer` + `VizChannel`.
///
/// The toggle and dropdown that used to live here alongside it are gone: M2
/// landed real framework widgets, so the editor below uses `Toggle` and
/// `Dropdown` from `sunmao_gui` instead.
pub struct SpectrumSkeleton {
    pub bounds: Rect,
    source: Arc<SpectrumPublisher>,
}

impl SpectrumSkeleton {
    pub fn new(source: Arc<SpectrumPublisher>) -> Self {
        Self {
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            source,
        }
    }

    /// Bar heights in the range `0.0..=1.0`, newest values each call.
    pub fn bars(&self) -> [f32; SPECTRUM_BANDS] {
        let mut bars = self.source.snapshot();
        for bar in &mut bars {
            *bar = bar.clamp(0.0, 1.0);
        }
        bars
    }
}

// ============ View ============

/// Widths the controls are declared at; the column stretches them across its
/// padded width, so only the heights below are load-bearing.
const CONTROL_HEIGHT: f32 = 28.0;
const KNOB_SIZE: f32 = 72.0;

struct WidgetsViewState {
    /// Every parameter control lives in this tree. The binder walks it, so
    /// there is no per-control callback code in this file at all.
    controls: Stack,
    binder: ParamBinder,
    spectrum: SpectrumSkeleton,
    theme: Theme,
}

impl WidgetsViewState {
    fn new(
        context: Arc<dyn ViewContext>,
        spectrum: Arc<SpectrumPublisher>,
        width: f32,
        height: f32,
    ) -> Self {
        let theme = Theme::dark();
        let controls = Column::new()
            .gap(10.0)
            .padding(16.0)
            .child(Knob::new("gain").with_bounds(Rect::new(0.0, 0.0, KNOB_SIZE, KNOB_SIZE)))
            .child(
                Dropdown::new("mode", &MODE_NAMES)
                    .with_theme(theme)
                    .with_bounds(Rect::new(0.0, 0.0, 140.0, CONTROL_HEIGHT)),
            )
            .child(
                Toggle::new("bypass")
                    .with_theme(theme)
                    .with_label("Bypass")
                    .with_bounds(Rect::new(0.0, 0.0, 56.0, CONTROL_HEIGHT)),
            );

        let mut state = Self {
            controls,
            binder: ParamBinder::new(ViewContextHost::shared(context)),
            spectrum: SpectrumSkeleton::new(spectrum),
            theme,
        };
        state.relayout(width, height);
        state
    }

    fn relayout(&mut self, width: f32, height: f32) {
        // Controls take the top of the editor; the spectrum fills what is left.
        let control_height = self.controls.content_extent();
        self.controls
            .layout(Rect::new(0.0, 0.0, width, control_height));
        let top = control_height;
        self.spectrum.bounds = Rect::new(
            16.0,
            top,
            (width - 32.0).max(0.0),
            (height - top - 16.0).max(0.0),
        );
    }
}

impl ViewState for WidgetsViewState {
    fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32) {
        // One call pulls every bound control up to date with the host.
        self.binder.sync(&mut self.controls);

        ctx.fill_rect(0.0, 0.0, width, height, Fill::Solid(self.theme.background));

        // Spectrum first, so an open dropdown paints over it.
        let s = self.spectrum.bounds;
        ctx.fill_rect(
            s.x,
            s.y,
            s.width,
            s.height,
            Fill::Solid(Color::rgb(0.07, 0.07, 0.11)),
        );
        let bars = self.spectrum.bars();
        let slot = s.width / SPECTRUM_BANDS as f32;
        for (index, bar) in bars.iter().enumerate() {
            let bar_height = s.height * bar;
            ctx.fill_rect(
                s.x + slot * index as f32 + 2.0,
                s.y + s.height - bar_height,
                (slot - 4.0).max(1.0),
                bar_height,
                Fill::Solid(self.theme.accent),
            );
        }

        self.controls.draw(ctx);
    }

    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
        // The binder dispatches, detects the change, pushes it to the host and
        // brackets the gesture. No hand-written callback.
        self.binder.handle_event(&mut self.controls, event)
    }

    /// Keyboard reaches parameters through the same binder as the mouse: Tab
    /// moves focus, arrows and Space edit the focused control, and each edit is
    /// bracketed as its own automation gesture.
    fn on_keyboard_event(&mut self, event: &GuiEvent) -> bool {
        self.binder.handle_event(&mut self.controls, event)
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.relayout(width, height);
    }
}

sunmao::sunmao_export!(WidgetsPlugin, gui);

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// Counts allocator traffic on this thread while armed, so "the audio path
    /// does not allocate" is measured rather than asserted in a comment. Same
    /// shape as the backends' allocation matrix and `sunmao/src/voice.rs`.
    struct TestAllocator;

    thread_local! {
        static ALLOCATOR_CALL_COUNT: Cell<isize> = const { Cell::new(-1) };
    }

    unsafe impl GlobalAlloc for TestAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocator_call();
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            record_allocator_call();
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocator_call();
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record_allocator_call();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    fn record_allocator_call() {
        let _ = ALLOCATOR_CALL_COUNT.try_with(|count| {
            let current = count.get();
            if current >= 0 {
                count.set(current + 1);
            }
        });
    }

    #[global_allocator]
    static TEST_ALLOCATOR: TestAllocator = TestAllocator;

    struct AllocationScope;

    impl Drop for AllocationScope {
        fn drop(&mut self) {
            ALLOCATOR_CALL_COUNT.with(|count| count.set(-1));
        }
    }

    fn count_allocator_calls<R>(callback: impl FnOnce() -> R) -> (R, usize) {
        ALLOCATOR_CALL_COUNT.with(|count| {
            assert_eq!(count.get(), -1);
            count.set(0);
        });
        let scope = AllocationScope;
        let result = callback();
        let calls = ALLOCATOR_CALL_COUNT.with(|count| count.get() as usize);
        drop(scope);
        (result, calls)
    }

    fn render(plugin: &mut WidgetsPlugin, input: &[f32]) -> Vec<f32> {
        let left = input.to_vec();
        let right = input.to_vec();
        let inputs: [&[f32]; 2] = [&left, &right];
        let mut out_l = vec![0.0; input.len()];
        let mut out_r = vec![0.0; input.len()];
        let mut outputs: [&mut [f32]; 2] = [&mut out_l, &mut out_r];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, input.len());
        let events = EventQueue::default();
        plugin.process(&mut buffer, &events, &ProcessContext::default());
        out_l
    }

    fn sine(freq: f64, sample_rate: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| ((i as f64 / sample_rate) * freq * std::f64::consts::TAU).sin() as f32 * 0.5)
            .collect()
    }

    #[test]
    fn gain_scales_the_output() {
        let mut plugin = WidgetsPlugin::default();
        plugin.initialize(48_000.0, 512);
        plugin.params.gain.set(2.0);
        let out = render(&mut plugin, &[0.25; 64]);
        // Clean mode leaves the signal alone before the gain stage.
        assert!((out[63] - 0.5).abs() < 1e-3, "got {}", out[63]);
    }

    #[test]
    fn bypass_leaves_the_signal_untouched() {
        let mut plugin = WidgetsPlugin::default();
        plugin.initialize(48_000.0, 512);
        plugin.params.gain.set(2.0);
        plugin.params.bypass.set(true);
        let out = render(&mut plugin, &[0.25; 64]);
        assert!((out[63] - 0.25).abs() < 1e-6, "got {}", out[63]);
    }

    #[test]
    fn spectrum_is_published_from_the_audio_thread() {
        let mut plugin = WidgetsPlugin::default();
        plugin.initialize(48_000.0, 2048);
        let handle = plugin.spectrum();
        // Silence first: nothing to show.
        render(&mut plugin, &[0.0; 2048]);
        assert!(handle.snapshot().iter().all(|band| *band < 1e-4));

        // A 4 kHz tone should light band 5 (centre 4 kHz) more than band 0
        // (centre 60 Hz).
        let tone = sine(4_000.0, 48_000.0, 4096);
        render(&mut plugin, &tone);
        let bands = handle.snapshot();
        assert!(
            bands[5] > bands[0] * 4.0,
            "4 kHz tone did not concentrate in its band: {bands:?}"
        );
    }

    #[test]
    fn reset_clears_the_published_spectrum() {
        let mut plugin = WidgetsPlugin::default();
        plugin.initialize(48_000.0, 2048);
        let handle = plugin.spectrum();
        render(&mut plugin, &sine(1_800.0, 48_000.0, 2048));
        assert!(handle.snapshot().iter().any(|band| *band > 1e-3));
        plugin.reset();
        assert!(handle.snapshot().iter().all(|band| *band == 0.0));
    }

    #[test]
    fn spectrum_bars_are_clamped_for_display() {
        let publisher = Arc::new(SpectrumPublisher::default());
        publisher.publish(0, 4.0);
        publisher.publish(1, -1.0);
        let skeleton = SpectrumSkeleton::new(Arc::clone(&publisher));
        let bars = skeleton.bars();
        assert_eq!(bars[0], 1.0);
        assert_eq!(bars[1], 0.0);
    }

    #[test]
    fn process_and_spectrum_publish_do_not_allocate() {
        let mut plugin = WidgetsPlugin::default();
        plugin.initialize(48_000.0, 512);
        let input = sine(1_000.0, 48_000.0, 512);
        let right = input.clone();
        let inputs: [&[f32]; 2] = [&input, &right];
        let mut out_l = vec![0.0; input.len()];
        let mut out_r = vec![0.0; input.len()];
        let events = EventQueue::default();
        let context = ProcessContext::default();

        // Everything that allocates is built before the counter is armed.
        let (_, calls) = count_allocator_calls(|| {
            let mut outputs: [&mut [f32]; 2] = [&mut out_l, &mut out_r];
            let mut buffer = AudioBuffer::new(&inputs, &mut outputs, input.len());
            plugin.process(&mut buffer, &events, &context)
        });
        assert_eq!(calls, 0, "the audio path allocated {calls} times");
    }

    /// The editor is now built from framework widgets and driven by
    /// `ParamBinder`, so this asserts the tree rather than the deleted
    /// skeletons: three bound controls, laid out without overlap, each
    /// reporting the parameter id the plugin declares. The controls' own
    /// behaviour is covered by `sunmao_gui`'s widget tests.
    #[test]
    fn the_editor_binds_one_control_per_parameter() {
        struct StubContext;
        impl ViewContext for StubContext {
            fn get_param(&self, _id: &str) -> Option<f32> {
                None
            }
            fn set_param(&self, _id: &str, _value: f32) {}
            fn begin_edit(&self, _id: &str) {}
            fn end_edit(&self, _id: &str) {}
            fn request_resize(&self, _width: u32, _height: u32) -> bool {
                false
            }
        }

        let publisher = Arc::new(SpectrumPublisher::default());
        let mut state = WidgetsViewState::new(
            Arc::new(StubContext) as Arc<dyn ViewContext>,
            publisher,
            420.0,
            260.0,
        );

        let ids: Vec<String> = (0..state.controls.len())
            .filter_map(|index| {
                state
                    .controls
                    .child_at_mut(index)
                    .and_then(|child| child.as_parameter())
                    .map(|param| param.param_id().to_string())
            })
            .collect();
        assert_eq!(ids, vec!["gain", "mode", "bypass"]);

        // Laid out top to bottom without overlapping.
        let rects: Vec<Rect> = (0..state.controls.len())
            .map(|index| state.controls.child_bounds(index).unwrap())
            .collect();
        for pair in rects.windows(2) {
            assert!(
                pair[1].y >= pair[0].y + pair[0].height,
                "controls overlap: {:?} then {:?}",
                pair[0],
                pair[1]
            );
        }
        // The spectrum sits below the controls, not on top of them.
        let last = rects.last().unwrap();
        assert!(state.spectrum.bounds.y >= last.y + last.height - 16.0);
    }

    #[test]
    fn mode_cutoff_stays_below_nyquist() {
        // At a low sample rate every mode must still produce a stable cutoff.
        for mode in -1..=5 {
            let cutoff = WidgetsPlugin::mode_cutoff(mode, 8_000.0);
            assert!(cutoff <= 8_000.0 * 0.45 + 1e-9, "mode {mode} -> {cutoff}");
        }
    }
}
