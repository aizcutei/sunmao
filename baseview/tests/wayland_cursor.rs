#![cfg(all(target_os = "linux", feature = "wayland", feature = "opengl"))]

use baseview::{
    Event, EventStatus, MouseCursor, MouseEvent, Size, Window, WindowHandler, WindowOpenOptions,
    WindowScalePolicy,
};
use glow::HasContext;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

struct Handler {
    gl: glow::Context,
    desired: Arc<AtomicUsize>,
    entered: Arc<AtomicUsize>,
    left: Arc<AtomicUsize>,
}
impl WindowHandler for Handler {
    fn on_frame(&mut self, window: &mut Window) {
        let cursor = match self.desired.load(Ordering::SeqCst) {
            0 => MouseCursor::Hidden,
            1 => MouseCursor::Crosshair,
            _ => MouseCursor::Hand,
        };
        window.set_mouse_cursor(cursor);
        let context = window.gl_context().unwrap();
        unsafe {
            context.make_current().unwrap();
            self.gl.clear_color(1.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        context.swap_buffers().unwrap();
    }
    fn on_event(&mut self, _: &mut Window, event: Event) -> EventStatus {
        match event {
            Event::Mouse(MouseEvent::CursorEntered) => {
                self.entered.fetch_add(1, Ordering::SeqCst);
            }
            Event::Mouse(MouseEvent::CursorLeft) => {
                self.left.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        EventStatus::Ignored
    }
}

fn wait(mut predicate: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "Wayland cursor: {}", label);
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn capture() -> Vec<u8> {
    let result = std::process::Command::new("python3")
        .arg(std::env::var_os("SUNMAO_CURSOR_CAPTURE").expect("screenshot observer path"))
        .output()
        .expect("screenshot observer");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout.len(), 64 * 64 * 3);
    result.stdout
}

#[test]
fn native_wayland_cursor_changes_compositor_pixels_and_survives_reentry() {
    let Some(display) = std::env::var_os("SUNMAO_INPUT_DISPLAY") else {
        println!("WAYLAND CURSOR SKIPPED: no nested compositor");
        return;
    };
    assert!(std::env::var_os("DISPLAY").is_none());
    let compositor = std::env::var("SUNMAO_INPUT_WINDOW").unwrap();
    let desired = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(AtomicUsize::new(0));
    let left = Arc::new(AtomicUsize::new(0));
    let (handler_desired, handler_entered, handler_left) =
        (desired.clone(), entered.clone(), left.clone());
    let mut options = WindowOpenOptions::new(
        "Wayland cursor acceptance",
        Size::new(640.0, 480.0),
        WindowScalePolicy::ScaleFactor(1.0),
    );
    options.gl_config = Some(baseview::gl::GlConfig::default());
    let mut handle = Window::open_floating(options, move |window| {
        let context = window.gl_context().expect("native EGL context");
        unsafe {
            context.make_current().unwrap();
        }
        let gl =
            unsafe { glow::Context::from_loader_function(|name| context.get_proc_address(name)) };
        Handler {
            gl,
            desired: handler_desired,
            entered: handler_entered,
            left: handler_left,
        }
    });
    assert!(handle.is_open());
    let inject = |args: &[&str]| {
        assert!(std::process::Command::new("xdotool")
            .env("DISPLAY", &display)
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    inject(&["mousemove", "--sync", "--window", &compositor, "320", "240"]);
    wait(|| entered.load(Ordering::SeqCst) > 0, "pointer entry");
    let hidden = [255_u8, 0, 0].repeat(64 * 64);
    wait(
        || capture() == hidden,
        "hidden cursor must reveal red compositor pixels",
    );
    desired.store(1, Ordering::SeqCst);
    let mut crosshair = Vec::new();
    wait(
        || {
            crosshair = capture();
            crosshair != hidden
        },
        "crosshair must be visible",
    );
    desired.store(2, Ordering::SeqCst);
    let mut hand = Vec::new();
    wait(
        || {
            hand = capture();
            hand != hidden && hand != crosshair
        },
        "hand must differ from crosshair",
    );
    desired.store(0, Ordering::SeqCst);
    wait(
        || capture() == hidden,
        "hide must remove the previous cursor",
    );
    let prior_left = left.load(Ordering::SeqCst);
    inject(&["mousemove", "--sync", "700", "500"]);
    wait(|| left.load(Ordering::SeqCst) > prior_left, "pointer leave");
    desired.store(2, Ordering::SeqCst);
    let prior_entered = entered.load(Ordering::SeqCst);
    inject(&["mousemove", "--sync", "--window", &compositor, "320", "240"]);
    wait(
        || entered.load(Ordering::SeqCst) > prior_entered,
        "pointer re-entry",
    );
    wait(
        || capture() == hand,
        "re-entry must restore the requested hand using the new serial",
    );
    handle.close();
    println!("WAYLAND CURSOR VERIFIED: compositor pixels show crosshair, hand, hidden and re-entry without editor X11");
}
