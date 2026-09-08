//! Per-pointer cursor surfaces, using the core protocol on every compositor.
use std::time::{Duration, Instant};
use wayland_client::protocol::{wl_compositor, wl_pointer, wl_surface};
use wayland_client::{Proxy, QueueHandle};
use wayland_cursor::{Cursor, CursorTheme};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

use super::window::OpenState;
use crate::MouseCursor;

#[derive(Default)]
pub(super) struct PointerCursor {
    scale: i32,
    viewport: Option<wp_viewport::WpViewport>,
    serial: Option<u32>,
    surface: Option<wl_surface::WlSurface>,
    selected: Option<MouseCursor>,
    image: Option<Cursor>,
    frame: usize,
    next_frame: Option<Instant>,
}

impl PointerCursor {
    pub(super) fn enter(&mut self, serial: u32) {
        self.serial = Some(serial);
        // The image is undefined on every enter, including after re-entry.
        self.selected = None;
        self.next_frame = None;
    }

    pub(super) fn leave(&mut self) {
        self.serial = None;
        self.next_frame = None;
    }

    pub(super) fn entered(&self) -> bool {
        self.serial.is_some()
    }

    pub(super) fn update(
        &mut self,
        pointer: &wl_pointer::WlPointer,
        requested: MouseCursor,
        scale: i32,
        viewporter: Option<&wp_viewporter::WpViewporter>,
        theme: Option<&mut CursorTheme>,
        compositor: &wl_compositor::WlCompositor,
        handle: &QueueHandle<OpenState>,
        now: Instant,
    ) -> Result<(), String> {
        let Some(serial) = self.serial else {
            return Ok(());
        };
        if self.scale != scale {
            self.scale = scale;
            self.selected = None;
        }
        if self.selected == Some(requested) && self.next_frame.is_none_or(|deadline| now < deadline)
        {
            return Ok(());
        }
        if requested == MouseCursor::Hidden {
            pointer.set_cursor(serial, None, 0, 0);
            self.selected = Some(requested);
            self.image = None;
            self.next_frame = None;
            return Ok(());
        }
        if self.selected != Some(requested) {
            let theme = theme.ok_or("wl_shm is unavailable for cursor images")?;
            self.image = cursor_names(requested)
                .iter()
                .find_map(|name| theme.get_cursor(name).cloned());
            if self.image.is_none() {
                eprintln!(
                    "Wayland cursor {:?} is absent from the theme; using the default arrow",
                    requested
                );
                self.image = ["default", "left_ptr"]
                    .iter()
                    .find_map(|name| theme.get_cursor(name).cloned());
            }
            if self
                .image
                .as_ref()
                .is_none_or(|image| image.image_count() == 0)
            {
                return Err("system theme has no usable default cursor".into());
            }
            self.selected = Some(requested);
            self.frame = 0;
        } else if let Some(image) = &self.image {
            self.frame = (self.frame + 1) % image.image_count();
        }
        let image = self.image.as_ref().ok_or("missing cursor image")?;
        let buffer = &image[self.frame];
        let surface = self
            .surface
            .get_or_insert_with(|| compositor.create_surface(handle, ()));
        let (width, height) = buffer.dimensions();
        let (x, y) = buffer.hotspot();
        if self.viewport.is_none() {
            self.viewport = viewporter.map(|manager| manager.get_viewport(surface, handle, ()));
        }
        let mut applied_scale = scale;
        if let Some(viewport) = &self.viewport {
            if surface.version() >= 3 {
                surface.set_buffer_scale(1);
            }
            viewport.set_destination(
                (width as f64 / f64::from(scale)).ceil() as i32,
                (height as f64 / f64::from(scale)).ceil() as i32,
            );
        } else {
            // Themes may return their closest available size. Core protocol
            // buffers must be divisible by the integer scale on both axes.
            if width % scale as u32 != 0 || height % scale as u32 != 0 {
                eprintln!("baseview: cursor theme dimensions cannot use scale {scale}; using unscaled image");
                applied_scale = 1;
            }
            if surface.version() >= 3 {
                surface.set_buffer_scale(applied_scale);
            }
        }
        pointer.set_cursor(
            serial,
            Some(surface),
            x as i32 / applied_scale,
            y as i32 / applied_scale,
        );
        surface.attach(Some(buffer), 0, 0);
        surface.damage(0, 0, i32::MAX, i32::MAX);
        surface.commit();
        self.next_frame = (image.image_count() > 1)
            .then(|| now + Duration::from_millis(u64::from(buffer.delay().max(1))));
        Ok(())
    }
}

impl Drop for PointerCursor {
    fn drop(&mut self) {
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
        if let Some(surface) = self.surface.take() {
            surface.destroy();
        }
    }
}

fn cursor_names(cursor: MouseCursor) -> &'static [&'static str] {
    use MouseCursor::*;
    match cursor {
        Default => &["default", "left_ptr"],
        Hand => &["pointer", "hand2"],
        HandGrabbing => &["grabbing", "closedhand"],
        Help => &["help", "question_arrow"],
        Hidden => &[],
        Text => &["text", "xterm"],
        VerticalText => &["vertical-text"],
        Working => &["wait", "watch"],
        PtrWorking => &["progress", "left_ptr_watch"],
        NotAllowed => &["not-allowed", "crossed_circle"],
        PtrNotAllowed => &["no-drop", "not-allowed"],
        ZoomIn => &["zoom-in"],
        ZoomOut => &["zoom-out"],
        Alias => &["alias", "dnd-link"],
        Copy => &["copy", "dnd-copy"],
        Move => &["move", "fleur"],
        AllScroll => &["all-scroll", "fleur"],
        Cell => &["cell", "plus"],
        Crosshair => &["crosshair", "cross"],
        EResize => &["e-resize", "right_side"],
        NResize => &["n-resize", "top_side"],
        NeResize => &["ne-resize", "top_right_corner"],
        NwResize => &["nw-resize", "top_left_corner"],
        SResize => &["s-resize", "bottom_side"],
        SeResize => &["se-resize", "bottom_right_corner"],
        SwResize => &["sw-resize", "bottom_left_corner"],
        WResize => &["w-resize", "left_side"],
        EwResize => &["ew-resize", "sb_h_double_arrow"],
        NsResize => &["ns-resize", "sb_v_double_arrow"],
        NwseResize => &["nwse-resize", "size_fdiag"],
        NeswResize => &["nesw-resize", "size_bdiag"],
        ColResize => &["col-resize", "split_h"],
        RowResize => &["row-resize", "split_v"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    proptest::proptest! {
        #[test]
        fn every_entry_invalidates_the_old_cursor_serial(
            serials in proptest::collection::vec(proptest::num::u32::ANY, 1..64)
        ) {
            let mut cursor = PointerCursor::default();
            for serial in serials {
                cursor.enter(serial);
                proptest::prop_assert_eq!(cursor.serial, Some(serial));
                proptest::prop_assert_eq!(cursor.selected, None);
                cursor.selected = Some(MouseCursor::Hidden);
                cursor.leave();
                proptest::prop_assert!(!cursor.entered());
                proptest::prop_assert!(cursor.next_frame.is_none());
            }
        }
    }
}
