//! Does a screen reader actually see this editor?
//!
//! Every other accessibility test in the tree checks a data structure. This one
//! checks the thing that matters: it opens a real window, then asks **UI
//! Automation** — the same API a screen reader uses — what is inside it.
//!
//! Windows only, and not because the other platforms are less important. UIA
//! needs no special permission, while macOS's AXUIElement is gated behind TCC
//! that a CI runner will not grant, and AT-SPI needs an accessibility bus the
//! runner does not run. This is the one platform where the round trip can be
//! asserted rather than assumed.

#![cfg(all(target_os = "windows", feature = "accessibility"))]

use std::time::{Duration, Instant};

use sunmao::prelude::*;
use sunmao_fx_widgets_gui_gl::WidgetsPlugin;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
    UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_SliderControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumThreadWindows, IsWindowVisible, PeekMessageW, TranslateMessage, MSG,
    PM_REMOVE,
};

/// The first visible top-level window belonging to this thread.
fn visible_window_on_this_thread() -> Option<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Option<HWND>` handed to
        // `EnumThreadWindows` below, valid for the duration of the call.
        let found = unsafe { &mut *(lparam.0 as *mut Option<HWND>) };
        if found.is_none() && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            *found = Some(hwnd);
        }
        BOOL(1)
    }

    let mut found: Option<HWND> = None;
    unsafe {
        let _ = EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect),
            LPARAM(&mut found as *mut Option<HWND> as isize),
        );
    }
    found
}

/// Run the window's message pump for `duration`.
///
/// UIA answers through `WM_GETOBJECT`, which only arrives if the window is
/// pumping. A test that queried without pumping would time out and look like a
/// missing tree rather than a stalled one.
fn pump(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Control types found under `root`, as UIA reports them.
fn control_types(automation: &IUIAutomation, root: &IUIAutomationElement) -> Vec<i32> {
    let mut found = Vec::new();
    unsafe {
        let Ok(condition) = automation.CreateTrueCondition() else {
            return found;
        };
        let Ok(children) = root.FindAll(TreeScope_Descendants, &condition) else {
            return found;
        };
        let count = children.Length().unwrap_or(0);
        for index in 0..count {
            let Ok(element) = children.GetElement(index) else {
                continue;
            };
            // `CurrentControlType` rather than the generic property getter:
            // it returns the id directly instead of a VARIANT to unpack.
            if let Ok(control_type) = element.CurrentControlType() {
                found.push(control_type.0);
            }
        }
    }
    found
}

/// The editor's knob, dropdown and toggle must be visible to UI Automation as a
/// slider, a combo box and a check box.
///
/// If this fails, a screen-reader user is looking at an opaque rectangle no
/// matter how correct the in-process tree is.
#[test]
fn a_screen_reader_can_see_the_editor_controls() {
    unsafe {
        // The adapter talks to UIA on this thread; an STA is what a GUI thread
        // normally is.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let plugin = WidgetsPlugin::default();
    let Some(view) = plugin.view() else {
        panic!("the fixture has no editor");
    };
    let context: Arc<dyn ViewContext> = Arc::new(ParamsViewContext::new(plugin.params()));
    let Some(handle) = view.open_floating(context) else {
        // A headless session with no window station cannot host a window at
        // all. Failing here would report a windowing problem as an
        // accessibility one.
        eprintln!("skipping: no floating window could be opened in this session");
        return;
    };

    // Let the window come up and paint at least once: the adapter publishes the
    // tree after a frame.
    pump(Duration::from_millis(500));

    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .expect("UI Automation is unavailable");

    // `ViewHandle` deliberately hides the platform handle, and adding an
    // accessor just for a test would leak Win32 into the core abstraction. The
    // floating window was created on this thread, so ask the OS instead.
    let hwnd = visible_window_on_this_thread().expect("the floating window did not appear");

    // Retry: UIA attaches asynchronously, and the first query can land before
    // the provider is registered.
    let mut types = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        pump(Duration::from_millis(200));
        let Ok(element) = (unsafe { automation.ElementFromHandle(hwnd) }) else {
            continue;
        };
        types = control_types(&automation, &element);
        if types.contains(&UIA_SliderControlTypeId.0) {
            break;
        }
    }

    drop(handle);

    assert!(
        types.contains(&UIA_SliderControlTypeId.0),
        "UI Automation did not see the knob as a slider; saw {types:?}"
    );
    assert!(
        types.contains(&UIA_ComboBoxControlTypeId.0),
        "UI Automation did not see the dropdown as a combo box; saw {types:?}"
    );
    assert!(
        types.contains(&UIA_CheckBoxControlTypeId.0),
        "UI Automation did not see the toggle as a check box; saw {types:?}"
    );
    println!("UIA saw {} elements under the editor", types.len());
}
