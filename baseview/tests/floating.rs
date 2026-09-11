//! Native owner/title readback; custom harness keeps AppKit on the main thread.
use baseview::{
    Event, EventStatus, Size, TransientParent, Window, WindowHandle, WindowHandler,
    WindowOpenOptions, WindowScalePolicy,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
struct Handler;
impl WindowHandler for Handler {
    fn on_frame(&mut self, _: &mut Window) {}
    fn on_event(&mut self, _: &mut Window, _: Event) -> EventStatus {
        EventStatus::Ignored
    }
}
fn options(title: &str) -> WindowOpenOptions {
    WindowOpenOptions::new(
        title,
        Size::new(240., 160.),
        WindowScalePolicy::ScaleFactor(1.),
    )
}
fn open(title: &str) -> WindowHandle {
    Window::open_floating(options(title), |_| Handler)
}
fn main() {
    if std::env::var_os("SUNMAO_NATIVE_FLOATING_TEST").is_none() {
        println!("NATIVE FLOATING SKIPPED: requires a desktop session");
        return;
    }
    native::initialize();
    let mut owner = open("Host one");
    let mut other = open("Host two");
    assert!(owner.is_open() && other.is_open());
    let parent = native::parent(&owner);
    for _ in 0..2 {
        let mut config = options("Track 3 — 合成器");
        config.transient_parent = Some(parent);
        let mut editor = Window::open_floating(config, |_| Handler);
        assert!(editor.is_open());
        native::verify(&editor, Some(&owner), "Track 3 — 合成器");
        assert!(editor.set_transient(native::parent(&other)));
        assert!(editor.set_title("Track 4 — é"));
        native::verify(&editor, Some(&other), "Track 4 — é");
        assert!(!editor.set_transient(native::parent(&editor)));
        assert!(
            !other.set_transient(native::parent(&editor)),
            "owner cycle accepted"
        );
        assert!(!editor.set_title("bad\0title"));
        native::verify(&editor, Some(&other), "Track 4 — é");
        let mut embedded = Window::open_parented(&owner, options("Embedded"), |_| Handler);
        assert!(embedded.is_open());
        assert!(!embedded.set_title("host title must survive"));
        assert!(!embedded.set_transient(parent));
        embedded.close();
        editor.close();
        native::pump();
        assert!(!editor.set_title("closed"));
        assert!(!editor.set_transient(parent));
        assert!(
            owner.is_open() && other.is_open(),
            "closing editor destroyed its host"
        );
        native::verify(&owner, None, "Host one");
        native::verify(&other, None, "Host two");
    }
    let mut plain = open("Fresh editor");
    native::verify(&plain, None, "Fresh editor");
    plain.close();
    owner.close();
    other.close();
    native::pump();
    println!("NATIVE FLOATING VERIFIED: owner/title readback, updates, cycle rejection, embedded isolation, close and reopen");
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use cocoa::{
        appkit::{NSApp, NSApplication},
        base::{id, nil},
        foundation::NSString,
    };
    use objc::{msg_send, sel, sel_impl};
    pub fn initialize() {
        unsafe {
            NSApp().finishLaunching();
        }
    }
    pub fn pump() {}
    fn view(handle: &WindowHandle) -> id {
        let RawWindowHandle::AppKit(raw) = handle.window_handle().unwrap().as_raw() else {
            panic!()
        };
        raw.ns_view.as_ptr() as id
    }
    fn window(handle: &WindowHandle) -> id {
        unsafe { msg_send![view(handle), window] }
    }
    pub fn parent(handle: &WindowHandle) -> TransientParent {
        TransientParent::AppKit(view(handle) as usize)
    }
    pub fn verify(handle: &WindowHandle, owner: Option<&WindowHandle>, title: &str) {
        unsafe {
            let window = window(handle);
            let actual: id = msg_send![window, parentWindow];
            assert_eq!(actual, owner.map(self::window).unwrap_or(nil));
            let name: id = msg_send![window, title];
            assert_eq!(
                std::ffi::CStr::from_ptr(name.UTF8String())
                    .to_str()
                    .unwrap(),
                title
            );
        }
    }
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use winapi::{shared::windef::HWND, um::winuser::*};
    pub fn initialize() {}
    pub fn pump() {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    fn window(handle: &WindowHandle) -> HWND {
        let RawWindowHandle::Win32(raw) = handle.window_handle().unwrap().as_raw() else {
            panic!()
        };
        raw.hwnd.get() as HWND
    }
    pub fn parent(handle: &WindowHandle) -> TransientParent {
        TransientParent::Win32(window(handle) as isize)
    }
    pub fn verify(handle: &WindowHandle, owner: Option<&WindowHandle>, title: &str) {
        unsafe {
            let hwnd = window(handle);
            assert_eq!(
                GetWindow(hwnd, GW_OWNER),
                owner.map(window).unwrap_or(std::ptr::null_mut())
            );
            let mut name = vec![0u16; GetWindowTextLengthW(hwnd) as usize + 1];
            let len = GetWindowTextW(hwnd, name.as_mut_ptr(), name.len() as i32);
            assert_eq!(String::from_utf16(&name[..len as usize]).unwrap(), title);
        }
    }
}

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    pub fn initialize() {}
    pub fn pump() {}
    fn window(handle: &WindowHandle) -> u32 {
        match handle.window_handle().unwrap().as_raw() {
            RawWindowHandle::Xlib(raw) => raw.window as u32,
            RawWindowHandle::Xcb(raw) => raw.window.get(),
            _ => panic!("requires X11"),
        }
    }
    pub fn parent(handle: &WindowHandle) -> TransientParent {
        TransientParent::X11(window(handle))
    }
    pub fn verify(handle: &WindowHandle, owner: Option<&WindowHandle>, title: &str) {
        let (connection, _) = x11rb::connect(None).unwrap();
        let id = window(handle);
        let property = connection
            .get_property(
                false,
                id,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            property.value32().and_then(|mut values| values.next()),
            owner.map(window)
        );
        let name = connection
            .intern_atom(false, b"_NET_WM_NAME")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let utf8 = connection
            .intern_atom(false, b"UTF8_STRING")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let value = connection
            .get_property(false, id, name, utf8, 0, 4096)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(String::from_utf8(value.value).unwrap(), title);
    }
}
