//! Exercises the real SunMao renderer through the floating-window entry point.
use super::*;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;
use sunmao_gui::Fill;

struct Context;
impl ViewContext for Context {
    fn get_param(&self, _: &str) -> Option<f32> {
        None
    }
    fn set_param(&self, _: &str, _: f32) {}
    fn begin_edit(&self, _: &str) {}
    fn end_edit(&self, _: &str) {}
    fn request_resize(&self, _: u32, _: u32) -> bool {
        false
    }
}

struct State {
    dropped: Arc<AtomicUsize>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
impl ViewState for State {
    fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32) {
        ctx.fill_rect(0.0, 0.0, width / 2.0, height, Fill::Solid(Color::RED));
        ctx.fill_rect(
            width / 2.0,
            0.0,
            width / 2.0,
            height,
            Fill::Solid(Color::GREEN),
        );
    }
    fn on_mouse_event(&mut self, _: &GuiEvent) -> bool {
        false
    }
}

fn wait_for_frame(width: u32, height: u32) {
    // These deliberately small sizes are not downsampled by the pixel probe.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (mut actual_width, mut actual_height) = (0, 0);
        let mut pixels = vec![0; 4096];
        let count = unsafe {
            pixel_probe::sunmao_debug_read_frame(
                &mut actual_width,
                &mut actual_height,
                pixels.as_mut_ptr(),
                pixels.len(),
            )
        };
        if (actual_width, actual_height) == (width, height) && count == (width * height) as i32 {
            let left = pixels[(height / 2 * width + width / 4) as usize].to_ne_bytes();
            let right = pixels[(height / 2 * width + width * 3 / 4) as usize].to_ne_bytes();
            if left[0] > 240 && left[1] < 15 && right[0] < 15 && right[1] > 240 {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "SunMao renderer did not produce the expected {width}x{height} red/green frame"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_wayland_editor_renders_resizes_and_reopens() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        println!("WAYLAND EDITOR SKIPPED: no compositor selected");
        return;
    }
    assert!(
        std::env::var_os("DISPLAY").is_none(),
        "acceptance must run without X11 fallback"
    );
    assert!(
        pixel_probe::enabled(),
        "SUNMAO_GUI_PIXEL_PROBE must be enabled"
    );
    let dropped = Arc::new(AtomicUsize::new(0));
    for cycle in 1..=2 {
        let state_dropped = dropped.clone();
        let view = BaseviewView::new(
            BaseviewConfig {
                width: 40,
                height: 32,
                ..Default::default()
            },
            move |_| State {
                dropped: state_dropped.clone(),
            },
        );
        let mut handle = view
            .open_floating(Arc::new(Context))
            .expect("native Wayland editor");
        wait_for_frame(40, 32);
        assert!(handle.resize(56, 48));
        wait_for_frame(56, 48);
        drop(handle);
        assert_eq!(
            dropped.load(Ordering::SeqCst),
            cycle,
            "close must join renderer destruction"
        );
    }
    println!("WAYLAND EDITOR VERIFIED: shader rendering, resize, close and reopen without X11");
}

struct PointerState {
    phase: Arc<AtomicUsize>,
}
impl ViewState for PointerState {
    fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32) {
        let color = if self.phase.load(Ordering::SeqCst) == 2 {
            Color::BLUE
        } else {
            Color::RED
        };
        ctx.fill_rect(0.0, 0.0, width, height, Fill::Solid(color));
    }

    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
        match event {
            GuiEvent::MouseDown {
                x,
                y,
                button: sunmao_gui::MouseButton::Left,
                ..
            } if *x > 0.0 && *y > 0.0 => {
                self.phase.store(1, Ordering::SeqCst);
                true
            }
            GuiEvent::MouseUp {
                button: sunmao_gui::MouseButton::Left,
                ..
            } if self.phase.load(Ordering::SeqCst) == 1 => {
                self.phase.store(2, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }
}

fn wait_for_solid_frame(blue: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut pixels = vec![0_u32; 16384];
    loop {
        let (mut width, mut height) = (0, 0);
        let count = unsafe {
            pixel_probe::sunmao_debug_read_frame(
                &mut width,
                &mut height,
                pixels.as_mut_ptr(),
                pixels.len(),
            )
        };
        if count > 0 && count as u32 == width * height {
            let pixel = pixels[count as usize / 2].to_ne_bytes();
            if pixel[1] < 15
                && if blue {
                    pixel[2] > 240 && pixel[0] < 15
                } else {
                    pixel[0] > 240 && pixel[2] < 15
                }
            {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "expected solid frame (blue={blue})"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_wayland_pointer_changes_rendered_pixels() {
    let Some(display) = std::env::var_os("SUNMAO_INPUT_DISPLAY") else {
        println!("WAYLAND POINTER SKIPPED: no nested input testbed");
        return;
    };
    assert!(
        std::env::var_os("DISPLAY").is_none(),
        "editor must not use X11"
    );
    assert!(pixel_probe::enabled());
    let window = std::env::var("SUNMAO_INPUT_WINDOW").expect("nested compositor window");
    let phase = Arc::new(AtomicUsize::new(0));
    let state_phase = phase.clone();
    let view = BaseviewView::new(
        BaseviewConfig {
            width: 640,
            height: 480,
            ..Default::default()
        },
        move |_| PointerState {
            phase: state_phase.clone(),
        },
    );
    let handle = view
        .open_floating(Arc::new(Context))
        .expect("Wayland pointer editor");
    wait_for_solid_frame(false);
    // XTest targets the nested compositor; the editor connects only to Wayland.
    let status = std::process::Command::new("xdotool")
        .env("DISPLAY", display)
        .args([
            "mousemove",
            "--sync",
            "--window",
            &window,
            "320",
            "240",
            "click",
            "1",
        ])
        .status()
        .expect("XTest input injector");
    assert!(status.success(), "pointer injection failed");
    wait_for_solid_frame(true);
    assert_eq!(
        phase.load(Ordering::SeqCst),
        2,
        "ordered press/release must reach ViewState"
    );
    drop(handle);
    println!("WAYLAND POINTER VERIFIED: compositor click reaches editor and changes shader pixels without X11");
}
