//! TTF/OTF rasterization, behind the `text` feature.
//!
//! [`TtfFont`] is a [`GlyphSource`] backed by `fontdue`. The caller supplies
//! the font bytes: SunMao ships no font of its own, because bundling one is a
//! licensing and binary-size decision that belongs to whoever ships the plugin.
//! Without a font, [`Font::default`] still measures — see [`MetricsOnlyFont`].
//!
//! [`Font::default`]: crate::Font
//! [`MetricsOnlyFont`]: crate::MetricsOnlyFont

use crate::{GlyphBitmap, GlyphMetrics, GlyphSource, LineMetrics};

/// Why a font could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontError(pub String);

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not load font: {}", self.0)
    }
}

impl std::error::Error for FontError {}

/// A parsed TTF/OTF face.
pub struct TtfFont {
    inner: fontdue::Font,
}

impl TtfFont {
    /// Parse font bytes.
    ///
    /// ```
    /// # use sunmao_gui::TtfFont;
    /// // Anything that is not a font is rejected rather than half-parsed.
    /// assert!(TtfFont::from_bytes(b"not a font").is_err());
    /// ```
    pub fn from_bytes(data: &[u8]) -> Result<Self, FontError> {
        let inner = fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
            .map_err(|error| FontError(error.to_string()))?;
        Ok(Self { inner })
    }

    /// The underlying face, for callers that need `fontdue` directly.
    pub fn face(&self) -> &fontdue::Font {
        &self.inner
    }
}

/// Sizes that cannot produce a sensible glyph. Rasterizing at a non-finite or
/// non-positive size would either panic inside the rasterizer or allocate an
/// absurd bitmap.
fn usable(size: f32) -> Option<f32> {
    (size.is_finite() && size > 0.0 && size <= 512.0).then_some(size)
}

impl GlyphSource for TtfFont {
    fn glyph_metrics(&self, ch: char, size: f32) -> GlyphMetrics {
        let Some(size) = usable(size) else {
            return GlyphMetrics {
                advance: 0.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
                width: 0,
                height: 0,
            };
        };
        let metrics = self.inner.metrics(ch, size);
        GlyphMetrics {
            advance: metrics.advance_width,
            bearing_x: metrics.xmin as f32,
            bearing_y: (metrics.height as i32 + metrics.ymin) as f32,
            width: metrics.width as u32,
            height: metrics.height as u32,
        }
    }

    fn rasterize(&self, ch: char, size: f32) -> GlyphBitmap {
        let Some(size) = usable(size) else {
            return GlyphBitmap::blank(0.0);
        };
        let (metrics, coverage) = self.inner.rasterize(ch, size);
        GlyphBitmap {
            metrics: GlyphMetrics {
                advance: metrics.advance_width,
                bearing_x: metrics.xmin as f32,
                bearing_y: (metrics.height as i32 + metrics.ymin) as f32,
                width: metrics.width as u32,
                height: metrics.height as u32,
            },
            coverage,
        }
    }

    fn line_metrics(&self, size: f32) -> LineMetrics {
        let Some(size) = usable(size) else {
            return LineMetrics {
                ascent: 0.0,
                descent: 0.0,
                line_height: 0.0,
            };
        };
        match self.inner.horizontal_line_metrics(size) {
            Some(metrics) => LineMetrics {
                ascent: metrics.ascent,
                // fontdue reports descent below the baseline as negative.
                descent: -metrics.descent,
                line_height: metrics.new_line_size,
            },
            // A face without horizontal metrics is unusual but legal; fall back
            // to proportions rather than reporting a zero-height line, which
            // would stack every line on top of the last.
            None => LineMetrics {
                ascent: size * 0.8,
                descent: size * 0.2,
                line_height: size * 1.2,
            },
        }
    }

    fn has_glyph(&self, ch: char) -> bool {
        self.inner.lookup_glyph_index(ch) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Font;

    fn ubuntu() -> TtfFont {
        TtfFont::from_bytes(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font should parse")
    }

    #[test]
    fn garbage_is_rejected_rather_than_half_parsed() {
        assert!(TtfFont::from_bytes(b"").is_err(), "empty bytes parsed");
        assert!(
            TtfFont::from_bytes(b"not a font at all").is_err(),
            "arbitrary bytes parsed as a font"
        );
        match TtfFont::from_bytes(b"\x00\x01\x02\x03") {
            Err(error) => assert!(
                !error.to_string().is_empty(),
                "the rejection carried no diagnosis"
            ),
            Ok(_) => panic!("four arbitrary bytes parsed as a font"),
        }
    }

    #[test]
    fn a_real_face_rasterizes_ink_for_printable_characters() {
        let font = ubuntu();
        let glyph = font.rasterize('A', 24.0);
        assert!(glyph.metrics.width > 0 && glyph.metrics.height > 0);
        assert_eq!(
            glyph.coverage.len(),
            (glyph.metrics.width * glyph.metrics.height) as usize,
            "coverage does not match the reported bitmap size"
        );
        assert!(
            glyph.coverage.iter().any(|value| *value > 0),
            "'A' rasterized to an entirely empty bitmap"
        );
        // A space occupies width but no ink.
        let space = font.rasterize(' ', 24.0);
        assert!(space.metrics.advance > 0.0);
        assert!(space.coverage.iter().all(|value| *value == 0));
    }

    #[test]
    fn advances_grow_with_size_and_wide_glyphs_exceed_narrow_ones() {
        let font = ubuntu();
        let small = font.glyph_metrics('m', 12.0).advance;
        let large = font.glyph_metrics('m', 24.0).advance;
        assert!(large > small, "advance did not scale: {small} -> {large}");
        assert!(
            font.glyph_metrics('m', 24.0).advance > font.glyph_metrics('i', 24.0).advance,
            "a proportional face reported 'i' as wide as 'm'"
        );
    }

    #[test]
    fn line_metrics_are_positive_and_ascent_leads_descent() {
        let font = ubuntu();
        let metrics = font.line_metrics(20.0);
        assert!(metrics.ascent > 0.0, "ascent {}", metrics.ascent);
        assert!(metrics.descent >= 0.0, "descent {}", metrics.descent);
        assert!(
            metrics.line_height >= metrics.ascent + metrics.descent - 1.0e-3,
            "lines would overlap: height {} vs {}+{}",
            metrics.line_height,
            metrics.ascent,
            metrics.descent
        );
    }

    #[test]
    fn nonsense_sizes_produce_empty_glyphs_rather_than_panicking() {
        let font = ubuntu();
        for size in [0.0f32, -12.0, f32::NAN, f32::INFINITY, 1.0e9] {
            let glyph = font.rasterize('A', size);
            assert!(glyph.is_blank(), "size {size} produced ink");
            assert_eq!(glyph.metrics.advance, 0.0);
            let line = font.line_metrics(size);
            assert!(line.line_height.is_finite());
        }
    }

    #[test]
    fn coverage_is_reported_for_glyphs_the_face_has_and_withheld_for_ones_it_lacks() {
        let font = ubuntu();
        assert!(font.has_glyph('A'));
        // Ubuntu Light has no CJK coverage; callers use this to substitute
        // rather than silently drawing a blank.
        assert!(!font.has_glyph('漢'));
    }

    #[test]
    fn a_real_face_drives_the_shared_font_cache_and_layout() {
        let mut font = Font::new(Box::new(ubuntu()));
        let metrics = font.measure("Hello", 16.0);
        assert!(metrics.width > 0.0);
        assert!(metrics.ascent > 0.0);

        let _ = font.glyph('H', 16.0);
        let _ = font.glyph('H', 16.0);
        assert_eq!(font.cached_glyphs(), 1, "the cache did not hold");

        let placed = font.layout("Hello world", 16.0, Some(metrics.width));
        assert!(
            placed.iter().any(|glyph| glyph.line > 0),
            "the second word should have wrapped"
        );
    }
}
