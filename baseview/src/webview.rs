//! Cross-platform child WebView support for baseview windows.
//!
//! On Linux, GTK and WebKitGTK are permanently bound to the first thread that
//! initializes GTK; touching them from another thread deadlocks inside
//! WebKit's synchronous UI-process IPC. Plugin hosts open and close editors
//! from short-lived event threads, so all GTK work runs on one dedicated,
//! process-lifetime thread and the public [`WebView`] type is a proxy that
//! marshals calls onto it.

use raw_window_handle::HasWindowHandle;
use std::error::Error;
use std::fmt;
use std::sync::mpsc;

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
    #[cfg(not(target_os = "linux"))]
    webview: wry::WebView,
    #[cfg(target_os = "linux")]
    proxy: gtk_thread::WebViewProxy,
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

        let (sender, receiver) = mpsc::channel();
        let bridge = format!(
            "window.{message_handler_name} = {{ postMessage: function(data) {{ \
             window.ipc.postMessage(typeof data === 'string' ? data : JSON.stringify(data)); \
             }} }};"
        );

        #[cfg(target_os = "linux")]
        {
            let proxy = gtk_thread::create_webview(
                parent,
                width,
                height,
                bridge,
                html.to_string(),
                sender,
            )?;
            Ok((Self { proxy }, receiver))
        }

        #[cfg(not(target_os = "linux"))]
        {
            let bounds = bounds(width, height);
            let webview = wry::WebViewBuilder::new()
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
    }

    pub fn load_html(&self, html: &str) -> Result<(), WebViewError> {
        #[cfg(target_os = "linux")]
        return self.proxy.load_html(html);
        #[cfg(not(target_os = "linux"))]
        self.webview.load_html(html).map_err(Into::into)
    }

    pub fn evaluate_js(&self, javascript: &str) -> Result<(), WebViewError> {
        #[cfg(target_os = "linux")]
        return self.proxy.evaluate_js(javascript);
        #[cfg(not(target_os = "linux"))]
        self.webview.evaluate_script(javascript).map_err(Into::into)
    }

    pub fn set_size(&self, width: f64, height: f64) -> Result<(), WebViewError> {
        #[cfg(target_os = "linux")]
        return self.proxy.set_size(width, height);
        #[cfg(not(target_os = "linux"))]
        self.webview
            .set_bounds(bounds(width, height))
            .map_err(Into::into)
    }

    pub fn poll_events(&self) {
        // The dedicated Linux GTK thread pumps events continuously; other
        // platforms integrate with the host event loop.
    }

    pub fn pump_platform_events() {
        #[cfg(target_os = "linux")]
        gtk_thread::synchronize();
    }
}

#[cfg(not(target_os = "linux"))]
fn bounds(width: f64, height: f64) -> wry::Rect {
    use wry::dpi::{LogicalPosition, LogicalSize};
    wry::Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(width.max(0.0), height.max(0.0)).into(),
    }
}

#[cfg(target_os = "linux")]
mod gtk_thread {
    use super::WebViewError;
    use raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
    };
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::sync::mpsc;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    /// How long proxy calls wait for the GTK thread before reporting an
    /// error. The GTK thread never blocks, so this only trips if WebKit
    /// itself wedges — the caller then gets a diagnosis instead of a hang.
    const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

    type Reply<T> = mpsc::Sender<T>;

    enum Command {
        Create {
            parent: u64,
            width: f64,
            height: f64,
            bridge: String,
            html: String,
            ipc: mpsc::Sender<String>,
            reply: Reply<Result<u64, String>>,
        },
        LoadHtml {
            id: u64,
            html: String,
            reply: Reply<Result<(), String>>,
        },
        EvaluateJs {
            id: u64,
            javascript: String,
            reply: Reply<Result<(), String>>,
        },
        SetSize {
            id: u64,
            width: f64,
            height: f64,
            reply: Reply<Result<(), String>>,
        },
        Destroy {
            id: u64,
            reply: Reply<()>,
        },
        Synchronize {
            reply: Reply<()>,
        },
    }

    /// Wraps a raw X11 window id so wry can adopt it as a parent. wry's X11
    /// child path only reads the window field.
    struct XlibParent(u64);

    impl HasWindowHandle for XlibParent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let handle = XlibWindowHandle::new(self.0 as std::ffi::c_ulong);
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(handle)) })
        }
    }

    fn command_sender() -> Result<&'static mpsc::Sender<Command>, WebViewError> {
        static SENDER: OnceLock<Result<mpsc::Sender<Command>, String>> = OnceLock::new();
        SENDER
            .get_or_init(|| {
                let (ready_sender, ready_receiver) = mpsc::channel();
                std::thread::Builder::new()
                    .name("sunmao-gtk-webview".into())
                    .spawn(move || run_gtk_thread(ready_sender))
                    .map_err(|error| format!("failed to spawn GTK thread: {error}"))?;
                ready_receiver
                    .recv_timeout(REPLY_TIMEOUT)
                    .map_err(|_| "GTK thread did not start".to_string())?
            })
            .as_ref()
            .map_err(|error| WebViewError::Initialization(error.clone()))
    }

    fn run_gtk_thread(ready: mpsc::Sender<Result<mpsc::Sender<Command>, String>>) {
        let program = CString::new(
            std::env::args()
                .next()
                .unwrap_or_else(|| "sunmao".to_string()),
        )
        .unwrap_or_else(|_| CString::new("sunmao").expect("static name"));
        let mut argc = 1;
        let mut argv = vec![program.as_ptr().cast_mut(), std::ptr::null_mut()];
        let mut argv_ptr = argv.as_mut_ptr();
        let initialized = unsafe { gtk::ffi::gtk_init_check(&mut argc, &mut argv_ptr) };
        if initialized == 0 {
            let _ = ready.send(Err(
                "GTK could not initialize on the X11 display".to_string()
            ));
            return;
        }

        let (sender, receiver) = mpsc::channel();
        if ready.send(Ok(sender)).is_err() {
            return;
        }

        let mut webviews: HashMap<u64, wry::WebView> = HashMap::new();
        let mut next_id: u64 = 1;
        loop {
            // Serve commands first so editor open/close never waits behind
            // rendering work.
            loop {
                match receiver.try_recv() {
                    Ok(command) => handle_command(command, &mut webviews, &mut next_id),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }

            // Pump GTK with a time budget: WebKitGTK re-arms frame-clock
            // sources continuously, so an unbounded drain never terminates.
            let deadline = Instant::now() + Duration::from_millis(10);
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
                if Instant::now() >= deadline {
                    break;
                }
            }

            match receiver.recv_timeout(Duration::from_millis(4)) {
                Ok(command) => handle_command(command, &mut webviews, &mut next_id),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn handle_command(
        command: Command,
        webviews: &mut HashMap<u64, wry::WebView>,
        next_id: &mut u64,
    ) {
        match command {
            Command::Create {
                parent,
                width,
                height,
                bridge,
                html,
                ipc,
                reply,
            } => {
                let result = build_webview(parent, width, height, &bridge, &html, ipc);
                let _ = reply.send(result.map(|webview| {
                    let id = *next_id;
                    *next_id += 1;
                    webviews.insert(id, webview);
                    id
                }));
            }
            Command::LoadHtml { id, html, reply } => {
                let _ = reply.send(with_webview(webviews, id, |webview| {
                    webview.load_html(&html).map_err(|error| error.to_string())
                }));
            }
            Command::EvaluateJs {
                id,
                javascript,
                reply,
            } => {
                let _ = reply.send(with_webview(webviews, id, |webview| {
                    webview
                        .evaluate_script(&javascript)
                        .map_err(|error| error.to_string())
                }));
            }
            Command::SetSize {
                id,
                width,
                height,
                reply,
            } => {
                let _ = reply.send(with_webview(webviews, id, |webview| {
                    webview
                        .set_bounds(linux_bounds(width, height))
                        .map_err(|error| error.to_string())
                }));
            }
            Command::Destroy { id, reply } => {
                webviews.remove(&id);
                // Deliver WebKit's teardown work now, while the X11 parent
                // still exists, so its foreign GdkWindow does not outlive it.
                let deadline = Instant::now() + Duration::from_millis(100);
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                    if Instant::now() >= deadline {
                        break;
                    }
                }
                let _ = reply.send(());
            }
            Command::Synchronize { reply } => {
                let _ = reply.send(());
            }
        }
    }

    fn with_webview(
        webviews: &HashMap<u64, wry::WebView>,
        id: u64,
        operation: impl FnOnce(&wry::WebView) -> Result<(), String>,
    ) -> Result<(), String> {
        match webviews.get(&id) {
            Some(webview) => operation(webview),
            None => Err("WebView was already destroyed".to_string()),
        }
    }

    fn build_webview(
        parent: u64,
        width: f64,
        height: f64,
        bridge: &str,
        html: &str,
        ipc: mpsc::Sender<String>,
    ) -> Result<wry::WebView, String> {
        let parent = XlibParent(parent);
        let webview = wry::WebViewBuilder::new()
            .with_bounds(linux_bounds(width, height))
            .with_initialization_script(bridge)
            .with_ipc_handler(move |request| {
                let _ = ipc.send(request.body().clone());
            })
            .build_as_child(&parent)
            .map_err(|error| error.to_string())?;
        webview.load_html(html).map_err(|error| error.to_string())?;
        Ok(webview)
    }

    fn linux_bounds(width: f64, height: f64) -> wry::Rect {
        use wry::dpi::{LogicalPosition, LogicalSize};
        wry::Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(width.max(0.0), height.max(0.0)).into(),
        }
    }

    fn request<T>(
        build: impl FnOnce(Reply<T>) -> Command,
        context: &str,
    ) -> Result<T, WebViewError> {
        let sender = command_sender()?;
        let (reply_sender, reply_receiver) = mpsc::channel();
        sender
            .send(build(reply_sender))
            .map_err(|_| WebViewError::Initialization("GTK thread exited".to_string()))?;
        reply_receiver.recv_timeout(REPLY_TIMEOUT).map_err(|_| {
            WebViewError::Initialization(format!("GTK thread did not answer {context}"))
        })
    }

    pub(super) struct WebViewProxy {
        id: u64,
    }

    impl WebViewProxy {
        pub(super) fn load_html(&self, html: &str) -> Result<(), WebViewError> {
            request(
                |reply| Command::LoadHtml {
                    id: self.id,
                    html: html.to_string(),
                    reply,
                },
                "load_html",
            )?
            .map_err(WebViewError::Initialization)
        }

        pub(super) fn evaluate_js(&self, javascript: &str) -> Result<(), WebViewError> {
            request(
                |reply| Command::EvaluateJs {
                    id: self.id,
                    javascript: javascript.to_string(),
                    reply,
                },
                "evaluate_js",
            )?
            .map_err(WebViewError::Initialization)
        }

        pub(super) fn set_size(&self, width: f64, height: f64) -> Result<(), WebViewError> {
            request(
                |reply| Command::SetSize {
                    id: self.id,
                    width,
                    height,
                    reply,
                },
                "set_size",
            )?
            .map_err(WebViewError::Initialization)
        }
    }

    impl Drop for WebViewProxy {
        fn drop(&mut self) {
            // Block until WebKit teardown ran so callers may destroy the X11
            // parent immediately afterwards.
            let _ = request(|reply| Command::Destroy { id: self.id, reply }, "destroy");
        }
    }

    pub(super) fn create_webview<W: HasWindowHandle>(
        parent: &W,
        width: f64,
        height: f64,
        bridge: String,
        html: String,
        ipc: mpsc::Sender<String>,
    ) -> Result<WebViewProxy, WebViewError> {
        let raw = parent
            .window_handle()
            .map_err(|error| WebViewError::Initialization(format!("invalid parent: {error}")))?
            .as_raw();
        let RawWindowHandle::Xlib(handle) = raw else {
            return Err(WebViewError::Initialization(
                "Linux WebView requires an Xlib parent window".to_string(),
            ));
        };
        let id = request(
            |reply| Command::Create {
                parent: handle.window as u64,
                width,
                height,
                bridge,
                html,
                ipc,
                reply,
            },
            "create",
        )?
        .map_err(WebViewError::Initialization)?;
        Ok(WebViewProxy { id })
    }

    /// Wait until the GTK thread has processed all previously submitted
    /// commands (a barrier; the thread itself never blocks on WebKit).
    pub(super) fn synchronize() {
        let _ = request(|reply| Command::Synchronize { reply }, "synchronize");
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
