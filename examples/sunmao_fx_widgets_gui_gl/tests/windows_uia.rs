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
//!
//! The editor is **embedded in a parent window** the test creates, not floated.
//! The first version floated one and skipped on the hosted runner, which cannot
//! open a top-level window from a test process — and because a skip still
//! reports `ok`, that looked like a pass. Embedding is also the path plugins
//! actually take in a host, so it is the better thing to assert anyway.

#![cfg(all(target_os = "windows", feature = "accessibility"))]

use std::time::{Duration, Instant};

use sunmao::prelude::*;
use sunmao_fx_widgets_gui_gl::WidgetsPlugin;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
    UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_SliderControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindow, PeekMessageW,
    RegisterClassW, TranslateMessage, CW_USEDEFAULT, GW_CHILD, MSG, PM_REMOVE, WINDOW_EX_STYLE,
    WNDCLASSW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
};

/// Create a plain host window for the editor to embed in.
///
/// This mirrors what a DAW does, and what the project's own test runner already
/// does successfully on this platform.
unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn create_host_window() -> Option<HWND> {
    let class_name = windows::core::w!("SunmaoUiaTestHost");
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSW {
            // `DefWindowProcW` is exposed as a Rust fn; the class wants a
            // "system" fn pointer, so wrap it.
            lpfnWndProc: Some(host_wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        // A duplicate registration is fine: the class survives from a previous
        // test in the same process.
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            windows::core::w!("SunMao UIA host"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            520,
            360,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;
        Some(hwnd)
    }
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

/// Every descendant of `root`, as `(control type, name)`.
///
/// The name comes along because a bare list of numeric control types is nearly
/// unreadable when this fails: "50000" could be the editor's button or the host
/// window's close box.
fn describe(automation: &IUIAutomation, root: &IUIAutomationElement) -> Vec<(i32, String)> {
    let mut found = Vec::new();
    // The **raw** view, not the default control view. The control view hides
    // elements it deems uninteresting — an unnamed pane, for one — which is
    // exactly what a custom-drawn editor's container looks like. Walking the
    // control view and finding nothing would be ambiguous between "no provider"
    // and "provider filtered out".
    unsafe {
        let Ok(walker) = automation.RawViewWalker() else {
            return found;
        };
        walk(&walker, root, 0, &mut found);
    }
    found
}

/// Depth-first raw-view walk, bounded so a cyclic or enormous tree cannot hang
/// the test.
unsafe fn walk(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    depth: usize,
    out: &mut Vec<(i32, String)>,
) {
    if depth > 8 || out.len() > 200 {
        return;
    }
    let control_type = unsafe { element.CurrentControlType() }
        .map(|id| id.0)
        .unwrap_or(0);
    let name = unsafe { element.CurrentName() }
        .map(|name| name.to_string())
        .unwrap_or_default();
    out.push((control_type, name));

    let mut child = unsafe { walker.GetFirstChildElement(element) }.ok();
    while let Some(current) = child {
        unsafe { walk(walker, &current, depth + 1, out) };
        child = unsafe { walker.GetNextSiblingElement(&current) }.ok();
    }
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
    // Marked distinctly so a CI log shows which path was taken: a test that
    // silently skips reads exactly like one that passed.
    let Some(parent) = create_host_window() else {
        println!("UIA SKIPPED: this session cannot create a window at all");
        return;
    };

    let Some(handle) = view.open(
        ParentWindow::Win32(parent.0 as *mut std::ffi::c_void),
        context,
    ) else {
        println!("UIA SKIPPED: the editor could not be embedded in the host window");
        unsafe {
            let _ = DestroyWindow(parent);
        }
        return;
    };

    // Let the window come up and paint at least once: the adapter publishes the
    // tree after a frame.
    pump(Duration::from_millis(500));

    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .expect("UI Automation is unavailable");

    // The editor is a child HWND of the host window. Query it directly as well
    // as through the parent: if the parent walk misses it but the direct query
    // finds it, the provider works and the traversal is what needs fixing.
    let child = unsafe { GetWindow(parent, GW_CHILD) }.ok();

    // Retry: UIA attaches asynchronously, and the first query can land before
    // the provider is registered.
    let mut found: Vec<(i32, String)> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        pump(Duration::from_millis(200));
        for hwnd in [child, Some(parent)].into_iter().flatten() {
            let Ok(element) = (unsafe { automation.ElementFromHandle(hwnd) }) else {
                continue;
            };
            let described = describe(&automation, &element);
            for entry in described {
                if !found.contains(&entry) {
                    found.push(entry);
                }
            }
        }
        if found
            .iter()
            .any(|(control_type, _)| *control_type == UIA_SliderControlTypeId.0)
        {
            break;
        }
    }

    println!("UIA child window present: {}", child.is_some());
    for (control_type, name) in &found {
        println!("UIA element: type={control_type} name={name:?}");
    }
    let types: Vec<i32> = found
        .iter()
        .map(|(control_type, _)| *control_type)
        .collect();

    drop(handle);
    unsafe {
        let _ = DestroyWindow(parent);
    }

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
    // The marker CI greps for. Without it, a skipped run and a verified run are
    // indistinguishable in the log — the trap run #82 fell into.
    println!(
        "UIA VERIFIED: slider + combo box + check box among {} elements",
        types.len()
    );
}
