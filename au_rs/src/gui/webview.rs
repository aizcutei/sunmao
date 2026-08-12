//! Safe wrappers for WKWebView operations.
//!
//! This module provides APIs for creating and interacting with WKWebView
//! including JavaScript communication.

use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::{CString, c_void};
use std::sync::Once;

#[link(name = "WebKit", kind = "framework")]
unsafe extern "C" {}

/// Message handler callback type
pub type MessageCallback = fn(message: &str, user_data: *mut c_void);

static REGISTER_MESSAGE_HANDLER: Once = Once::new();
static mut MESSAGE_CALLBACK: Option<MessageCallback> = None;
static mut MESSAGE_USER_DATA: *mut c_void = std::ptr::null_mut();

/// Creates a WKWebView and adds it to the parent view.
pub fn create_wkwebview(view: *mut Object, frame: au_sys::NSRect) -> *mut Object {
    unsafe {
        if view.is_null() {
            return std::ptr::null_mut();
        }
        let config: *mut Object = msg_send![class!(WKWebViewConfiguration), new];
        let webview: *mut Object = msg_send![class!(WKWebView), alloc];
        let webview: *mut Object = msg_send![webview, initWithFrame: frame configuration: config];
        let _: () = msg_send![view, addSubview: webview];
        webview
    }
}

/// Creates a WKWebView with a script message handler for JS-to-Rust communication.
pub fn create_wkwebview_with_handler(
    view: *mut Object,
    frame: au_sys::NSRect,
    handler_name: &str,
    callback: MessageCallback,
    user_data: *mut c_void,
) -> *mut Object {
    unsafe {
        if view.is_null() {
            return std::ptr::null_mut();
        }

        // Store callback globally (simple approach for single webview)
        MESSAGE_CALLBACK = Some(callback);
        MESSAGE_USER_DATA = user_data;

        // Register our message handler class
        register_message_handler_class();

        let config: *mut Object = msg_send![class!(WKWebViewConfiguration), new];
        let content_controller: *mut Object = msg_send![config, userContentController];

        // Create and add message handler
        let handler_class =
            Class::get("RustWKScriptMessageHandler").expect("Handler class missing");
        let handler: *mut Object = msg_send![handler_class, new];

        let name_cstr = CString::new(handler_name).unwrap_or_default();
        let name_ns: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: name_cstr.as_ptr()];
        let _: () = msg_send![content_controller, addScriptMessageHandler: handler name: name_ns];

        let webview: *mut Object = msg_send![class!(WKWebView), alloc];
        let webview: *mut Object = msg_send![webview, initWithFrame: frame configuration: config];
        let _: () = msg_send![view, addSubview: webview];
        webview
    }
}

fn register_message_handler_class() {
    REGISTER_MESSAGE_HANDLER.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("RustWKScriptMessageHandler", superclass)
            .expect("Failed to create message handler class");

        decl.add_method(
            sel!(userContentController:didReceiveScriptMessage:),
            did_receive_script_message as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );

        decl.register();
    });
}

extern "C" fn did_receive_script_message(
    _this: &Object,
    _sel: Sel,
    _controller: *mut Object,
    message: *mut Object,
) {
    unsafe {
        if message.is_null() {
            return;
        }

        // Get message body
        let body: *mut Object = msg_send![message, body];
        if body.is_null() {
            return;
        }

        // Convert to string
        let utf8: *const i8 = msg_send![body, UTF8String];
        if utf8.is_null() {
            return;
        }

        let c_str = std::ffi::CStr::from_ptr(utf8);
        if let Ok(s) = c_str.to_str() {
            if let Some(callback) = MESSAGE_CALLBACK {
                callback(s, MESSAGE_USER_DATA);
            }
        }
    }
}

/// Loads a URL in the webview.
pub fn load_url(webview: *mut Object, url: &str) {
    unsafe {
        if webview.is_null() {
            return;
        }
        let Ok(cstr) = CString::new(url) else {
            return;
        };
        let ns_string: *mut Object = msg_send![class!(NSString), alloc];
        let ns_string: *mut Object = msg_send![ns_string, initWithUTF8String: cstr.as_ptr()];
        let ns_url: *mut Object = msg_send![class!(NSURL), URLWithString: ns_string];
        if ns_url.is_null() {
            return;
        }
        let request: *mut Object = msg_send![class!(NSURLRequest), requestWithURL: ns_url];
        let _: () = msg_send![webview, loadRequest: request];
    }
}

/// Loads HTML content directly in the webview.
pub fn load_html(webview: *mut Object, html: &str) {
    unsafe {
        if webview.is_null() {
            return;
        }
        let Ok(html_cstr) = CString::new(html) else {
            return;
        };
        let html_ns: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: html_cstr.as_ptr()];

        // Use about:blank as base URL
        let base_cstr = CString::new("about:blank").unwrap();
        let base_ns: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: base_cstr.as_ptr()];
        let base_url: *mut Object = msg_send![class!(NSURL), URLWithString: base_ns];

        let _: () = msg_send![webview, loadHTMLString: html_ns baseURL: base_url];
    }
}

/// Evaluates JavaScript in the webview.
pub fn evaluate_js(webview: *mut Object, script: &str) {
    unsafe {
        if webview.is_null() {
            return;
        }
        let Ok(script_cstr) = CString::new(script) else {
            return;
        };
        let script_ns: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: script_cstr.as_ptr()];

        // evaluateJavaScript:completionHandler: with nil handler
        let nil: *mut Object = std::ptr::null_mut();
        let _: () = msg_send![webview, evaluateJavaScript: script_ns completionHandler: nil];
    }
}

/// Sets the frame of the webview.
pub fn set_frame(webview: *mut Object, frame: au_sys::NSRect) {
    unsafe {
        if webview.is_null() {
            return;
        }
        let _: () = msg_send![webview, setFrame: frame];
    }
}
