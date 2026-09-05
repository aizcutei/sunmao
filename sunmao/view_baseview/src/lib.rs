//! Baseview Integration for SunMao Plugin Editors
//!
//! This crate provides cross-platform view implementations that bridge
//! `SunmaoView` to baseview for windowing. Three rendering backends are
//! supported:
//!
//! - **OpenGL** (`gl` feature): `BaseviewView<S: ViewState>` — uses `GlContext`
//! - **wgpu** (`wgpu` feature): `BaseviewWgpuView<S: WgpuViewState>` — uses `WgpuContext`
//! - **WebView** (`webview` feature): `BaseviewWebView<S: WebViewState>`
//!
//! All three implement `SunmaoView`, so plugin authors can use them
//! interchangeably with `sunmao_export_au_with_view!` for unified
//! AU/VST3/CLAP GUI export.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use baseview::{
    Event, EventStatus, MouseEvent, ScrollDelta, Window, WindowEvent, WindowHandler,
    WindowOpenOptions,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle};

use sunmao_core::{
    ParentWindow, StandaloneViewOptions, StandaloneViewResult, SunmaoView, ViewContext, ViewHandle,
};
use sunmao_gui::{
    Color, Event as GuiEvent, GuiContext, KeyCode as GuiKeyCode, Modifiers,
    MouseButton as GuiMouseButton,
};

mod pixel_probe;

#[used]
static _SUNMAO_DEBUG_READ_FRAME: unsafe extern "C" fn(*mut u32, *mut u32, *mut u32, usize) -> i32 =
    pixel_probe::sunmao_debug_read_frame;

#[used]
static _SUNMAO_DEBUG_PIXEL_PROBE_STATUS: extern "C" fn() -> i32 =
    pixel_probe::sunmao_debug_pixel_probe_status;

fn resize_baseview_window(handle: &mut baseview::WindowHandle, width: u32, height: u32) -> bool {
    handle.resize(baseview::Size::new(width as f64, height as f64));
    true
}

/// The editor window plus the logical size it was created at.
///
/// A host-driven DPI change is answered by resizing to `base × factor`, which
/// is what a non-DPI-aware editor must do on Windows and X11. macOS hosts are
/// not expected to call this at all — AppKit owns backing scale there — so the
/// path simply stays unused rather than being special-cased.
struct ScalableWindow {
    handle: baseview::WindowHandle,
    base_width: u32,
    base_height: u32,
    /// Keys the host forwarded through `IPlugView::onKeyDown`, waiting for the
    /// window's own thread to pick them up.
    ///
    /// A host calls `onKeyDown` on *its* thread, but the editor's widgets live
    /// on the window thread and baseview offers no way to inject an event into
    /// a live handler. Queueing and draining on the next frame is the crossing
    /// point. The lock is only ever taken on GUI threads — never the audio
    /// thread — so a mutex is the right tool here.
    keys: Arc<HostKeyQueue>,
}

/// Host-forwarded keys plus whether the editor consumed the last one.
#[derive(Default)]
pub(crate) struct HostKeyQueue {
    pending: std::sync::Mutex<std::collections::VecDeque<sunmao_core::ViewKey>>,
    /// Set when a drained key was consumed by a widget. The host asked
    /// synchronously whether the key was used, but the answer only exists a
    /// frame later, so `send_key` optimistically reports acceptance and this
    /// records the truth for the *next* query.
    consumed_last: AtomicBool,
}

impl HostKeyQueue {
    fn push(&self, key: sunmao_core::ViewKey) -> bool {
        let Ok(mut pending) = self.pending.lock() else {
            return false;
        };
        // Bound the queue: a host that forwards keys faster than the editor
        // paints must not grow it without limit.
        if pending.len() >= 64 {
            return false;
        }
        pending.push_back(key);
        true
    }

    fn drain(&self) -> Vec<sunmao_core::ViewKey> {
        match self.pending.lock() {
            Ok(mut pending) => pending.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn record_consumed(&self, consumed: bool) {
        self.consumed_last.store(consumed, Ordering::Release);
    }

    pub(crate) fn last_was_consumed(&self) -> bool {
        self.consumed_last.load(Ordering::Acquire)
    }
}

/// Translate a host key into the GUI event vocabulary.
///
/// The character, when the host supplied one, becomes a `TextInput` alongside
/// the key event — the same pairing the platform keyboard path uses, so an
/// editor handles host-forwarded and native keys identically.
pub(crate) fn host_key_events(key: sunmao_core::ViewKey) -> Vec<GuiEvent> {
    let modifiers = Modifiers::default();
    let code = host_key_code(key);
    let mut events = vec![if key.pressed {
        GuiEvent::KeyDown {
            key: code,
            modifiers,
        }
    } else {
        GuiEvent::KeyUp {
            key: code,
            modifiers,
        }
    }];
    if key.pressed {
        if let Some(character) = key.character {
            if !character.is_control() {
                events.push(GuiEvent::TextInput {
                    text: character.to_string(),
                });
            }
        }
    }
    events
}

/// Map the neutral host key onto the framework's key code.
fn host_key_code(key: sunmao_core::ViewKey) -> GuiKeyCode {
    use sunmao_core::ViewKeyCode;
    match key.code {
        ViewKeyCode::Backspace => GuiKeyCode::Backspace,
        ViewKeyCode::Tab => GuiKeyCode::Tab,
        ViewKeyCode::Enter => GuiKeyCode::Enter,
        ViewKeyCode::Escape => GuiKeyCode::Escape,
        ViewKeyCode::Space => GuiKeyCode::Space,
        ViewKeyCode::End => GuiKeyCode::End,
        ViewKeyCode::Home => GuiKeyCode::Home,
        ViewKeyCode::Left => GuiKeyCode::Left,
        ViewKeyCode::Up => GuiKeyCode::Up,
        ViewKeyCode::Right => GuiKeyCode::Right,
        ViewKeyCode::Down => GuiKeyCode::Down,
        ViewKeyCode::PageUp => GuiKeyCode::PageUp,
        ViewKeyCode::PageDown => GuiKeyCode::PageDown,
        ViewKeyCode::Unknown => GuiKeyCode::Unknown,
    }
}

fn resize_scalable_window(window: &mut ScalableWindow, width: u32, height: u32) -> bool {
    resize_baseview_window(&mut window.handle, width, height)
}

fn send_key_to_scalable_window(window: &mut ScalableWindow, key: sunmao_core::ViewKey) -> bool {
    // The editor answers a frame later, so report whether the key was queued
    // and let the previous frame's verdict inform the host.
    window.keys.push(key) && window.keys.last_was_consumed()
}

fn scale_scalable_window(window: &mut ScalableWindow, factor: f32) -> bool {
    // `ViewHandle::set_scale` already rejected non-finite and non-positive
    // factors; what is left is guarding the *product* against overflowing a
    // sane window size.
    let width = (window.base_width as f32 * factor).round();
    let height = (window.base_height as f32 * factor).round();
    let limit = u16::MAX as f32;
    if !(1.0..=limit).contains(&width) || !(1.0..=limit).contains(&height) {
        return false;
    }
    window
        .handle
        .resize(baseview::Size::new(width as f64, height as f64));
    true
}

struct StandaloneWindowHandler<H> {
    inner: H,
    close_after_frames: Option<u32>,
    rendered_frames: u32,
    smoke_completed: Arc<AtomicBool>,
}

const STANDALONE_GUI_SMOKE_TIMEOUT: Duration = Duration::from_secs(15);
const STANDALONE_GUI_STOP_RETRY: Duration = Duration::from_millis(25);

struct StandaloneSmokeWatchdog {
    cancel: mpsc::Sender<()>,
    timed_out: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StandaloneSmokeWatchdog {
    fn start(timeout: Duration) -> std::io::Result<Self> {
        Self::start_with_stop(timeout, baseview::request_event_loop_stop)
    }

    fn start_with_stop<F>(timeout: Duration, request_stop: F) -> std::io::Result<Self>
    where
        F: Fn() + Send + 'static,
    {
        let (cancel, cancelled) = mpsc::channel();
        let timed_out = Arc::new(AtomicBool::new(false));
        let timed_out_in_thread = Arc::clone(&timed_out);
        let thread = std::thread::Builder::new()
            .name("sunmao-standalone-gui-watchdog".into())
            .spawn(move || {
                match cancelled.recv_timeout(timeout) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                timed_out_in_thread.store(true, Ordering::Release);
                loop {
                    request_stop();
                    match cancelled.recv_timeout(STANDALONE_GUI_STOP_RETRY) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            })?;
        Ok(Self {
            cancel,
            timed_out,
            thread: Some(thread),
        })
    }

    fn finish(mut self) -> bool {
        let _ = self.cancel.send(());
        let thread_panicked = self
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err());
        thread_panicked || self.timed_out.load(Ordering::Acquire)
    }
}

impl Drop for StandaloneSmokeWatchdog {
    fn drop(&mut self) {
        let _ = self.cancel.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl<H: WindowHandler> WindowHandler for StandaloneWindowHandler<H> {
    fn on_frame(&mut self, window: &mut Window) {
        self.inner.on_frame(window);
        let Some(close_after_frames) = self.close_after_frames else {
            return;
        };
        self.rendered_frames = self.rendered_frames.saturating_add(1);
        if self.rendered_frames >= close_after_frames.max(1) {
            self.smoke_completed.store(true, Ordering::Release);
            window.close();
        }
    }

    fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
        self.inner.on_event(window, event)
    }
}

fn open_standalone_window<H, B>(
    options: WindowOpenOptions,
    view_options: StandaloneViewOptions,
    initialized: Arc<AtomicBool>,
    build: B,
) -> StandaloneViewResult
where
    H: WindowHandler + 'static,
    B: FnOnce(&mut Window) -> H + Send + 'static,
{
    let close_after_frames = view_options.close_after_frames();
    let smoke_completed = Arc::new(AtomicBool::new(false));
    let smoke_completed_in_handler = Arc::clone(&smoke_completed);
    let watchdog = match close_after_frames {
        Some(_) => match StandaloneSmokeWatchdog::start(STANDALONE_GUI_SMOKE_TIMEOUT) {
            Ok(watchdog) => Some(watchdog),
            Err(error) => {
                eprintln!("Failed to start standalone GUI smoke watchdog: {error}");
                return StandaloneViewResult::Failed;
            }
        },
        None => None,
    };
    let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Window::open_blocking(options, move |window| StandaloneWindowHandler {
            inner: build(window),
            close_after_frames,
            rendered_frames: 0,
            smoke_completed: smoke_completed_in_handler,
        });
    }))
    .is_ok();
    let timed_out = watchdog.is_some_and(StandaloneSmokeWatchdog::finish);

    let initialized = initialized.load(Ordering::Acquire);
    let smoke_completed = smoke_completed.load(Ordering::Acquire);
    if opened && !timed_out && initialized && (close_after_frames.is_none() || smoke_completed) {
        StandaloneViewResult::Closed
    } else {
        eprintln!(
            "Standalone GUI lifecycle failed: opened={opened} timed_out={timed_out} \
             initialized={initialized} rendered={smoke_completed}"
        );
        StandaloneViewResult::Failed
    }
}

// ============ Shared Types ============

/// Wrapper to make ParentWindow implement HasWindowHandle
struct ParentWindowWrapper(ParentWindow);

impl HasWindowHandle for ParentWindowWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        use std::ptr::NonNull;

        let raw = match self.0 {
            ParentWindow::AppKit(ptr) => {
                let handle = raw_window_handle::AppKitWindowHandle::new(
                    NonNull::new(ptr).expect("null NSView"),
                );
                RawWindowHandle::AppKit(handle)
            }
            ParentWindow::Win32(ptr) => {
                use std::num::NonZeroIsize;
                let isize_ptr = ptr as isize;
                let handle = raw_window_handle::Win32WindowHandle::new(
                    NonZeroIsize::new(isize_ptr).expect("null HWND"),
                );
                RawWindowHandle::Win32(handle)
            }
            ParentWindow::X11(window) => {
                use std::num::NonZeroU32;
                let handle = raw_window_handle::XcbWindowHandle::new(
                    NonZeroU32::new(window).expect("null X11 window"),
                );
                RawWindowHandle::Xcb(handle)
            }
        };

        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// Configuration for a baseview-backed editor.
#[derive(Clone)]
pub struct BaseviewConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub scale_policy: WindowScalePolicy,
    pub background: Color,
}

impl Default for BaseviewConfig {
    fn default() -> Self {
        Self {
            title: "SunMao Editor".to_string(),
            width: 400,
            height: 300,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgba(0.12, 0.12, 0.18, 1.0),
        }
    }
}

/// Type alias for the view builder function
pub type ViewBuilder<S> = Arc<dyn Fn(Arc<dyn ViewContext>) -> S + Send + Sync>;

// ============ Shared event conversion helpers ============

fn convert_mouse_event(event: &MouseEvent, mouse_x: f32, mouse_y: f32) -> Option<GuiEvent> {
    match event {
        MouseEvent::CursorMoved {
            position,
            modifiers,
        } => Some(GuiEvent::MouseMove {
            x: position.x as f32,
            y: position.y as f32,
            modifiers: convert_modifiers(modifiers),
        }),
        MouseEvent::ButtonPressed { button, modifiers } => Some(GuiEvent::MouseDown {
            x: mouse_x,
            y: mouse_y,
            button: convert_button(*button),
            modifiers: convert_modifiers(modifiers),
        }),
        MouseEvent::ButtonReleased { button, modifiers } => Some(GuiEvent::MouseUp {
            x: mouse_x,
            y: mouse_y,
            button: convert_button(*button),
            modifiers: convert_modifiers(modifiers),
        }),
        MouseEvent::WheelScrolled { delta, modifiers } => {
            let (dx, dy) = match delta {
                ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                ScrollDelta::Pixels { x, y } => (*x, *y),
            };
            Some(GuiEvent::Scroll {
                x: mouse_x,
                y: mouse_y,
                delta_x: dx,
                delta_y: dy,
                modifiers: convert_modifiers(modifiers),
            })
        }
        _ => None,
    }
}

fn convert_button(button: baseview::MouseButton) -> GuiMouseButton {
    match button {
        baseview::MouseButton::Left => GuiMouseButton::Left,
        baseview::MouseButton::Middle => GuiMouseButton::Middle,
        baseview::MouseButton::Right => GuiMouseButton::Right,
        _ => GuiMouseButton::Left,
    }
}

fn convert_modifiers(mods: &keyboard_types::Modifiers) -> Modifiers {
    Modifiers {
        shift: mods.contains(keyboard_types::Modifiers::SHIFT),
        ctrl: mods.contains(keyboard_types::Modifiers::CONTROL),
        alt: mods.contains(keyboard_types::Modifiers::ALT),
        meta: mods.contains(keyboard_types::Modifiers::META),
    }
}

fn convert_key_code(code: keyboard_types::Code) -> GuiKeyCode {
    use keyboard_types::Code;

    match code {
        Code::KeyA => GuiKeyCode::A,
        Code::KeyB => GuiKeyCode::B,
        Code::KeyC => GuiKeyCode::C,
        Code::KeyD => GuiKeyCode::D,
        Code::KeyE => GuiKeyCode::E,
        Code::KeyF => GuiKeyCode::F,
        Code::KeyG => GuiKeyCode::G,
        Code::KeyH => GuiKeyCode::H,
        Code::KeyI => GuiKeyCode::I,
        Code::KeyJ => GuiKeyCode::J,
        Code::KeyK => GuiKeyCode::K,
        Code::KeyL => GuiKeyCode::L,
        Code::KeyM => GuiKeyCode::M,
        Code::KeyN => GuiKeyCode::N,
        Code::KeyO => GuiKeyCode::O,
        Code::KeyP => GuiKeyCode::P,
        Code::KeyQ => GuiKeyCode::Q,
        Code::KeyR => GuiKeyCode::R,
        Code::KeyS => GuiKeyCode::S,
        Code::KeyT => GuiKeyCode::T,
        Code::KeyU => GuiKeyCode::U,
        Code::KeyV => GuiKeyCode::V,
        Code::KeyW => GuiKeyCode::W,
        Code::KeyX => GuiKeyCode::X,
        Code::KeyY => GuiKeyCode::Y,
        Code::KeyZ => GuiKeyCode::Z,
        Code::Digit0 | Code::Numpad0 => GuiKeyCode::Num0,
        Code::Digit1 | Code::Numpad1 => GuiKeyCode::Num1,
        Code::Digit2 | Code::Numpad2 => GuiKeyCode::Num2,
        Code::Digit3 | Code::Numpad3 => GuiKeyCode::Num3,
        Code::Digit4 | Code::Numpad4 => GuiKeyCode::Num4,
        Code::Digit5 | Code::Numpad5 => GuiKeyCode::Num5,
        Code::Digit6 | Code::Numpad6 => GuiKeyCode::Num6,
        Code::Digit7 | Code::Numpad7 => GuiKeyCode::Num7,
        Code::Digit8 | Code::Numpad8 => GuiKeyCode::Num8,
        Code::Digit9 | Code::Numpad9 => GuiKeyCode::Num9,
        Code::F1 => GuiKeyCode::F1,
        Code::F2 => GuiKeyCode::F2,
        Code::F3 => GuiKeyCode::F3,
        Code::F4 => GuiKeyCode::F4,
        Code::F5 => GuiKeyCode::F5,
        Code::F6 => GuiKeyCode::F6,
        Code::F7 => GuiKeyCode::F7,
        Code::F8 => GuiKeyCode::F8,
        Code::F9 => GuiKeyCode::F9,
        Code::F10 => GuiKeyCode::F10,
        Code::F11 => GuiKeyCode::F11,
        Code::F12 => GuiKeyCode::F12,
        Code::Escape => GuiKeyCode::Escape,
        Code::Tab => GuiKeyCode::Tab,
        Code::Space => GuiKeyCode::Space,
        Code::Enter | Code::NumpadEnter => GuiKeyCode::Enter,
        Code::Backspace | Code::NumpadBackspace => GuiKeyCode::Backspace,
        Code::Delete => GuiKeyCode::Delete,
        Code::ArrowLeft => GuiKeyCode::Left,
        Code::ArrowRight => GuiKeyCode::Right,
        Code::ArrowUp => GuiKeyCode::Up,
        Code::ArrowDown => GuiKeyCode::Down,
        Code::Home => GuiKeyCode::Home,
        Code::End => GuiKeyCode::End,
        Code::PageUp => GuiKeyCode::PageUp,
        Code::PageDown => GuiKeyCode::PageDown,
        Code::ShiftLeft | Code::ShiftRight => GuiKeyCode::Shift,
        Code::ControlLeft | Code::ControlRight => GuiKeyCode::Control,
        Code::AltLeft | Code::AltRight => GuiKeyCode::Alt,
        Code::MetaLeft | Code::MetaRight => GuiKeyCode::Meta,
        _ => GuiKeyCode::Unknown,
    }
}

/// Text produced by a key press, if any.
///
/// This is the **international and IME path**. `Code` is a physical key
/// position — `KeyA` is the same key whether the layout is QWERTY, AZERTY or
/// Dvorak — so a French `é`, a German `ü` or a committed CJK phrase can only
/// arrive through the *logical* `Key::Character`, which the platform has
/// already run through the keyboard layout and any input method.
///
/// Composition-in-progress events are skipped: `is_composing` marks the
/// preedit that an IME is still editing, and inserting it would type every
/// intermediate candidate. The platform sends the committed text as a separate,
/// non-composing event.
fn text_input_from_key(event: &keyboard_types::KeyboardEvent) -> Option<GuiEvent> {
    if event.state != keyboard_types::KeyState::Down || event.is_composing {
        return None;
    }
    let keyboard_types::Key::Character(text) = &event.key else {
        return None;
    };
    // Control characters reach us as Character on some platforms; they are
    // commands, not text.
    if text.is_empty() || text.chars().any(|ch| ch.is_control()) {
        return None;
    }
    Some(GuiEvent::TextInput { text: text.clone() })
}

fn dispatch_keyboard_event(
    event: &keyboard_types::KeyboardEvent,
    mut handler: impl FnMut(&GuiEvent) -> bool,
) -> EventStatus {
    let key = convert_key_code(event.code);
    let modifiers = convert_modifiers(&event.modifiers);
    let gui_event = match event.state {
        keyboard_types::KeyState::Down => GuiEvent::KeyDown { key, modifiers },
        keyboard_types::KeyState::Up => GuiEvent::KeyUp { key, modifiers },
    };

    let mut consumed = handler(&gui_event);
    // The key event goes out first so a control can act on Enter or an arrow,
    // then the text it produced. A widget that consumed the key still sees the
    // text: a text field wants both, and a knob ignores the text anyway.
    if let Some(text_event) = text_input_from_key(event) {
        consumed |= handler(&text_event);
    }

    if consumed {
        EventStatus::Captured
    } else {
        EventStatus::Ignored
    }
}

// ============ OpenGL Backend ============

#[cfg(feature = "gl")]
mod gl_backend {
    #[cfg(all(feature = "wgpu", target_os = "windows"))]
    use super::wgpu_backend::{WgpuHandler, WgpuViewState};
    use super::*;
    use baseview::gl::GlConfig;
    use sunmao_gui::gl::GlContext;
    #[cfg(all(feature = "wgpu", target_os = "windows"))]
    use sunmao_gui::wgpu::WgpuContext;

    /// Trait for custom view state that receives events and draws the UI.
    ///
    /// The normal renderer is OpenGL. When a hosted Windows driver exposes
    /// only legacy WGL, the same state may be rendered by the optional WGPU
    /// compatibility path without changing plugin code.
    pub trait ViewState: Send + 'static {
        fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32);
        fn on_mouse_event(&mut self, event: &GuiEvent) -> bool;
        fn on_keyboard_event(&mut self, _event: &GuiEvent) -> bool {
            false
        }
        fn on_resize(&mut self, _width: f32, _height: f32) {}

        /// Describe this editor to assistive technology.
        ///
        /// Build it with `sunmao_gui::accessibility_tree` and translate with
        /// `accesskit_update`. `None` — the default — means the editor does not
        /// describe itself, and the platform falls back to announcing a bare
        /// window.
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            None
        }
    }

    /// Wraps a [`ViewState`] so keys the host forwarded through
    /// `IPlugView::onKeyDown` reach it on the window's own thread.
    ///
    /// The host calls on its thread; widgets live here. Draining at the top of
    /// `draw` means an edit is visible in the very frame that follows the key.
    struct HostKeyedState<S> {
        inner: S,
        keys: Arc<super::HostKeyQueue>,
    }

    impl<S: ViewState> ViewState for HostKeyedState<S> {
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            self.inner.accessibility_tree()
        }

        fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32) {
            for key in self.keys.drain() {
                let mut consumed = false;
                for event in super::host_key_events(key) {
                    consumed |= self.inner.on_keyboard_event(&event);
                }
                self.keys.record_consumed(consumed);
            }
            self.inner.draw(ctx, width, height);
        }

        fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
            self.inner.on_mouse_event(event)
        }

        fn on_keyboard_event(&mut self, event: &GuiEvent) -> bool {
            self.inner.on_keyboard_event(event)
        }

        fn on_resize(&mut self, width: f32, height: f32) {
            self.inner.on_resize(width, height)
        }
    }

    /// A SunmaoView implementation using baseview + OpenGL.
    pub struct BaseviewView<S: ViewState + 'static> {
        config: BaseviewConfig,
        builder: ViewBuilder<S>,
    }

    impl<S: ViewState + 'static> BaseviewView<S> {
        pub fn new<B>(config: BaseviewConfig, builder: B) -> Self
        where
            B: Fn(Arc<dyn ViewContext>) -> S + Send + Sync + 'static,
        {
            Self {
                config,
                builder: Arc::new(builder),
            }
        }

        /// Build the editor and hand it to `open_window`, which decides whether
        /// the window is embedded in a parent or floating.
        ///
        /// Everything else — GL config, the WGPU fallback, the initialization
        /// check, the `ViewHandle` wiring — is identical for both, so it lives
        /// here once rather than being copied per mode.
        fn open_with<F>(&self, context: Arc<dyn ViewContext>, open_window: F) -> Option<ViewHandle>
        where
            F: FnOnce(
                WindowOpenOptions,
                Box<dyn FnOnce(&mut baseview::Window) -> BaseviewHandler<HostKeyedState<S>> + Send>,
            ) -> baseview::WindowHandle,
        {
            let config = self.config.clone();
            let keys = Arc::new(super::HostKeyQueue::default());
            let state = HostKeyedState {
                inner: (self.builder)(context.clone()),
                keys: Arc::clone(&keys),
            };

            let mut options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );
            let mut gl_config = GlConfig::default();
            // Hosted Xvfb/Mesa and the Windows basic driver may expose no
            // sRGB framebuffer even though ordinary RGBA GL is available.
            gl_config.srgb = false;
            options.gl_config = Some(gl_config);

            let background = config.background;
            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);

            let mut handle = open_window(
                options,
                Box::new(move |window: &mut baseview::Window| {
                    match GlHandler::try_new(state, background, window) {
                        Ok(handler) => {
                            initialized_in_builder.store(true, Ordering::Release);
                            BaseviewHandler::Gl(handler)
                        }
                        Err(state) => {
                            #[cfg(all(feature = "wgpu", target_os = "windows"))]
                            {
                                if let Ok(handler) = pollster::block_on(WgpuHandler::new(
                                    WgpuFallbackState(state),
                                    background,
                                    config.width,
                                    config.height,
                                    window,
                                )) {
                                    initialized_in_builder.store(true, Ordering::Release);
                                    return BaseviewHandler::Wgpu(handler);
                                }
                            }
                            #[cfg(not(all(feature = "wgpu", target_os = "windows")))]
                            let _ = state;
                            window.close();
                            BaseviewHandler::Failed
                        }
                    }
                }),
            );

            if initialized.load(Ordering::Acquire) && handle.is_open() {
                Some(
                    ViewHandle::builder(ScalableWindow {
                        handle,
                        base_width: config.width,
                        base_height: config.height,
                        keys,
                    })
                    .resizable(resize_scalable_window)
                    .scalable(scale_scalable_window)
                    .keyboard(send_key_to_scalable_window)
                    .build(),
                )
            } else {
                handle.close();
                None
            }
        }
    }

    impl<S: ViewState + 'static> SunmaoView for BaseviewView<S> {
        fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        fn can_resize(&self) -> bool {
            true
        }

        fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            let parent_wrapper = ParentWindowWrapper(parent);
            self.open_with(context, |options, build| {
                Window::open_parented(&parent_wrapper, options, build)
            })
        }

        fn supports_floating(&self) -> bool {
            true
        }

        /// Floating windows go through the same construction as embedded ones,
        /// so a bug fixed in one is fixed in both — the only difference is
        /// which baseview entry point creates the window.
        fn open_floating(&self, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            self.open_with(context, Window::open_floating)
        }

        fn open_standalone(
            &self,
            context: Arc<dyn ViewContext>,
            view_options: StandaloneViewOptions,
        ) -> StandaloneViewResult {
            let config = self.config.clone();
            let state = (self.builder)(context);
            let mut options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );
            let mut gl_config = GlConfig::default();
            gl_config.srgb = false;
            options.gl_config = Some(gl_config);

            let background = config.background;
            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            open_standalone_window(options, view_options, initialized, move |window| {
                match GlHandler::try_new(state, background, window) {
                    Ok(handler) => {
                        initialized_in_builder.store(true, Ordering::Release);
                        BaseviewHandler::Gl(handler)
                    }
                    Err(state) => {
                        #[cfg(all(feature = "wgpu", target_os = "windows"))]
                        {
                            if let Ok(handler) = pollster::block_on(WgpuHandler::new(
                                WgpuFallbackState(state),
                                background,
                                config.width,
                                config.height,
                                window,
                            )) {
                                initialized_in_builder.store(true, Ordering::Release);
                                return BaseviewHandler::Wgpu(handler);
                            }
                        }
                        #[cfg(not(all(feature = "wgpu", target_os = "windows")))]
                        let _ = state;
                        window.close();
                        BaseviewHandler::Failed
                    }
                }
            })
        }
    }

    enum BaseviewHandler<S: ViewState> {
        Gl(GlHandler<S>),
        #[cfg(all(feature = "wgpu", target_os = "windows"))]
        Wgpu(WgpuHandler<WgpuFallbackState<S>>),
        Failed,
    }

    impl<S: ViewState> WindowHandler for BaseviewHandler<S> {
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            match self {
                Self::Gl(handler) => handler.accessibility_tree(),
                #[cfg(all(feature = "wgpu", target_os = "windows"))]
                Self::Wgpu(handler) => handler.accessibility_tree(),
                Self::Failed => None,
            }
        }

        fn on_frame(&mut self, window: &mut Window) {
            match self {
                Self::Gl(handler) => handler.on_frame(window),
                #[cfg(all(feature = "wgpu", target_os = "windows"))]
                Self::Wgpu(handler) => handler.on_frame(window),
                Self::Failed => window.close(),
            }
        }

        fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
            match self {
                Self::Gl(handler) => handler.on_event(window, event),
                #[cfg(all(feature = "wgpu", target_os = "windows"))]
                Self::Wgpu(handler) => handler.on_event(window, event),
                Self::Failed => EventStatus::Ignored,
            }
        }
    }

    #[cfg(all(feature = "wgpu", target_os = "windows"))]
    struct WgpuFallbackState<S: ViewState>(S);

    #[cfg(all(feature = "wgpu", target_os = "windows"))]
    impl<S: ViewState> WgpuViewState for WgpuFallbackState<S> {
        fn draw(&mut self, ctx: &mut WgpuContext, width: f32, height: f32) {
            self.0.draw(ctx, width, height);
        }

        fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
            self.0.on_mouse_event(event)
        }

        fn on_keyboard_event(&mut self, event: &GuiEvent) -> bool {
            self.0.on_keyboard_event(event)
        }

        fn on_resize(&mut self, width: f32, height: f32) {
            self.0.on_resize(width, height);
        }

        /// Forwarding this is not optional on Windows: when GL initialization
        /// fails there the editor runs through this fallback, and a missing
        /// override here silently costs the whole window its accessibility.
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            self.0.accessibility_tree()
        }
    }

    struct GlHandler<S: ViewState> {
        state: S,
        gl: Option<GlContext>,
        background: Color,
        width: f32,
        height: f32,
        scale_factor: f32,
        mouse_x: f32,
        mouse_y: f32,
    }

    impl<S: ViewState> GlHandler<S> {
        fn try_new(state: S, background: Color, window: &mut Window) -> Result<Self, S> {
            let Some(gl_ctx) = window.gl_context() else {
                return Err(state);
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                gl_ctx
                    .make_current()
                    .map_err(|error| format!("could not make OpenGL context current: {error}"))?;
                if ["glCreateShader", "glCreateProgram"]
                    .iter()
                    .any(|symbol| gl_ctx.get_proc_address(symbol).is_null())
                {
                    let _ = gl_ctx.make_not_current();
                    return Err("required modern OpenGL entry points are unavailable".into());
                }
                GlContext::from_loader(|s| gl_ctx.get_proc_address(s), 400.0, 300.0)
            }));
            let gl = match result {
                Ok(Ok(ctx)) => ctx,
                Ok(Err(error)) => {
                    eprintln!("Failed to create GL renderer: {error}");
                    let _ = unsafe { gl_ctx.make_not_current() };
                    return Err(state);
                }
                Err(_) => {
                    eprintln!("GL renderer initialization panicked; trying compatibility renderer");
                    let _ = unsafe { gl_ctx.make_not_current() };
                    return Err(state);
                }
            };

            crate::pixel_probe::begin_renderer_session();

            Ok(Self {
                state,
                gl: Some(gl),
                background,
                width: 400.0,
                height: 300.0,
                scale_factor: 1.0,
                mouse_x: 0.0,
                mouse_y: 0.0,
            })
        }
    }

    impl<S: ViewState> WindowHandler for GlHandler<S> {
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            self.state.accessibility_tree()
        }

        fn on_frame(&mut self, window: &mut Window) {
            if let Some(gl_ctx) = window.gl_context() {
                if let Err(error) = unsafe { gl_ctx.make_current() } {
                    eprintln!("OpenGL context activation failed: {error}");
                    window.close();
                    return;
                }

                if let Some(ref mut gl) = self.gl {
                    let physical_width = (self.width * self.scale_factor) as u32;
                    let physical_height = (self.height * self.scale_factor) as u32;

                    gl.set_viewport(physical_width, physical_height);
                    gl.set_scale(self.scale_factor);
                    gl.clear(self.background);
                    gl.begin_frame();

                    self.state.draw(gl, self.width, self.height);

                    gl.end_frame();
                    if crate::pixel_probe::enabled() {
                        if let Ok((width, height, bytes)) = gl.read_rgba_bytes() {
                            crate::pixel_probe::store_sampled_rgba(width, height, &bytes, 4, false);
                        }
                    }
                }

                if let Err(error) = gl_ctx.swap_buffers() {
                    eprintln!("OpenGL buffer swap failed: {error}");
                    window.close();
                }
            }
        }

        fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
            match event {
                Event::Mouse(ref mouse_event) => {
                    if let MouseEvent::CursorMoved { position, .. } = mouse_event {
                        self.mouse_x = position.x as f32;
                        self.mouse_y = position.y as f32;
                    }

                    if let Some(gui_event) =
                        convert_mouse_event(mouse_event, self.mouse_x, self.mouse_y)
                    {
                        if self.state.on_mouse_event(&gui_event) {
                            return EventStatus::Captured;
                        }
                    }
                }
                Event::Keyboard(ref keyboard_event) => {
                    return dispatch_keyboard_event(keyboard_event, |gui_event| {
                        self.state.on_keyboard_event(gui_event)
                    });
                }
                Event::Window(WindowEvent::Resized(info)) => {
                    self.width = info.logical_size().width as f32;
                    self.height = info.logical_size().height as f32;
                    self.scale_factor = info.scale() as f32;
                    self.state.on_resize(self.width, self.height);
                }
                _ => {}
            }

            EventStatus::Ignored
        }
    }
}

#[cfg(feature = "gl")]
pub use gl_backend::{BaseviewView, ViewState};

// ============ wgpu Backend ============

#[cfg(feature = "wgpu")]
pub(crate) mod wgpu_backend {
    use super::*;
    use sunmao_gui::wgpu::WgpuContext;

    fn platform_default_backends() -> wgpu::Backends {
        #[cfg(target_os = "macos")]
        {
            wgpu::Backends::METAL
        }
        #[cfg(target_os = "windows")]
        {
            wgpu::Backends::DX12 | wgpu::Backends::VULKAN | wgpu::Backends::GL
        }
        #[cfg(target_os = "linux")]
        {
            wgpu::Backends::VULKAN | wgpu::Backends::GL
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            wgpu::Backends::PRIMARY
        }
    }

    fn selected_backends() -> Result<wgpu::Backends, String> {
        match wgpu::util::backend_bits_from_env() {
            Some(backends) if backends.is_empty() => {
                Err("WGPU_BACKEND does not name a supported backend".into())
            }
            Some(backends) => Ok(backends),
            None => Ok(platform_default_backends()),
        }
    }

    /// Trait for custom view state that draws using wgpu.
    pub trait WgpuViewState: Send + 'static {
        fn draw(&mut self, ctx: &mut WgpuContext, width: f32, height: f32);
        fn on_mouse_event(&mut self, event: &GuiEvent) -> bool;
        fn on_keyboard_event(&mut self, _event: &GuiEvent) -> bool {
            false
        }
        fn on_resize(&mut self, _width: f32, _height: f32) {}

        /// Describe this editor to assistive technology. See
        /// [`ViewState::accessibility_tree`].
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            None
        }
    }

    /// A SunmaoView implementation using baseview + wgpu.
    pub struct BaseviewWgpuView<S: WgpuViewState + 'static> {
        config: BaseviewConfig,
        builder: ViewBuilder<S>,
    }

    impl<S: WgpuViewState + 'static> BaseviewWgpuView<S> {
        pub fn new<B>(config: BaseviewConfig, builder: B) -> Self
        where
            B: Fn(Arc<dyn ViewContext>) -> S + Send + Sync + 'static,
        {
            Self {
                config,
                builder: Arc::new(builder),
            }
        }
    }

    impl<S: WgpuViewState + 'static> SunmaoView for BaseviewWgpuView<S> {
        fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        fn can_resize(&self) -> bool {
            true
        }

        fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            let config = self.config.clone();
            let state = (self.builder)(context.clone());

            let options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );

            let parent_wrapper = ParentWindowWrapper(parent);
            let background = config.background;
            let width = config.width;
            let height = config.height;

            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            let mut handle = Window::open_parented(&parent_wrapper, options, move |window| {
                match pollster::block_on(WgpuHandler::new(state, background, width, height, window))
                {
                    Ok(handler) => {
                        initialized_in_builder.store(true, Ordering::Release);
                        WgpuHandlerState::Ready(handler)
                    }
                    Err(error) => {
                        eprintln!("Failed to initialize embedded WGPU view: {error}");
                        window.close();
                        WgpuHandlerState::Failed
                    }
                }
            });

            if initialized.load(Ordering::Acquire) && handle.is_open() {
                // No `.keyboard(..)`: only the GL backend drains the host key
                // queue today, so the wgpu and WebView editors report that they
                // did not take the key and the host keeps its own shortcut.
                // That is the honest answer rather than swallowing it.
                Some(ViewHandle::scalable(
                    ScalableWindow {
                        handle,
                        base_width: config.width,
                        base_height: config.height,
                        keys: Arc::new(super::HostKeyQueue::default()),
                    },
                    Some(resize_scalable_window),
                    scale_scalable_window,
                ))
            } else {
                handle.close();
                None
            }
        }

        fn open_standalone(
            &self,
            context: Arc<dyn ViewContext>,
            view_options: StandaloneViewOptions,
        ) -> StandaloneViewResult {
            let config = self.config.clone();
            let state = (self.builder)(context);
            let options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );
            let background = config.background;
            let width = config.width;
            let height = config.height;
            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            open_standalone_window(options, view_options, initialized, move |window| {
                match pollster::block_on(WgpuHandler::new(state, background, width, height, window))
                {
                    Ok(handler) => {
                        initialized_in_builder.store(true, Ordering::Release);
                        WgpuHandlerState::Ready(handler)
                    }
                    Err(error) => {
                        eprintln!("Failed to initialize standalone WGPU view: {error}");
                        window.close();
                        WgpuHandlerState::Failed
                    }
                }
            })
        }
    }

    enum WgpuHandlerState<S: WgpuViewState> {
        Ready(WgpuHandler<S>),
        Failed,
    }

    pub(super) struct WgpuHandler<S: WgpuViewState> {
        state: S,
        wgpu_ctx: WgpuContext,
        surface: wgpu::Surface<'static>,
        surface_config: wgpu::SurfaceConfiguration,
        background: Color,
        width: f32,
        height: f32,
        scale_factor: f32,
        mouse_x: f32,
        mouse_y: f32,
    }

    impl<S: WgpuViewState> WgpuHandler<S> {
        pub(super) async fn new(
            state: S,
            background: Color,
            width: u32,
            height: u32,
            window: &mut Window<'_>,
        ) -> Result<Self, String> {
            let backends = selected_backends()?;
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            });
            // The baseview child window outlives the handler and its surface. Using
            // raw handles lets wgpu select Metal, DX12/Vulkan, or Vulkan/GL without
            // coupling this backend to a platform-specific layer type.
            let target = unsafe { wgpu::SurfaceTargetUnsafe::from_window(window) }
                .map_err(|error| format!("window handle unavailable: {error}"))?;
            let surface = unsafe { instance.create_surface_unsafe(target) }
                .map_err(|error| format!("surface creation failed: {error}"))?;

            let adapter_options = wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            };
            let adapter = match instance.request_adapter(&adapter_options).await {
                Some(adapter) => adapter,
                None => {
                    // Hosted/headless Windows runners may expose only the
                    // software WARP adapter. Keep the same surface constraint
                    // while allowing that adapter as a deterministic fallback.
                    instance
                        .request_adapter(&wgpu::RequestAdapterOptions {
                            force_fallback_adapter: true,
                            ..adapter_options
                        })
                        .await
                        .ok_or_else(|| {
                            format!(
                                "no adapter supports the window surface for backends {backends:?}"
                            )
                        })?
                }
            };

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .map_err(|error| format!("device request failed: {error}"))?;

            let width = width.max(1);
            let height = height.max(1);

            let surface_caps = surface.get_capabilities(&adapter);
            let surface_format = surface_caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .or_else(|| surface_caps.formats.first().copied())
                .ok_or_else(|| "surface reports no texture formats".to_string())?;
            let alpha_mode = surface_caps
                .alpha_modes
                .first()
                .copied()
                .ok_or_else(|| "surface reports no alpha modes".to_string())?;
            if !surface_caps
                .usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                return Err("surface does not support render attachments".into());
            }

            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &surface_config);

            let wgpu_ctx =
                WgpuContext::new(device, queue, surface_format, width as f32, height as f32);

            crate::pixel_probe::begin_renderer_session();

            Ok(Self {
                state,
                wgpu_ctx,
                surface,
                surface_config,
                background,
                width: width as f32,
                height: height as f32,
                scale_factor: 1.0,
                mouse_x: 0.0,
                mouse_y: 0.0,
            })
        }

        fn configure_surface(&mut self) {
            let pw = (self.width * self.scale_factor).max(1.0) as u32;
            let ph = (self.height * self.scale_factor).max(1.0) as u32;
            if pw > 0 && ph > 0 {
                self.surface_config.width = pw;
                self.surface_config.height = ph;
                self.surface
                    .configure(self.wgpu_ctx.device(), &self.surface_config);
                // Drawing coordinates are logical pixels. The surface alone uses
                // physical pixels, otherwise high-DPI views render at half size.
                self.wgpu_ctx.resize(self.width, self.height);
                self.wgpu_ctx.set_scale(self.scale_factor);
            }
        }

        fn capture_pixel_probe(&mut self) {
            if !crate::pixel_probe::enabled() {
                return;
            }
            let width = 128_u32;
            let height = 128_u32;
            let format = self.surface_config.format;
            let texture = self
                .wgpu_ctx
                .device()
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("sunmao-gui-pixel-probe"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
            let probe_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.wgpu_ctx
                .end_frame_with_clear(&probe_view, self.background);

            let unpadded_bytes_per_row = width * 4;
            let bytes_per_row =
                unpadded_bytes_per_row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer = self
                .wgpu_ctx
                .device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sunmao-gui-pixel-probe-buffer"),
                    size: u64::from(bytes_per_row) * u64::from(height),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
            let mut encoder =
                self.wgpu_ctx
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("sunmao-gui-pixel-probe-copy"),
                    });
            encoder.copy_texture_to_buffer(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyBuffer {
                    buffer: &buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            self.wgpu_ctx.queue().submit(Some(encoder.finish()));
            let slice = buffer.slice(..);
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
            self.wgpu_ctx.device().poll(wgpu::Maintain::Wait);
            if receiver.recv().ok().and_then(Result::ok).is_none() {
                return;
            }
            let data = slice.get_mapped_range();
            let bgra = matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            let mut packed = Vec::with_capacity((width * height) as usize);
            for row in 0..height as usize {
                let start = row * bytes_per_row as usize;
                let row_bytes = &data[start..start + unpadded_bytes_per_row as usize];
                for pixel in row_bytes.chunks_exact(4) {
                    packed.push(if bgra {
                        u32::from_ne_bytes([pixel[2], pixel[1], pixel[0], 0])
                    } else {
                        u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], 0])
                    });
                }
            }
            drop(data);
            buffer.unmap();
            crate::pixel_probe::store_sampled(width, height, |x, y| {
                packed[(y * width + x) as usize]
            });
        }
    }

    impl<S: WgpuViewState> WindowHandler for WgpuHandler<S> {
        #[cfg(feature = "accessibility")]
        fn accessibility_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            self.state.accessibility_tree()
        }

        fn on_frame(&mut self, _window: &mut Window) {
            let output = match self.surface.get_current_texture() {
                Ok(t) => t,
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    self.configure_surface();
                    return;
                }
                Err(_) => return,
            };

            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            self.wgpu_ctx.begin_frame();
            self.state.draw(&mut self.wgpu_ctx, self.width, self.height);
            self.wgpu_ctx.end_frame_with_clear(&view, self.background);
            self.capture_pixel_probe();

            output.present();
        }

        fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
            match event {
                Event::Mouse(ref mouse_event) => {
                    if let MouseEvent::CursorMoved { position, .. } = mouse_event {
                        self.mouse_x = position.x as f32;
                        self.mouse_y = position.y as f32;
                    }

                    if let Some(gui_event) =
                        convert_mouse_event(mouse_event, self.mouse_x, self.mouse_y)
                    {
                        if self.state.on_mouse_event(&gui_event) {
                            return EventStatus::Captured;
                        }
                    }
                }
                Event::Keyboard(ref keyboard_event) => {
                    return dispatch_keyboard_event(keyboard_event, |gui_event| {
                        self.state.on_keyboard_event(gui_event)
                    });
                }
                Event::Window(WindowEvent::Resized(info)) => {
                    self.width = info.logical_size().width as f32;
                    self.height = info.logical_size().height as f32;
                    self.scale_factor = info.scale() as f32;
                    self.configure_surface();
                    self.state.on_resize(self.width, self.height);
                }
                _ => {}
            }

            EventStatus::Ignored
        }
    }

    impl<S: WgpuViewState> WindowHandler for WgpuHandlerState<S> {
        fn on_frame(&mut self, window: &mut Window) {
            match self {
                Self::Ready(handler) => handler.on_frame(window),
                Self::Failed => window.close(),
            }
        }

        fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
            match self {
                Self::Ready(handler) => handler.on_event(window, event),
                Self::Failed => EventStatus::Ignored,
            }
        }
    }
}

#[cfg(feature = "wgpu")]
pub use wgpu_backend::{BaseviewWgpuView, WgpuViewState};

// ============ WebView Backend ============

#[cfg(feature = "webview")]
mod webview_backend {
    use super::*;
    use baseview::webview::WebView;
    use std::sync::mpsc;

    /// Trait for view state that communicates with a platform WebView.
    pub trait WebViewState: Send + 'static {
        /// Return the HTML content to load.
        fn html(&self) -> &str;

        /// Called when JavaScript sends a message via the message handler.
        fn on_message(&mut self, message: &str, context: &dyn ViewContext);

        /// Called when the view is resized.
        fn on_resize(&mut self, _width: f32, _height: f32) {}
    }

    /// A SunmaoView implementation using baseview + the platform WebView.
    pub struct BaseviewWebView<S: WebViewState + 'static> {
        config: BaseviewConfig,
        builder: ViewBuilder<S>,
        message_handler_name: String,
    }

    impl<S: WebViewState + 'static> BaseviewWebView<S> {
        pub fn new<B>(config: BaseviewConfig, builder: B, message_handler_name: &str) -> Self
        where
            B: Fn(Arc<dyn ViewContext>) -> S + Send + Sync + 'static,
        {
            Self {
                config,
                builder: Arc::new(builder),
                message_handler_name: message_handler_name.to_string(),
            }
        }
    }

    impl<S: WebViewState + 'static> SunmaoView for BaseviewWebView<S> {
        fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        fn can_resize(&self) -> bool {
            true
        }

        fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            let config = self.config.clone();
            let state = (self.builder)(context.clone());
            let handler_name = self.message_handler_name.clone();

            let options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );

            let parent_wrapper = ParentWindowWrapper(parent);

            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            let mut handle = Window::open_parented(&parent_wrapper, options, move |window| {
                match WebView::new(
                    window,
                    config.width as f64,
                    config.height as f64,
                    &handler_name,
                    state.html(),
                ) {
                    Ok((webview, receiver)) => {
                        crate::pixel_probe::begin_native_session();
                        initialized_in_builder.store(true, Ordering::Release);
                        WebviewHandlerState::Ready(WebviewHandler {
                            state,
                            context,
                            webview: Some(webview),
                            receiver,
                            width: config.width as f32,
                            height: config.height as f32,
                        })
                    }
                    Err(error) => {
                        eprintln!("Failed to create WebView: {error}");
                        window.close();
                        WebviewHandlerState::Failed
                    }
                }
            });

            if initialized.load(Ordering::Acquire) && handle.is_open() {
                // No `.keyboard(..)`: only the GL backend drains the host key
                // queue today, so the wgpu and WebView editors report that they
                // did not take the key and the host keeps its own shortcut.
                // That is the honest answer rather than swallowing it.
                Some(ViewHandle::scalable(
                    ScalableWindow {
                        handle,
                        base_width: config.width,
                        base_height: config.height,
                        keys: Arc::new(super::HostKeyQueue::default()),
                    },
                    Some(resize_scalable_window),
                    scale_scalable_window,
                ))
            } else {
                handle.close();
                None
            }
        }

        fn open_standalone(
            &self,
            context: Arc<dyn ViewContext>,
            view_options: StandaloneViewOptions,
        ) -> StandaloneViewResult {
            let config = self.config.clone();
            let state = (self.builder)(Arc::clone(&context));
            let handler_name = self.message_handler_name.clone();
            let options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );
            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            open_standalone_window(options, view_options, initialized, move |window| {
                match WebView::new(
                    window,
                    config.width as f64,
                    config.height as f64,
                    &handler_name,
                    state.html(),
                ) {
                    Ok((webview, receiver)) => {
                        crate::pixel_probe::begin_native_session();
                        initialized_in_builder.store(true, Ordering::Release);
                        WebviewHandlerState::Ready(WebviewHandler {
                            state,
                            context,
                            webview: Some(webview),
                            receiver,
                            width: config.width as f32,
                            height: config.height as f32,
                        })
                    }
                    Err(error) => {
                        eprintln!("Failed to create WebView: {error}");
                        window.close();
                        WebviewHandlerState::Failed
                    }
                }
            })
        }
    }

    struct WebviewHandler<S: WebViewState> {
        state: S,
        context: Arc<dyn ViewContext>,
        webview: Option<WebView>,
        receiver: mpsc::Receiver<String>,
        width: f32,
        height: f32,
    }

    impl<S: WebViewState> WindowHandler for WebviewHandler<S> {
        fn on_frame(&mut self, _window: &mut Window) {
            if let Some(webview) = self.webview.as_ref() {
                webview.poll_events();
            }
            while let Ok(msg) = self.receiver.try_recv() {
                self.state.on_message(&msg, self.context.as_ref());
            }
        }

        fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
            match event {
                Event::Window(WindowEvent::WillClose) => {
                    // Drop WebView child resources before baseview destroys
                    // the parent X11 window. This ordering is required by
                    // WebKitGTK during close/recreate.
                    self.webview.take();
                    WebView::pump_platform_events();
                }
                Event::Window(WindowEvent::Resized(info)) => {
                    self.width = info.logical_size().width as f32;
                    self.height = info.logical_size().height as f32;
                    if let Some(webview) = self.webview.as_ref() {
                        if let Err(error) = webview.set_size(self.width as f64, self.height as f64)
                        {
                            eprintln!("Failed to resize WebView: {error}");
                        }
                    }
                    self.state.on_resize(self.width, self.height);
                }
                _ => {}
            }
            EventStatus::Ignored
        }
    }

    enum WebviewHandlerState<S: WebViewState> {
        Ready(WebviewHandler<S>),
        Failed,
    }

    impl<S: WebViewState> WindowHandler for WebviewHandlerState<S> {
        fn on_frame(&mut self, window: &mut Window) {
            match self {
                Self::Ready(handler) => handler.on_frame(window),
                Self::Failed => window.close(),
            }
        }

        fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
            match self {
                Self::Ready(handler) => handler.on_event(window, event),
                Self::Failed => EventStatus::Ignored,
            }
        }
    }
}

#[cfg(feature = "webview")]
pub use webview_backend::{BaseviewWebView, WebViewState};

// Re-exports for convenience
pub use baseview::{Size, WindowScalePolicy};

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard_types::{Code, Key, KeyState, KeyboardEvent, Location};
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn standalone_watchdog_cancels_without_requesting_stop() {
        let stop_count = Arc::new(AtomicUsize::new(0));
        let stop_count_in_thread = Arc::clone(&stop_count);
        let watchdog =
            StandaloneSmokeWatchdog::start_with_stop(Duration::from_secs(1), move || {
                stop_count_in_thread.fetch_add(1, Ordering::AcqRel);
            })
            .unwrap();

        assert!(!watchdog.finish());
        assert_eq!(stop_count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn standalone_watchdog_reports_timeout_and_requests_stop() {
        let stop_count = Arc::new(AtomicUsize::new(0));
        let stop_count_in_thread = Arc::clone(&stop_count);
        let watchdog =
            StandaloneSmokeWatchdog::start_with_stop(Duration::from_millis(10), move || {
                stop_count_in_thread.fetch_add(1, Ordering::AcqRel);
            })
            .unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while stop_count.load(Ordering::Acquire) == 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }

        assert!(watchdog.finish());
        assert!(stop_count.load(Ordering::Acquire) > 0);
    }

    fn keyboard_event(
        state: KeyState,
        code: Code,
        modifiers: keyboard_types::Modifiers,
    ) -> KeyboardEvent {
        KeyboardEvent {
            state,
            key: Key::Unidentified,
            code,
            location: Location::Standard,
            modifiers,
            repeat: false,
            is_composing: false,
        }
    }

    #[test]
    fn keyboard_dispatch_converts_down_events_and_capture_status() {
        let event = keyboard_event(
            KeyState::Down,
            Code::KeyA,
            keyboard_types::Modifiers::SHIFT | keyboard_types::Modifiers::CONTROL,
        );
        let mut called = false;

        let status = dispatch_keyboard_event(&event, |event| {
            called = true;
            match event {
                GuiEvent::KeyDown { key, modifiers } => {
                    assert_eq!(*key, GuiKeyCode::A);
                    assert!(modifiers.shift);
                    assert!(modifiers.ctrl);
                    assert!(!modifiers.alt);
                    assert!(!modifiers.meta);
                }
                _ => panic!("expected key-down event"),
            }
            true
        });

        assert!(called);
        assert_eq!(status, EventStatus::Captured);
    }

    #[test]
    fn keyboard_dispatch_converts_up_events_and_ignored_status() {
        let event = keyboard_event(KeyState::Up, Code::Escape, keyboard_types::Modifiers::META);

        let status = dispatch_keyboard_event(&event, |event| {
            match event {
                GuiEvent::KeyUp { key, modifiers } => {
                    assert_eq!(*key, GuiKeyCode::Escape);
                    assert!(modifiers.meta);
                }
                _ => panic!("expected key-up event"),
            }
            false
        });

        assert_eq!(status, EventStatus::Ignored);
    }

    #[test]
    fn keyboard_code_conversion_covers_numpad_and_unknown_keys() {
        assert_eq!(convert_key_code(Code::Numpad7), GuiKeyCode::Num7);
        assert_eq!(convert_key_code(Code::NumpadEnter), GuiKeyCode::Enter);
        assert_eq!(convert_key_code(Code::Unidentified), GuiKeyCode::Unknown);
    }

    fn key_event(
        state: keyboard_types::KeyState,
        key: keyboard_types::Key,
        composing: bool,
    ) -> keyboard_types::KeyboardEvent {
        keyboard_types::KeyboardEvent {
            state,
            key,
            code: keyboard_types::Code::KeyA,
            location: keyboard_types::Location::Standard,
            modifiers: keyboard_types::Modifiers::empty(),
            repeat: false,
            is_composing: composing,
        }
    }

    fn character(text: &str) -> keyboard_types::Key {
        keyboard_types::Key::Character(text.to_string())
    }

    /// `Code` is a physical position, so it cannot represent what an
    /// international layout or an IME actually produced. Only the logical
    /// `Key::Character` can, and this is the path that carries it.
    #[test]
    fn international_text_reaches_the_editor_as_text_input() {
        for text in ["e", "é", "ü", "漢", "字", "🎛"] {
            let event = key_event(keyboard_types::KeyState::Down, character(text), false);
            match text_input_from_key(&event) {
                Some(GuiEvent::TextInput { text: produced }) => assert_eq!(produced, text),
                other => panic!("{text:?} produced {other:?} instead of text input"),
            }
        }
    }

    /// An IME's preedit is still being edited. Inserting it would type every
    /// intermediate candidate before the user commits.
    #[test]
    fn composition_in_progress_produces_no_text() {
        let composing = key_event(keyboard_types::KeyState::Down, character("に"), true);
        assert!(text_input_from_key(&composing).is_none());
        // The committed text arrives as a separate non-composing event.
        let committed = key_event(keyboard_types::KeyState::Down, character("日本"), false);
        assert!(matches!(
            text_input_from_key(&committed),
            Some(GuiEvent::TextInput { .. })
        ));
    }

    #[test]
    fn key_releases_and_non_character_keys_produce_no_text() {
        let release = key_event(keyboard_types::KeyState::Up, character("a"), false);
        assert!(
            text_input_from_key(&release).is_none(),
            "a key release typed"
        );
        let named = key_event(
            keyboard_types::KeyState::Down,
            keyboard_types::Key::Enter,
            false,
        );
        assert!(text_input_from_key(&named).is_none());
    }

    /// Control characters arrive as `Character` on some platforms. They are
    /// commands, not text, and inserting them would put a literal control byte
    /// into a text field.
    #[test]
    fn control_characters_are_not_treated_as_text() {
        for text in ["\u{1b}", "\u{7f}", "\u{0}", "\r", ""] {
            let event = key_event(keyboard_types::KeyState::Down, character(text), false);
            assert!(
                text_input_from_key(&event).is_none(),
                "{text:?} was treated as text"
            );
        }
    }

    /// A key press delivers the key first and the text it produced second, so
    /// a control can act on Enter while a text field still receives what was
    /// typed. Both must reach the handler.
    #[test]
    fn a_character_press_dispatches_both_the_key_and_its_text() {
        let event = key_event(keyboard_types::KeyState::Down, character("é"), false);
        let mut seen: Vec<String> = Vec::new();
        let status = dispatch_keyboard_event(&event, |gui_event| {
            match gui_event {
                GuiEvent::KeyDown { .. } => seen.push("key".into()),
                GuiEvent::TextInput { text } => seen.push(format!("text:{text}")),
                _ => {}
            }
            true
        });
        assert_eq!(seen, vec!["key".to_string(), "text:é".to_string()]);
        assert!(matches!(status, EventStatus::Captured));
    }

    /// A handler that ignores both must leave the event uncaptured, so the host
    /// still gets its own keyboard shortcuts.
    #[test]
    fn an_ignored_keystroke_is_not_captured_from_the_host() {
        let event = key_event(keyboard_types::KeyState::Down, character("a"), false);
        let status = dispatch_keyboard_event(&event, |_| false);
        assert!(matches!(status, EventStatus::Ignored));
    }

    fn host_key(
        code: sunmao_core::ViewKeyCode,
        character: Option<char>,
        pressed: bool,
    ) -> sunmao_core::ViewKey {
        sunmao_core::ViewKey {
            character,
            code,
            pressed,
        }
    }

    /// The neutral code an editor sees must survive the mapping unchanged; the
    /// VST3-specific numbering is the backend's problem, and is asserted there
    /// against the constants transcribed into `vst3_sys`.
    #[test]
    fn every_neutral_key_maps_to_a_framework_code() {
        use sunmao_core::ViewKeyCode;
        for (code, expected) in [
            (ViewKeyCode::Tab, GuiKeyCode::Tab),
            (ViewKeyCode::Enter, GuiKeyCode::Enter),
            (ViewKeyCode::Escape, GuiKeyCode::Escape),
            (ViewKeyCode::Space, GuiKeyCode::Space),
            (ViewKeyCode::Left, GuiKeyCode::Left),
            (ViewKeyCode::Up, GuiKeyCode::Up),
            (ViewKeyCode::Right, GuiKeyCode::Right),
            (ViewKeyCode::Down, GuiKeyCode::Down),
            (ViewKeyCode::Home, GuiKeyCode::Home),
            (ViewKeyCode::End, GuiKeyCode::End),
            (ViewKeyCode::PageUp, GuiKeyCode::PageUp),
            (ViewKeyCode::PageDown, GuiKeyCode::PageDown),
            (ViewKeyCode::Unknown, GuiKeyCode::Unknown),
        ] {
            let events = host_key_events(host_key(code, None, true));
            match events.first() {
                Some(GuiEvent::KeyDown { key, .. }) => assert_eq!(*key, expected, "{code:?}"),
                other => panic!("{code:?} produced {other:?}"),
            }
        }
    }

    /// Host-forwarded and native keys must look identical to an editor, so a
    /// printable character produces text alongside the key event here too.
    #[test]
    fn a_printable_host_key_also_produces_text() {
        let events = host_key_events(host_key(sunmao_core::ViewKeyCode::Unknown, Some('é'), true));
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1], GuiEvent::TextInput { ref text } if text == "é"));

        // A release types nothing, and a control character is a command.
        assert_eq!(
            host_key_events(host_key(
                sunmao_core::ViewKeyCode::Unknown,
                Some('a'),
                false
            ))
            .len(),
            1
        );
        assert_eq!(
            host_key_events(host_key(
                sunmao_core::ViewKeyCode::Unknown,
                Some('\u{1b}'),
                true
            ))
            .len(),
            1
        );
    }

    #[test]
    fn the_host_key_queue_drains_in_order_and_refuses_to_grow_without_bound() {
        let queue = HostKeyQueue::default();
        for index in 0..64 {
            assert!(
                queue.push(host_key(sunmao_core::ViewKeyCode::Unknown, None, true)),
                "push {index}"
            );
        }
        // A host forwarding faster than the editor paints must not grow it
        // forever.
        assert!(
            !queue.push(host_key(sunmao_core::ViewKeyCode::Unknown, None, true)),
            "the queue was unbounded"
        );

        let drained = queue.drain();
        assert_eq!(drained.len(), 64);
        assert!(queue.drain().is_empty(), "drain left something behind");

        // The verdict is remembered for the host's next query.
        assert!(!queue.last_was_consumed());
        queue.record_consumed(true);
        assert!(queue.last_was_consumed());
    }
}
