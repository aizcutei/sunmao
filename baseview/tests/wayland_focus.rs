#![cfg(all(target_os = "linux", feature = "wayland", feature = "opengl"))]

use baseview::{
    Event, EventStatus, Size, Window, WindowEvent, WindowHandler, WindowOpenOptions,
    WindowScalePolicy,
};
use glow::HasContext;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Default)]
struct State {
    focused: AtomicBool,
    request: AtomicBool,
    requests: AtomicUsize,
    keys: AtomicUsize,
}
struct Handler {
    gl: glow::Context,
    state: Arc<State>,
}
impl WindowHandler for Handler {
    fn on_frame(&mut self, window: &mut Window) {
        if self.state.request.swap(false, Ordering::SeqCst) {
            let before = window.has_focus();
            window.focus();
            assert_eq!(
                window.has_focus(),
                before,
                "focus() must not fabricate keyboard focus"
            );
            self.state.requests.fetch_add(1, Ordering::SeqCst);
        }
        let context = window.gl_context().unwrap();
        unsafe {
            context.make_current().unwrap();
            self.gl.clear_color(0.2, 0.4, 0.6, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        context.swap_buffers().unwrap();
    }
    fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
        match event {
            Event::Window(WindowEvent::Focused) => {
                assert!(window.has_focus());
                self.state.focused.store(true, Ordering::SeqCst);
            }
            Event::Window(WindowEvent::Unfocused) => {
                assert!(!window.has_focus());
                self.state.focused.store(false, Ordering::SeqCst);
            }
            Event::Keyboard(_) => {
                self.state.keys.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        EventStatus::Ignored
    }
}
fn open(title: &str, state: Arc<State>) -> baseview::WindowHandle {
    let mut options = WindowOpenOptions::new(
        title,
        Size::new(320.0, 240.0),
        WindowScalePolicy::ScaleFactor(1.0),
    );
    options.gl_config = Some(baseview::gl::GlConfig::default());
    Window::open_floating(options, move |window| {
        let context = window.gl_context().unwrap();
        unsafe {
            context.make_current().unwrap();
        }
        let gl =
            unsafe { glow::Context::from_loader_function(|name| context.get_proc_address(name)) };
        Handler { gl, state }
    })
}
fn wait(mut condition: impl FnMut() -> bool, reason: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < until, "Wayland focus: {}", reason);
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn key() {
    assert!(std::process::Command::new("wtype")
        .args(["-k", "q"])
        .status()
        .unwrap()
        .success());
}
#[test]
fn native_wayland_focus_request_obeys_compositor_policy() {
    if std::env::var_os("SUNMAO_FOCUS_TEST").is_none() {
        println!("WAYLAND FOCUS SKIPPED: no activation compositor");
        return;
    }
    assert!(std::env::var_os("DISPLAY").is_none());
    let first = Arc::new(State::default());
    let mut a = open("SunMao focus source", first.clone());
    assert!(a.is_open());
    // A headless seat gets its keyboard from the virtual-keyboard injector.
    key();
    wait(
        || first.focused.load(Ordering::SeqCst),
        "first keyboard enter",
    );
    first.request.store(true, Ordering::SeqCst);
    wait(
        || first.requests.load(Ordering::SeqCst) == 1,
        "foreground activation request",
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(first.focused.load(Ordering::SeqCst));

    let second = Arc::new(State::default());
    let mut b = open("SunMao focus peer", second.clone());
    assert!(b.is_open());
    key();
    wait(
        || second.focused.load(Ordering::SeqCst) && !first.focused.load(Ordering::SeqCst),
        "peer keyboard focus",
    );
    // The compositor is configured to reject activation-driven focus changes.
    // A background token/Done must not generate a synthetic Focused event.
    first.request.store(true, Ordering::SeqCst);
    wait(
        || first.requests.load(Ordering::SeqCst) == 2,
        "background activation request",
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(!first.focused.load(Ordering::SeqCst));
    let keys = second.keys.load(Ordering::SeqCst);
    let old_keys = first.keys.load(Ordering::SeqCst);
    key();
    wait(
        || second.keys.load(Ordering::SeqCst) > keys,
        "keys still reach the peer after denied activation",
    );
    assert_eq!(first.keys.load(Ordering::SeqCst), old_keys);
    b.close();
    wait(
        || first.focused.load(Ordering::SeqCst),
        "real compositor focus returns on peer close",
    );
    a.close();
    println!("WAYLAND FOCUS VERIFIED: advisory activation preserves compositor keyboard focus and key routing");
}
