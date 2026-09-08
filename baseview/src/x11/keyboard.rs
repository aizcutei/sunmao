//! X11 server-owned layouts, effective modifiers and locale compose sequences.
use super::XcbConnection;
use crate::xkb_keyboard::Keymap;
use keyboard_types::{KeyState, KeyboardEvent, Modifiers};
use x11rb::protocol::{
    xkb,
    xproto::{KeyButMask, KeyPressEvent},
};

pub(super) struct Keyboard {
    map: Keymap,
    held: Vec<(u8, KeyboardEvent)>,
}
impl Keyboard {
    pub(super) fn new(connection: &XcbConnection) -> Result<Self, Box<dyn std::error::Error>> {
        let extension = xkb::use_extension(&connection.conn, 1, 0)?.reply()?;
        if !extension.supported {
            return Err("X11 XKB extension unavailable".into());
        }
        // Request presses-only autorepeat so release keeps its physical meaning.
        // This is per connection and never changes the host's keyboard settings.
        xkb::per_client_flags(
            &connection.conn,
            xkb::ID::USE_CORE_KBD.into(),
            xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
            xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
            xkb::BoolCtrl::default(),
            xkb::BoolCtrl::default(),
            xkb::BoolCtrl::default(),
        )?
        .reply()?;
        let events = xkb::EventType::NEW_KEYBOARD_NOTIFY | xkb::EventType::MAP_NOTIFY;
        let parts = xkb::MapPart::from(255u16);
        xkb::select_events(
            &connection.conn,
            xkb::ID::USE_CORE_KBD.into(),
            xkb::EventType::default(),
            events,
            parts,
            parts,
            &xkb::SelectEventsAux::default(),
        )?
        .check()?;
        Ok(Self {
            map: Self::load(connection)?,
            held: Vec::new(),
        })
    }
    fn load(connection: &XcbConnection) -> Result<Keymap, Box<dyn std::error::Error>> {
        let api =
            xkbcommon_dl::x11::xkbcommon_x11_option().ok_or("libxkbcommon-x11 unavailable")?;
        let conn = connection.conn.get_raw_xcb_connection();
        let device = unsafe { (api.xkb_x11_get_core_keyboard_device_id)(conn) };
        if device < 0 {
            return Err("X11 core keyboard unavailable".into());
        }
        Ok(Keymap::from_constructor(|_, context| unsafe {
            (api.xkb_x11_keymap_new_from_device)(
                context,
                conn,
                device,
                xkbcommon_dl::xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        })?)
    }
    pub(super) fn reload(
        &mut self,
        connection: &XcbConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.map = Self::load(connection)?;
        Ok(())
    }
    pub(super) fn cancel(&mut self) -> Vec<KeyboardEvent> {
        self.map.reset();
        self.held
            .drain(..)
            .map(|(_, mut event)| {
                event.state = KeyState::Up;
                event.repeat = false;
                event.is_composing = false;
                event.modifiers = Modifiers::empty();
                event
            })
            .collect()
    }
    pub(super) fn event(&mut self, event: &KeyPressEvent, pressed: bool) -> Option<KeyboardEvent> {
        // Core events carry the effective modifier bits and XKB group (bits 13–14).
        // Use this event's snapshot: querying current server state would race queued keys.
        let state = u16::from(event.state);
        self.map
            .update(u32::from(state & 255), 0, 0, u32::from((state >> 13) & 3));
        let index = self.held.iter().position(|(key, _)| *key == event.detail);
        if !pressed {
            let (_, mut result) = self.held.remove(index?);
            result.state = KeyState::Up;
            result.modifiers = self.map.modifiers();
            result.is_composing = false;
            return Some(result);
        }
        let mut result = self.map.event(
            u32::from(event.detail.checked_sub(8)?),
            true,
            index.is_some(),
        )?;
        if let Some(index) = index {
            // A held composed character repeats its committed text, never the final raw key.
            result.key = self.held[index].1.key.clone();
            result.is_composing = self.held[index].1.is_composing;
        } else {
            self.held.push((event.detail, result.clone()));
        }
        Some(result)
    }
}

// Extracts the keyboard modifiers from, e.g., the `state` field of
// `x11rb::protocol::xproto::ButtonPressEvent`
pub(super) fn key_mods(mods: KeyButMask) -> Modifiers {
    let mut ret = Modifiers::default();
    let key_masks = [
        (KeyButMask::SHIFT, Modifiers::SHIFT),
        (KeyButMask::CONTROL, Modifiers::CONTROL),
        // X11's mod keys are configurable, but this seems
        // like a reasonable default for US keyboards, at least,
        // where the "windows" key seems to be MOD_MASK_4.
        (KeyButMask::MOD1, Modifiers::ALT),
        (KeyButMask::MOD2, Modifiers::NUM_LOCK),
        (KeyButMask::MOD4, Modifiers::META),
        (KeyButMask::LOCK, Modifiers::CAPS_LOCK),
    ];
    for (mask, modifiers) in &key_masks {
        if mods.contains(*mask) {
            ret |= *modifiers;
        }
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn held_keys_release_their_original_logical_key_and_cancel_is_idempotent(
            sequence in proptest::collection::vec((8u8..128, proptest::bool::ANY, 0u16..256), 0..64)
        ) {
            let mut keyboard = Keyboard {
                map: Keymap::new(b"xkb_keymap { xkb_keycodes { include \"evdev+aliases(qwerty)\" }; xkb_types { include \"complete\" }; xkb_compatibility { include \"complete\" }; xkb_symbols { include \"pc+us(intl)\" }; };\0").unwrap(),
                held: Vec::new(),
            };
            let mut expected = std::collections::HashMap::new();
            for (detail, pressed, state) in sequence {
                let input = KeyPressEvent { detail, state: state.into(), ..Default::default() };
                let result = keyboard.event(&input, pressed);
                if pressed {
                    let event = result.unwrap();
                    proptest::prop_assert_eq!(event.repeat, expected.contains_key(&detail));
                    expected.entry(detail).or_insert(event.key);
                } else {
                    match expected.remove(&detail) {
                        Some(key) => proptest::prop_assert_eq!(result.unwrap().key, key),
                        None => proptest::prop_assert!(result.is_none()),
                    }
                }
                proptest::prop_assert_eq!(keyboard.held.len(), expected.len());
            }
            let released = keyboard.cancel();
            proptest::prop_assert_eq!(released.len(), expected.len());
            proptest::prop_assert!(released.iter().all(|event| event.state == KeyState::Up && !event.repeat && !event.is_composing && event.modifiers.is_empty()));
            proptest::prop_assert!(keyboard.cancel().is_empty());
            proptest::prop_assert!(keyboard.held.is_empty());
        }
    }
}
