//! Clipboard access for editors.
//!
//! Behind a trait rather than calling a platform API directly, for the same
//! reason [`GlyphSource`] is: the logic that decides *what* to copy is worth
//! testing without a windowing system attached, and a host that already owns a
//! clipboard connection can supply its own.
//!
//! [`GlyphSource`]: crate::GlyphSource

/// Read and write the system clipboard's text.
pub trait Clipboard: Send {
    /// Current clipboard text, or `None` when it holds something else — an
    /// image, say — or cannot be read.
    fn get_text(&mut self) -> Option<String>;
    /// Replace the clipboard's contents. `false` if the write failed.
    fn set_text(&mut self, text: &str) -> bool;
}

/// An in-process clipboard.
///
/// Used by tests, and as the fallback when the platform clipboard cannot be
/// opened — copying into a void that at least round-trips within the editor is
/// less surprising than a control that silently does nothing.
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    text: Option<String>,
}

impl Clipboard for MemoryClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: &str) -> bool {
        self.text = Some(text.to_string());
        true
    }
}

/// The platform clipboard, behind the `clipboard` feature.
#[cfg(feature = "clipboard")]
pub struct SystemClipboard {
    inner: Option<arboard::Clipboard>,
}

#[cfg(feature = "clipboard")]
impl SystemClipboard {
    /// Connect to the platform clipboard.
    ///
    /// A failure here is not fatal: a plugin running under a session with no
    /// clipboard (a headless CI runner, for one) should still open its editor.
    /// The handle then reports every operation as failed rather than panicking.
    pub fn new() -> Self {
        Self {
            inner: arboard::Clipboard::new().ok(),
        }
    }

    /// Whether a platform clipboard was actually available.
    pub fn is_connected(&self) -> bool {
        self.inner.is_some()
    }
}

#[cfg(feature = "clipboard")]
impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "clipboard")]
impl Clipboard for SystemClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.inner.as_mut()?.get_text().ok()
    }

    fn set_text(&mut self, text: &str) -> bool {
        self.inner
            .as_mut()
            .is_some_and(|clipboard| clipboard.set_text(text).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_clipboard_round_trips_text() {
        let mut clipboard = MemoryClipboard::default();
        assert_eq!(clipboard.get_text(), None);
        assert!(clipboard.set_text("0.75"));
        assert_eq!(clipboard.get_text().as_deref(), Some("0.75"));
        // Overwriting replaces rather than appends.
        clipboard.set_text("Warm");
        assert_eq!(clipboard.get_text().as_deref(), Some("Warm"));
    }

    /// The platform clipboard is unavailable on a headless runner, which must
    /// degrade rather than panic — a plugin still has to open its editor there.
    #[cfg(feature = "clipboard")]
    #[test]
    fn a_disconnected_system_clipboard_fails_every_operation_quietly() {
        let mut clipboard = SystemClipboard { inner: None };
        assert!(!clipboard.is_connected());
        assert_eq!(clipboard.get_text(), None);
        assert!(!clipboard.set_text("anything"));
    }
}
