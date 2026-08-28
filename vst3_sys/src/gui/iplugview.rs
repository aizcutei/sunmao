//! IPlugView and IPlugFrame interfaces

use crate::base::types::*;
use std::ffi::c_void;

// =============================================================================
// ViewRect
// =============================================================================

/// View rectangle
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ViewRect {
    pub left: int32,
    pub top: int32,
    pub right: int32,
    pub bottom: int32,
}

impl ViewRect {
    pub fn new(left: int32, top: int32, right: int32, bottom: int32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> int32 {
        self.right - self.left
    }
    pub fn height(&self) -> int32 {
        self.bottom - self.top
    }
}

/// Platform UI type strings
pub const kPlatformTypeHWND: &[u8] = b"HWND\0";
pub const kPlatformTypeNSView: &[u8] = b"NSView\0";
pub const kPlatformTypeUIView: &[u8] = b"UIView\0";
pub const kPlatformTypeX11EmbedWindowID: &[u8] = b"X11EmbedWindowID\0";

// =============================================================================
// IPlugView VTable
// =============================================================================

/// IPlugView vtable
#[repr(C)]
pub struct IPlugViewVtbl {
    pub unknown: IUnknownVtbl,
    pub is_platform_type_supported:
        unsafe extern "system" fn(this: *mut c_void, type_: FIDString) -> tresult,
    pub attached: unsafe extern "system" fn(
        this: *mut c_void,
        parent: *mut c_void,
        type_: FIDString,
    ) -> tresult,
    pub removed: unsafe extern "system" fn(this: *mut c_void) -> tresult,
    pub on_wheel: unsafe extern "system" fn(this: *mut c_void, distance: f32) -> tresult,
    pub on_key_down: unsafe extern "system" fn(
        this: *mut c_void,
        key: char16,
        key_code: int16,
        modifiers: int16,
    ) -> tresult,
    pub on_key_up: unsafe extern "system" fn(
        this: *mut c_void,
        key: char16,
        key_code: int16,
        modifiers: int16,
    ) -> tresult,
    pub get_size: unsafe extern "system" fn(this: *mut c_void, size: *mut ViewRect) -> tresult,
    pub on_size: unsafe extern "system" fn(this: *mut c_void, new_size: *mut ViewRect) -> tresult,
    pub on_focus: unsafe extern "system" fn(this: *mut c_void, state: TBool) -> tresult,
    pub set_frame: unsafe extern "system" fn(this: *mut c_void, frame: *mut c_void) -> tresult,
    pub can_resize: unsafe extern "system" fn(this: *mut c_void) -> tresult,
    pub check_size_constraint:
        unsafe extern "system" fn(this: *mut c_void, rect: *mut ViewRect) -> tresult,
}

// =============================================================================
// IPlugFrame VTable
// =============================================================================

/// IPlugFrame vtable
#[repr(C)]
pub struct IPlugFrameVtbl {
    pub unknown: IUnknownVtbl,
    pub resize_view: unsafe extern "system" fn(
        this: *mut c_void,
        view: *mut c_void,
        new_size: *mut ViewRect,
    ) -> tresult,
}

// =============================================================================
// Linux host run-loop interfaces
// =============================================================================

/// Millisecond interval used by [`IRunLoopVtbl::register_timer`].
pub type TimerInterval = uint64;

/// Native file descriptor watched by a Linux VST3 host run loop.
pub type FileDescriptor = int32;

/// Plug-in callback invoked when a registered file descriptor is readable.
#[repr(C)]
pub struct IEventHandlerVtbl {
    pub unknown: IUnknownVtbl,
    pub on_fd_is_set: unsafe extern "system" fn(this: *mut c_void, fd: FileDescriptor),
}

/// Plug-in callback invoked by a registered host timer.
#[repr(C)]
pub struct ITimerHandlerVtbl {
    pub unknown: IUnknownVtbl,
    pub on_timer: unsafe extern "system" fn(this: *mut c_void),
}

/// Linux host run loop used to marshal GUI work onto the host UI thread.
#[repr(C)]
pub struct IRunLoopVtbl {
    pub unknown: IUnknownVtbl,
    pub register_event_handler: unsafe extern "system" fn(
        this: *mut c_void,
        handler: *mut c_void,
        fd: FileDescriptor,
    ) -> tresult,
    pub unregister_event_handler:
        unsafe extern "system" fn(this: *mut c_void, handler: *mut c_void) -> tresult,
    pub register_timer: unsafe extern "system" fn(
        this: *mut c_void,
        handler: *mut c_void,
        milliseconds: TimerInterval,
    ) -> tresult,
    pub unregister_timer:
        unsafe extern "system" fn(this: *mut c_void, handler: *mut c_void) -> tresult,
}
