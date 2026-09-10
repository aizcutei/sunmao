// Uses a real Win32 window, message queue, keyboard layout and TranslateMessage.
// No WM_CHAR message or Unicode payload is supplied by the test.
#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() {
    if std::env::var_os("SUNMAO_WINDOWS_KEYBOARD_TEST").is_none() {
        println!("WINDOWS KEYBOARD SKIPPED: native acceptance requires opt-in");
        return;
    }
    unsafe { native::run() }
}

#[cfg(target_os = "windows")]
mod native {
    use baseview::{
        Event, EventStatus, Size, Window, WindowHandler, WindowOpenOptions, WindowScalePolicy,
    };
    use keyboard_types::{Code, Key, KeyState, KeyboardEvent, Modifiers};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{
        mem, ptr,
        sync::mpsc,
        time::{Duration, Instant},
    };
    use winapi::{
        shared::{minwindef::HKL, windef::HWND},
        um::winuser::*,
    };

    struct Restore {
        layout: HKL,
        state: [u8; 256],
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                assert!(!ActivateKeyboardLayout(self.layout, 0).is_null());
                assert_ne!(SetKeyboardState(self.state.as_mut_ptr()), 0);
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
    unsafe fn pump() {
        let start = Instant::now();
        let mut msg = mem::zeroed();
        while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "message pump did not settle"
            );
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    unsafe fn press(
        hwnd: HWND,
        layout: HKL,
        scan: u32,
        shift: bool,
        rx: &mpsc::Receiver<KeyboardEvent>,
    ) -> KeyboardEvent {
        let mut state = [0u8; 256];
        if shift {
            state[VK_SHIFT as usize] = 0x80;
            state[VK_LSHIFT as usize] = 0x80;
        }
        assert_ne!(SetKeyboardState(state.as_mut_ptr()), 0);
        let vk = MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, layout);
        assert_ne!(vk, 0);
        let bits = ((scan as isize) << 16) | 1;
        assert_ne!(PostMessageW(hwnd, WM_KEYDOWN, vk as usize, bits), 0);
        pump();
        let down = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(down.state, KeyState::Down);
        assert!(rx.try_recv().is_err(), "one logical event per press");
        assert_ne!(
            PostMessageW(hwnd, WM_KEYUP, vk as usize, bits | 0xC0000000u32 as isize),
            0
        );
        pump();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap().state,
            KeyState::Up
        );
        assert!(rx.try_recv().is_err(), "one release per press");
        down
    }
    pub unsafe fn run() {
        let mut restore = Restore {
            layout: GetKeyboardLayout(0),
            state: [0; 256],
        };
        assert_ne!(GetKeyboardState(restore.state.as_mut_ptr()), 0);
        let german: Vec<u16> = "00000407\0".encode_utf16().collect();
        let layout = LoadKeyboardLayoutW(german.as_ptr(), 0);
        assert!(!layout.is_null(), "German layout must be available");
        assert!(!ActivateKeyboardLayout(layout, 0).is_null());
        assert_eq!(GetKeyboardLayout(0), layout);
        let (tx, rx) = mpsc::channel();
        let mut window = Window::open_floating(
            WindowOpenOptions::new(
                "Windows international keyboard acceptance",
                Size::new(160.0, 100.0),
                WindowScalePolicy::ScaleFactor(1.0),
            ),
            move |_| Handler(tx),
        );
        let hwnd = match window.window_handle().unwrap().as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as HWND,
            _ => panic!("expected Win32"),
        };
        pump();
        let z = press(hwnd, layout, 0x15, false, &rx);
        assert_eq!(z.code, Code::KeyY);
        assert_eq!(z.key, Key::Character("z".into()));
        assert_eq!(
            press(hwnd, layout, 0x1a, false, &rx).key,
            Key::Character("ü".into())
        );
        let upper = press(hwnd, layout, 0x1a, true, &rx);
        assert_eq!(upper.key, Key::Character("Ü".into()));
        assert!(upper.modifiers.contains(Modifiers::SHIFT));
        assert_eq!(press(hwnd, layout, 0x0d, false, &rx).key, Key::Dead);
        assert_eq!(
            press(hwnd, layout, 0x12, false, &rx).key,
            Key::Character("é".into())
        );
        window.close();
        pump();
        drop(window);
        drop(restore);
        println!("WINDOWS KEYBOARD VERIFIED: physical keys, German layout, Shift and native dead-key composition through the window message hook");
    }
}
