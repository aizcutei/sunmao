#![cfg(target_os = "linux")]

use baseview::{
    Event, EventStatus, Size, Window, WindowHandler, WindowOpenOptions, WindowScalePolicy,
};
use keyboard_types::{Code, Key, KeyState, KeyboardEvent, Modifiers};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{process::Command, sync::mpsc, time::Duration};
use x11rb::protocol::{
    xproto::{ConnectionExt, InputFocus, KEY_PRESS_EVENT, KEY_RELEASE_EVENT},
    xtest::ConnectionExt as _,
};

struct Handler(mpsc::Sender<Event>);
impl WindowHandler for Handler {
    fn on_frame(&mut self, _: &mut Window) {}
    fn on_event(&mut self, _: &mut Window, event: Event) -> EventStatus {
        self.0.send(event).unwrap();
        EventStatus::Ignored
    }
}
fn layout(name: &str, variant: &str) {
    assert!(Command::new("setxkbmap")
        .args(["-layout", name, "-variant", variant, "-option", ""])
        .status()
        .unwrap()
        .success());
}
fn next_key(events: &mpsc::Receiver<Event>, state: KeyState) -> KeyboardEvent {
    loop {
        if let Event::Keyboard(event) = events
            .recv_timeout(Duration::from_secs(5))
            .expect("native X11 keyboard event")
        {
            if event.state == state {
                return event;
            }
        }
    }
}
#[test]
fn native_x11_layout_changes_and_compose_reach_the_window() {
    if std::env::var_os("SUNMAO_X11_KEYBOARD_TEST").is_none() {
        println!("X11 KEYBOARD SKIPPED: requires an isolated X server");
        return;
    }
    assert!(std::env::var_os("DISPLAY").is_some());
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    let (conn, _) = x11rb::connect(None).unwrap();
    layout("de", "");
    let (tx, rx) = mpsc::channel();
    let mut window = Window::open_floating(
        WindowOpenOptions::new(
            "X11 international keyboard acceptance",
            Size::new(160.0, 100.0),
            WindowScalePolicy::ScaleFactor(1.0),
        ),
        move |_| Handler(tx),
    );
    let id = match window.window_handle().unwrap().as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => panic!("expected X11 window"),
    };
    conn.set_input_focus(InputFocus::PARENT, id, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    let press = |code| {
        conn.xtest_fake_input(KEY_PRESS_EVENT, code, 0, 0, 0, 0, 0)
            .unwrap()
            .check()
            .unwrap();
    };
    let release = |code| {
        conn.xtest_fake_input(KEY_RELEASE_EVENT, code, 0, 0, 0, 0, 0)
            .unwrap()
            .check()
            .unwrap();
    };
    let tap = |code| {
        press(code);
        release(code);
    };
    // Physical evdev Y (21 + X11 offset 8) on a German keyboard yields z.
    tap(29);
    let event = next_key(&rx, KeyState::Down);
    assert_eq!(event.code, Code::KeyY);
    assert_eq!(event.key, Key::Character("z".into()));
    // Shift and AltGr are taken from the server's effective event state.
    press(50);
    tap(29);
    release(50);
    assert_eq!(next_key(&rx, KeyState::Down).key, Key::Shift);
    let event = next_key(&rx, KeyState::Down);
    assert_eq!(event.key, Key::Character("Z".into()));
    assert!(event.modifiers.contains(Modifiers::SHIFT));
    press(108);
    tap(24); // German AltGr+Q -> @
    release(108);
    assert_eq!(next_key(&rx, KeyState::Down).key, Key::AltGraph);
    assert_eq!(
        next_key(&rx, KeyState::Down).key,
        Key::Character("@".into())
    );
    // Change the server keymap while the same editor stays open.
    layout("us", "intl");
    tap(48); // dead acute
    let event = next_key(&rx, KeyState::Down);
    assert_eq!(event.key, Key::Dead);
    assert!(event.is_composing);
    tap(26); // e commits é, rather than leaking the dead key or raw e
    let event = next_key(&rx, KeyState::Down);
    assert_eq!(event.code, Code::KeyE);
    assert_eq!(event.key, Key::Character("é".into()));
    assert!(!event.is_composing);
    // Losing focus must discard an unfinished compose sequence.
    tap(48);
    assert!(next_key(&rx, KeyState::Down).is_composing);
    conn.set_input_focus(InputFocus::POINTER_ROOT, x11rb::NONE, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    conn.set_input_focus(InputFocus::PARENT, id, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    tap(26);
    assert_eq!(
        next_key(&rx, KeyState::Down).key,
        Key::Character("e".into())
    );
    window.close();
    println!("X11 KEYBOARD VERIFIED: native physical keys, German layout, Shift, AltGr, live layout change, compose and focus reset");
}
