//! Per-seat pointer lifetime and ordered frame delivery.
use keyboard_types::Modifiers;
use wayland_client::protocol::{wl_pointer, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use super::window::OpenState;
use crate::{Event, MouseButton, MouseEvent, Point, ScrollDelta};

pub(super) struct Seat {
    seat: wl_seat::WlSeat,
    pointer: Option<wl_pointer::WlPointer>,
    frame: PointerFrame,
}

impl Seat {
    pub(super) fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            seat,
            pointer: None,
            frame: PointerFrame::default(),
        }
    }

    pub(super) fn remove_pointer(&mut self, events: &mut Vec<Event>) {
        self.frame.cancel(events);
        if let Some(pointer) = self.pointer.take() {
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        if let Some(pointer) = self.pointer.take() {
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
        if self.seat.version() >= 5 {
            self.seat.release();
        }
    }
}

#[derive(Default)]
struct PointerFrame {
    pending: Vec<MouseEvent>,
    pressed: Vec<MouseButton>,
    entered: bool,
}

impl PointerFrame {
    fn enter(&mut self, x: f64, y: f64) {
        self.entered = true;
        self.pending.push(MouseEvent::CursorEntered);
        // Enter carries the position even if no subsequent motion occurs.
        self.motion(x, y);
    }

    fn motion(&mut self, x: f64, y: f64) {
        self.pending.push(MouseEvent::CursorMoved {
            position: Point::new(x, y),
            modifiers: Modifiers::empty(),
        });
    }

    fn button(&mut self, code: u32, pressed: bool) {
        // Linux input-event-codes.h BTN_MOUSE group. Do not narrow unrelated
        // button codes into u8 and accidentally turn them into a left click.
        let button = match code {
            0x110 => MouseButton::Left,
            0x111 => MouseButton::Right,
            0x112 => MouseButton::Middle,
            0x113 | 0x116 => MouseButton::Back,
            0x114 | 0x115 => MouseButton::Forward,
            0x117..=0x11f => MouseButton::Other((code - 0x110) as u8),
            _ => return,
        };
        if pressed {
            if !self.pressed.contains(&button) {
                self.pressed.push(button);
            }
            self.pending.push(MouseEvent::ButtonPressed {
                button,
                modifiers: Modifiers::empty(),
            });
        } else {
            self.pressed.retain(|value| *value != button);
            self.pending.push(MouseEvent::ButtonReleased {
                button,
                modifiers: Modifiers::empty(),
            });
        }
    }

    fn finish(&mut self, events: &mut Vec<Event>) {
        events.extend(self.pending.drain(..).map(Event::Mouse));
    }

    fn cancel(&mut self, events: &mut Vec<Event>) {
        self.finish(events);
        for button in self.pressed.drain(..) {
            events.push(Event::Mouse(MouseEvent::ButtonReleased {
                button,
                modifiers: Modifiers::empty(),
            }));
        }
        if self.entered {
            events.push(Event::Mouse(MouseEvent::CursorLeft));
            self.entered = false;
        }
    }
}

impl Dispatch<wl_seat::WlSeat, u32> for OpenState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        name: &u32,
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let Some(input) = state.seats.get_mut(name) else {
            return;
        };
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Pointer) {
                if input.pointer.is_none() {
                    input.pointer = Some(seat.get_pointer(handle, *name));
                }
            } else {
                input.remove_pointer(&mut state.events);
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for OpenState {
    fn event(
        state: &mut Self,
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(input) = state.seats.get_mut(name) else {
            return;
        };
        if input.pointer.as_ref() != Some(pointer) {
            return;
        }
        match event {
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            } => input.frame.enter(surface_x, surface_y),
            wl_pointer::Event::Leave { .. } => {
                input.frame.pending.push(MouseEvent::CursorLeft);
                input.frame.entered = false;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => input.frame.motion(surface_x, surface_y),
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(value),
                ..
            } => {
                input
                    .frame
                    .button(button, value == wl_pointer::ButtonState::Pressed);
            }
            wl_pointer::Event::Axis {
                axis: WEnum::Value(axis),
                value,
                ..
            } => {
                // baseview uses positive-up/positive-right scroll; Wayland's
                // vertical axis is positive-down, horizontal is positive-right.
                let (x, y) = match axis {
                    wl_pointer::Axis::VerticalScroll => (0.0, -value as f32),
                    wl_pointer::Axis::HorizontalScroll => (value as f32, 0.0),
                    _ => return,
                };
                input.frame.pending.push(MouseEvent::WheelScrolled {
                    delta: ScrollDelta::Pixels { x, y },
                    modifiers: Modifiers::empty(),
                });
            }
            wl_pointer::Event::Frame => input.frame.finish(&mut state.events),
            // AxisDiscrete supplements Axis: consuming both would double-scroll.
            _ => {}
        }
        if pointer.version() < 5 {
            input.frame.finish(&mut state.events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_then_click_preserves_coordinates_and_frame_order() {
        let mut frame = PointerFrame::default();
        frame.enter(12.5, 9.25);
        frame.button(0x110, true);
        let mut events = Vec::new();
        frame.finish(&mut events);
        assert!(matches!(events[0], Event::Mouse(MouseEvent::CursorEntered)));
        assert!(
            matches!(events[1], Event::Mouse(MouseEvent::CursorMoved { position, .. }) if position == Point::new(12.5, 9.25))
        );
        assert!(matches!(
            events[2],
            Event::Mouse(MouseEvent::ButtonPressed {
                button: MouseButton::Left,
                ..
            })
        ));
        frame.finish(&mut events);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn removing_a_pointer_releases_its_drag_once() {
        let mut frame = PointerFrame::default();
        frame.enter(1.0, 2.0);
        frame.button(0x111, true);
        let mut events = Vec::new();
        frame.cancel(&mut events);
        assert!(matches!(
            events[3],
            Event::Mouse(MouseEvent::ButtonReleased {
                button: MouseButton::Right,
                ..
            })
        ));
        assert!(matches!(events[4], Event::Mouse(MouseEvent::CursorLeft)));
        frame.cancel(&mut events);
        assert_eq!(events.len(), 5);
    }

    proptest::proptest! {
        #[test]
        fn removal_never_leaves_a_button_held(
            actions in proptest::collection::vec((0x110_u32..0x120, proptest::bool::ANY), 0..128)
        ) {
            let mut frame = PointerFrame::default();
            let mut events = Vec::new();
            frame.enter(10.0, 10.0);
            for (code, pressed) in actions {
                frame.button(code, pressed);
            }
            frame.cancel(&mut events);
            let mut held = Vec::new();
            for event in &events {
                match event {
                    Event::Mouse(MouseEvent::ButtonPressed { button, .. }) => {
                        if !held.contains(button) { held.push(*button); }
                    }
                    Event::Mouse(MouseEvent::ButtonReleased { button, .. }) => {
                        held.retain(|value| value != button);
                    }
                    _ => {}
                }
            }
            proptest::prop_assert!(held.is_empty());
            let count = events.len();
            frame.cancel(&mut events);
            proptest::prop_assert_eq!(events.len(), count);
        }
    }
}
