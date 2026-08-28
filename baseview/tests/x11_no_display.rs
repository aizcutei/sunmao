#![cfg(target_os = "linux")]

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use baseview::{
    Event, EventStatus, Window, WindowEvent, WindowHandler, WindowOpenOptions, WindowScalePolicy,
};

const NO_DISPLAY_CHILD_ENV: &str = "SUNMAO_BASEVIEW_NO_DISPLAY_CHILD";
const LIFECYCLE_CHILD_ENV: &str = "SUNMAO_BASEVIEW_LIFECYCLE_CHILD";
#[cfg(feature = "opengl")]
const GLX_CHILD_ENV: &str = "SUNMAO_BASEVIEW_GLX_CHILD";

struct UnreachableHandler;

impl WindowHandler for UnreachableHandler {
    fn on_frame(&mut self, _window: &mut Window) {}

    fn on_event(&mut self, _window: &mut Window, _event: Event) -> EventStatus {
        EventStatus::Ignored
    }
}

struct ClosingHandler {
    frame_count: Arc<AtomicUsize>,
    saw_will_close: Arc<AtomicBool>,
}

#[cfg(feature = "opengl")]
struct GlxClosingHandler {
    rendered: Arc<AtomicBool>,
}

#[cfg(feature = "opengl")]
impl WindowHandler for GlxClosingHandler {
    fn on_frame(&mut self, window: &mut Window) {
        let context = window
            .gl_context()
            .expect("requested GLX context was not created");
        unsafe {
            context
                .make_current()
                .expect("could not make GLX context current");
        }
        assert!(
            !context.get_proc_address("glGetString").is_null(),
            "GLX context did not expose glGetString"
        );
        context.swap_buffers().expect("could not swap GLX buffers");
        unsafe {
            context
                .make_not_current()
                .expect("could not release current GLX context");
        }
        self.rendered.store(true, Ordering::Relaxed);
        window.close();
    }

    fn on_event(&mut self, _window: &mut Window, _event: Event) -> EventStatus {
        EventStatus::Ignored
    }
}

impl WindowHandler for ClosingHandler {
    fn on_frame(&mut self, window: &mut Window) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
        window.close();
    }

    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        if matches!(event, Event::Window(WindowEvent::WillClose)) {
            self.saw_will_close.store(true, Ordering::Relaxed);
        }
        EventStatus::Ignored
    }
}

fn run_test_child(test_name: &str, child_env: &str, remove_display: bool) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(child_env, "1");
    if remove_display {
        command.env_remove("DISPLAY");
    }
    let mut child = command
        .spawn()
        .expect("failed to spawn X11 regression child");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll regression child") {
            assert!(status.success(), "X11 regression child failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("X11 regression child did not finish within five seconds");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn open_blocking_without_display_returns() {
    if std::env::var_os(NO_DISPLAY_CHILD_ENV).is_some() {
        let options = WindowOpenOptions::new(
            "baseview no-display regression",
            baseview::Size::new(64.0, 64.0),
            WindowScalePolicy::SystemScaleFactor,
        );
        Window::open_blocking(options, |_window| -> UnreachableHandler {
            panic!("the window builder ran even though no X display was available")
        });
        return;
    }

    run_test_child(
        "open_blocking_without_display_returns",
        NO_DISPLAY_CHILD_ENV,
        true,
    );
}

#[test]
fn open_blocking_with_display_runs_frame_and_closes() {
    if std::env::var_os("DISPLAY").is_none() {
        return;
    }

    if std::env::var_os(LIFECYCLE_CHILD_ENV).is_some() {
        let frame_count = Arc::new(AtomicUsize::new(0));
        let saw_will_close = Arc::new(AtomicBool::new(false));
        let options = WindowOpenOptions::new(
            "baseview X11 lifecycle regression",
            baseview::Size::new(64.0, 64.0),
            WindowScalePolicy::SystemScaleFactor,
        );
        let handler_frame_count = Arc::clone(&frame_count);
        let handler_saw_will_close = Arc::clone(&saw_will_close);
        Window::open_blocking(options, move |_window| ClosingHandler {
            frame_count: handler_frame_count,
            saw_will_close: handler_saw_will_close,
        });

        assert!(frame_count.load(Ordering::Relaxed) > 0);
        assert!(saw_will_close.load(Ordering::Relaxed));
        return;
    }

    run_test_child(
        "open_blocking_with_display_runs_frame_and_closes",
        LIFECYCLE_CHILD_ENV,
        false,
    );
}

#[cfg(feature = "opengl")]
#[test]
fn glx_context_is_current_and_swaps_before_close() {
    if std::env::var_os("DISPLAY").is_none() {
        return;
    }

    if std::env::var_os(GLX_CHILD_ENV).is_some() {
        let rendered = Arc::new(AtomicBool::new(false));
        let mut options = WindowOpenOptions::new(
            "baseview GLX lifecycle regression",
            baseview::Size::new(64.0, 64.0),
            WindowScalePolicy::SystemScaleFactor,
        );
        let mut gl_config = baseview::gl::GlConfig::default();
        gl_config.srgb = false;
        gl_config.vsync = false;
        options.gl_config = Some(gl_config);
        let handler_rendered = Arc::clone(&rendered);
        Window::open_blocking(options, move |_window| GlxClosingHandler {
            rendered: handler_rendered,
        });

        assert!(rendered.load(Ordering::Relaxed));
        return;
    }

    run_test_child(
        "glx_context_is_current_and_swaps_before_close",
        GLX_CHILD_ENV,
        false,
    );
}
