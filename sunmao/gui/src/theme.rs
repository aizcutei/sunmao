//! Theme tokens.
//!
//! Widgets read colours from a [`Theme`] rather than hard-coding them, so a
//! plugin can restyle its whole editor by swapping one value. The tokens are
//! named by *role* (`surface`, `accent`, `track`) rather than by appearance
//! (`dark_grey`), which is what lets the same widget code render correctly in
//! both [`Theme::dark`] and [`Theme::light`].

use crate::Color;

/// A palette of role-named colours plus the few metrics widgets need.
///
/// ```
/// # use sunmao_gui::Theme;
/// let dark = Theme::dark();
/// let light = Theme::light();
/// // Foreground and background swap contrast between the two.
/// assert!(dark.foreground.luminance() > dark.background.luminance());
/// assert!(light.foreground.luminance() < light.background.luminance());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Editor backdrop.
    pub background: Color,
    /// Raised areas: control plates, dropdown bodies.
    pub surface: Color,
    /// Primary text and pointers.
    pub foreground: Color,
    /// Secondary text, disabled controls.
    pub muted: Color,
    /// The colour that indicates value or "on".
    pub accent: Color,
    /// Accent under the pointer.
    pub accent_hover: Color,
    /// Unfilled part of a knob arc or slider groove.
    pub track: Color,
    /// Hairlines and focus rings.
    pub border: Color,
    /// Corner rounding in logical pixels, for renderers that support it.
    pub corner_radius: f32,
    /// Default gap between siblings in a [`crate::Column`] or [`crate::Row`].
    pub spacing: f32,
}

impl Theme {
    /// The default dark theme. Matches the colours the widgets shipped with
    /// before themes existed, so existing editors are unchanged.
    pub const fn dark() -> Self {
        Self {
            background: Color::rgb(0.15, 0.15, 0.18),
            surface: Color::rgb(0.20, 0.20, 0.22),
            foreground: Color::rgb(0.90, 0.90, 0.90),
            muted: Color::rgb(0.55, 0.55, 0.60),
            accent: Color::rgb(0.20, 0.60, 1.00),
            accent_hover: Color::rgb(0.30, 0.70, 1.00),
            track: Color::rgb(0.30, 0.30, 0.35),
            border: Color::rgb(0.35, 0.35, 0.40),
            corner_radius: 4.0,
            spacing: 8.0,
        }
    }

    /// A light theme with the same roles.
    pub const fn light() -> Self {
        Self {
            background: Color::rgb(0.94, 0.94, 0.96),
            surface: Color::rgb(1.00, 1.00, 1.00),
            foreground: Color::rgb(0.12, 0.12, 0.15),
            muted: Color::rgb(0.45, 0.45, 0.50),
            accent: Color::rgb(0.10, 0.45, 0.85),
            accent_hover: Color::rgb(0.15, 0.55, 0.95),
            track: Color::rgb(0.82, 0.82, 0.86),
            border: Color::rgb(0.70, 0.70, 0.75),
            corner_radius: 4.0,
            spacing: 8.0,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text has to stay readable against its own background in both themes.
    /// A widget that looked right only in the dark palette would be a
    /// regression nobody notices until a user switches.
    #[test]
    fn both_themes_keep_text_readable_against_their_surfaces() {
        for (name, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for (role, surface) in [("background", theme.background), ("surface", theme.surface)] {
                let contrast = (theme.foreground.luminance() - surface.luminance()).abs();
                assert!(
                    contrast > 0.4,
                    "{name} theme: foreground on {role} has only {contrast:.3} luminance contrast"
                );
            }
            // Muted text is deliberately lower contrast, but must not vanish.
            let muted = (theme.muted.luminance() - theme.background.luminance()).abs();
            assert!(muted > 0.1, "{name} theme: muted text is invisible");
            // The accent has to read against the track it is drawn over.
            let accent = (theme.accent.luminance() - theme.track.luminance()).abs();
            assert!(accent > 0.05, "{name} theme: accent is lost against track");
        }
    }

    #[test]
    fn the_default_theme_is_the_dark_one() {
        assert_eq!(Theme::default(), Theme::dark());
    }
}
