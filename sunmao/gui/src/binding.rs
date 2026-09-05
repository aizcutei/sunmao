//! Two-way parameter binding.
//!
//! Before this existed, every editor hand-wrote the same three things per
//! control: pull the host value in before painting, push the new value out
//! after an edit, and bracket drags with begin/end-edit so automation records
//! one gesture instead of a hundred. [`ParamBinder`] does all three for a whole
//! widget tree, so an editor declares its controls and nothing else.

use crate::{Event, MouseButton, ParameterWidget, Stack};
use std::sync::Arc;

/// The editor's view onto host parameters.
///
/// `sunmao_gui` deliberately does not depend on `sunmao_core`, so this is the
/// minimal surface a binder needs. The facade adapts `ViewContext` to it.
pub trait ParamHost: Send + Sync {
    /// Current normalized value, or `None` if the host does not know this id.
    fn get(&self, id: &str) -> Option<f32>;
    /// Push a user-driven normalized value.
    fn set(&self, id: &str, value: f32);
    /// Open an automation gesture.
    fn begin_edit(&self, id: &str);
    /// Close an automation gesture.
    fn end_edit(&self, id: &str);
}

/// Drives a widget tree against a [`ParamHost`].
///
/// The tree's children must be `Send` because editor state crosses to the GUI
/// thread as one object; see [`Stack`].
///
/// Call [`ParamBinder::sync`] before painting and [`ParamBinder::handle_event`]
/// instead of the stack's own `handle_event`.
pub struct ParamBinder {
    host: Arc<dyn ParamHost>,
    /// Parameter id of the control currently mid-gesture, if any.
    editing: Option<String>,
}

impl ParamBinder {
    pub fn new(host: Arc<dyn ParamHost>) -> Self {
        Self {
            host,
            editing: None,
        }
    }

    /// Pull every bound control's value from the host.
    ///
    /// Skipped for the control currently being dragged: the host's value can
    /// lag a gesture by a block, and applying it mid-drag makes the control
    /// stutter backwards under the pointer.
    pub fn sync(&self, stack: &mut Stack) {
        for child in stack.iter_mut() {
            let Some(param) = child.as_parameter() else {
                continue;
            };
            if self.editing.as_deref() == Some(param.param_id()) {
                continue;
            }
            let Some(value) = self.host.get(param.param_id()) else {
                continue;
            };
            // `set_value` is the non-notifying path, so this cannot loop back
            // into `set` and fight the host for the value.
            param.set_value(value);
        }
    }

    /// Dispatch an event and publish any resulting parameter change.
    ///
    /// Returns whether a widget consumed the event.
    pub fn handle_event(&mut self, stack: &mut Stack, event: &Event) -> bool {
        // Snapshot before/after values so a change is detected wherever it came
        // from, rather than requiring each widget to report through a callback.
        let before: Vec<(String, f32)> = collect_values(stack);
        let consumed = stack.handle_event(event);
        let after = collect_values(stack);

        if let Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            ..
        } = event
        {
            if consumed && self.editing.is_none() {
                if let Some(id) = parameter_at(stack, *x, *y) {
                    self.host.begin_edit(&id);
                    self.editing = Some(id);
                }
            }
        }

        // A keyboard nudge is a complete gesture on its own: there is no
        // press/release to bracket it with, so wrap each change in its own
        // begin/end. Without this the host records an automation point with no
        // surrounding gesture, which some DAWs discard.
        let keyboard_edit = matches!(event, Event::KeyDown { .. }) && self.editing.is_none();

        for ((id, old), (_, new)) in before.iter().zip(after.iter()) {
            if old != new {
                if keyboard_edit {
                    self.host.begin_edit(id);
                }
                self.host.set(id, *new);
                if keyboard_edit {
                    self.host.end_edit(id);
                }
            }
        }

        if matches!(
            event,
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            }
        ) {
            if let Some(id) = self.editing.take() {
                self.host.end_edit(&id);
            }
        }

        consumed
    }

    /// Parameter id of the control currently mid-gesture.
    pub fn editing(&self) -> Option<&str> {
        self.editing.as_deref()
    }
}

fn collect_values(stack: &mut Stack) -> Vec<(String, f32)> {
    stack
        .iter_mut()
        .filter_map(|child| {
            child
                .as_parameter()
                .map(|param| (param.param_id().to_string(), param.value()))
        })
        .collect()
}

/// Parameter id of the topmost control under a point.
fn parameter_at(stack: &mut Stack, x: f32, y: f32) -> Option<String> {
    let mut found = None;
    for child in stack.iter_mut() {
        if !child.hit_test(x, y) {
            continue;
        }
        if let Some(param) = child.as_parameter() {
            // Later children paint on top, so keep overwriting.
            found = Some(param.param_id().to_string());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Modifiers, Rect, Toggle};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingHost {
        values: Mutex<Vec<(String, f32)>>,
        log: Mutex<Vec<String>>,
    }

    impl RecordingHost {
        fn seed(&self, id: &str, value: f32) {
            self.values.lock().unwrap().push((id.to_string(), value));
        }
        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl ParamHost for RecordingHost {
        fn get(&self, id: &str) -> Option<f32> {
            self.values
                .lock()
                .unwrap()
                .iter()
                .find(|(name, _)| name == id)
                .map(|(_, value)| *value)
        }
        fn set(&self, id: &str, value: f32) {
            self.log.lock().unwrap().push(format!("set {id}={value}"));
            let mut values = self.values.lock().unwrap();
            if let Some(slot) = values.iter_mut().find(|(name, _)| name == id) {
                slot.1 = value;
            } else {
                values.push((id.to_string(), value));
            }
        }
        fn begin_edit(&self, id: &str) {
            self.log.lock().unwrap().push(format!("begin {id}"));
        }
        fn end_edit(&self, id: &str) {
            self.log.lock().unwrap().push(format!("end {id}"));
        }
    }

    fn stack_with_toggle() -> Stack {
        let mut stack =
            Column::new().child(Toggle::new("bypass").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)));
        stack.layout(Rect::new(0.0, 0.0, 40.0, 40.0));
        stack
    }

    fn down(x: f32, y: f32) -> Event {
        Event::MouseDown {
            x,
            y,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        }
    }

    fn up(x: f32, y: f32) -> Event {
        Event::MouseUp {
            x,
            y,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn a_click_produces_one_gesture_around_one_value_change() {
        let host = Arc::new(RecordingHost::default());
        host.seed("bypass", 0.0);
        let mut binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();

        binder.handle_event(&mut stack, &down(5.0, 5.0));
        binder.handle_event(&mut stack, &up(5.0, 5.0));

        assert_eq!(
            host.log(),
            vec![
                "begin bypass".to_string(),
                "set bypass=1".to_string(),
                "end bypass".to_string(),
            ],
            "the edit was not bracketed by exactly one gesture"
        );
    }

    #[test]
    fn syncing_from_the_host_never_writes_back() {
        let host = Arc::new(RecordingHost::default());
        host.seed("bypass", 1.0);
        let binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();

        binder.sync(&mut stack);
        let value = stack
            .child_at_mut(0)
            .unwrap()
            .as_parameter()
            .unwrap()
            .value();
        assert_eq!(value, 1.0, "the host value did not reach the widget");
        assert!(
            host.log().is_empty(),
            "sync echoed back to the host: {:?}",
            host.log()
        );
    }

    #[test]
    fn a_drag_in_progress_is_not_overwritten_by_a_stale_host_value() {
        let host = Arc::new(RecordingHost::default());
        host.seed("bypass", 0.0);
        let mut binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();

        // Press starts a gesture; the widget is now authoritative.
        binder.handle_event(&mut stack, &down(5.0, 5.0));
        assert_eq!(binder.editing(), Some("bypass"));

        // The host still reports the old value for a block. Syncing must not
        // drag the control backwards under the pointer.
        binder.sync(&mut stack);
        binder.handle_event(&mut stack, &up(5.0, 5.0));
        assert_eq!(binder.editing(), None);
        assert_eq!(host.get("bypass"), Some(1.0));
    }

    #[test]
    fn a_click_that_hits_nothing_opens_no_gesture() {
        let host = Arc::new(RecordingHost::default());
        host.seed("bypass", 0.0);
        let mut binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();

        binder.handle_event(&mut stack, &down(500.0, 500.0));
        binder.handle_event(&mut stack, &up(500.0, 500.0));
        assert!(binder.editing().is_none());
        assert!(
            host.log().is_empty(),
            "stray click produced {:?}",
            host.log()
        );
    }

    #[test]
    fn a_parameter_the_host_does_not_know_is_left_alone() {
        let host = Arc::new(RecordingHost::default());
        // Nothing seeded: `get` returns None for every id.
        let binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();
        stack
            .child_at_mut(0)
            .unwrap()
            .as_parameter()
            .unwrap()
            .set_value(1.0);

        binder.sync(&mut stack);
        let value = stack
            .child_at_mut(0)
            .unwrap()
            .as_parameter()
            .unwrap()
            .value();
        assert_eq!(value, 1.0, "an unknown id reset the control");
    }

    #[test]
    fn a_keyboard_nudge_is_a_gesture_of_its_own() {
        let host = Arc::new(RecordingHost::default());
        host.seed("bypass", 0.0);
        let mut binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();
        stack.set_focus(Some(0));

        binder.handle_event(
            &mut stack,
            &Event::KeyDown {
                key: crate::KeyCode::Space,
                modifiers: Modifiers::default(),
            },
        );

        // A key press has no release to bracket it, so the binder must supply
        // the whole gesture itself. Some DAWs discard an automation point that
        // arrives outside one.
        assert_eq!(
            host.log(),
            vec![
                "begin bypass".to_string(),
                "set bypass=1".to_string(),
                "end bypass".to_string(),
            ]
        );
    }

    #[test]
    fn a_keystroke_that_changes_nothing_records_no_gesture() {
        let host = Arc::new(RecordingHost::default());
        host.seed("bypass", 0.0);
        let mut binder = ParamBinder::new(host.clone());
        let mut stack = stack_with_toggle();
        stack.set_focus(Some(0));

        binder.handle_event(
            &mut stack,
            &Event::KeyDown {
                key: crate::KeyCode::F1,
                modifiers: Modifiers::default(),
            },
        );
        assert!(
            host.log().is_empty(),
            "an inert key logged {:?}",
            host.log()
        );
    }
}
