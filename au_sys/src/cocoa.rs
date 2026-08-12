#![cfg(target_os = "macos")]
#![allow(unsafe_op_in_unsafe_fn)]

use crate::{
    AudioUnit, AudioUnitCocoaViewInfo, AudioUnitGetProperty, CFStringCreateWithCString,
    kAudioUnitProperty_AuRsInstance, kAudioUnitScope_Global, kCFAllocatorDefault,
    kCFStringEncodingUTF8,
};
use libc;
use objc::declare::ClassDecl;
use objc::runtime::{
    BOOL, Class, NO, Object, Protocol, Sel, YES, class_getName, class_getSuperclass,
};
use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, OnceLock};

#[repr(C)]
struct DlInfo {
    dli_fname: *const libc::c_char,
    dli_fbase: *mut libc::c_void,
    dli_sname: *const libc::c_char,
    dli_saddr: *mut libc::c_void,
}

unsafe extern "C" {
    fn dladdr(addr: *const libc::c_void, info: *mut DlInfo) -> libc::c_int;
    fn dlsym(handle: *mut libc::c_void, symbol: *const libc::c_char) -> *mut libc::c_void;
    fn class_replaceMethod(
        cls: *mut Class,
        name: Sel,
        imp: *const libc::c_void,
        types: *const libc::c_char,
    ) -> *const libc::c_void;
    fn class_getInstanceMethod(cls: *const Class, name: Sel) -> *mut libc::c_void;
    fn method_setImplementation(
        method: *mut libc::c_void,
        imp: *const libc::c_void,
    ) -> *const libc::c_void;
    fn objc_getMetaClass(name: *const libc::c_char) -> *const Class;
}

const RTLD_DEFAULT: *mut libc::c_void = std::ptr::null_mut();

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "OpenGL", kind = "framework")]
unsafe extern "C" {}

#[link(name = "MetalKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {}

static MAIN_THREAD_RUNNER: OnceLock<usize> = OnceLock::new();

#[derive(Clone, Copy)]
pub struct CocoaViewConfig {
    pub factory_class: &'static str,
    pub view_class: &'static str,
    pub view_superclass: &'static str,
    pub description: &'static str,
    pub view_init: Option<fn(*mut Object, NSSize, *mut c_void)>,
    pub preferred_size: Option<NSSize>,
}

#[derive(Clone, Copy)]
pub struct ViewCallbacks {
    pub draw: Option<fn(*mut Object, *mut c_void, *mut c_void, NSRect)>,
    pub reshape: Option<fn(*mut Object, *mut c_void, *mut c_void)>,
    pub mouse_down: Option<fn(*mut Object, *mut c_void, *mut c_void, NSPoint, u64)>,
    pub mouse_dragged: Option<fn(*mut Object, *mut c_void, *mut c_void, NSPoint, u64)>,
    pub mouse_up: Option<fn(*mut Object, *mut c_void, *mut c_void, NSPoint, u64)>,
    pub key_down: Option<fn(*mut Object, *mut c_void, *mut c_void, u16, u64)>,
    pub deinit: Option<fn(*mut Object, *mut c_void)>,
}

struct GuiEntry {
    config: CocoaViewConfig,
    callbacks: ViewCallbacks,
}

static REGISTRY: OnceLock<Mutex<HashMap<String, Box<GuiEntry>>>> = OnceLock::new();
static LAST_FACTORY: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, Box<GuiEntry>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn last_factory() -> &'static Mutex<Option<String>> {
    LAST_FACTORY.get_or_init(|| Mutex::new(None))
}

fn is_main_thread() -> bool {
    unsafe {
        let is_main: bool = msg_send![class!(NSThread), isMainThread];
        is_main
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NSPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NSSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NSRect {
    pub origin: NSPoint,
    pub size: NSSize,
}

unsafe impl Encode for NSPoint {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGPoint=dd}") }
    }
}

unsafe impl Encode for NSSize {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGSize=dd}") }
    }
}

unsafe impl Encode for NSRect {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

pub fn init_cocoa_view_factory(config: CocoaViewConfig, callbacks: ViewCallbacks) {
    let factory_key = config.factory_class.to_string();
    au_debug_log(&format!(
        "init_cocoa_view_factory: registering {}",
        factory_key
    ));
    let mut map = registry().lock().expect("GUI registry lock poisoned");
    if map.contains_key(&factory_key) {
        au_debug_log(&format!(
            "init_cocoa_view_factory: {} already registered, skipping",
            factory_key
        ));
        return;
    }
    map.insert(
        factory_key.clone(),
        Box::new(GuiEntry { config, callbacks }),
    );
    let entry = map.get(&factory_key).expect("GUI registry insert failed");
    unsafe {
        register_view_class(&entry.config);
        register_factory_class(&entry.config);
    }
    if let Ok(mut last) = last_factory().lock() {
        *last = Some(factory_key);
    }
}

/// Find the .component bundle URL for the AU plugin binary.
///
/// Uses `dladdr` on a function pointer from the plugin binary to find the binary path,
/// then walks up the directory tree to find the `.component` bundle.
///
/// This is needed because `NSBundle bundleForClass:` returns the main bundle for
/// dynamically registered ObjC classes, not the plugin bundle. AU hosts like Logic Pro
/// need the correct bundle URL to load the CocoaUI factory class.
///
/// # Safety
/// `binary_fn` must be a valid function pointer to a function in the AU plugin binary.
unsafe fn find_plugin_bundle_url_from_fn(binary_fn: *const c_void) -> *mut c_void {
    if binary_fn.is_null() {
        au_debug_log("find_plugin_bundle_url: null function pointer");
        return std::ptr::null_mut();
    }

    // Use dladdr to find the binary path from the function pointer
    let mut info = DlInfo {
        dli_fname: std::ptr::null(),
        dli_fbase: std::ptr::null_mut(),
        dli_sname: std::ptr::null(),
        dli_saddr: std::ptr::null_mut(),
    };
    if dladdr(binary_fn as *const libc::c_void, &mut info) == 0 || info.dli_fname.is_null() {
        au_debug_log("find_plugin_bundle_url: dladdr failed");
        return std::ptr::null_mut();
    }

    let binary_path = std::ffi::CStr::from_ptr(info.dli_fname)
        .to_string_lossy()
        .into_owned();
    au_debug_log(&format!(
        "find_plugin_bundle_url: binary_path={}",
        binary_path
    ));

    // Walk up from the binary path to find the .component bundle
    // Binary is at: <name>.component/Contents/MacOS/<binary>
    let mut current = std::path::Path::new(&binary_path);
    let bundle_url = loop {
        if let Some(parent) = current.parent() {
            if parent.extension().and_then(|e| e.to_str()) == Some("component") {
                break Some(parent);
            }
            current = parent;
        } else {
            break None;
        }
    };

    let Some(bundle_path) = bundle_url else {
        au_debug_log(&format!(
            "find_plugin_bundle_url: no .component bundle found for {}",
            binary_path
        ));
        return std::ptr::null_mut();
    };

    let path_str = bundle_path.to_str().unwrap_or("");
    au_debug_log(&format!("find_plugin_bundle_url: bundle_path={}", path_str));

    // Create CFURL from the bundle path
    let path_cstr = match CString::new(path_str) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let path_cf = CFStringCreateWithCString(
        kCFAllocatorDefault,
        path_cstr.as_ptr(),
        kCFStringEncodingUTF8,
    );
    if path_cf.is_null() {
        return std::ptr::null_mut();
    }

    let url = crate::CFURLCreateWithFileSystemPath(kCFAllocatorDefault, path_cf, 0, true);
    crate::CFRelease(path_cf as crate::CFTypeRef);
    if url.is_null() {
        au_debug_log("find_plugin_bundle_url: CFURLCreateWithFileSystemPath failed");
        return std::ptr::null_mut();
    }

    let bundle = crate::CFBundleCreate(kCFAllocatorDefault, url);
    crate::CFRelease(url as crate::CFTypeRef);
    if bundle.is_null() {
        au_debug_log("find_plugin_bundle_url: CFBundleCreate failed");
        return std::ptr::null_mut();
    }

    let bundle_url = crate::CFBundleCopyBundleURL(bundle);
    crate::CFRelease(bundle as crate::CFTypeRef);
    if bundle_url.is_null() {
        au_debug_log("find_plugin_bundle_url: CFBundleCopyBundleURL failed");
        return std::ptr::null_mut();
    }

    au_debug_log("find_plugin_bundle_url: success");
    bundle_url as *mut c_void
}

/// Find the .component bundle URL using `dlsym` to locate the `RustAUFactory` symbol.
/// Falls back gracefully if the symbol is not found.
unsafe fn find_plugin_bundle_url() -> *mut c_void {
    // Use dladdr on a function known to be in the same dylib (init_cocoa_view_factory
    // is in au_sys which is statically linked into each plugin). dlsym("RustAUFactory")
    // fails because the symbol may not be exported from the dylib.
    find_plugin_bundle_url_from_fn(init_cocoa_view_factory as *const c_void)
}

/// Build the `AudioUnitCocoaViewInfo` for the registered CocoaUI factory.
///
/// Uses the provided `factory_fn` pointer (from the plugin binary) to locate the
/// correct `.component` bundle URL via `dladdr`. Falls back to `dlsym` and then
/// `NSBundle bundleForClass:` if needed.
fn build_cocoa_view_info(factory_fn: Option<*const c_void>) -> AudioUnitCocoaViewInfo {
    init_if_needed();
    let map = registry().lock().expect("GUI registry lock poisoned");
    let last = last_factory().lock().ok().and_then(|last| last.clone());
    au_debug_log(&format!(
        "build_cocoa_view_info: last_factory={:?}, registry_keys={:?}",
        last,
        map.keys().collect::<Vec<_>>()
    ));
    let factory = last
        .or_else(|| map.keys().next().cloned())
        .expect("Cocoa view factory is not initialized");
    au_debug_log(&format!(
        "build_cocoa_view_info: selected factory={}",
        factory
    ));
    let entry = map
        .get(&factory)
        .expect("Cocoa view factory is not initialized");
    unsafe {
        let cls =
            Class::get(entry.config.factory_class).expect("Cocoa UI factory class is missing");
        let name_ptr = class_getName(cls) as *const i8;
        let class_name =
            CFStringCreateWithCString(kCFAllocatorDefault, name_ptr, kCFStringEncodingUTF8);

        // Try to find the correct bundle URL using the factory function pointer.
        // For dynamically registered ObjC classes, bundleForClass: returns the main
        // bundle (the host app), not the plugin's .component bundle. AU hosts like
        // Logic Pro need the correct bundle URL to load the CocoaUI factory class.
        let url = if let Some(fn_ptr) = factory_fn {
            find_plugin_bundle_url_from_fn(fn_ptr)
        } else {
            find_plugin_bundle_url()
        };

        if url.is_null() {
            au_debug_log(
                "build_cocoa_view_info: bundle lookup failed, falling back to bundleForClass:",
            );
            let bundle: *mut Object = msg_send![class!(NSBundle), bundleForClass: cls];
            let url: *mut Object = msg_send![bundle, bundleURL];
            let url: *mut Object = msg_send![url, retain];
            return AudioUnitCocoaViewInfo {
                mCocoaAUViewBundleLocation: url as *const c_void,
                mCocoaAUViewClass: [class_name],
            };
        }
        AudioUnitCocoaViewInfo {
            mCocoaAUViewBundleLocation: url as *const c_void,
            mCocoaAUViewClass: [class_name],
        }
    }
}

pub fn cocoa_view_info() -> AudioUnitCocoaViewInfo {
    build_cocoa_view_info(None)
}

/// Build `AudioUnitCocoaViewInfo` using a function pointer from the plugin binary
/// to locate the correct `.component` bundle URL via `dladdr`.
///
/// This is the preferred entry point for AU hosts (like Logic Pro) that need the
/// correct bundle URL to load the CocoaUI factory class. The `factory_fn` should
/// be a pointer to a function defined in the AU component binary (e.g., the
/// `RustAUFactory` entry point).
pub fn cocoa_view_info_for_plugin(factory_fn: *const c_void) -> AudioUnitCocoaViewInfo {
    build_cocoa_view_info(Some(factory_fn))
}

pub fn gl_get_proc_address(name: &str) -> *const c_void {
    let Ok(cname) = CString::new(name) else {
        return std::ptr::null();
    };
    unsafe { libc::dlsym(libc::RTLD_DEFAULT, cname.as_ptr()) as *const c_void }
}

pub fn set_view_user_data(view: *mut Object, data: *mut c_void) {
    unsafe {
        if !view.is_null() {
            (*view).set_ivar("au_user_data", data);
        }
    }
}

pub fn get_view_user_data(view: *mut Object) -> *mut c_void {
    unsafe {
        if view.is_null() {
            return std::ptr::null_mut();
        }
        *(*view).get_ivar::<*mut c_void>("au_user_data")
    }
}

fn init_if_needed() {
    if registry().lock().map(|m| m.is_empty()).unwrap_or(true) {
        init_cocoa_view_factory(
            CocoaViewConfig {
                factory_class: "RustAUCocoaViewFactory",
                view_class: "RustAUCocoaView",
                view_superclass: "NSView",
                description: "Rust Audio Unit",
                view_init: None,
                preferred_size: None,
            },
            ViewCallbacks {
                draw: None,
                reshape: None,
                mouse_down: None,
                mouse_dragged: None,
                mouse_up: None,
                key_down: None,
                deinit: None,
            },
        );
    }
}

/// Replace a method implementation on an existing class.
/// Uses class_getInstanceMethod + method_setImplementation for existing methods,
/// or class_replaceMethod for new methods.
unsafe fn replace_method(cls: &Class, sel: Sel, imp: *const c_void) {
    let method = class_getInstanceMethod(cls, sel);
    if !method.is_null() {
        method_setImplementation(method, imp);
    } else {
        // Method doesn't exist yet, add it with class_replaceMethod (no type info)
        class_replaceMethod(
            cls as *const Class as *mut Class,
            sel,
            imp,
            std::ptr::null(),
        );
    }
}

unsafe fn register_view_class(config: &CocoaViewConfig) {
    if let Some(cls) = Class::get(config.view_class) {
        // Class already exists (from compiled ObjC stub). Replace method implementations.
        replace_method(cls, sel!(drawRect:), draw_rect as *const c_void);
        replace_method(cls, sel!(reshape), reshape as *const c_void);
        replace_method(cls, sel!(setFrameSize:), set_frame_size as *const c_void);
        replace_method(cls, sel!(setFrame:), set_frame as *const c_void);
        replace_method(
            cls,
            sel!(intrinsicContentSize),
            intrinsic_content_size as *const c_void,
        );
        replace_method(cls, sel!(isFlipped), is_flipped as *const c_void);
        replace_method(cls, sel!(mouseDown:), mouse_down as *const c_void);
        replace_method(cls, sel!(mouseDragged:), mouse_dragged as *const c_void);
        replace_method(cls, sel!(mouseUp:), mouse_up as *const c_void);
        replace_method(cls, sel!(keyDown:), key_down as *const c_void);
        replace_method(cls, sel!(dealloc), dealloc as *const c_void);
        replace_method(
            cls,
            sel!(acceptsFirstResponder),
            accepts_first_responder as *const c_void,
        );
        replace_method(
            cls,
            sel!(viewDidMoveToWindow),
            view_did_move_to_window as *const c_void,
        );
        replace_method(cls, sel!(au_tick:), au_tick as *const c_void);
        return;
    }
    let superclass = Class::get(config.view_superclass).expect("View superclass not found");
    let mut view_decl =
        ClassDecl::new(config.view_class, superclass).expect("Failed to create view class");
    view_decl.add_ivar::<*mut c_void>("au_unit");
    view_decl.add_ivar::<*mut c_void>("au_instance");
    view_decl.add_ivar::<*mut c_void>("au_user_data");
    view_decl.add_ivar::<*const c_void>("au_superclass");
    view_decl.add_ivar::<*const c_void>("au_callbacks");
    view_decl.add_ivar::<u8>("au_is_opengl");
    view_decl.add_ivar::<*mut Object>("au_timer");
    view_decl.add_ivar::<NSSize>("au_preferred_size");
    view_decl.add_method(
        sel!(drawRect:),
        draw_rect as extern "C" fn(&Object, Sel, NSRect),
    );
    view_decl.add_method(sel!(reshape), reshape as extern "C" fn(&Object, Sel));
    view_decl.add_method(
        sel!(setFrameSize:),
        set_frame_size as extern "C" fn(&Object, Sel, NSSize),
    );
    view_decl.add_method(
        sel!(setFrame:),
        set_frame as extern "C" fn(&Object, Sel, NSRect),
    );
    view_decl.add_method(
        sel!(intrinsicContentSize),
        intrinsic_content_size as extern "C" fn(&Object, Sel) -> NSSize,
    );
    view_decl.add_method(
        sel!(isFlipped),
        is_flipped as extern "C" fn(&Object, Sel) -> BOOL,
    );
    view_decl.add_method(
        sel!(mouseDown:),
        mouse_down as extern "C" fn(&Object, Sel, *mut Object),
    );
    view_decl.add_method(
        sel!(mouseDragged:),
        mouse_dragged as extern "C" fn(&Object, Sel, *mut Object),
    );
    view_decl.add_method(
        sel!(mouseUp:),
        mouse_up as extern "C" fn(&Object, Sel, *mut Object),
    );
    view_decl.add_method(
        sel!(keyDown:),
        key_down as extern "C" fn(&Object, Sel, *mut Object),
    );
    view_decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));
    view_decl.add_method(
        sel!(acceptsFirstResponder),
        accepts_first_responder as extern "C" fn(&Object, Sel) -> bool,
    );
    view_decl.add_method(
        sel!(viewDidMoveToWindow),
        view_did_move_to_window as extern "C" fn(&Object, Sel),
    );
    view_decl.add_method(
        sel!(au_tick:),
        au_tick as extern "C" fn(&Object, Sel, *mut Object),
    );
    view_decl.register();
}

unsafe fn register_factory_class(config: &CocoaViewConfig) {
    if let Some(cls) = Class::get(config.factory_class) {
        // Class already exists (from compiled ObjC stub). Replace method implementations.
        replace_method(
            cls,
            sel!(uiViewForAudioUnit:withSize:),
            ui_view_for_audio_unit_instance as *const c_void,
        );
        replace_method(
            cls,
            sel!(uiViewForAudioUnit:preferredSize:),
            ui_view_for_audio_unit_preferred_instance as *const c_void,
        );
        replace_method(
            cls,
            sel!(interfaceVersion),
            interface_version_instance as *const c_void,
        );
        replace_method(
            cls,
            sel!(description),
            description_instance as *const c_void,
        );
        // Class methods: get the metaclass and replace
        let meta = objc_getMetaClass(class_getName(cls));
        if !meta.is_null() {
            replace_method(
                &*meta,
                sel!(uiViewForAudioUnit:withSize:),
                ui_view_for_audio_unit as *const c_void,
            );
            replace_method(
                &*meta,
                sel!(uiViewForAudioUnit:preferredSize:),
                ui_view_for_audio_unit_preferred as *const c_void,
            );
            replace_method(
                &*meta,
                sel!(interfaceVersion),
                interface_version as *const c_void,
            );
            replace_method(&*meta, sel!(description), description as *const c_void);
        }
        return;
    }
    let superclass = class!(NSObject);
    let mut decl =
        ClassDecl::new(config.factory_class, superclass).expect("Failed to create factory class");
    if let Some(proto) = Protocol::get("AUCocoaUIBase") {
        decl.add_protocol(proto);
    }
    decl.add_method(
        sel!(uiViewForAudioUnit:withSize:),
        ui_view_for_audio_unit_instance
            as extern "C" fn(&Object, Sel, *mut c_void, NSSize) -> *mut Object,
    );
    decl.add_method(
        sel!(uiViewForAudioUnit:preferredSize:),
        ui_view_for_audio_unit_preferred_instance
            as extern "C" fn(&Object, Sel, *mut c_void, NSSize) -> *mut Object,
    );
    decl.add_method(
        sel!(interfaceVersion),
        interface_version_instance as extern "C" fn(&Object, Sel) -> u32,
    );
    decl.add_method(
        sel!(description),
        description_instance as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_class_method(
        sel!(uiViewForAudioUnit:withSize:),
        ui_view_for_audio_unit as extern "C" fn(&Class, Sel, *mut c_void, NSSize) -> *mut Object,
    );
    decl.add_class_method(
        sel!(uiViewForAudioUnit:preferredSize:),
        ui_view_for_audio_unit_preferred
            as extern "C" fn(&Class, Sel, *mut c_void, NSSize) -> *mut Object,
    );
    decl.add_class_method(
        sel!(interfaceVersion),
        interface_version as extern "C" fn(&Class, Sel) -> u32,
    );
    decl.add_class_method(
        sel!(description),
        description as extern "C" fn(&Class, Sel) -> *mut Object,
    );
    decl.register();
}

fn au_debug_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/sunmao_au_gui.log")
    {
        let _ = writeln!(f, "[cocoa] {}", msg);
    }
}

fn build_view(factory_class: &str, size: NSSize, audio_unit: *mut c_void) -> *mut Object {
    au_debug_log(&format!(
        "build_view called: factory={}, size={}x{}, au={:?}, main_thread={}",
        factory_class,
        size.width,
        size.height,
        audio_unit,
        is_main_thread()
    ));
    let entry = {
        let map = registry().lock().expect("GUI registry lock poisoned");
        let found = map.contains_key(factory_class);
        au_debug_log(&format!("build_view: registry contains key={}", found));
        map.get(factory_class)
            .map(|entry| (entry.config, &entry.callbacks as *const ViewCallbacks))
    };
    let Some((config, callbacks_ptr)) = entry else {
        au_debug_log("build_view: entry not found in registry!");
        return std::ptr::null_mut();
    };

    if !is_main_thread() {
        let mut request = BuildViewRequest {
            factory_class: factory_class.to_string(),
            config,
            callbacks_ptr,
            size,
            audio_unit,
            result: std::ptr::null_mut(),
        };
        unsafe {
            let runner_class = main_thread_runner_class();
            let runner: *mut Object = msg_send![runner_class, new];
            let value: *mut Object = msg_send![class!(NSValue), valueWithPointer: (&mut request as *mut BuildViewRequest as *mut c_void)];
            let _: () = msg_send![runner, performSelectorOnMainThread: sel!(run:) withObject: value waitUntilDone: YES];
            let _: () = msg_send![runner, release];
        }
        return request.result;
    }

    build_view_inner(factory_class, config, callbacks_ptr, size, audio_unit)
}

struct BuildViewRequest {
    factory_class: String,
    config: CocoaViewConfig,
    callbacks_ptr: *const ViewCallbacks,
    size: NSSize,
    audio_unit: *mut c_void,
    result: *mut Object,
}

extern "C" fn build_view_dispatch(context: *mut c_void) {
    let request = unsafe { &mut *(context as *mut BuildViewRequest) };
    request.result = build_view_inner(
        &request.factory_class,
        request.config,
        request.callbacks_ptr,
        request.size,
        request.audio_unit,
    );
}

fn main_thread_runner_class() -> *const Class {
    let ptr = *MAIN_THREAD_RUNNER.get_or_init(|| unsafe {
        if let Some(existing) = Class::get("SunmaoMainThreadRunner") {
            return existing as *const Class as usize;
        }
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("SunmaoMainThreadRunner", superclass)
            .expect("Failed to create SunmaoMainThreadRunner");
        decl.add_method(
            sel!(run:),
            main_thread_run as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.register() as *const Class as usize
    });
    ptr as *const Class
}

extern "C" fn main_thread_run(_this: &Object, _sel: Sel, value: *mut Object) {
    if value.is_null() {
        return;
    }
    unsafe {
        let ptr: *mut c_void = msg_send![value, pointerValue];
        build_view_dispatch(ptr);
    }
}

fn build_view_inner(
    factory_class: &str,
    config: CocoaViewConfig,
    callbacks_ptr: *const ViewCallbacks,
    size: NSSize,
    audio_unit: *mut c_void,
) -> *mut Object {
    au_debug_log(&format!(
        "build_view_inner: start, factory={}, view_class={}",
        factory_class, config.view_class
    ));
    unsafe {
        let pool: *mut Object = msg_send![class!(NSAutoreleasePool), new];
        au_debug_log("build_view_inner: autoreleasepool created");
        let mut size = size;
        if size.width <= 1.0 || size.height <= 1.0 {
            if let Some(preferred) = config.preferred_size {
                size = preferred;
            }
        }
        if size.width <= 1.0 || size.height <= 1.0 {
            size = NSSize {
                width: 400.0,
                height: 200.0,
            };
        }
        au_debug_log(&format!(
            "build_view_inner: size={}x{}",
            size.width, size.height
        ));
        let view_class = Class::get(config.view_class).expect("Cocoa view class is missing");
        au_debug_log("build_view_inner: view_class found");
        let view: *mut Object = msg_send![view_class, alloc];
        au_debug_log("build_view_inner: alloc done");
        let rect = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size,
        };
        let view: *mut Object = msg_send![view, initWithFrame: rect];
        au_debug_log(&format!(
            "build_view_inner: initWithFrame done, view={:?}",
            view
        ));
        (*view).set_ivar("au_unit", audio_unit);
        au_debug_log("build_view_inner: set_ivar au_unit done");
        let instance_ptr = crate::instance_ptr_for_unit(audio_unit);
        let _ = instance_ptr;
        au_debug_log("build_view_inner: instance_ptr_for_unit done");
        (*view).set_ivar("au_instance", audio_unit);
        (*view).set_ivar("au_user_data", std::ptr::null_mut::<c_void>());
        (*view).set_ivar("au_callbacks", callbacks_ptr as *const c_void);
        let superclass = Class::get(config.view_superclass).unwrap_or(class!(NSView));
        (*view).set_ivar("au_superclass", superclass as *const Class as *const c_void);
        let is_opengl = if config.view_superclass == "NSOpenGLView" {
            1u8
        } else {
            0u8
        };
        (*view).set_ivar("au_is_opengl", is_opengl);
        (*view).set_ivar("au_timer", std::ptr::null_mut::<Object>());
        (*view).set_ivar("au_preferred_size", size);
        let _: () = msg_send![view, setAutoresizingMask: 0x3f_u64];
        let _: () = msg_send![view, setAutoresizesSubviews: YES];
        // Allow host AutoLayout constraints to resize this view.
        let _: () = msg_send![view, setTranslatesAutoresizingMaskIntoConstraints: NO];
        au_debug_log(&format!(
            "build_view_inner: about to call view_init, fn_ptr={:?}",
            config.view_init.map(|f| f as *const u8)
        ));
        if let Some(init) = config.view_init {
            au_debug_log(&format!(
                "build_view_inner: calling view_init with view={:?}, size={}x{}, au={:?}",
                view, size.width, size.height, audio_unit
            ));
            init(view, size, audio_unit);
            au_debug_log("build_view_inner: view_init returned");
        } else {
            au_debug_log("build_view_inner: no view_init callback");
        }
        let _: () = msg_send![view, setNeedsDisplay: YES];
        let _: () = msg_send![pool, drain];
        au_debug_log("build_view_inner: done, returning view");
        view
    }
}

fn class_name(cls: *const Class) -> String {
    if cls.is_null() {
        return String::new();
    }
    unsafe {
        let name_ptr = class_getName(cls) as *const i8;
        std::ffi::CStr::from_ptr(name_ptr)
            .to_string_lossy()
            .into_owned()
    }
}

fn callbacks_for_view(this: &Object) -> Option<&'static ViewCallbacks> {
    unsafe {
        let ptr = *this.get_ivar::<*const c_void>("au_callbacks");
        if ptr.is_null() {
            None
        } else {
            Some(&*(ptr as *const ViewCallbacks))
        }
    }
}

fn is_opengl_view(this: &Object) -> bool {
    unsafe { *this.get_ivar::<u8>("au_is_opengl") != 0 }
}

fn view_preferred_size(this: &Object) -> Option<NSSize> {
    unsafe {
        let size = *this.get_ivar::<NSSize>("au_preferred_size");
        if size.width > 1.0 && size.height > 1.0 {
            Some(size)
        } else {
            None
        }
    }
}

fn set_view_preferred_size(this: &Object, size: NSSize) {
    unsafe {
        let this_mut = this as *const _ as *mut Object;
        (*this_mut).set_ivar("au_preferred_size", size);
    }
}

fn view_timer(this: &Object) -> *mut Object {
    unsafe { *this.get_ivar::<*mut Object>("au_timer") }
}

fn set_view_timer(this: &Object, timer: *mut Object) {
    unsafe {
        let this_mut = this as *const _ as *mut Object;
        (*this_mut).set_ivar("au_timer", timer);
    }
}

fn view_superclass(this: &Object) -> *const Class {
    unsafe {
        let ptr = *this.get_ivar::<*const c_void>("au_superclass");
        if !ptr.is_null() {
            return ptr as *const Class;
        }
        let cls: *const Class = msg_send![this, class];
        if cls.is_null() {
            return std::ptr::null();
        }
        class_getSuperclass(cls)
    }
}

extern "C" fn ui_view_for_audio_unit(
    this: &Class,
    _sel: Sel,
    audio_unit: *mut c_void,
    size: NSSize,
) -> *mut Object {
    let factory = class_name(this as *const Class);
    build_view(&factory, size, audio_unit)
}

extern "C" fn ui_view_for_audio_unit_instance(
    this: &Object,
    _sel: Sel,
    audio_unit: *mut c_void,
    size: NSSize,
) -> *mut Object {
    let cls: *const Class = unsafe { msg_send![this, class] };
    let factory = class_name(cls);
    build_view(&factory, size, audio_unit)
}

extern "C" fn ui_view_for_audio_unit_preferred(
    this: &Class,
    _sel: Sel,
    audio_unit: *mut c_void,
    preferred: NSSize,
) -> *mut Object {
    let factory = class_name(this as *const Class);
    build_view(&factory, preferred, audio_unit)
}

extern "C" fn ui_view_for_audio_unit_preferred_instance(
    this: &Object,
    _sel: Sel,
    audio_unit: *mut c_void,
    preferred: NSSize,
) -> *mut Object {
    let cls: *const Class = unsafe { msg_send![this, class] };
    let factory = class_name(cls);
    build_view(&factory, preferred, audio_unit)
}

extern "C" fn interface_version(_this: &Class, _sel: Sel) -> u32 {
    0
}

extern "C" fn interface_version_instance(_this: &Object, _sel: Sel) -> u32 {
    0
}

extern "C" fn description(this: &Class, _sel: Sel) -> *mut Object {
    let factory = class_name(this as *const Class);
    let map = registry().lock().expect("GUI registry lock poisoned");
    let Some(entry) = map.get(&factory) else {
        return std::ptr::null_mut();
    };
    unsafe {
        let title: *mut Object = msg_send![class!(NSString), alloc];
        let Ok(cstr) = CString::new(entry.config.description) else {
            return std::ptr::null_mut();
        };
        let title: *mut Object = msg_send![title, initWithUTF8String: cstr.as_ptr()];
        let title: *mut Object = msg_send![title, autorelease];
        title
    }
}

extern "C" fn description_instance(this: &Object, _sel: Sel) -> *mut Object {
    let cls: *const Class = unsafe { msg_send![this, class] };
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { description(&*cls, _sel) }
}

extern "C" fn draw_rect(this: &Object, _sel: Sel, rect: NSRect) {
    if !is_main_thread() {
        return;
    }
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(draw) = callbacks.draw {
            let result = catch_unwind(AssertUnwindSafe(|| {
                draw(
                    this as *const _ as *mut Object,
                    audio_unit(this),
                    user_data(this),
                    rect,
                );
            }));
        }
    }
}

extern "C" fn reshape(this: &Object, _sel: Sel) {
    if !is_main_thread() {
        return;
    }
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(reshape) = callbacks.reshape {
            let result = catch_unwind(AssertUnwindSafe(|| {
                reshape(
                    this as *const _ as *mut Object,
                    audio_unit(this),
                    user_data(this),
                );
            }));
        }
    }
}

extern "C" fn set_frame_size(this: &Object, _sel: Sel, size: NSSize) {
    unsafe {
        let superclass_ptr = view_superclass(this);
        let superclass = if superclass_ptr.is_null() {
            Class::get("NSView").unwrap_or(class!(NSView))
        } else {
            &*superclass_ptr
        };
        let _: () = msg_send![super(this, superclass), setFrameSize: size];
        // Only call update on NSOpenGLView (it doesn't exist on plain NSView)
        if is_opengl_view(this) {
            let _: () = msg_send![this, update];
        }
        set_view_preferred_size(this, size);
        let _: () = msg_send![this, invalidateIntrinsicContentSize];
        let _: () = msg_send![this, setNeedsDisplay: YES];
    }
}

extern "C" fn set_frame(this: &Object, _sel: Sel, rect: NSRect) {
    unsafe {
        let superclass_ptr = view_superclass(this);
        let superclass = if superclass_ptr.is_null() {
            Class::get("NSView").unwrap_or(class!(NSView))
        } else {
            &*superclass_ptr
        };
        let _: () = msg_send![super(this, superclass), setFrame: rect];
        // Only call update on NSOpenGLView (it doesn't exist on plain NSView)
        if is_opengl_view(this) {
            let _: () = msg_send![this, update];
        }
        set_view_preferred_size(this, rect.size);
        let _: () = msg_send![this, invalidateIntrinsicContentSize];
        let _: () = msg_send![this, setNeedsDisplay: YES];
    }
}

extern "C" fn intrinsic_content_size(_this: &Object, _sel: Sel) -> NSSize {
    if let Some(size) = view_preferred_size(_this) {
        size
    } else {
        NSSize {
            width: -1.0,
            height: -1.0,
        }
    }
}

extern "C" fn is_flipped(_this: &Object, _sel: Sel) -> BOOL {
    YES
}

extern "C" fn mouse_down(this: &Object, _sel: Sel, event: *mut Object) {
    if !is_main_thread() {
        return;
    }
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(cb) = callbacks.mouse_down {
            let (point, flags) = event_location(this, event);
            let result = catch_unwind(AssertUnwindSafe(|| {
                cb(
                    this as *const _ as *mut Object,
                    audio_unit(this),
                    user_data(this),
                    point,
                    flags,
                );
            }));
        }
    }
}

extern "C" fn mouse_dragged(this: &Object, _sel: Sel, event: *mut Object) {
    if !is_main_thread() {
        return;
    }
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(cb) = callbacks.mouse_dragged {
            let (point, flags) = event_location(this, event);
            let result = catch_unwind(AssertUnwindSafe(|| {
                cb(
                    this as *const _ as *mut Object,
                    audio_unit(this),
                    user_data(this),
                    point,
                    flags,
                );
            }));
        }
    }
}

extern "C" fn mouse_up(this: &Object, _sel: Sel, event: *mut Object) {
    if !is_main_thread() {
        return;
    }
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(cb) = callbacks.mouse_up {
            let (point, flags) = event_location(this, event);
            let result = catch_unwind(AssertUnwindSafe(|| {
                cb(
                    this as *const _ as *mut Object,
                    audio_unit(this),
                    user_data(this),
                    point,
                    flags,
                );
            }));
        }
    }
}

extern "C" fn key_down(this: &Object, _sel: Sel, event: *mut Object) {
    if !is_main_thread() {
        return;
    }
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(cb) = callbacks.key_down {
            let flags: u64 = unsafe { msg_send![event, modifierFlags] };
            let key_code: u16 = unsafe { msg_send![event, keyCode] };
            let result = catch_unwind(AssertUnwindSafe(|| {
                cb(
                    this as *const _ as *mut Object,
                    audio_unit(this),
                    user_data(this),
                    key_code,
                    flags,
                );
            }));
        }
    }
}

extern "C" fn accepts_first_responder(_this: &Object, _sel: Sel) -> bool {
    true
}

extern "C" fn view_did_move_to_window(this: &Object, _sel: Sel) {
    unsafe {
        let window: *mut Object = msg_send![this, window];
        if window.is_null() {
            let timer = view_timer(this);
            if !timer.is_null() {
                let _: () = msg_send![timer, invalidate];
                let _: () = msg_send![timer, release];
                set_view_timer(this, std::ptr::null_mut());
            }
            return;
        }

        let _: () = msg_send![window, makeFirstResponder: this];

        if view_timer(this).is_null() {
            let interval: f64 = 1.0 / 30.0;
            let nil: *mut Object = std::ptr::null_mut();
            let timer: *mut Object = msg_send![
                class!(NSTimer),
                scheduledTimerWithTimeInterval: interval
                target: this
                selector: sel!(au_tick:)
                userInfo: nil
                repeats: YES
            ];
            if !timer.is_null() {
                let _: () = msg_send![timer, retain];
                set_view_timer(this, timer);
            }
        }
    }
}

extern "C" fn au_tick(this: &Object, _sel: Sel, _timer: *mut Object) {
    if !is_main_thread() {
        return;
    }
    unsafe {
        let _: () = msg_send![this, setNeedsDisplay: YES];
    }
}

fn audio_unit(this: &Object) -> *mut c_void {
    unsafe {
        let instance = *this.get_ivar::<*mut c_void>("au_instance");
        if !instance.is_null() {
            instance
        } else {
            *this.get_ivar::<*mut c_void>("au_unit")
        }
    }
}

fn user_data(this: &Object) -> *mut c_void {
    unsafe { *this.get_ivar::<*mut c_void>("au_user_data") }
}

fn event_location(this: &Object, event: *mut Object) -> (NSPoint, u64) {
    unsafe {
        let window_point: NSPoint = msg_send![event, locationInWindow];
        let nil: *mut Object = std::ptr::null_mut();
        let view_point: NSPoint = msg_send![this, convertPoint: window_point fromView: nil];
        let flags: u64 = msg_send![event, modifierFlags];
        (view_point, flags)
    }
}

extern "C" fn dealloc(this: &Object, _sel: Sel) {
    if let Some(callbacks) = callbacks_for_view(this) {
        if let Some(cb) = callbacks.deinit {
            let result = catch_unwind(AssertUnwindSafe(|| {
                cb(this as *const _ as *mut Object, user_data(this));
            }));
        }
    }
    unsafe {
        let timer = view_timer(this);
        if !timer.is_null() {
            let _: () = msg_send![timer, invalidate];
            let _: () = msg_send![timer, release];
            set_view_timer(this, std::ptr::null_mut());
        }
    }
    unsafe {
        let superclass_ptr = view_superclass(this);
        let superclass = if superclass_ptr.is_null() {
            Class::get("NSView").unwrap_or(class!(NSView))
        } else {
            &*superclass_ptr
        };
        let _: () = msg_send![super(this, superclass), dealloc];
    }
}
