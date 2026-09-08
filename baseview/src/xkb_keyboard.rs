//! Shared native Linux layout and compose translation.
use keyboard_types::{Key, KeyState, KeyboardEvent, Modifiers};
use std::ffi::CString;
use xkbcommon_dl as xkb;

pub(crate) struct Keymap {
    pub(crate) api: &'static xkb::XkbCommon,
    context: *mut xkb::xkb_context,
    pub(crate) map: *mut xkb::xkb_keymap,
    state: *mut xkb::xkb_state,
    pub(crate) composed: bool,
    compose: Option<(&'static xkb::XkbCommonCompose, *mut xkb::xkb_compose_state)>,
}

impl Keymap {
    #[cfg(any(feature = "wayland", test))]
    pub(crate) fn new(bytes: &[u8]) -> Result<Self, String> {
        let text = std::ffi::CStr::from_bytes_with_nul(bytes)
            .map_err(|_| "Wayland keymap is not a NUL-terminated string")?;
        Self::from_constructor(|api, context| unsafe {
            (api.xkb_keymap_new_from_string)(
                context,
                text.as_ptr(),
                xkb::xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1,
                xkb::xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        })
    }

    pub(crate) fn from_constructor(
        create: impl FnOnce(&xkb::XkbCommon, *mut xkb::xkb_context) -> *mut xkb::xkb_keymap,
    ) -> Result<Self, String> {
        let api = xkb::xkbcommon_option().ok_or("libxkbcommon is unavailable")?;
        // Each object is owned by this worker; partial initialization uses the
        // same destructor as successful initialization. No X11 connection needed.
        unsafe {
            let context = (api.xkb_context_new)(xkb::xkb_context_flags::XKB_CONTEXT_NO_FLAGS);
            if context.is_null() {
                return Err("xkb context creation failed".into());
            }
            let mut result = Self {
                api,
                context,
                map: std::ptr::null_mut(),
                state: std::ptr::null_mut(),
                compose: None,
                composed: false,
            };
            result.map = create(api, context);
            if result.map.is_null() {
                return Err("XKB keymap compilation failed".into());
            }
            result.state = (api.xkb_state_new)(result.map);
            if result.state.is_null() {
                return Err("xkb state creation failed".into());
            }
            let compose_api =
                xkb::xkbcommon_compose_option().ok_or("xkb compose is unavailable")?;
            let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
                .iter()
                .filter_map(|name| std::env::var(name).ok())
                .find(|s| !s.is_empty())
                .unwrap_or_else(|| "C".into());
            let locale = CString::new(locale).map_err(|_| "invalid compose locale")?;
            let table = (compose_api.xkb_compose_table_new_from_locale)(
                context,
                locale.as_ptr(),
                xkb::xkb_compose_compile_flags::XKB_COMPOSE_COMPILE_NO_FLAGS,
            );
            if table.is_null() {
                return Err("xkb compose table creation failed".into());
            }
            let compose = (compose_api.xkb_compose_state_new)(
                table,
                xkb::xkb_compose_state_flags::XKB_COMPOSE_STATE_NO_FLAGS,
            );
            (compose_api.xkb_compose_table_unref)(table);
            if compose.is_null() {
                return Err("xkb compose state creation failed".into());
            }
            result.compose = Some((compose_api, compose));
            Ok(result)
        }
    }

    pub(crate) fn update(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        unsafe {
            (self.api.xkb_state_update_mask)(self.state, depressed, latched, locked, 0, 0, group);
        }
    }

    pub(crate) fn modifiers(&self) -> Modifiers {
        let mut result = Modifiers::empty();
        for (name, flag) in [
            (xkb::XKB_MOD_NAME_SHIFT, Modifiers::SHIFT),
            (xkb::XKB_MOD_NAME_CAPS, Modifiers::CAPS_LOCK),
            (xkb::XKB_MOD_NAME_CTRL, Modifiers::CONTROL),
            (xkb::XKB_MOD_NAME_ALT, Modifiers::ALT),
            (xkb::XKB_MOD_NAME_NUM, Modifiers::NUM_LOCK),
            (xkb::XKB_MOD_NAME_LOGO, Modifiers::META),
            (b"LevelThree\0".as_slice(), Modifiers::ALT_GRAPH),
        ] {
            if unsafe {
                (self.api.xkb_state_mod_name_is_active)(
                    self.state,
                    name.as_ptr().cast(),
                    xkb::xkb_state_component::XKB_STATE_MODS_EFFECTIVE,
                )
            } > 0
            {
                result.insert(flag);
            }
        }
        result
    }

    pub(crate) fn event(&mut self, raw: u32, pressed: bool, repeat: bool) -> Option<KeyboardEvent> {
        let code = raw.checked_add(8)?;
        let physical = crate::keyboard::hardware_keycode_to_code(code);
        let mut sym = unsafe { (self.api.xkb_state_key_get_one_sym)(self.state, code) };
        self.composed = false;
        let mut composing = false;
        let mut text = None;
        if pressed && !repeat {
            if let Some((api, compose)) = self.compose {
                unsafe {
                    (api.xkb_compose_state_feed)(compose, sym);
                    match (api.xkb_compose_state_get_status)(compose) {
                        xkb::xkb_compose_status::XKB_COMPOSE_COMPOSING => composing = true,
                        xkb::xkb_compose_status::XKB_COMPOSE_COMPOSED => {
                            self.composed = true;
                            text = utf8(|buf, len| {
                                (api.xkb_compose_state_get_utf8)(compose, buf, len)
                            });
                            sym = (api.xkb_compose_state_get_one_sym)(compose);
                            (api.xkb_compose_state_reset)(compose);
                        }
                        xkb::xkb_compose_status::XKB_COMPOSE_CANCELLED => {
                            composing = true; // cancelled sequences never commit their last character
                            (api.xkb_compose_state_reset)(compose);
                        }
                        _ => {}
                    }
                }
            }
        }
        let key = if composing {
            Key::Dead
        } else if let Some(text) = text {
            Key::Character(text)
        } else {
            logical_key(self.api, sym)
        };
        Some(KeyboardEvent {
            state: if pressed {
                KeyState::Down
            } else {
                KeyState::Up
            },
            key,
            code: physical,
            location: crate::keyboard::code_to_location(physical),
            modifiers: self.modifiers(),
            repeat,
            is_composing: composing,
        })
    }

    pub(crate) fn reset(&mut self) {
        self.update(0, 0, 0, 0);
        if let Some((api, compose)) = self.compose {
            unsafe {
                (api.xkb_compose_state_reset)(compose);
            }
        }
    }
}

impl Drop for Keymap {
    fn drop(&mut self) {
        unsafe {
            if let Some((api, compose)) = self.compose {
                (api.xkb_compose_state_unref)(compose);
            }
            if !self.state.is_null() {
                (self.api.xkb_state_unref)(self.state);
            }
            if !self.map.is_null() {
                (self.api.xkb_keymap_unref)(self.map);
            }
            (self.api.xkb_context_unref)(self.context);
        }
    }
}

fn utf8(mut get: impl FnMut(*mut std::os::raw::c_char, usize) -> i32) -> Option<String> {
    let len = get(std::ptr::null_mut(), 0);
    if !(1..=65536).contains(&len) {
        return None;
    }
    let mut bytes = vec![0; len as usize + 1];
    let written = get(bytes.as_mut_ptr().cast(), bytes.len());
    if written != len {
        return None;
    }
    bytes.truncate(len as usize);
    String::from_utf8(bytes).ok()
}

#[allow(non_upper_case_globals)]
fn logical_key(api: &xkb::XkbCommon, sym: u32) -> Key {
    use xkb::keysyms::*;
    match sym {
        BackSpace => Key::Backspace,
        Tab | ISO_Left_Tab => Key::Tab,
        Return | KP_Enter => Key::Enter,
        Escape => Key::Escape,
        Delete | KP_Delete => Key::Delete,
        Insert | KP_Insert => Key::Insert,
        Home | KP_Home => Key::Home,
        End | KP_End => Key::End,
        Left | KP_Left => Key::ArrowLeft,
        Right | KP_Right => Key::ArrowRight,
        Up | KP_Up => Key::ArrowUp,
        Down | KP_Down => Key::ArrowDown,
        Page_Up | KP_Page_Up => Key::PageUp,
        Page_Down | KP_Page_Down => Key::PageDown,
        Shift_L | Shift_R => Key::Shift,
        Control_L | Control_R => Key::Control,
        Alt_L | Alt_R => Key::Alt,
        Super_L | Super_R | Meta_L | Meta_R => Key::Meta,
        ISO_Level3_Shift | Mode_switch => Key::AltGraph,
        Caps_Lock => Key::CapsLock,
        Num_Lock => Key::NumLock,
        Scroll_Lock => Key::ScrollLock,
        Menu => Key::ContextMenu,
        Pause => Key::Pause,
        Print => Key::PrintScreen,
        KP_Begin => Key::Clear,
        Henkan => Key::Convert,
        Muhenkan => Key::NonConvert,
        Kana_Lock => Key::KanaMode,
        Hangul => Key::HangulMode,
        Hangul_Hanja => Key::HanjaMode,
        Redo => Key::Again,
        Undo => Key::Undo,
        Select => Key::Select,
        Find => Key::Find,
        Help => Key::Help,
        XF86_AudioMute => Key::AudioVolumeMute,
        XF86_AudioLowerVolume => Key::AudioVolumeDown,
        XF86_AudioRaiseVolume => Key::AudioVolumeUp,
        XF86_Copy => Key::Copy,
        XF86_Open => Key::Open,
        XF86_Paste => Key::Paste,
        XF86_Cut => Key::Cut,
        XF86_Calculator => Key::LaunchApplication2,
        XF86_WakeUp => Key::WakeUp,
        XF86_Mail => Key::LaunchMail,
        XF86_Favorites => Key::BrowserFavorites,
        XF86_Back => Key::BrowserBack,
        XF86_Forward => Key::BrowserForward,
        XF86_Eject => Key::Eject,
        XF86_AudioNext => Key::MediaTrackNext,
        XF86_AudioPlay => Key::MediaPlayPause,
        XF86_AudioPrev => Key::MediaTrackPrevious,
        XF86_AudioStop => Key::MediaStop,
        XF86_AudioMedia => Key::LaunchMediaPlayer,
        XF86_HomePage => Key::BrowserHome,
        XF86_Refresh => Key::BrowserRefresh,
        XF86_Search => Key::BrowserSearch,
        XF86_Stop => Key::BrowserStop,
        F1 => Key::F1,
        F2 => Key::F2,
        F3 => Key::F3,
        F4 => Key::F4,
        F5 => Key::F5,
        F6 => Key::F6,
        F7 => Key::F7,
        F8 => Key::F8,
        F9 => Key::F9,
        F10 => Key::F10,
        F11 => Key::F11,
        F12 => Key::F12,
        _ => char::from_u32(unsafe { (api.xkb_keysym_to_utf32)(sym) })
            .filter(|character| !character.is_control())
            .map(|character| Key::Character(character.to_string()))
            .unwrap_or(Key::Unidentified),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const INTERNATIONAL: &[u8] = b"xkb_keymap { xkb_keycodes { include \"evdev+aliases(qwerty)\" }; xkb_types { include \"complete\" }; xkb_compatibility { include \"complete\" }; xkb_symbols { include \"pc+us(intl)+de:2+inet(evdev)\" }; };\0";

    #[test]
    fn compositor_layout_masks_and_compose_determine_text() {
        let mut map = Keymap::new(INTERNATIONAL).expect("system XKB data and library");
        // Linux input-event-codes.h: KEY_A=30, KEY_Y=21, KEY_APOSTROPHE=40, KEY_E=18.
        assert_eq!(
            map.event(30, true, false).unwrap().key,
            Key::Character("a".into())
        );
        map.update(1, 0, 0, 0); // Shift in the supplied evdev keymap
        let event = map.event(30, true, false).unwrap();
        assert_eq!(event.code, keyboard_types::Code::KeyA);
        assert_eq!(event.key, Key::Character("A".into()));
        assert!(event.modifiers.contains(Modifiers::SHIFT));
        map.update(0, 0, 0, 1); // German group: physical Y produces z.
        let event = map.event(21, true, false).unwrap();
        assert_eq!(event.code, keyboard_types::Code::KeyY);
        assert_eq!(event.key, Key::Character("z".into()));
        map.reset();
        assert!(map.event(40, true, false).unwrap().is_composing);
        assert_eq!(
            map.event(18, true, false).unwrap().key,
            Key::Character("é".into())
        );
        assert!(map.event(u32::MAX, true, false).is_none());
    }

    proptest::proptest! {
        #[test]
        fn focus_reset_clears_modifiers_and_unfinished_composition(
            masks in (0_u32..256, 0_u32..256, 0_u32..256),
            keys in proptest::collection::vec(1_u32..128, 0..32)
        ) {
            let mut map = Keymap::new(INTERNATIONAL).unwrap();
            map.update(masks.0, masks.1, masks.2, 0);
            for key in keys { map.event(key, true, false); }
            map.reset();
            let event = map.event(30, true, false).unwrap();
            proptest::prop_assert_eq!(event.key, Key::Character("a".into()));
            proptest::prop_assert!(event.modifiers.is_empty());
            proptest::prop_assert!(!event.is_composing);
        }
    }
}
