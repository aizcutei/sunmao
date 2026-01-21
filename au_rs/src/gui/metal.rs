use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {}

pub fn attach_metal_layer(view: *mut Object) -> *mut Object {
    unsafe {
        if view.is_null() {
            return std::ptr::null_mut();
        }
        let layer: *mut Object = msg_send![class!(CAMetalLayer), layer];
        let _: () = msg_send![view, setWantsLayer: true];
        let _: () = msg_send![view, setLayer: layer];
        layer
    }
}

pub fn set_layer_size(layer: *mut Object, width: f64, height: f64) {
    unsafe {
        if layer.is_null() {
            return;
        }
        let size = au_sys::NSSize { width, height };
        let _: () = msg_send![layer, setDrawableSize: size];
    }
}
