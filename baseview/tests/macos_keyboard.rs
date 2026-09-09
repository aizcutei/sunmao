// AppKit requires the process main thread, so this test uses a custom harness.
#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    if std::env::var_os("SUNMAO_MACOS_KEYBOARD_TEST").is_none() {
        println!("MACOS KEYBOARD SKIPPED: requires a desktop session");
        return;
    }
    native::run();
}

#[cfg(target_os = "macos")]
mod native {
    use baseview::{
        Event, EventStatus, Size, Window, WindowHandler, WindowOpenOptions, WindowScalePolicy,
    };
    use cocoa::appkit::{NSApp, NSApplication};
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSAutoreleasePool, NSString};
    use core_foundation::base::CFRelease;
    use keyboard_types::{Code, Key, KeyboardEvent, Modifiers};
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{
        ffi::{c_void, CStr},
        sync::mpsc,
    };

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceCreate(state: i32) -> *const c_void;
        fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool)
            -> *const c_void;
        fn CGEventSetFlags(event: *const c_void, flags: u64);
    }
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
        fn TISCopyCurrentKeyboardInputSource() -> *const c_void;
        fn TISCreateInputSourceList(filter: *const c_void, include_all: u8) -> *const c_void;
        fn TISSelectInputSource(source: *const c_void) -> i32;
        fn TISGetInputSourceProperty(
            source: *const c_void,
            property: *const c_void,
        ) -> *const c_void;
        static kTISPropertyInputSourceID: *const c_void;
    }
    struct Owned(*const c_void);
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) }
            }
        }
    }
    // TIS selection affects the desktop session. Always restore the precise
    // original input source (including an IME), also during assertion unwinding.
    struct RestoreInputSource(Owned);
    impl RestoreInputSource {
        unsafe fn select_test_layout() -> Self {
            let original = Owned(TISCopyCurrentKeyboardInputSource());
            assert!(!original.0.is_null());
            let restore = Self(original);
            let sources = Owned(TISCreateInputSourceList(std::ptr::null(), 0));
            assert!(!sources.0.is_null());
            let array = sources.0 as core_foundation::array::CFArrayRef;
            for index in 0..core_foundation::array::CFArrayGetCount(array) {
                let source = core_foundation::array::CFArrayGetValueAtIndex(array, index);
                let name = TISGetInputSourceProperty(source, kTISPropertyInputSourceID) as id;
                if name.is_null() {
                    continue;
                }
                let name = CStr::from_ptr(name.UTF8String()).to_str().unwrap();
                if matches!(name, "com.apple.keylayout.US" | "com.apple.keylayout.ABC") {
                    assert_eq!(TISSelectInputSource(source), 0, "select test input source");
                    return restore;
                }
            }
            panic!("US or ABC keyboard layout must be enabled for this test");
        }
    }
    impl Drop for RestoreInputSource {
        fn drop(&mut self) {
            let status = unsafe { TISSelectInputSource(self.0 .0) };
            if status != 0 {
                eprintln!("failed to restore input source: {}", status);
                // Do not print a success marker after a failed restoration.
                std::process::exit(1);
            }
        }
    }
    struct Handler(mpsc::Sender<KeyboardEvent>);
    impl WindowHandler for Handler {
        fn on_frame(&mut self, _: &mut Window) {}
        fn on_event(&mut self, _: &mut Window, event: Event) -> EventStatus {
            if let Event::Keyboard(key) = event {
                self.0.send(key).unwrap();
            }
            EventStatus::Captured
        }
    }
    pub fn run() {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let app = NSApp();
            app.finishLaunching();
            let restore = RestoreInputSource::select_test_layout();
            let layout = Owned(TISCopyCurrentKeyboardLayoutInputSource());
            assert!(!layout.0.is_null());
            let name = TISGetInputSourceProperty(layout.0, kTISPropertyInputSourceID) as id;
            assert!(!name.is_null());
            let name = CStr::from_ptr(name.UTF8String()).to_str().unwrap();
            assert!(
                matches!(name, "com.apple.keylayout.US" | "com.apple.keylayout.ABC"),
                "requires US/ABC layout, got {}",
                name
            );
            let source = Owned(CGEventSourceCreate(-1));
            assert!(!source.0.is_null());
            let (tx, rx) = mpsc::channel();
            let mut window = Window::open_floating(
                WindowOpenOptions::new(
                    "macOS international keyboard acceptance",
                    Size::new(160.0, 100.0),
                    WindowScalePolicy::ScaleFactor(1.0),
                ),
                move |_| Handler(tx),
            );
            let view = match window.window_handle().unwrap().as_raw() {
                RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as id,
                _ => panic!("expected AppKit view"),
            };
            let send = |code, flags, down| {
                let cg = Owned(CGEventCreateKeyboardEvent(source.0, code, down));
                assert!(!cg.0.is_null());
                CGEventSetFlags(cg.0, flags);
                // The OS computes Unicode from physical keys. No Unicode
                // payload is supplied by this test. Dispatch to the real
                // baseview NSView, then observe its public WindowHandler.
                let event: id = msg_send![class!(NSEvent), eventWithCGEvent: cg.0];
                assert!(!event.is_null());
                if down {
                    let _: () = msg_send![view, keyDown: event];
                } else {
                    let _: () = msg_send![view, keyUp: event];
                }
                rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()
            };
            let press = |code, flags| {
                let down = send(code, flags, true);
                let up = send(code, 0, false);
                assert_eq!(up.state, keyboard_types::KeyState::Up);
                assert_eq!(up.key, down.key, "release preserves logical key");
                down
            };
            let a = press(0, 0);
            assert_eq!(a.code, Code::KeyA);
            assert_eq!(a.key, Key::Character("a".into()));
            let international = press(0, 1 << 19);
            assert_eq!(international.key, Key::Character("å".into()));
            assert!(international.modifiers.contains(Modifiers::ALT));
            let dead = press(14, 1 << 19);
            assert_eq!(dead.key, Key::Dead);
            assert!(dead.is_composing);
            let composed = press(14, 0);
            assert_eq!(composed.code, Code::KeyE);
            assert_eq!(composed.key, Key::Character("é".into()));
            assert!(!composed.is_composing);
            let dead = press(14, 1 << 19);
            assert_eq!(dead.key, Key::Dead);
            let resigned: cocoa::base::BOOL = msg_send![view, resignFirstResponder];
            assert_eq!(resigned, cocoa::base::YES);
            let focused: cocoa::base::BOOL = msg_send![view, becomeFirstResponder];
            assert_eq!(focused, cocoa::base::YES);
            let reset = press(14, 0);
            assert_eq!(reset.key, Key::Character("e".into()));
            assert!(!reset.is_composing);
            let dead = press(14, 1 << 19);
            assert_eq!(dead.key, Key::Dead);
            assert_eq!(press(53, 0).key, Key::Escape);
            assert_eq!(press(14, 0).key, Key::Character("e".into()));
            let held = send(0, 0, true);
            let resigned: cocoa::base::BOOL = msg_send![view, resignFirstResponder];
            assert_eq!(resigned, cocoa::base::YES);
            let released = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert_eq!(released.state, keyboard_types::KeyState::Up);
            assert_eq!(released.key, held.key);
            assert!(rx.try_recv().is_err(), "one release per held key");
            window.close();

            drop(source);
            drop(layout);
            drop(restore);
            println!("MACOS KEYBOARD VERIFIED: native layout, Option character, dead key, composed character and focus reset through NSView");
            pool.drain();
        }
    }
}
