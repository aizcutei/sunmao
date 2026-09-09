// Copyright 2020 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Baseview modifications to druid code:
// - move from_nsstring function to this file
// - update imports, paths etc

//! Conversion of platform keyboard event into cross-platform event.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use cocoa::appkit::{NSEvent, NSEventModifierFlags, NSEventType};
use cocoa::base::id;
use cocoa::foundation::NSString;
use keyboard_types::{Code, Key, KeyState, KeyboardEvent, Modifiers};
use objc::{msg_send, sel, sel_impl};

use crate::keyboard::code_to_location;

pub(crate) fn from_nsstring(s: id) -> String {
    unsafe {
        let slice = std::slice::from_raw_parts(s.UTF8String() as *const _, s.len());
        let result = std::str::from_utf8_unchecked(slice);
        result.into()
    }
}

/// State for processing of keyboard events.
///
/// Native layout translation retains per-window dead-key state. This handles
/// international keyboard composition; a full NSTextInputClient IME commit
/// and preedit protocol is not implemented here.
///
/// Most of the logic in this module is adapted from Mozilla, and in particular
/// TextInputHandler.mm.
pub(crate) struct KeyboardState {
    last_mods: Cell<NSEventModifierFlags>,
    compose: RefCell<LayoutCompose>,
    held: RefCell<HeldKeys>,
}

/// Convert a macOS platform key code (keyCode field of NSEvent).
///
/// The primary source for this mapping is:
/// https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/code/code_values
///
/// It should also match up with CODE_MAP_MAC bindings in
/// NativeKeyToDOMCodeName.h.
fn key_code_to_code(key_code: u16) -> Code {
    match key_code {
        0x00 => Code::KeyA,
        0x01 => Code::KeyS,
        0x02 => Code::KeyD,
        0x03 => Code::KeyF,
        0x04 => Code::KeyH,
        0x05 => Code::KeyG,
        0x06 => Code::KeyZ,
        0x07 => Code::KeyX,
        0x08 => Code::KeyC,
        0x09 => Code::KeyV,
        0x0a => Code::IntlBackslash,
        0x0b => Code::KeyB,
        0x0c => Code::KeyQ,
        0x0d => Code::KeyW,
        0x0e => Code::KeyE,
        0x0f => Code::KeyR,
        0x10 => Code::KeyY,
        0x11 => Code::KeyT,
        0x12 => Code::Digit1,
        0x13 => Code::Digit2,
        0x14 => Code::Digit3,
        0x15 => Code::Digit4,
        0x16 => Code::Digit6,
        0x17 => Code::Digit5,
        0x18 => Code::Equal,
        0x19 => Code::Digit9,
        0x1a => Code::Digit7,
        0x1b => Code::Minus,
        0x1c => Code::Digit8,
        0x1d => Code::Digit0,
        0x1e => Code::BracketRight,
        0x1f => Code::KeyO,
        0x20 => Code::KeyU,
        0x21 => Code::BracketLeft,
        0x22 => Code::KeyI,
        0x23 => Code::KeyP,
        0x24 => Code::Enter,
        0x25 => Code::KeyL,
        0x26 => Code::KeyJ,
        0x27 => Code::Quote,
        0x28 => Code::KeyK,
        0x29 => Code::Semicolon,
        0x2a => Code::Backslash,
        0x2b => Code::Comma,
        0x2c => Code::Slash,
        0x2d => Code::KeyN,
        0x2e => Code::KeyM,
        0x2f => Code::Period,
        0x30 => Code::Tab,
        0x31 => Code::Space,
        0x32 => Code::Backquote,
        0x33 => Code::Backspace,
        0x34 => Code::NumpadEnter,
        0x35 => Code::Escape,
        0x36 => Code::MetaRight,
        0x37 => Code::MetaLeft,
        0x38 => Code::ShiftLeft,
        0x39 => Code::CapsLock,
        // Note: in the linked source doc, this is "OSLeft"
        0x3a => Code::AltLeft,
        0x3b => Code::ControlLeft,
        0x3c => Code::ShiftRight,
        // Note: in the linked source doc, this is "OSRight"
        0x3d => Code::AltRight,
        0x3e => Code::ControlRight,
        0x3f => Code::Fn, // No events fired
        //0x40 => Code::F17,
        0x41 => Code::NumpadDecimal,
        0x43 => Code::NumpadMultiply,
        0x45 => Code::NumpadAdd,
        0x47 => Code::NumLock,
        0x48 => Code::AudioVolumeUp,
        0x49 => Code::AudioVolumeDown,
        0x4a => Code::AudioVolumeMute,
        0x4b => Code::NumpadDivide,
        0x4c => Code::NumpadEnter,
        0x4e => Code::NumpadSubtract,
        //0x4f => Code::F18,
        //0x50 => Code::F19,
        0x51 => Code::NumpadEqual,
        0x52 => Code::Numpad0,
        0x53 => Code::Numpad1,
        0x54 => Code::Numpad2,
        0x55 => Code::Numpad3,
        0x56 => Code::Numpad4,
        0x57 => Code::Numpad5,
        0x58 => Code::Numpad6,
        0x59 => Code::Numpad7,
        //0x5a => Code::F20,
        0x5b => Code::Numpad8,
        0x5c => Code::Numpad9,
        0x5d => Code::IntlYen,
        0x5e => Code::IntlRo,
        0x5f => Code::NumpadComma,
        0x60 => Code::F5,
        0x61 => Code::F6,
        0x62 => Code::F7,
        0x63 => Code::F3,
        0x64 => Code::F8,
        0x65 => Code::F9,
        0x66 => Code::Lang2,
        0x67 => Code::F11,
        0x68 => Code::Lang1,
        // Note: this is listed as F13, but in testing with a standard
        // USB kb, this the code produced by PrtSc.
        0x69 => Code::PrintScreen,
        //0x6a => Code::F16,
        //0x6b => Code::F14,
        0x6d => Code::F10,
        0x6e => Code::ContextMenu,
        0x6f => Code::F12,
        //0x71 => Code::F15,
        0x72 => Code::Help,
        0x73 => Code::Home,
        0x74 => Code::PageUp,
        0x75 => Code::Delete,
        0x76 => Code::F4,
        0x77 => Code::End,
        0x78 => Code::F2,
        0x79 => Code::PageDown,
        0x7a => Code::F1,
        0x7b => Code::ArrowLeft,
        0x7c => Code::ArrowRight,
        0x7d => Code::ArrowDown,
        0x7e => Code::ArrowUp,
        _ => Code::Unidentified,
    }
}

/// Convert code to key.
///
/// On macOS, for non-printable keys, the keyCode we get from the event serves is
/// really more of a key than a physical scan code.
///
/// When this function returns None, the code can be considered printable.
///
/// The logic for this function is derived from KEY_MAP_COCOA bindings in
/// NativeKeyToDOMKeyName.h.
fn code_to_key(code: Code) -> Option<Key> {
    Some(match code {
        Code::Escape => Key::Escape,
        Code::ShiftLeft | Code::ShiftRight => Key::Shift,
        Code::AltLeft | Code::AltRight => Key::Alt,
        Code::MetaLeft | Code::MetaRight => Key::Meta,
        Code::ControlLeft | Code::ControlRight => Key::Control,
        Code::CapsLock => Key::CapsLock,
        // kVK_ANSI_KeypadClear
        Code::NumLock => Key::Clear,
        Code::Fn => Key::Fn,
        Code::F1 => Key::F1,
        Code::F2 => Key::F2,
        Code::F3 => Key::F3,
        Code::F4 => Key::F4,
        Code::F5 => Key::F5,
        Code::F6 => Key::F6,
        Code::F7 => Key::F7,
        Code::F8 => Key::F8,
        Code::F9 => Key::F9,
        Code::F10 => Key::F10,
        Code::F11 => Key::F11,
        Code::F12 => Key::F12,
        Code::Pause => Key::Pause,
        Code::ScrollLock => Key::ScrollLock,
        Code::PrintScreen => Key::PrintScreen,
        Code::Insert => Key::Insert,
        Code::Delete => Key::Delete,
        Code::Tab => Key::Tab,
        Code::Backspace => Key::Backspace,
        Code::ContextMenu => Key::ContextMenu,
        // kVK_JIS_Kana
        Code::Lang1 => Key::KanjiMode,
        // kVK_JIS_Eisu
        Code::Lang2 => Key::Eisu,
        Code::Home => Key::Home,
        Code::End => Key::End,
        Code::PageUp => Key::PageUp,
        Code::PageDown => Key::PageDown,
        Code::ArrowLeft => Key::ArrowLeft,
        Code::ArrowRight => Key::ArrowRight,
        Code::ArrowUp => Key::ArrowUp,
        Code::ArrowDown => Key::ArrowDown,
        Code::Enter => Key::Enter,
        Code::NumpadEnter => Key::Enter,
        Code::Help => Key::Help,
        _ => return None,
    })
}

fn is_valid_key(s: &str) -> bool {
    match s.chars().next() {
        None => false,
        Some(c) => c >= ' ' && c != '\x7f' && !('\u{e000}'..'\u{f900}').contains(&c),
    }
}

fn is_modifier_code(code: Code) -> bool {
    matches!(
        code,
        Code::ShiftLeft
            | Code::ShiftRight
            | Code::AltLeft
            | Code::AltRight
            | Code::ControlLeft
            | Code::ControlRight
            | Code::MetaLeft
            | Code::MetaRight
            | Code::CapsLock
            | Code::Help
    )
}

impl KeyboardState {
    pub(crate) fn new() -> KeyboardState {
        let last_mods = Cell::new(NSEventModifierFlags::empty());
        KeyboardState {
            last_mods,
            compose: RefCell::new(LayoutCompose::default()),
            held: RefCell::new(HeldKeys::default()),
        }
    }

    pub(crate) fn reset_composition(&self) -> Vec<KeyboardEvent> {
        self.compose.borrow_mut().dead = 0;
        self.last_mods.set(NSEventModifierFlags::empty());
        self.held.borrow_mut().cancel()
    }

    pub(crate) fn last_mods(&self) -> NSEventModifierFlags {
        self.last_mods.get()
    }

    pub(crate) fn process_native_event(&self, event: id) -> Option<KeyboardEvent> {
        unsafe {
            let event_type = event.eventType();
            let key_code = event.keyCode();
            let code = key_code_to_code(key_code);
            let location = code_to_location(code);
            let raw_mods = event.modifierFlags();
            let modifiers = make_modifiers(raw_mods);
            let state = match event_type {
                NSEventType::NSKeyDown => KeyState::Down,
                NSEventType::NSKeyUp => KeyState::Up,
                NSEventType::NSFlagsChanged => {
                    // We use `bits` here because we want to distinguish the
                    // device dependent bits (when both left and right keys
                    // may be pressed, for example).
                    let any_down = raw_mods.bits() & !self.last_mods.get().bits();
                    self.last_mods.set(raw_mods);
                    if is_modifier_code(code) {
                        if any_down == 0 {
                            KeyState::Up
                        } else {
                            KeyState::Down
                        }
                    } else {
                        // HandleFlagsChanged has some logic for this; it might
                        // happen when an app is deactivated by Command-Tab. In
                        // that case, the best thing to do is synthesize the event
                        // from the modifiers. But a challenge there is that we
                        // might get multiple events.
                        return None;
                    }
                }
                _ => unreachable!(),
            };
            let mut is_composing = false;
            let repeat: bool = event_type == NSEventType::NSKeyDown && msg_send![event, isARepeat];
            if let Some(event) = self
                .held
                .borrow_mut()
                .existing(key_code, state, modifiers, repeat)
            {
                return Some(event);
            }
            let key = if let Some(key) = code_to_key(code) {
                if state == KeyState::Down && !is_modifier_code(code) {
                    // Escape, editing and navigation keys cancel an unfinished
                    // dead-key sequence without inserting the pending accent.
                    self.compose.borrow_mut().dead = 0;
                }
                key
            } else {
                let mut characters = from_nsstring(event.characters());
                if state == KeyState::Down {
                    let mut compose = self.compose.borrow_mut();
                    if characters.is_empty() || compose.dead != 0 {
                        if let Some(text) = compose.translate(key_code, modifiers) {
                            characters = text;
                        }
                    }
                }
                if characters.is_empty() {
                    // AppKit emits no characters for a dead key. Falling back
                    // to charactersIgnoringModifiers would insert its base
                    // letter before the composed character arrives.
                    is_composing = true;
                    Key::Dead
                } else if is_valid_key(&characters) {
                    Key::Character(characters)
                } else {
                    let chars_ignoring = from_nsstring(event.charactersIgnoringModifiers());
                    if is_valid_key(&chars_ignoring) {
                        Key::Character(chars_ignoring)
                    } else {
                        // There may be more heroic things we can do here.
                        Key::Unidentified
                    }
                }
            };
            let event = KeyboardEvent {
                code,
                key,
                location,
                modifiers,
                state,
                is_composing,
                repeat,
            };
            if state == KeyState::Down {
                self.held.borrow_mut().0.insert(key_code, event.clone());
            }
            Some(event)
        }
    }
}

const MODIFIER_MAP: &[(NSEventModifierFlags, Modifiers)] = &[
    (NSEventModifierFlags::NSShiftKeyMask, Modifiers::SHIFT),
    (NSEventModifierFlags::NSAlternateKeyMask, Modifiers::ALT),
    (NSEventModifierFlags::NSControlKeyMask, Modifiers::CONTROL),
    (NSEventModifierFlags::NSCommandKeyMask, Modifiers::META),
    (
        NSEventModifierFlags::NSAlphaShiftKeyMask,
        Modifiers::CAPS_LOCK,
    ),
];

pub(crate) fn make_modifiers(raw: NSEventModifierFlags) -> Modifiers {
    let mut modifiers = Modifiers::empty();
    for &(flags, mods) in MODIFIER_MAP {
        if raw.contains(flags) {
            modifiers |= mods;
        }
    }
    modifiers
}

// UCKeyTranslate owns no process-global compose state: each window retains its
// own dead-key state and the system layout data that gives that state meaning.
#[derive(Default)]
struct LayoutCompose {
    layout: Option<core_foundation::data::CFData>,
    dead: u32,
}
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *const std::ffi::c_void;
    fn TISGetInputSourceProperty(
        source: *const std::ffi::c_void,
        property: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    static kTISPropertyUnicodeKeyLayoutData: *const std::ffi::c_void;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const std::ffi::c_void,
        key: u16,
        action: u16,
        modifiers: u32,
        keyboard_type: u32,
        options: u32,
        dead: *mut u32,
        capacity: u32,
        length: *mut u32,
        output: *mut u16,
    ) -> i32;
}
impl LayoutCompose {
    fn translate(&mut self, key: u16, modifiers: Modifiers) -> Option<String> {
        use core_foundation::{
            base::{CFRelease, TCFType},
            data::CFData,
        };
        unsafe {
            let source = TISCopyCurrentKeyboardLayoutInputSource();
            if source.is_null() {
                self.dead = 0;
                return None;
            }
            let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
            let layout = if data.is_null() {
                None
            } else {
                Some(CFData::wrap_under_get_rule(data as _))
            };
            CFRelease(source);
            let layout = match layout {
                Some(layout) => layout,
                None => {
                    self.dead = 0;
                    return None;
                }
            };
            if self.layout.as_ref() != Some(&layout) {
                self.dead = 0;
                self.layout = Some(layout);
            }
            let layout = self.layout.as_ref().unwrap();
            // Carbon modifier bits shifted right by 8, as UCKeyTranslate
            // requires. NSEvent modifier bit positions are different.
            let mut flags = 0;
            for (modifier, bit) in [
                (Modifiers::META, 1),
                (Modifiers::SHIFT, 2),
                (Modifiers::CAPS_LOCK, 4),
                (Modifiers::ALT, 8),
                (Modifiers::CONTROL, 16),
            ] {
                if modifiers.contains(modifier) {
                    flags |= bit;
                }
            }
            let mut output = [0u16; 256];
            let mut length = 0;
            let status = UCKeyTranslate(
                layout.bytes().as_ptr() as _,
                key,
                0,
                flags,
                LMGetKbdType() as u32,
                0,
                &mut self.dead,
                output.len() as u32,
                &mut length,
                output.as_mut_ptr(),
            );
            if status != 0 || length as usize > output.len() {
                self.dead = 0;
                eprintln!("macOS keyboard layout translation failed: {}", status);
                return None;
            }
            match String::from_utf16(&output[..length as usize]) {
                Ok(text) => Some(text),
                Err(_) => {
                    self.dead = 0;
                    eprintln!("macOS keyboard layout returned invalid UTF-16");
                    None
                }
            }
        }
    }
}

// Releases and repeats retain the logical key chosen on the initial press,
// even if a modifier or the input source changes while the key is held.
#[derive(Default)]
struct HeldKeys(HashMap<u16, KeyboardEvent>);
impl HeldKeys {
    fn existing(
        &mut self,
        code: u16,
        state: KeyState,
        modifiers: Modifiers,
        repeat: bool,
    ) -> Option<KeyboardEvent> {
        let mut event = if state == KeyState::Up {
            self.0.remove(&code)?
        } else if repeat {
            self.0.get(&code)?.clone()
        } else {
            return None;
        };
        event.state = state;
        event.modifiers = modifiers;
        event.repeat = repeat;
        Some(event)
    }
    fn cancel(&mut self) -> Vec<KeyboardEvent> {
        self.0
            .drain()
            .map(|(_, mut event)| {
                event.state = KeyState::Up;
                event.modifiers = Modifiers::empty();
                event.repeat = false;
                event
            })
            .collect()
    }
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn logical_keys_survive_modifier_changes_and_cancel_releases_once(
            keys in proptest::collection::vec((0u16..128, any::<char>()), 0..256)
        ) {
            let mut held = HeldKeys::default();
            for (physical, character) in keys {
                let event = KeyboardEvent {
                    code: Code::KeyA, key: Key::Character(character.to_string()),
                    state: KeyState::Down, location: keyboard_types::Location::Standard,
                    modifiers: Modifiers::ALT, repeat: false, is_composing: false,
                };
                held.0.insert(physical, event.clone());
                let repeated = held.existing(physical, KeyState::Down, Modifiers::SHIFT, true).unwrap();
                prop_assert_eq!(&repeated.key, &event.key);
                prop_assert!(repeated.repeat);
                let released = held.existing(physical, KeyState::Up, Modifiers::empty(), false).unwrap();
                prop_assert_eq!(&released.key, &event.key);
                prop_assert!(held.existing(physical, KeyState::Up, Modifiers::empty(), false).is_none());
                held.0.insert(physical, event);
            }
            let count = held.0.len();
            let releases = held.cancel();
            prop_assert_eq!(releases.len(), count);
            prop_assert!(releases.iter().all(|e| e.state == KeyState::Up && !e.repeat && e.modifiers.is_empty()));
            prop_assert!(held.cancel().is_empty());
        }
    }
}
