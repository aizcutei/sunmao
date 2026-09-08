//! Advisory activation; only wl_keyboard events establish keyboard focus.
use std::time::{Duration, Instant};

use wayland_client::protocol::wl_surface;
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};

use super::window::OpenState;

#[derive(Default)]
pub(super) struct Activation {
    pub manager: Option<(u32, xdg_activation_v1::XdgActivationV1)>,
    pub input: InputSerial,
    pending: Option<(xdg_activation_token_v1::XdgActivationTokenV1, Instant)>,
    warned_unavailable: bool,
}

#[derive(Default)]
pub(super) struct InputSerial(Option<(u32, u32)>);

impl InputSerial {
    pub fn record(&mut self, seat: u32, serial: u32) {
        // Event arrival order is authoritative, including serial wraparound.
        self.0 = Some((seat, serial));
    }

    pub fn remove(&mut self, seat: u32) {
        if self.0.is_some_and(|(name, _)| name == seat) {
            self.0 = None;
        }
    }
}

impl Activation {
    fn cancel(&mut self) {
        if let Some((token, _)) = self.pending.take() {
            token.destroy();
        }
    }

    pub fn remove_global(&mut self, name: u32) {
        self.input.remove(name);
        if self.manager.as_ref().is_some_and(|(id, _)| *id == name) {
            self.cancel();
            self.manager.take().unwrap().1.destroy();
        }
    }
}

impl Drop for Activation {
    fn drop(&mut self) {
        self.cancel();
        if let Some((_, manager)) = self.manager.take() {
            manager.destroy();
        }
    }
}

impl OpenState {
    pub(super) fn update_activation(
        &mut self,
        requested: bool,
        surface: &wl_surface::WlSurface,
        handle: &QueueHandle<Self>,
    ) {
        if self
            .activation
            .pending
            .as_ref()
            .is_some_and(|(_, started)| started.elapsed() >= Duration::from_secs(5))
        {
            self.activation.cancel();
            eprintln!("baseview: Wayland activation token request timed out");
        }
        if !requested || self.activation.pending.is_some() {
            return;
        }
        let Some((_, manager)) = &self.activation.manager else {
            if !self.activation.warned_unavailable {
                eprintln!(
                    "baseview: compositor has no xdg_activation_v1; focus request unavailable"
                );
                self.activation.warned_unavailable = true;
            }
            return;
        };
        let token = manager.get_activation_token(handle, surface.clone());
        if let Some((name, serial)) = self.activation.input.0 {
            if let Some(seat) = self.seats.get(&name) {
                token.set_serial(serial, &seat.seat);
            }
        }
        // This surface is requesting its own activation. The compositor can
        // reject a background request; receiving Done does not imply success.
        token.set_surface(surface);
        token.set_app_id("sunmao".into());
        token.commit();
        self.activation.pending = Some((token, Instant::now()));
    }
}

delegate_noop!(OpenState: ignore xdg_activation_v1::XdgActivationV1);

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, wl_surface::WlSurface> for OpenState {
    fn event(
        state: &mut Self,
        proxy: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if !state
            .activation
            .pending
            .as_ref()
            .is_some_and(|(pending, _)| pending == proxy)
        {
            return;
        }
        if let xdg_activation_token_v1::Event::Done { token } = event {
            if let Some((_, manager)) = &state.activation.manager {
                manager.activate(token, surface);
            }
            state.activation.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn removed_seats_never_supply_activation_serials(
            events in proptest::collection::vec((0_u32..8, proptest::num::u32::ANY), 1..128),
            removed in 0_u32..8,
        ) {
            let mut input = InputSerial::default();
            for &(seat, serial) in &events {
                input.record(seat, serial);
            }
            proptest::prop_assert_eq!(input.0, events.last().copied());
            input.remove(removed);
            proptest::prop_assert!(!input.0.is_some_and(|(seat, _)| seat == removed));
        }
    }
}
