//! Cross-platform child WebView support for baseview windows.

use raw_window_handle::HasWindowHandle;
use std::error::Error;
#[cfg(target_os = "linux")]
use std::ffi::CString;
use std::fmt;
use std::sync::mpsc;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebViewBuilder};

#[derive(Debug)]
pub enum WebViewError {
    InvalidHandlerName(String),
    Initialization(String),
    Backend(wry::Error),
}

impl fmt::Display for WebViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandlerName(name) => {
                write!(formatter, "invalid JavaScript message handler name: {name}")
            }
            Self::Initialization(message) => formatter.write_str(message),
            Self::Backend(error) => error.fmt(formatter),
        }
    }
}

impl Error for WebViewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            _ => None,
        }
    }
}

impl From<wry::Error> for WebViewError {
    fn from(error: wry::Error) -> Self {
        Self::Backend(error)
    }
}

pub struct WebView {
    webview: wry::WebView,
}

#[cfg(target_os = "linux")]
static GTK_INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();

#[cfg(target_os = "linux")]
fn ensure_gtk_initialized() -> Result<(), WebViewError> {
    GTK_INITIALIZED
        .get_or_init(|| {
            let program = CString::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "sunmao".to_string()),
            )
            .map_err(|error| format!("invalid GTK program name: {error}"))?;
            let mut argc = 1;
            let mut argv = vec![program.as_ptr().cast_mut(), std::ptr::null_mut()];
            let mut argv_ptr = argv.as_mut_ptr();
            let initialized = unsafe { gtk::ffi::gtk_init_check(&mut argc, &mut argv_ptr) };
            if initialized == 0 {
                Err("GTK could not initialize on the X11 display".to_string())
            } else {
                Ok(())
            }
        })
        .clone()
        .map_err(WebViewError::Initialization)
}

impl WebView {
    pub fn new<W: HasWindowHandle>(
        parent: &W,
        width: f64,
        height: f64,
        message_handler_name: &str,
        html: &str,
    ) -> Result<(Self, mpsc::Receiver<String>), WebViewError> {
        if !is_javascript_identifier(message_handler_name) {
            return Err(WebViewError::InvalidHandlerName(
                message_handler_name.to_string(),
            ));
        }

        #[cfg(target_os = "linux")]
        #[cfg(target_os = "linux")]
        ensure_gtk_initialized()?;

        let (sender, receiver) = mpsc::channel();
        let bridge = format!(
            "window.{message_handler_name} = {{ postMessage: function(data) {{ \
             window.ipc.postMessage(typeof data === 'string' ? data : JSON.stringify(data)); \
             }} }};"
        );
        let bounds = bounds(width, height);
        let webview = WebViewBuilder::new()
            .with_bounds(bounds)
            .with_initialization_script(bridge)
            .with_ipc_handler(move |request| {
                let _ = sender.send(request.body().clone());
            })
            .build_as_child(parent)?;
        // Attach the native child before starting navigation. WKWebView can
        // otherwise remain on its initial white page when created inside an
        // already-embedded plugin view.
        webview.load_html(html)?;

        Ok((Self { webview }, receiver))
    }

    pub fn load_html(&self, html: &str) -> Result<(), WebViewError> {
        self.webview.load_html(html).map_err(Into::into)
    }

    pub fn evaluate_js(&self, javascript: &str) -> Result<(), WebViewError> {
        self.webview.evaluate_script(javascript).map_err(Into::into)
    }

    pub fn set_size(&self, width: f64, height: f64) -> Result<(), WebViewError> {
        self.webview
            .set_bounds(bounds(width, height))
            .map_err(Into::into)
    }

    pub fn poll_events(&self) {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    pub fn pump_platform_events() {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
}

fn bounds(width: f64, height: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(width.max(0.0), height.max(0.0)).into(),
    }
}

fn is_javascript_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|character| {
            character == '_' || character == '$' || character.is_ascii_alphanumeric()
        })
}

#[cfg(test)]
mod tests {
    use super::is_javascript_identifier;

    #[test]
    fn javascript_handler_names_are_validated_before_interpolation() {
        assert!(is_javascript_identifier("sunmao"));
        assert!(is_javascript_identifier("_bridge2"));
        assert!(is_javascript_identifier("$bridge"));
        assert!(!is_javascript_identifier(""));
        assert!(!is_javascript_identifier("2bridge"));
        assert!(!is_javascript_identifier("bridge-name"));
        assert!(!is_javascript_identifier("bridge;alert(1)"));
    }
}
