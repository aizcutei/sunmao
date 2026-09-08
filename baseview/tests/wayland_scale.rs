#![cfg(all(target_os = "linux", feature = "wayland", feature = "opengl"))]

use baseview::{
    Event, EventStatus, MouseCursor, Size, Window, WindowEvent, WindowHandler, WindowInfo,
    WindowOpenOptions, WindowScalePolicy,
};
use glow::HasContext;
use khronos_egl as egl;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::{Duration, Instant};

struct Frame {
    info: WindowInfo,
    egl: (i32, i32),
    edge: [u8; 4],
}
struct Handler {
    gl: glow::Context,
    api: egl::DynamicInstance<egl::EGL1_5>,
    info: WindowInfo,
    tx: SyncSender<Frame>,
}
impl WindowHandler for Handler {
    fn on_frame(&mut self, window: &mut Window) {
        window.set_mouse_cursor(MouseCursor::Crosshair);
        let context = window.gl_context().unwrap();
        unsafe {
            context.make_current().unwrap();
        }
        let display = self.api.get_current_display().unwrap();
        let surface = self.api.get_current_surface(egl::DRAW).unwrap();
        let actual = (
            self.api
                .query_surface(display, surface, egl::WIDTH)
                .unwrap(),
            self.api
                .query_surface(display, surface, egl::HEIGHT)
                .unwrap(),
        );
        let expected = self.info.physical_size();
        let mut edge = [0; 4];
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.clear_color(1.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            if actual == (expected.width as i32, expected.height as i32) {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(actual.0 - 1, actual.1 - 1, 1, 1);
                self.gl.clear_color(0.0, 1.0, 0.0, 1.0);
                self.gl.clear(glow::COLOR_BUFFER_BIT);
                self.gl.disable(glow::SCISSOR_TEST);
                self.gl.read_pixels(
                    actual.0 - 1,
                    actual.1 - 1,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut edge)),
                );
            }
        }
        context.swap_buffers().unwrap();
        let _ = self.tx.try_send(Frame {
            info: self.info,
            egl: actual,
            edge,
        });
    }
    fn on_event(&mut self, _: &mut Window, event: Event) -> EventStatus {
        if let Event::Window(WindowEvent::Resized(info)) = event {
            self.info = info;
        }
        EventStatus::Ignored
    }
}
fn open(policy: WindowScalePolicy) -> (baseview::WindowHandle, Receiver<Frame>) {
    let (tx, rx) = sync_channel(8);
    let mut options =
        WindowOpenOptions::new("SunMao scale acceptance", Size::new(160.0, 120.0), policy);
    options.gl_config = Some(baseview::gl::GlConfig::default());
    let handle = Window::open_floating(options, move |window| {
        let context = window.gl_context().unwrap();
        unsafe {
            context.make_current().unwrap();
        }
        let gl =
            unsafe { glow::Context::from_loader_function(|name| context.get_proc_address(name)) };
        let api = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required().unwrap() };
        Handler {
            gl,
            api,
            info: WindowInfo::from_logical_size(Size::new(160.0, 120.0), 1.0),
            tx,
        }
    });
    assert!(handle.is_open());
    (handle, rx)
}
fn wait_frame(rx: &Receiver<Frame>, size: Size, scale: f64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let physical = baseview::WindowInfo::from_logical_size(size, scale).physical_size();
    loop {
        let frame = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("scaled frame before deadline");
        if frame.info.logical_size() == size
            && frame.info.scale() == scale
            && frame.info.physical_size() == physical
            && frame.egl == (physical.width as i32, physical.height as i32)
            && frame.edge == [0, 255, 0, 255]
        {
            println!(
                "WAYLAND SCALE FRAME: logical {}x{}, scale {}, EGL {}x{}, edge pixel verified",
                size.width, size.height, scale, frame.egl.0, frame.egl.1
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "unexpected final frame: {:?}, EGL {:?}",
            frame.info,
            frame.egl
        );
    }
}
fn sway(command: &str) {
    let output = std::process::Command::new("swaymsg")
        .args(["-r", command])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "sway command {}: {} {}",
        command,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn native_wayland_scaling_tracks_density_outputs_and_buffer_pixels() {
    let Ok(mode) = std::env::var("SUNMAO_SCALE_TEST") else {
        println!("WAYLAND SCALE SKIPPED: no controlled compositor");
        return;
    };
    assert!(std::env::var_os("DISPLAY").is_none());
    let (mut window, rx) = open(WindowScalePolicy::SystemScaleFactor);
    let size = Size::new(160.0, 120.0);
    if mode == "core" {
        // Weston 13 has no fractional-scale or preferred-buffer-scale event.
        wait_frame(&rx, size, 2.0);
        window.resize(Size::new(180.0, 130.0));
        wait_frame(&rx, Size::new(180.0, 130.0), 2.0);
        window.close();
        println!("WAYLAND CORE SCALE VERIFIED: output scale 2, logical resize, EGL pixels");
        return;
    }
    wait_frame(&rx, size, 1.0);
    for scale in [2.0, 1.5, 1.0] {
        sway(&format!("output HEADLESS-1 scale {}", scale));
        wait_frame(&rx, size, scale);
    }
    sway("[title=\"SunMao scale acceptance\"] move container to output HEADLESS-2");
    wait_frame(&rx, size, 2.0);
    sway("output HEADLESS-2 disable");
    wait_frame(&rx, size, 1.0);
    window.resize(Size::new(180.0, 130.0));
    wait_frame(&rx, Size::new(180.0, 130.0), 1.0);
    window.close();
    // Explicit density remains explicit across an output-scale change.
    let (mut fixed, rx) = open(WindowScalePolicy::ScaleFactor(1.25));
    wait_frame(&rx, size, 1.25);
    sway("output HEADLESS-1 scale 2");
    // Drain queued pre-change frames before observing the stable override.
    std::thread::sleep(Duration::from_millis(300));
    while rx.try_recv().is_ok() {}
    wait_frame(&rx, size, 1.25);
    fixed.close();
    println!("WAYLAND SCALE VERIFIED: integer/fractional density, output move/removal, resize and explicit override with EGL pixels");
}
