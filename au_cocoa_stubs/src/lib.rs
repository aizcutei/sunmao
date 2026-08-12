//! Link the Objective-C Cocoa classes required by AU GUI bundles.

/// Keep the static archive and its Objective-C class metadata in the final AU
/// cdylib. The call is intentionally in the AU GUI registration path, so an
/// ordinary host or CLAP/VST3 plugin never pulls these classes in.
#[cfg(target_os = "macos")]
#[inline(never)]
pub fn ensure_linked() {
    use std::ffi::c_void;

    unsafe extern "C" {
        #[link_name = "OBJC_CLASS_$_SunmaoAUCocoaViewFactoryAuto"]
        static FACTORY: *const c_void;
        #[link_name = "OBJC_CLASS_$_SunmaoAUCocoaViewAuto"]
        static VIEW: *const c_void;
    }

    unsafe {
        std::ptr::read_volatile(&FACTORY);
        std::ptr::read_volatile(&VIEW);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_linked() {}
