//! Virtual key codes, transcribed from `pluginterfaces/base/keycodes.h`.
//!
//! These are the SDK's **own** enumeration, not a platform scancode and not an
//! ASCII value. They are what a host passes to `IPlugView::onKeyDown`.

/// `KeyCodes` from the upstream header. The enum starts at 1 and increments
/// implicitly, so the order below is load-bearing.
pub mod key_codes {
    use crate::base::types::int16;

    pub const KEY_BACK: int16 = 1;
    pub const KEY_TAB: int16 = 2;
    pub const KEY_CLEAR: int16 = 3;
    pub const KEY_RETURN: int16 = 4;
    pub const KEY_PAUSE: int16 = 5;
    pub const KEY_ESCAPE: int16 = 6;
    pub const KEY_SPACE: int16 = 7;
    pub const KEY_NEXT: int16 = 8;
    pub const KEY_END: int16 = 9;
    pub const KEY_HOME: int16 = 10;
    pub const KEY_LEFT: int16 = 11;
    pub const KEY_UP: int16 = 12;
    pub const KEY_RIGHT: int16 = 13;
    pub const KEY_DOWN: int16 = 14;
    pub const KEY_PAGEUP: int16 = 15;
    pub const KEY_PAGEDOWN: int16 = 16;
}
