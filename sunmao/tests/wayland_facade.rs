//! Verifies the public facade feature without enabling implementation-crate features.
#![cfg(all(target_os = "linux", feature = "gui-wayland"))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use sunmao::prelude::*;

// Existing renderer diagnostic ABI: observe actual pixels without adding an
// implementation-crate dependency that could accidentally enable Wayland.
extern "C" {
    fn sunmao_debug_read_frame(
        width: *mut u32,
        height: *mut u32,
        pixels: *mut u32,
        len: usize,
    ) -> i32;
}

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
            sunmao_debug_read_frame(
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
fn facade_wayland_editor_renders_resizes_and_reopens() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        println!("WAYLAND FACADE SKIPPED: no compositor selected");
        return;
    }
    assert!(
        std::env::var_os("DISPLAY").is_none(),
        "acceptance must run without X11 fallback"
    );
    assert!(
        std::env::var_os("SUNMAO_GUI_PIXEL_PROBE").is_some(),
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
    println!("WAYLAND FACADE VERIFIED: shader rendering, resize, close and reopen without X11");
}
