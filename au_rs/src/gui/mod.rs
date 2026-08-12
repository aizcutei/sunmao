use objc::runtime::{NO, Object, YES};
use std::ffi::c_void;

use au_sys::{
    AudioUnitCocoaViewInfo, CocoaViewConfig, NSPoint, NSRect, NSSize, ViewCallbacks,
    cocoa_view_info as au_cocoa_view_info, init_cocoa_view_factory as au_init_cocoa_view_factory,
    set_view_user_data,
};
use objc::{class, msg_send, sel, sel_impl};

#[cfg(target_os = "macos")]
pub mod layer;
#[cfg(target_os = "macos")]
pub mod metal;
pub mod opengl;
#[cfg(target_os = "macos")]
pub mod webview;

pub use objc::runtime::Object as CocoaObject;

pub fn set_needs_display(view: *mut Object) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, setNeedsDisplay: YES];
        }
    }
}

pub fn view_bounds(view: *mut Object) -> NSRect {
    unsafe { msg_send![view, bounds] }
}

pub fn view_backing_bounds(view: *mut Object) -> NSRect {
    unsafe {
        let bounds: NSRect = msg_send![view, bounds];
        msg_send![view, convertRectToBacking: bounds]
    }
}

pub fn open_gl_context(view: *mut Object) -> *mut Object {
    unsafe { msg_send![view, openGLContext] }
}

pub fn make_current_context(ctx: *mut Object) {
    unsafe {
        if !ctx.is_null() {
            let _: () = msg_send![ctx, makeCurrentContext];
        }
    }
}

pub fn flush_context(ctx: *mut Object) {
    unsafe {
        if !ctx.is_null() {
            let _: () = msg_send![ctx, flushBuffer];
        }
    }
}

pub fn set_pixel_format(view: *mut Object, attrs: &[u32]) {
    unsafe {
        if view.is_null() {
            return;
        }
        let pixel_format: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
        let mut pixel_format: *mut Object =
            msg_send![pixel_format, initWithAttributes: attrs.as_ptr()];
        if pixel_format.is_null() {
            let fallback_legacy = [99, 0x1000, 73, 5, 0];
            let pf: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
            pixel_format = msg_send![pf, initWithAttributes: fallback_legacy.as_ptr()];
        }
        if pixel_format.is_null() {
            let fallback_basic = [73, 5, 0];
            let pf: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
            pixel_format = msg_send![pf, initWithAttributes: fallback_basic.as_ptr()];
        }
        if !pixel_format.is_null() {
            let _: () = msg_send![view, setPixelFormat: pixel_format];
        }
    }
}

pub fn set_best_resolution(view: *mut Object, enabled: bool) {
    unsafe {
        if view.is_null() {
            return;
        }
        let flag: objc::runtime::BOOL = if enabled { YES } else { NO };
        let _: () = msg_send![view, setWantsBestResolutionOpenGLSurface: flag];
    }
}

pub fn update_open_gl_view(view: *mut Object) {
    unsafe {
        if view.is_null() {
            return;
        }
        let _: () = msg_send![view, update];
    }
}

pub struct GuiConfig {
    pub factory_class: &'static str,
    pub view_class: &'static str,
    pub view_superclass: &'static str,
    pub description: &'static str,
    pub preferred_size: Option<NSSize>,
}

pub trait GuiHandler {
    fn init(&mut self, _view: *mut Object, _size: NSSize, _audio_unit: *mut c_void) {}
    fn draw(&mut self, _view: *mut Object, _audio_unit: *mut c_void, _rect: NSRect) {}
    fn reshape(&mut self, _view: *mut Object, _audio_unit: *mut c_void) {}
    fn mouse_down(
        &mut self,
        _view: *mut Object,
        _audio_unit: *mut c_void,
        _point: NSPoint,
        _flags: u64,
    ) {
    }
    fn mouse_dragged(
        &mut self,
        _view: *mut Object,
        _audio_unit: *mut c_void,
        _point: NSPoint,
        _flags: u64,
    ) {
    }
    fn mouse_up(
        &mut self,
        _view: *mut Object,
        _audio_unit: *mut c_void,
        _point: NSPoint,
        _flags: u64,
    ) {
    }
    fn key_down(
        &mut self,
        _view: *mut Object,
        _audio_unit: *mut c_void,
        _key_code: u16,
        _flags: u64,
    ) {
    }
    fn deinit(&mut self, _view: *mut Object) {}
}

pub fn register_gui<Gui: GuiHandler + Default + 'static>(
    config: GuiConfig,
) -> AudioUnitCocoaViewInfo {
    au_cocoa_stubs::ensure_linked();
    au_init_cocoa_view_factory(
        CocoaViewConfig {
            factory_class: config.factory_class,
            view_class: config.view_class,
            view_superclass: config.view_superclass,
            description: config.description,
            view_init: Some(view_init::<Gui>),
            preferred_size: config.preferred_size,
        },
        ViewCallbacks {
            draw: Some(draw::<Gui>),
            reshape: Some(reshape::<Gui>),
            mouse_down: Some(mouse_down::<Gui>),
            mouse_dragged: Some(mouse_dragged::<Gui>),
            mouse_up: Some(mouse_up::<Gui>),
            key_down: Some(key_down::<Gui>),
            deinit: Some(deinit::<Gui>),
        },
    );
    au_cocoa_view_info()
}

fn view_init<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    size: NSSize,
    audio_unit: *mut c_void,
) {
    let mut handler = Box::<Gui>::default();
    handler.init(view, size, audio_unit);
    set_view_user_data(view, Box::into_raw(handler) as *mut c_void);
}

fn draw<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    audio_unit: *mut c_void,
    user_data: *mut c_void,
    rect: NSRect,
) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.draw(view, audio_unit, rect);
    }
}

fn reshape<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    audio_unit: *mut c_void,
    user_data: *mut c_void,
) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.reshape(view, audio_unit);
    }
}

fn mouse_down<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    audio_unit: *mut c_void,
    user_data: *mut c_void,
    point: NSPoint,
    flags: u64,
) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.mouse_down(view, audio_unit, point, flags);
    }
}

fn mouse_dragged<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    audio_unit: *mut c_void,
    user_data: *mut c_void,
    point: NSPoint,
    flags: u64,
) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.mouse_dragged(view, audio_unit, point, flags);
    }
}

fn mouse_up<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    audio_unit: *mut c_void,
    user_data: *mut c_void,
    point: NSPoint,
    flags: u64,
) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.mouse_up(view, audio_unit, point, flags);
    }
}

fn key_down<Gui: GuiHandler + Default + 'static>(
    view: *mut Object,
    audio_unit: *mut c_void,
    user_data: *mut c_void,
    key_code: u16,
    flags: u64,
) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.key_down(view, audio_unit, key_code, flags);
    }
}

fn deinit<Gui: GuiHandler + Default + 'static>(view: *mut Object, user_data: *mut c_void) {
    if let Some(handler) = handler::<Gui>(user_data) {
        handler.deinit(view);
    }
    if !user_data.is_null() {
        unsafe {
            let _ = Box::from_raw(user_data as *mut Gui);
        }
    }
}

fn handler<Gui: GuiHandler + Default + 'static>(
    user_data: *mut c_void,
) -> Option<&'static mut Gui> {
    if user_data.is_null() {
        return None;
    }
    Some(unsafe { &mut *(user_data as *mut Gui) })
}
