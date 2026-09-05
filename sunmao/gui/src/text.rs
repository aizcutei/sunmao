//! Font rasterization, text metrics and line layout.
//!
//! Before this module, `measure_text` returned `0.0` and `draw_text` was a
//! no-op, so every editor that drew a label was laying it out against a lie.
//!
//! Rasterization sits behind [`GlyphSource`] rather than being hard-wired to
//! one font library. That keeps the layout logic — advances, wrapping,
//! alignment, caching — testable without a font file, and leaves room for a
//! plugin to supply its own rasterizer.
//!
//! # Ownership
//!
//! A [`Font`] owns its glyph cache, and a [`Font`] belongs to the editor's
//! window handler. It must **not** be a process-wide static: two plugin
//! instances would then share one cache and free each other's glyphs. See
//! `docs/phase4/ownership.md`.

use std::collections::HashMap;

/// Metrics for one glyph, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphMetrics {
    /// How far the pen moves after drawing this glyph.
    pub advance: f32,
    /// Offset from the pen to the left edge of the bitmap.
    pub bearing_x: f32,
    /// Offset from the baseline to the top edge of the bitmap, upward positive.
    pub bearing_y: f32,
    pub width: u32,
    pub height: u32,
}

/// Vertical metrics shared by every glyph at a given size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineMetrics {
    /// Distance from the baseline to the top of the tallest glyph.
    pub ascent: f32,
    /// Distance from the baseline to the bottom of the lowest glyph, positive.
    pub descent: f32,
    /// Baseline-to-baseline distance.
    pub line_height: f32,
}

/// An 8-bit coverage bitmap for one glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphBitmap {
    pub metrics: GlyphMetrics,
    /// `width * height` coverage values, row-major, 0 = transparent.
    pub coverage: Vec<u8>,
}

impl GlyphBitmap {
    /// A glyph that occupies space but draws nothing, such as a space.
    pub fn blank(advance: f32) -> Self {
        Self {
            metrics: GlyphMetrics {
                advance,
                bearing_x: 0.0,
                bearing_y: 0.0,
                width: 0,
                height: 0,
            },
            coverage: Vec::new(),
        }
    }

    pub fn is_blank(&self) -> bool {
        self.coverage.is_empty()
    }

    /// Horizontal runs of identical non-zero coverage.
    ///
    /// Each entry is `(row, start_column, length, coverage)`. A renderer
    /// without a glyph atlas draws one rectangle per run, which keeps a solid
    /// stem to a single draw instead of one per pixel. Zero-coverage pixels are
    /// skipped entirely: they are transparent, and drawing them would be both
    /// wasted work and — with blending on — visible banding.
    pub fn runs(&self) -> Vec<(u32, u32, u32, u8)> {
        let width = self.metrics.width as usize;
        if width == 0 || self.coverage.len() < width {
            return Vec::new();
        }
        let mut runs = Vec::new();
        for row in 0..self.metrics.height as usize {
            let base = row * width;
            if base + width > self.coverage.len() {
                break;
            }
            let mut column = 0usize;
            while column < width {
                let coverage = self.coverage[base + column];
                if coverage == 0 {
                    column += 1;
                    continue;
                }
                let start = column;
                while column < width && self.coverage[base + column] == coverage {
                    column += 1;
                }
                runs.push((row as u32, start as u32, (column - start) as u32, coverage));
            }
        }
        runs
    }
}

/// Something that can turn a `char` into metrics and pixels.
pub trait GlyphSource: Send {
    fn glyph_metrics(&self, ch: char, size: f32) -> GlyphMetrics;
    fn rasterize(&self, ch: char, size: f32) -> GlyphBitmap;
    fn line_metrics(&self, size: f32) -> LineMetrics;
    /// Whether this source has a real outline for `ch`. Callers use it to
    /// decide whether to substitute, rather than silently drawing a blank.
    fn has_glyph(&self, ch: char) -> bool;
}

/// Measured size of a run of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    /// Number of lines the text occupies, at least 1.
    pub lines: usize,
}

/// One glyph placed by [`Font::layout`].
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedGlyph {
    pub ch: char,
    /// Pen x for this glyph, relative to the layout origin.
    pub x: f32,
    /// Baseline y for this glyph, relative to the layout origin.
    pub baseline: f32,
    pub line: usize,
}

/// A rasterizer plus its glyph cache.
pub struct Font {
    source: Box<dyn GlyphSource>,
    /// Keyed by char and by size in 1/64ths of a pixel, so two nearly-equal
    /// sizes do not thrash the cache but genuinely different ones do not
    /// collide.
    cache: HashMap<(char, u32), GlyphBitmap>,
}

fn size_key(size: f32) -> u32 {
    if !size.is_finite() || size <= 0.0 {
        return 0;
    }
    (size * 64.0).round() as u32
}

impl Font {
    pub fn new(source: Box<dyn GlyphSource>) -> Self {
        Self {
            source,
            cache: HashMap::new(),
        }
    }

    pub fn line_metrics(&self, size: f32) -> LineMetrics {
        self.source.line_metrics(size)
    }

    pub fn has_glyph(&self, ch: char) -> bool {
        self.source.has_glyph(ch)
    }

    /// Number of glyphs currently cached. Exposed so an editor can bound its
    /// memory, and so tests can prove the cache is actually used.
    pub fn cached_glyphs(&self) -> usize {
        self.cache.len()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Rasterize `ch`, reusing a cached bitmap when possible.
    pub fn glyph(&mut self, ch: char, size: f32) -> &GlyphBitmap {
        let key = (ch, size_key(size));
        self.cache
            .entry(key)
            .or_insert_with(|| self.source.rasterize(ch, size))
    }

    /// Width of a single line of text, without wrapping.
    pub fn measure(&self, text: &str, size: f32) -> TextMetrics {
        let line = self.source.line_metrics(size);
        let width = text
            .chars()
            .filter(|ch| *ch != '\n')
            .map(|ch| self.source.glyph_metrics(ch, size).advance)
            .sum();
        let lines = text.split('\n').count().max(1);
        TextMetrics {
            width,
            height: line.line_height * lines as f32,
            ascent: line.ascent,
            descent: line.descent,
            lines,
        }
    }

    /// Lay text out, breaking lines at `max_width` when it is `Some`.
    ///
    /// Breaks happen at spaces where possible. A single word longer than
    /// `max_width` is broken mid-word rather than overflowing forever, because
    /// a control that silently paints outside its bounds is worse than an ugly
    /// break.
    pub fn layout(&self, text: &str, size: f32, max_width: Option<f32>) -> Vec<PositionedGlyph> {
        let line_metrics = self.source.line_metrics(size);
        let mut placed: Vec<PositionedGlyph> = Vec::new();
        let mut line_index = 0usize;
        let mut pen = 0.0f32;

        // Index into `placed` where the current word starts, so a wrap can move
        // the whole word down rather than splitting it.
        let mut word_start: Option<usize> = None;

        for ch in text.chars() {
            if ch == '\n' {
                line_index += 1;
                pen = 0.0;
                word_start = None;
                continue;
            }
            let advance = self.source.glyph_metrics(ch, size).advance;

            if let Some(limit) = max_width {
                // Step 1: if the in-progress word started part-way along the
                // line, move the whole word down rather than splitting it. A
                // word already at the left margin would land in exactly the
                // same place, so there is nothing to gain.
                if pen + advance > limit && pen > 0.0 {
                    if let Some(start) = word_start {
                        if start < placed.len() && placed[start].x > 0.0 {
                            line_index += 1;
                            let shift = placed[start].x;
                            for glyph in &mut placed[start..] {
                                glyph.x -= shift;
                                glyph.line = line_index;
                                glyph.baseline = line_metrics.line_height * line_index as f32
                                    + line_metrics.ascent;
                            }
                            pen -= shift;
                        }
                    }
                }
                // Step 2: moving the word may not have been enough — a word
                // longer than the whole limit still overflows after the move.
                // Break it here rather than letting it paint out of bounds.
                if pen + advance > limit && pen > 0.0 {
                    line_index += 1;
                    pen = 0.0;
                    word_start = None;
                }
            }

            if ch == ' ' {
                word_start = None;
            } else if word_start.is_none() {
                word_start = Some(placed.len());
            }

            placed.push(PositionedGlyph {
                ch,
                x: pen,
                baseline: line_metrics.line_height * line_index as f32 + line_metrics.ascent,
                line: line_index,
            });
            pen += advance;
        }
        placed
    }
}

/// Metrics-only source used when no font has been supplied.
///
/// It reports a plausible monospace advance so layout is *consistent* rather
/// than correct, and rasterizes nothing. This exists so `measure_text` stops
/// returning `0.0` — laying out against a zero width silently stacks every
/// label at the same spot.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsOnlyFont;

impl GlyphSource for MetricsOnlyFont {
    fn glyph_metrics(&self, _ch: char, size: f32) -> GlyphMetrics {
        let advance = if size.is_finite() && size > 0.0 {
            size * 0.5
        } else {
            0.0
        };
        GlyphMetrics {
            advance,
            bearing_x: 0.0,
            bearing_y: 0.0,
            width: 0,
            height: 0,
        }
    }

    fn rasterize(&self, _ch: char, size: f32) -> GlyphBitmap {
        GlyphBitmap::blank(self.glyph_metrics('x', size).advance)
    }

    fn line_metrics(&self, size: f32) -> LineMetrics {
        let size = if size.is_finite() && size > 0.0 {
            size
        } else {
            0.0
        };
        LineMetrics {
            ascent: size * 0.8,
            descent: size * 0.2,
            line_height: size * 1.2,
        }
    }

    fn has_glyph(&self, _ch: char) -> bool {
        false
    }
}

impl Default for Font {
    /// A font that measures but draws nothing. Replace it with a real
    /// rasterizer via [`Font::new`] before expecting glyphs on screen.
    fn default() -> Self {
        Self::new(Box::new(MetricsOnlyFont))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A rasterizer stub with a per-character advance, so layout can be tested
    /// without a font file.
    struct StubFont {
        advance: f32,
        calls: Arc<AtomicUsize>,
    }

    impl StubFont {
        fn new(advance: f32) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    advance,
                    calls: Arc::clone(&calls),
                },
                calls,
            )
        }
    }

    impl GlyphSource for StubFont {
        fn glyph_metrics(&self, _ch: char, _size: f32) -> GlyphMetrics {
            GlyphMetrics {
                advance: self.advance,
                bearing_x: 0.0,
                bearing_y: self.advance,
                width: 1,
                height: 1,
            }
        }
        fn rasterize(&self, ch: char, size: f32) -> GlyphBitmap {
            self.calls.fetch_add(1, Ordering::SeqCst);
            GlyphBitmap {
                metrics: self.glyph_metrics(ch, size),
                coverage: vec![255],
            }
        }
        fn line_metrics(&self, _size: f32) -> LineMetrics {
            LineMetrics {
                ascent: 8.0,
                descent: 2.0,
                line_height: 10.0,
            }
        }
        fn has_glyph(&self, ch: char) -> bool {
            ch.is_ascii_graphic() || ch == ' '
        }
    }

    fn font(advance: f32) -> (Font, Arc<AtomicUsize>) {
        let (stub, calls) = StubFont::new(advance);
        (Font::new(Box::new(stub)), calls)
    }

    #[test]
    fn measuring_sums_advances_and_reports_line_count() {
        let (font, _) = font(6.0);
        let metrics = font.measure("abcd", 12.0);
        assert_eq!(metrics.width, 24.0);
        assert_eq!(metrics.lines, 1);
        assert_eq!(metrics.height, 10.0);

        let two = font.measure("ab\ncd", 12.0);
        assert_eq!(two.lines, 2);
        assert_eq!(two.height, 20.0);
        // The newline itself takes no width.
        assert_eq!(two.width, 24.0);
    }

    #[test]
    fn the_glyph_cache_rasterizes_each_char_once_per_size() {
        let (mut font, calls) = font(6.0);
        for _ in 0..5 {
            let _ = font.glyph('a', 12.0);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the cache did not hold");
        assert_eq!(font.cached_glyphs(), 1);

        // A different size is a different glyph.
        let _ = font.glyph('a', 24.0);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(font.cached_glyphs(), 2);

        font.clear_cache();
        assert_eq!(font.cached_glyphs(), 0);
        let _ = font.glyph('a', 12.0);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn layout_places_glyphs_left_to_right_on_one_line() {
        let (font, _) = font(6.0);
        let placed = font.layout("abc", 12.0, None);
        assert_eq!(placed.len(), 3);
        assert_eq!(placed[0].x, 0.0);
        assert_eq!(placed[1].x, 6.0);
        assert_eq!(placed[2].x, 12.0);
        assert!(placed.iter().all(|glyph| glyph.line == 0));
        assert!(placed.iter().all(|glyph| glyph.baseline == 8.0));
    }

    #[test]
    fn an_explicit_newline_starts_a_new_line() {
        let (font, _) = font(6.0);
        let placed = font.layout("ab\ncd", 12.0, None);
        assert_eq!(placed.len(), 4);
        assert_eq!(placed[2].line, 1);
        assert_eq!(placed[2].x, 0.0);
        assert_eq!(placed[2].baseline, 18.0);
    }

    #[test]
    fn wrapping_moves_a_whole_word_to_the_next_line() {
        let (font, _) = font(10.0);
        // "aa bbb" at 10px each: "aa " is 30 wide, "bbb" would reach 60.
        let placed = font.layout("aa bbb", 12.0, Some(40.0));
        let lines: Vec<usize> = placed.iter().map(|glyph| glyph.line).collect();
        assert_eq!(
            lines,
            vec![0, 0, 0, 1, 1, 1],
            "the word was split: {lines:?}"
        );
        // The moved word restarts at the left margin.
        assert_eq!(placed[3].x, 0.0);
    }

    #[test]
    fn a_word_longer_than_the_limit_is_broken_rather_than_overflowing() {
        let (font, _) = font(10.0);
        let placed = font.layout("aaaaaa", 12.0, Some(25.0));
        // Two glyphs fit per line at 10px with a 25px limit.
        assert!(placed.iter().any(|glyph| glyph.line > 0));
        let max_x = placed
            .iter()
            .map(|glyph| glyph.x + 10.0)
            .fold(0.0f32, f32::max);
        assert!(
            max_x <= 25.0 + 1.0e-3,
            "a glyph escaped the limit: {max_x} > 25"
        );
    }

    #[test]
    fn the_metrics_only_font_measures_without_drawing() {
        let mut font = Font::default();
        let metrics = font.measure("abcd", 10.0);
        assert!(metrics.width > 0.0, "measure_text would still be lying");
        assert_eq!(metrics.width, 20.0);
        assert!(font.glyph('a', 10.0).is_blank());
        assert!(!font.has_glyph('a'));
    }

    #[test]
    fn a_nonsense_size_measures_as_zero_rather_than_nan() {
        let font = Font::default();
        for size in [0.0f32, -12.0, f32::NAN, f32::INFINITY] {
            let metrics = font.measure("abc", size);
            assert!(
                metrics.width.is_finite() && metrics.width >= 0.0,
                "size {size} produced width {}",
                metrics.width
            );
            assert!(metrics.height.is_finite() && metrics.height >= 0.0);
        }
    }

    #[test]
    fn empty_text_has_zero_width_but_one_line() {
        let (font, _) = font(6.0);
        let metrics = font.measure("", 12.0);
        assert_eq!(metrics.width, 0.0);
        assert_eq!(metrics.lines, 1);
        assert!(font.layout("", 12.0, Some(100.0)).is_empty());
    }

    #[test]
    fn coverage_runs_coalesce_equal_neighbours_and_skip_transparent_pixels() {
        let bitmap = GlyphBitmap {
            metrics: GlyphMetrics {
                advance: 4.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
                width: 4,
                height: 2,
            },
            // Row 0: two solid, one transparent, one faint.
            // Row 1: entirely transparent.
            coverage: vec![255, 255, 0, 80, 0, 0, 0, 0],
        };
        let runs = bitmap.runs();
        assert_eq!(
            runs,
            vec![(0, 0, 2, 255), (0, 3, 1, 80)],
            "runs did not coalesce, or a transparent pixel was drawn"
        );
    }

    #[test]
    fn a_malformed_bitmap_yields_no_runs_rather_than_indexing_out_of_bounds() {
        // Coverage shorter than width*height: a rasterizer bug must not become
        // a panic in the renderer.
        let bitmap = GlyphBitmap {
            metrics: GlyphMetrics {
                advance: 4.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
                width: 4,
                height: 4,
            },
            coverage: vec![255, 255],
        };
        assert!(bitmap.runs().is_empty());
        assert!(GlyphBitmap::blank(3.0).runs().is_empty());
    }
}
