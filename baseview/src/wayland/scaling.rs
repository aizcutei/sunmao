//! Surface-local geometry is independent of buffer pixel density.
use std::collections::{HashMap, HashSet};
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

use super::window::OpenState;
use crate::{Size, WindowInfo, WindowScalePolicy};

pub(super) struct Output {
    pub proxy: wl_output::WlOutput,
    pub pending: i32,
}
impl Drop for Output {
    fn drop(&mut self) {
        if self.proxy.version() >= 3 {
            self.proxy.release();
        }
    }
}

#[derive(Default)]
pub(super) struct Scaling {
    pub outputs: HashMap<u32, Output>,
    pub model: ScaleModel,
    pub surface: Option<wl_surface::WlSurface>,
    pub viewporter: Option<(u32, wp_viewporter::WpViewporter)>,
    pub fractional: Option<(
        u32,
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    )>,
}
impl Scaling {
    pub fn remove(&mut self, name: u32) {
        self.outputs.remove(&name);
        self.model.remove(name);
        if self.viewporter.as_ref().is_some_and(|(id, _)| *id == name) {
            self.viewporter.take().unwrap().1.destroy();
        }
        if self.fractional.as_ref().is_some_and(|(id, _)| *id == name) {
            self.fractional.take().unwrap().1.destroy();
        }
    }
}
impl Drop for Scaling {
    fn drop(&mut self) {
        if let Some((_, proxy)) = self.viewporter.take() {
            proxy.destroy();
        }
        if let Some((_, proxy)) = self.fractional.take() {
            proxy.destroy();
        }
    }
}

#[derive(Default)]
pub(super) struct ScaleModel {
    outputs: HashMap<u32, i32>,
    entered: HashSet<u32>,
    preferred_integer: Option<i32>,
    preferred_fractional: Option<u32>,
}
impl ScaleModel {
    fn remove(&mut self, name: u32) {
        self.outputs.remove(&name);
        self.entered.remove(&name);
    }
    pub fn system_scale(&self, fractional: bool) -> f64 {
        if fractional {
            if let Some(scale) = self.preferred_fractional {
                return f64::from(scale) / 120.0;
            }
        }
        f64::from(self.preferred_integer.unwrap_or_else(|| {
            self.entered
                .iter()
                .filter_map(|id| self.outputs.get(id))
                .copied()
                .max()
                .unwrap_or(1)
        }))
    }
}

/// These extensions must be destroyed before their wl_surface.
#[derive(Default)]
pub(super) struct SurfaceScaling {
    viewport: Option<wp_viewport::WpViewport>,
    fractional: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
}
impl SurfaceScaling {
    pub fn new(
        state: &Scaling,
        surface: &wl_surface::WlSurface,
        qh: &QueueHandle<OpenState>,
    ) -> Self {
        let viewport = state
            .viewporter
            .as_ref()
            .map(|(_, p)| p.get_viewport(surface, qh, ()));
        let fractional = viewport
            .as_ref()
            .and_then(|_| state.fractional.as_ref())
            .map(|(_, p)| p.get_fractional_scale(surface, qh, ()));
        Self {
            viewport,
            fractional,
        }
    }
    pub fn scale(
        &self,
        model: &ScaleModel,
        policy: WindowScalePolicy,
        surface: &wl_surface::WlSurface,
    ) -> f64 {
        match policy {
            WindowScalePolicy::ScaleFactor(scale) => scale,
            WindowScalePolicy::SystemScaleFactor
                if surface.version() < 3 && self.viewport.is_none() =>
            {
                1.0
            }
            WindowScalePolicy::SystemScaleFactor => model.system_scale(self.fractional.is_some()),
        }
    }
    pub fn apply(&self, surface: &wl_surface::WlSurface, info: WindowInfo) -> Result<(), String> {
        let logical = info.logical_size();
        if let Some(viewport) = &self.viewport {
            // fractional-scale-v1 requires buffer scale 1, including integer
            // preferences. Destination uses surface-local units, never pixels.
            if surface.version() >= 3 {
                surface.set_buffer_scale(1);
            }
            viewport.set_destination(logical.width as i32, logical.height as i32);
        } else if surface.version() >= 3 && info.scale().fract() == 0.0 {
            surface.set_buffer_scale(info.scale() as i32);
        } else if info.scale() != 1.0 {
            return Err(
                "requested Wayland scale needs wp_viewporter or integer buffer scaling".into(),
            );
        }
        // Do not commit old buffers with new scale/geometry. The renderer's
        // next buffer attachment and commit apply this state atomically.
        Ok(())
    }
}
impl Drop for SurfaceScaling {
    fn drop(&mut self) {
        if let Some(p) = self.fractional.take() {
            p.destroy();
        }
        if let Some(p) = self.viewport.take() {
            p.destroy();
        }
    }
}

pub(super) fn geometry(size: Size, scale: f64) -> Result<WindowInfo, String> {
    let size = Size::new(size.width.round(), size.height.round());
    if !scale.is_finite()
        || scale <= 0.0
        || scale > f64::from(i32::MAX)
        || [
            size.width,
            size.height,
            (size.width * scale).round(),
            (size.height * scale).round(),
        ]
        .iter()
        .any(|value| !value.is_finite() || *value < 1.0 || *value > f64::from(i32::MAX))
    {
        return Err("invalid Wayland logical size, scale or buffer dimensions".into());
    }
    Ok(WindowInfo::from_logical_size(size, scale))
}

pub(super) fn configured_size(current: Size, width: i32, height: i32) -> Size {
    Size::new(
        if width > 0 {
            f64::from(width)
        } else {
            current.width
        },
        if height > 0 {
            f64::from(height)
        } else {
            current.height
        },
    )
}

impl Dispatch<wl_output::WlOutput, u32> for OpenState {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .scaling
            .outputs
            .get_mut(name)
            .filter(|o| &o.proxy == proxy)
        else {
            return;
        };
        match event {
            wl_output::Event::Scale { factor } if factor > 0 => output.pending = factor,
            wl_output::Event::Done => {
                state.scaling.model.outputs.insert(*name, output.pending);
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for OpenState {
    fn event(
        state: &mut Self,
        surface: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Cursor surfaces have their own output events. They must not alter
        // the editor's size or the output set used for its fallback scale.
        if state.scaling.surface.as_ref() != Some(surface) {
            return;
        }
        match event {
            wl_surface::Event::Enter { output } => {
                if let Some((name, _)) = state
                    .scaling
                    .outputs
                    .iter()
                    .find(|(_, o)| o.proxy == output)
                {
                    state.scaling.model.entered.insert(*name);
                }
            }
            wl_surface::Event::Leave { output } => {
                if let Some((name, _)) = state
                    .scaling
                    .outputs
                    .iter()
                    .find(|(_, o)| o.proxy == output)
                {
                    state.scaling.model.entered.remove(name);
                }
            }
            wl_surface::Event::PreferredBufferScale { factor } if factor > 0 => {
                state.scaling.model.preferred_integer = Some(factor);
            }
            _ => {}
        }
    }
}
impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for OpenState {
    fn event(
        state: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            if scale > 0 {
                state.scaling.model.preferred_fractional = Some(scale);
            }
        }
    }
}
delegate_noop!(OpenState: ignore wp_viewporter::WpViewporter);
delegate_noop!(OpenState: ignore wp_viewport::WpViewport);
delegate_noop!(OpenState: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configure_uses_surface_units_and_preserves_each_unspecified_dimension() {
        let size = configured_size(Size::new(100.0, 80.0), 0, 90);
        let info = geometry(size, 1.5).unwrap();
        assert_eq!(info.logical_size(), Size::new(100.0, 90.0));
        assert_eq!(info.physical_size(), crate::PhySize::new(150, 135));
        assert!(geometry(size, f64::NAN).is_err());
        assert!(geometry(Size::new(f64::INFINITY, 1.0), 1.0).is_err());
        assert!(geometry(size, f64::from(i32::MAX)).is_err());
    }
    #[test]
    fn fractional_preference_overrides_integer_but_requires_the_extension_pair() {
        let model = ScaleModel {
            preferred_integer: Some(2),
            preferred_fractional: Some(180),
            ..Default::default()
        };
        assert_eq!(model.system_scale(true), 1.5);
        assert_eq!(model.system_scale(false), 2.0);
    }
    proptest::proptest! {
        #[test]
        fn output_removal_never_leaves_a_stale_scale(
            scales in proptest::collection::vec(1_i32..5, 1..16), removed in 0_usize..16
        ) {
            let mut model = ScaleModel::default();
            for (i, scale) in scales.iter().enumerate() {
                model.outputs.insert(i as u32, *scale);
                model.entered.insert(i as u32);
            }
            model.remove(removed as u32);
            let expected = scales.iter().enumerate().filter(|(i, _)| *i != removed).map(|(_, s)| *s).max().unwrap_or(1);
            proptest::prop_assert_eq!(model.system_scale(false), f64::from(expected));
        }
        #[test]
        fn buffer_rounding_preserves_surface_geometry(w in 1_u32..2048, h in 1_u32..2048, units in 120_u32..481) {
            let scale = f64::from(units) / 120.0;
            let info = geometry(Size::new(f64::from(w), f64::from(h)), scale).unwrap();
            proptest::prop_assert_eq!(info.logical_size(), Size::new(f64::from(w), f64::from(h)));
            proptest::prop_assert!((f64::from(info.physical_size().width) - f64::from(w) * scale).abs() <= 0.5);
            proptest::prop_assert!((f64::from(info.physical_size().height) - f64::from(h) * scale).abs() <= 0.5);
        }
    }
}
