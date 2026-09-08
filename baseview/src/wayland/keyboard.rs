//! The compositor owns the layout and modifier state (wl_keyboard v5).
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::time::{Duration, Instant};

use keyboard_types::{Key, KeyState, KeyboardEvent, Modifiers};
use wayland_client::protocol::wl_keyboard;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use xkbcommon_dl as xkb;

use super::window::OpenState;
use crate::{Event, WindowEvent};

use crate::xkb_keyboard::Keymap;

pub(super) struct Keyboard {
    pub(super) proxy: wl_keyboard::WlKeyboard,
    map: Option<Keymap>,
    pub(super) focused: bool,
    held: Vec<(u32, KeyboardEvent, bool)>,
    repeat: Option<(u32, Instant)>,
    rate: i32,
    delay: i32,
}
impl Keyboard {
    pub(super) fn new(proxy: wl_keyboard::WlKeyboard) -> Self {
        Self {
            proxy,
            map: None,
            focused: false,
            held: Vec::new(),
            repeat: None,
            rate: 25,
            delay: 600,
        }
    }
    pub(super) fn modifiers(&self) -> Modifiers {
        self.map
            .as_ref()
            .map_or(Modifiers::empty(), Keymap::modifiers)
    }
    pub(super) fn cancel(&mut self, events: &mut Vec<Event>) {
        self.repeat = None;
        for (_, mut event, _) in self.held.drain(..) {
            event.state = KeyState::Up;
            event.repeat = false;
            event.is_composing = false;
            event.modifiers = Modifiers::empty();
            events.push(Event::Keyboard(event));
        }
        if let Some(map) = self.map.as_mut() {
            map.reset();
        }
        self.focused = false;
    }
    fn key(&mut self, raw: u32, pressed: bool, events: &mut Vec<Event>) {
        if !self.focused {
            return;
        }
        let Some(map) = self.map.as_mut() else {
            return;
        };
        if pressed {
            if let Some(event) = map.event(raw, true, false) {
                self.held.retain(|(key, _, _)| *key != raw);
                self.held.push((raw, event.clone(), map.composed));
                events.push(Event::Keyboard(event));
                if self.rate > 0
                    && raw.checked_add(8).is_some_and(|key| unsafe {
                        (map.api.xkb_keymap_key_repeats)(map.map, key) != 0
                    })
                {
                    self.repeat = Some((
                        raw,
                        Instant::now() + Duration::from_millis(self.delay.max(0) as u64),
                    ));
                }
            }
        } else {
            if self.repeat.is_some_and(|(key, _)| key == raw) {
                self.repeat = None;
            }
            // Keys already down on Enter are not synthesized into presses, so
            // their eventual release must not invent an unmatched editor event.
            if let Some(index) = self.held.iter().position(|(key, _, _)| *key == raw) {
                let (_, mut event, _) = self.held.remove(index);
                event.state = KeyState::Up;
                event.modifiers = map.modifiers();
                event.is_composing = false;
                events.push(Event::Keyboard(event));
            }
        }
    }
    pub(super) fn tick(&mut self, now: Instant, events: &mut Vec<Event>) {
        if let Some((raw, deadline)) = self.repeat {
            if self.focused && self.rate > 0 && now >= deadline {
                if let Some(mut event) =
                    self.map.as_mut().and_then(|map| map.event(raw, true, true))
                {
                    if let Some((_, original, true)) =
                        self.held.iter().find(|(key, _, _)| *key == raw)
                    {
                        event.key = original.key.clone();
                    }
                    // A held dead key must not leak text while composition is active.
                    if !self
                        .held
                        .iter()
                        .any(|(key, event, _)| *key == raw && event.is_composing)
                    {
                        events.push(Event::Keyboard(event));
                    }
                }
                // Bound catch-up work after a stalled frame; never flood the GUI.
                self.repeat = Some((
                    raw,
                    now + Duration::from_secs_f64(1.0 / f64::from(self.rate)),
                ));
            }
        }
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        if self.proxy.version() >= 3 {
            self.proxy.release();
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, u32> for OpenState {
    fn event(
        state: &mut Self,
        proxy: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let was_focused = state
            .seats
            .values()
            .any(|seat| seat.keyboard.as_ref().is_some_and(|k| k.focused));
        let Some(seat) = state.seats.get_mut(name) else {
            return;
        };
        seat.flush_pointer(&mut state.events);
        let Some(input) = seat.keyboard.as_mut().filter(|input| &input.proxy == proxy) else {
            return;
        };
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                let seat_focused = input.focused;
                input.cancel(&mut state.events);
                // Replacement keymaps must not remove existing keyboard focus.
                input.focused = seat_focused;
                input.map = None;
                let result = (|| {
                    if format != WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                        return Err("unsupported Wayland keymap format".to_string());
                    }
                    if size == 0 || size > 16 * 1024 * 1024 {
                        return Err("invalid Wayland keymap size".into());
                    }
                    let file = File::from(fd);
                    let mut bytes = vec![0; size as usize];
                    file.read_exact_at(&mut bytes, 0)
                        .map_err(|e| e.to_string())?;
                    Keymap::new(&bytes)
                })();
                match result {
                    Ok(map) => input.map = Some(map),
                    Err(error) => {
                        eprintln!("Wayland keyboard initialization failed: {error}");
                        state.close_requested = true;
                    }
                }
            }
            wl_keyboard::Event::Enter { serial, .. } => {
                state.activation.input.record(*name, serial);
                input.focused = true;
            }
            wl_keyboard::Event::Leave { .. } => input.cancel(&mut state.events),
            wl_keyboard::Event::Key {
                serial,
                key,
                state: WEnum::Value(value),
                ..
            } => {
                state.activation.input.record(*name, serial);
                input.key(
                    key,
                    value == wl_keyboard::KeyState::Pressed,
                    &mut state.events,
                );
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(map) = input.map.as_mut() {
                    map.update(mods_depressed, mods_latched, mods_locked, group);
                }
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                input.rate = rate.max(0);
                input.delay = delay.max(0);
                input.repeat = None;
            }
            _ => {}
        }
        let focused = state
            .seats
            .values()
            .any(|seat| seat.keyboard.as_ref().is_some_and(|k| k.focused));
        if focused != was_focused {
            state.events.push(Event::Window(if focused {
                WindowEvent::Focused
            } else {
                WindowEvent::Unfocused
            }));
        }
    }
}
