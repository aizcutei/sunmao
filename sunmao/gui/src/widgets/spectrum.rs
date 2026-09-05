//! Spectrum and level meter displays.
//!
//! These read from a [`SpectrumSource`] rather than owning a channel, so the
//! transport stays in `sunmao_core` where the audio thread can reach it and the
//! widget stays testable with a plain array.

use super::next_widget_id;
use crate::{Event, Fill, GuiContext, Rect, Theme, Widget, WidgetId, WidgetState};

/// Maximum bars a display will draw. Fixed so painting never allocates.
pub const MAX_SPECTRUM_BARS: usize = 64;

/// Something that can supply the current bar magnitudes.
///
/// Implementations fill `out` and return how many bars they wrote, so the
/// widget can hold one fixed-size buffer instead of allocating per frame.
pub trait SpectrumSource: Send {
    fn fill(&mut self, out: &mut [f32]) -> usize;
}

/// A source backed by fixed values, for tests and static displays.
///
/// A newtype rather than `impl SpectrumSource for Vec<f32>`: this trait is in
/// the prelude, so implementing it on `Vec` would put a `fill` method on every
/// vector in every plugin and silently shadow `slice::fill` — inherent-method
/// priority does not save you, because the trait impl needs no deref and so
/// wins resolution.
pub struct StaticSpectrum(pub Vec<f32>);

impl StaticSpectrum {
    pub fn new(values: impl Into<Vec<f32>>) -> Self {
        Self(values.into())
    }
}

impl SpectrumSource for StaticSpectrum {
    fn fill(&mut self, out: &mut [f32]) -> usize {
        let count = self.0.len().min(out.len());
        out[..count].copy_from_slice(&self.0[..count]);
        count
    }
}

/// A bar-graph display of magnitudes in `0.0..=1.0`.
///
/// Values outside that range are clamped rather than dropped: a meter that
/// stops drawing when a signal clips is exactly backwards.
pub struct SpectrumAnalyzer {
    id: WidgetId,
    bounds: Rect,
    state: WidgetState,
    source: Box<dyn SpectrumSource>,
    bars: [f32; MAX_SPECTRUM_BARS],
    count: usize,
    /// Per-bar decay applied each paint, so a transient stays visible for a few
    /// frames instead of flashing for one.
    pub falloff: f32,
    pub theme: Theme,
}

impl SpectrumAnalyzer {
    pub fn new(source: Box<dyn SpectrumSource>) -> Self {
        Self {
            id: next_widget_id(),
            bounds: Rect::new(0.0, 0.0, 200.0, 80.0),
            state: WidgetState::default(),
            source,
            bars: [0.0; MAX_SPECTRUM_BARS],
            count: 0,
            falloff: 0.15,
            theme: Theme::dark(),
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Decay applied to a bar when the new value is lower, as a fraction of the
    /// gap per paint. `0.0` holds the peak forever; `1.0` follows exactly.
    pub fn with_falloff(mut self, falloff: f32) -> Self {
        self.falloff = if falloff.is_finite() {
            falloff.clamp(0.0, 1.0)
        } else {
            0.15
        };
        self
    }

    /// Pull the newest frame and apply the falloff. Called before painting.
    ///
    /// Kept separate from `draw` so a test can advance the display without a
    /// rendering context.
    pub fn refresh(&mut self) {
        let mut incoming = [0.0f32; MAX_SPECTRUM_BARS];
        let count = self.source.fill(&mut incoming).min(MAX_SPECTRUM_BARS);
        self.count = count;
        for index in 0..count {
            let target = if incoming[index].is_finite() {
                incoming[index].clamp(0.0, 1.0)
            } else {
                // A NaN from a broken analyser must not poison the display
                // permanently; treat it as silence for this frame.
                0.0
            };
            let current = self.bars[index];
            self.bars[index] = if target >= current {
                // Rise immediately: a peak the user cannot see is useless.
                target
            } else {
                current - (current - target) * self.falloff
            };
        }
    }

    /// Current bar heights, after falloff.
    pub fn bars(&self) -> &[f32] {
        &self.bars[..self.count]
    }
}

impl Widget for SpectrumAnalyzer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn state(&self) -> WidgetState {
        self.state
    }

    /// A display is not a control: it never takes an event, so a click passes
    /// through to whatever is behind it.
    fn handle_event(&mut self, _event: &Event) -> bool {
        false
    }

    fn draw(&self, ctx: &mut dyn GuiContext) {
        let b = self.bounds;
        ctx.fill_rect(b.x, b.y, b.width, b.height, Fill::Solid(self.theme.surface));
        if self.count == 0 || b.width <= 0.0 || b.height <= 0.0 {
            return;
        }
        let slot = b.width / self.count as f32;
        for (index, bar) in self.bars[..self.count].iter().enumerate() {
            let height = b.height * bar.clamp(0.0, 1.0);
            if height <= 0.0 {
                continue;
            }
            ctx.fill_rect(
                b.x + slot * index as f32 + 1.0,
                b.y + b.height - height,
                (slot - 2.0).max(1.0),
                height,
                Fill::Solid(self.theme.accent),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NullContext;

    fn analyzer(values: Vec<f32>) -> SpectrumAnalyzer {
        SpectrumAnalyzer::new(Box::new(StaticSpectrum::new(values)))
            .with_bounds(Rect::new(0.0, 0.0, 100.0, 50.0))
    }

    #[test]
    fn bars_rise_immediately_and_fall_gradually() {
        let mut display = analyzer(vec![1.0, 0.0]).with_falloff(0.5);
        display.refresh();
        // A peak must appear on the frame it happens; a user cannot see a peak
        // that was smoothed away.
        assert_eq!(display.bars(), &[1.0, 0.0]);

        // Now feed silence: the bar decays rather than snapping to zero.
        display.source = Box::new(StaticSpectrum::new(vec![0.0f32, 0.0]));
        display.refresh();
        assert_eq!(display.bars()[0], 0.5);
        display.refresh();
        assert_eq!(display.bars()[0], 0.25);
    }

    #[test]
    fn out_of_range_and_non_finite_values_are_contained() {
        let mut display = analyzer(vec![5.0, -1.0, f32::NAN, f32::INFINITY]).with_falloff(1.0);
        display.refresh();
        for bar in display.bars() {
            assert!(
                bar.is_finite() && (0.0..=1.0).contains(bar),
                "bar escaped the range: {bar}"
            );
        }
        // Clipping shows as full scale, not as a blank meter.
        assert_eq!(display.bars()[0], 1.0);
    }

    #[test]
    fn a_source_longer_than_the_display_is_truncated_rather_than_overflowing() {
        let mut display = analyzer(vec![0.5; MAX_SPECTRUM_BARS * 2]);
        display.refresh();
        assert_eq!(display.bars().len(), MAX_SPECTRUM_BARS);
        let mut ctx = NullContext::new(100.0, 50.0);
        display.draw(&mut ctx);
    }

    #[test]
    fn an_empty_source_draws_the_backdrop_without_panicking() {
        let mut display = analyzer(Vec::new());
        display.refresh();
        assert!(display.bars().is_empty());
        let mut ctx = NullContext::new(100.0, 50.0);
        display.draw(&mut ctx);
    }

    /// A display must not swallow clicks meant for the controls behind it.
    #[test]
    fn a_display_never_consumes_an_event() {
        let mut display = analyzer(vec![0.5]);
        let event = Event::MouseDown {
            x: 10.0,
            y: 10.0,
            button: crate::MouseButton::Left,
            modifiers: Default::default(),
        };
        assert!(!display.handle_event(&event));
        assert!(
            display.as_parameter().is_none(),
            "a display is not a control"
        );
    }

    #[test]
    fn a_nonsense_falloff_is_neutralised() {
        let display = analyzer(vec![0.5]).with_falloff(f32::NAN);
        assert!(display.falloff.is_finite());
        assert_eq!(analyzer(vec![0.5]).with_falloff(-1.0).falloff, 0.0);
        assert_eq!(analyzer(vec![0.5]).with_falloff(9.0).falloff, 1.0);
    }
}
