#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::collections::HashMap;
use std::ffi::c_void;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::ffi::CString;
use std::ptr;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::sync::Mutex;

// Global callback registry for window close handlers
#[cfg(any(target_os = "macos", target_os = "windows"))]
static CLOSE_CALLBACKS: Mutex<Option<HashMap<usize, Box<dyn Fn() + Send>>>> = Mutex::new(None);

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn register_close_callback(delegate_ptr: usize, cb: Box<dyn Fn() + Send>) {
    let mut map = CLOSE_CALLBACKS.lock().unwrap();
    if map.is_none() {
        *map = Some(HashMap::new());
    }
    map.as_mut().unwrap().insert(delegate_ptr, cb);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn take_close_callback(delegate_ptr: usize) -> Option<Box<dyn Fn() + Send>> {
    let mut map = CLOSE_CALLBACKS.lock().unwrap();
    map.as_mut().and_then(|m| m.remove(&delegate_ptr))
}

// ---- Geometry types ----

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct NSPoint {
    pub x: f64,
    pub y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct NSSize {
    pub width: f64,
    pub height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct NSRect {
    pub origin: NSPoint,
    pub size: NSSize,
}

// ---- ObjC runtime FFI (macOS only) ----

#[cfg(target_os = "macos")]
mod objc_ffi {
    use super::*;
    use std::ffi::c_char;

    extern "C" {
        pub fn objc_getClass(name: *const c_char) -> *mut c_void;
        pub fn sel_registerName(name: *const c_char) -> *mut c_void;
        pub fn objc_allocateClassPair(
            superclass: *mut c_void,
            name: *const c_char,
            extra_bytes: usize,
        ) -> *mut c_void;
        pub fn objc_registerClassPair(cls: *mut c_void);
        pub fn class_addMethod(
            cls: *mut c_void,
            name: *mut c_void,
            imp: *mut c_void,
            types: *const c_char,
        ) -> bool;
        pub fn objc_msgSend() -> *mut c_void;
    }

    /// RAII guard for an NSAutoreleasePool
    pub struct AutoreleasePool {
        pool: *mut c_void,
    }

    impl AutoreleasePool {
        pub unsafe fn new() -> Self {
            let ns_pool = objc_getClass(b"NSAutoreleasePool\0".as_ptr() as *const _);
            let sel_alloc = sel_registerName(b"alloc\0".as_ptr() as *const _);
            let sel_init = sel_registerName(b"init\0".as_ptr() as *const _);
            let alloc = msg_send0(ns_pool, sel_alloc);
            let pool = msg_send0(alloc, sel_init);
            AutoreleasePool { pool }
        }
    }

    impl Drop for AutoreleasePool {
        fn drop(&mut self) {
            if !self.pool.is_null() {
                unsafe {
                    let sel_drain = sel_registerName(b"drain\0".as_ptr() as *const _);
                    msg_send_void(self.pool, sel_drain);
                }
            }
        }
    }

    pub unsafe fn msg_send0(obj: *mut c_void, sel: *mut c_void) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel)
    }

    pub unsafe fn msg_send_void(obj: *mut c_void, sel: *mut c_void) {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel);
    }

    pub unsafe fn msg_send1(obj: *mut c_void, sel: *mut c_void, a: *mut c_void) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel, a)
    }

    pub unsafe fn msg_send_rect(obj: *mut c_void, sel: *mut c_void, r: NSRect) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel, r)
    }

    pub unsafe fn msg_send_size_void(obj: *mut c_void, sel: *mut c_void, size: NSSize) {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, NSSize) =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel, size);
    }

    pub unsafe fn msg_send_rect0(obj: *mut c_void, sel: *mut c_void) -> NSRect {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSRect =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel)
    }

    pub unsafe fn msg_send_init_window(
        obj: *mut c_void,
        sel: *mut c_void,
        rect: NSRect,
        style: u64,
        backing: u64,
        defer: bool,
    ) -> *mut c_void {
        let f: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            NSRect,
            u64,
            u64,
            bool,
        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel, rect, style, backing, defer)
    }

    pub unsafe fn msg_send_init_frame(
        obj: *mut c_void,
        sel: *mut c_void,
        rect: NSRect,
    ) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel, rect)
    }

    pub unsafe fn msg_send_init_url(
        obj: *mut c_void,
        sel: *mut c_void,
        url: *mut c_void,
    ) -> *mut c_void {
        msg_send1(obj, sel, url)
    }

    pub unsafe fn msg_send_uiview_for_au(
        obj: *mut c_void,
        sel: *mut c_void,
        au: *mut c_void,
        size: NSSize,
    ) -> *mut c_void {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, NSSize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        f(obj, sel, au, size)
    }
}

// NSWindow style masks
#[cfg(target_os = "macos")]
const NS_WINDOW_STYLE_MASK_TITLED: u64 = 1 << 0;
#[cfg(target_os = "macos")]
const NS_WINDOW_STYLE_MASK_CLOSABLE: u64 = 1 << 1;
#[cfg(target_os = "macos")]
const NS_WINDOW_STYLE_MASK_MINIATURIZABLE: u64 = 1 << 2;
#[cfg(target_os = "macos")]
const NS_BACKING_STORE_BUFFERED: u64 = 2;

#[cfg(target_os = "macos")]
fn macos_host_window_style() -> u64 {
    NS_WINDOW_STYLE_MASK_TITLED
        | NS_WINDOW_STYLE_MASK_CLOSABLE
        | NS_WINDOW_STYLE_MASK_MINIATURIZABLE
}

// ---- Public API ----

pub struct PluginGuiWindow {
    #[cfg(target_os = "macos")]
    window: *mut c_void,
    #[cfg(target_os = "macos")]
    content_view: *mut c_void,
    #[cfg(target_os = "macos")]
    delegate: *mut c_void,
    #[cfg(target_os = "windows")]
    window: *mut c_void,
    #[cfg(target_os = "windows")]
    content_view: *mut c_void,
    #[cfg(target_os = "linux")]
    x11: linux::LinuxWindow,
}

impl PluginGuiWindow {
    pub fn new(
        title: &str,
        width: f64,
        height: f64,
        close_cb: Box<dyn Fn() + Send>,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            unsafe { macos::create_ns_window(title, width, height, close_cb) }
        }
        #[cfg(target_os = "windows")]
        {
            unsafe { windows::create_window(title, width, height, close_cb) }
        }
        #[cfg(target_os = "linux")]
        {
            linux::initialize()?;
            linux::create_window(title, width, height, close_cb).map(|x11| PluginGuiWindow { x11 })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (title, width, height, close_cb);
            Err("Plugin GUI is not supported on this platform".into())
        }
    }

    pub fn content_view(&self) -> *mut c_void {
        #[cfg(target_os = "macos")]
        {
            self.content_view
        }
        #[cfg(target_os = "windows")]
        {
            self.content_view
        }
        #[cfg(target_os = "linux")]
        {
            self.x11.attachment_handle()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            ptr::null_mut()
        }
    }

    pub fn window_ptr(&self) -> *mut c_void {
        #[cfg(target_os = "macos")]
        {
            self.window
        }
        #[cfg(target_os = "windows")]
        {
            self.window
        }
        #[cfg(target_os = "linux")]
        {
            self.x11.attachment_handle()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            ptr::null_mut()
        }
    }

    pub fn set_content_view_size(&self, width: f64, height: f64) {
        #[cfg(target_os = "macos")]
        {
            macos::set_content_size(self.window, width, height);
        }
        #[cfg(target_os = "windows")]
        unsafe {
            windows::set_content_size(self.window, self.content_view, width, height);
        }
        #[cfg(target_os = "linux")]
        self.x11.set_content_size(width, height);
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let _ = (width, height);
    }

    pub fn pump_events() -> bool {
        #[cfg(target_os = "macos")]
        {
            macos::pump_events()
        }
        #[cfg(target_os = "windows")]
        {
            unsafe { windows::pump_events() }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            true
        }
    }

    pub fn verify_non_uniform_pixels(&self) -> Result<PixelEvidence, String> {
        #[cfg(target_os = "macos")]
        {
            macos::verify_non_uniform_pixels(self.window, self.content_view)
        }
        #[cfg(target_os = "windows")]
        {
            unsafe { windows::verify_non_uniform_pixels(self.content_view) }
        }
        #[cfg(target_os = "linux")]
        {
            self.x11.verify_non_uniform_pixels()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err("GUI pixel validation is not supported on this platform".into())
        }
    }

    pub fn drag_slider(
        &self,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    ) -> Result<InputDelivery, String> {
        #[cfg(target_os = "macos")]
        {
            macos::drag_slider(self.window, self.content_view, from_x, from_y, to_x, to_y)
        }
        #[cfg(target_os = "windows")]
        {
            unsafe { windows::drag_slider(self.content_view, from_x, from_y, to_x, to_y) }
        }
        #[cfg(target_os = "linux")]
        {
            self.x11.drag_slider(from_x, from_y, to_x, to_y)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (from_x, from_y, to_x, to_y);
            Err("GUI input validation is not supported on this platform".into())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDelivery {
    NativeMouse,
    #[cfg(target_os = "windows")]
    WindowsMessage,
    #[cfg(target_os = "windows")]
    WindowsMessageAndNativeMouse,
    #[cfg(target_os = "windows")]
    WindowsUiAutomation,
    WebViewDom,
}

impl std::fmt::Display for InputDelivery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NativeMouse => formatter.write_str("native mouse"),
            #[cfg(target_os = "windows")]
            Self::WindowsMessage => formatter.write_str("Win32 window messages"),
            #[cfg(target_os = "windows")]
            Self::WindowsMessageAndNativeMouse => {
                formatter.write_str("Win32 window messages and native mouse")
            }
            #[cfg(target_os = "windows")]
            Self::WindowsUiAutomation => formatter.write_str("Windows UI Automation"),
            Self::WebViewDom => formatter.write_str("WebView DOM gesture"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PixelEvidence {
    pub width: u32,
    pub height: u32,
    pub sampled_pixels: usize,
    pub distinct_colors: usize,
    pub intensity_range: u8,
    pub intensity_std_dev: f64,
}

pub fn read_plugin_pixel_probe(library: &libloading::Library) -> Result<PixelEvidence, String> {
    type ProbeFn = unsafe extern "C" fn(*mut u32, *mut u32, *mut u32, usize) -> i32;
    let probe = unsafe {
        library
            .get::<ProbeFn>(b"sunmao_debug_read_frame\0")
            .map_err(|error| format!("plugin does not export sunmao_debug_read_frame: {error}"))?
    };
    let mut width = 0_u32;
    let mut height = 0_u32;
    let mut pixels = vec![0_u32; 64 * 64];
    let count = unsafe { probe(&mut width, &mut height, pixels.as_mut_ptr(), pixels.len()) };
    if count <= 0 || width == 0 || height == 0 {
        return Err("in-process GUI pixel probe has no captured frame".into());
    }
    let count = count as usize;
    pixels.truncate(count.min(pixels.len()));
    non_uniform_pixel_evidence(width, height, pixels)
}

fn non_uniform_pixel_evidence(
    width: u32,
    height: u32,
    pixels: impl IntoIterator<Item = u32>,
) -> Result<PixelEvidence, String> {
    const SAMPLE_LIMIT: usize = 4096;
    const MIN_DISTINCT_COLORS: usize = 4;
    const MIN_INTENSITY_RANGE: u8 = 16;
    const MIN_INTENSITY_STD_DEV: f64 = 4.0;
    let mut samples = [0_u32; SAMPLE_LIMIT];
    let mut distinct = 0;
    let mut sampled = 0;
    let mut min_intensity = u8::MAX;
    let mut max_intensity = u8::MIN;
    let mut intensity_sum = 0.0;
    let mut intensity_square_sum = 0.0;

    for pixel in pixels.into_iter().take(SAMPLE_LIMIT) {
        let red = (pixel & 0xff) as u8;
        let green = ((pixel >> 8) & 0xff) as u8;
        let blue = ((pixel >> 16) & 0xff) as u8;
        let intensity = ((u16::from(red) + u16::from(green) + u16::from(blue)) / 3) as u8;
        min_intensity = min_intensity.min(intensity);
        max_intensity = max_intensity.max(intensity);
        let intensity = f64::from(intensity);
        intensity_sum += intensity;
        intensity_square_sum += intensity * intensity;
        sampled += 1;

        // Ignore low-bit rendering noise when counting visually distinct colors.
        let quantized = pixel & 0x00f8_f8f8;
        if !samples[..distinct].contains(&quantized) {
            samples[distinct] = quantized;
            distinct += 1;
        }
    }

    let intensity_range = max_intensity.saturating_sub(min_intensity);
    let mean = if sampled == 0 {
        0.0
    } else {
        intensity_sum / sampled as f64
    };
    let variance = if sampled == 0 {
        0.0
    } else {
        (intensity_square_sum / sampled as f64 - mean * mean).max(0.0)
    };
    let intensity_std_dev = variance.sqrt();
    let evidence = PixelEvidence {
        width,
        height,
        sampled_pixels: sampled,
        distinct_colors: distinct,
        intensity_range,
        intensity_std_dev,
    };

    if distinct >= MIN_DISTINCT_COLORS
        && intensity_range >= MIN_INTENSITY_RANGE
        && intensity_std_dev >= MIN_INTENSITY_STD_DEV
    {
        Ok(evidence)
    } else {
        Err(format!(
            "captured GUI content lacks visual variation ({}x{}, {} sampled pixels, {} colors, intensity range {}, std dev {:.2})",
            width, height, sampled, distinct, intensity_range, intensity_std_dev
        ))
    }
}

pub fn initialize_platform() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        unsafe {
            use objc_ffi::*;

            let _pool = AutoreleasePool::new();
            let app_class = objc_getClass(b"NSApplication\0".as_ptr() as *const _);
            if app_class.is_null() {
                return Err("NSApplication class not found".into());
            }
            let shared_application = sel_registerName(b"sharedApplication\0".as_ptr() as *const _);
            let application = msg_send0(app_class, shared_application);
            if application.is_null() {
                return Err("NSApplication initialization failed".into());
            }
            // Hosted macOS runners start this binary as a command-line tool.
            // Without a regular activation policy the window is not composited
            // and pixel capture sees an empty or uniform frame.
            let policy_selector = sel_registerName(b"setActivationPolicy:\0".as_ptr() as *const _);
            let set_policy: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) -> bool =
                std::mem::transmute(objc_msgSend as *const c_void);
            const NS_APPLICATION_ACTIVATION_POLICY_REGULAR: isize = 0;
            let _ = set_policy(
                application,
                policy_selector,
                NS_APPLICATION_ACTIVATION_POLICY_REGULAR,
            );
            let finish_selector = sel_registerName(b"finishLaunching\0".as_ptr() as *const _);
            msg_send_void(application, finish_selector);
            let activate_selector =
                sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as *const _);
            let activate: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const c_void);
            activate(application, activate_selector, true);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        linux::initialize()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub fn run_windows_ui_automation_helper(args: &[String]) -> Result<f64, String> {
    windows::run_ui_automation_helper(args)
}

impl Drop for PluginGuiWindow {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if !self.window.is_null() {
                unsafe {
                    use objc_ffi::*;
                    let sel_release = sel_registerName(b"release\0".as_ptr() as *const _);
                    let sel_set_del = sel_registerName(b"setDelegate:\0".as_ptr() as *const _);

                    // Remove delegate to prevent callbacks during dealloc
                    msg_send1(self.window, sel_set_del, ptr::null_mut());

                    // Release delegate
                    if !self.delegate.is_null() {
                        take_close_callback(self.delegate as usize);
                        msg_send_void(self.delegate, sel_release);
                    }

                    // Release content view (we retained it)
                    if !self.content_view.is_null() {
                        msg_send_void(self.content_view, sel_release);
                    }

                    // Release window (we retained it)
                    msg_send_void(self.window, sel_release);
                }
            }
        }
        #[cfg(target_os = "windows")]
        unsafe {
            windows::destroy_window(self.window);
            self.window = ptr::null_mut();
            self.content_view = ptr::null_mut();
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", test))]
fn native_pixel_size(width: f64, height: f64) -> (u32, u32) {
    fn dimension(value: f64) -> u32 {
        if !value.is_finite() || value < 1.0 {
            1
        } else {
            value.round().min(i32::MAX as f64) as u32
        }
    }

    (dimension(width), dimension(height))
}

#[cfg(any(target_os = "windows", test))]
fn windows_logical_to_client_pixel(value: f64, dpi: u32) -> f64 {
    value * f64::from(dpi) / 96.0
}

#[cfg(any(target_os = "windows", test))]
fn drag_rectangle_score(bounds: (i32, i32, i32, i32), from: (i32, i32), to: (i32, i32)) -> u8 {
    let (left, top, right, bottom) = bounds;
    if right <= left || bottom <= top {
        return 0;
    }

    let contains = |(x, y): (i32, i32)| x >= left && x < right && y >= top && y < bottom;
    let midpoint = (
        ((i64::from(from.0) + i64::from(to.0)) / 2) as i32,
        ((i64::from(from.1) + i64::from(to.1)) / 2) as i32,
    );
    let point_score =
        u8::from(contains(from)) * 4 + u8::from(contains(midpoint)) * 2 + u8::from(contains(to));
    if point_score != 0 {
        return point_score;
    }

    // Liang-Barsky clipping catches a segment that crosses a small slider
    // without any of the three sampled points landing inside it.
    let x0 = f64::from(from.0);
    let y0 = f64::from(from.1);
    let dx = f64::from(to.0) - x0;
    let dy = f64::from(to.1) - y0;
    let mut near = 0.0_f64;
    let mut far = 1.0_f64;
    for (direction, distance) in [
        (-dx, x0 - f64::from(left)),
        (dx, f64::from(right - 1) - x0),
        (-dy, y0 - f64::from(top)),
        (dy, f64::from(bottom - 1) - y0),
    ] {
        if direction == 0.0 {
            if distance < 0.0 {
                return 0;
            }
            continue;
        }
        let intersection = distance / direction;
        if direction < 0.0 {
            near = near.max(intersection);
        } else {
            far = far.min(intersection);
        }
        if near > far {
            return 0;
        }
    }
    1
}

// ---- Windows implementation ----

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::Win32::Foundation::{
        HWND as AutomationHwnd, POINT as AutomationPoint, RECT as AutomationRect,
    };
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::System::Variant::{VARIANT, VT_I4};
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation6, IUIAutomationRangeValuePattern, OrientationType_Horizontal,
        OrientationType_Vertical, TreeScope_Descendants, UIA_ControlTypePropertyId,
        UIA_NativeWindowHandlePropertyId, UIA_ProcessIdPropertyId, UIA_RangeValuePatternId,
        UIA_SliderControlTypeId,
    };
    use std::io::Read;
    use std::iter;
    use std::process::{Command, Stdio};
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, ScreenToClient, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC, SRCCOPY,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetFocus, SendInput, SetActiveWindow, SetFocus, INPUT, INPUT_0, INPUT_MOUSE,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow,
        DispatchMessageW, GetAncestor, GetClassNameW, GetClientRect, GetCursorPos,
        GetForegroundWindow, GetWindow, IsWindow, IsWindowVisible, LoadCursorW, PeekMessageW,
        PostMessageW, RegisterClassExW, SetCursorPos, SetForegroundWindow, SetWindowPos,
        ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GA_ROOT, GW_CHILD,
        GW_HWNDNEXT, IDC_ARROW, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SW_SHOW,
        WM_CLOSE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_QUIT, WM_SIZE,
        WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_MINIMIZEBOX,
        WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    };

    static CLASS_REGISTRATION: OnceLock<Result<(), u32>> = OnceLock::new();
    static CLASS_NAME: OnceLock<Vec<u16>> = OnceLock::new();
    static CONTENT_VIEWS: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);

    extern "system" {
        fn PrintWindow(hwnd: HWND, hdc_blt: HDC, flags: u32) -> i32;
    }
    const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;

    fn class_name() -> &'static [u16] {
        CLASS_NAME.get_or_init(|| {
            "SunMaoRunnerPluginHostWindow"
                .encode_utf16()
                .chain(iter::once(0))
                .collect()
        })
    }

    pub(super) fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(iter::once(0)).collect()
    }

    fn register_content_view(window: HWND, content_view: HWND) {
        let mut map = CONTENT_VIEWS.lock().unwrap();
        map.get_or_insert_with(HashMap::new)
            .insert(window as usize, content_view as usize);
    }

    fn take_content_view(window: HWND) -> Option<HWND> {
        let mut map = CONTENT_VIEWS.lock().unwrap();
        map.as_mut()
            .and_then(|map| map.remove(&(window as usize)))
            .map(|content_view| content_view as HWND)
    }

    fn content_view(window: HWND) -> Option<HWND> {
        let map = CONTENT_VIEWS.lock().unwrap();
        map.as_ref()
            .and_then(|map| map.get(&(window as usize)).copied())
            .map(|content_view| content_view as HWND)
    }

    unsafe extern "system" fn window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_SIZE => {
                if let Some(content_view) = content_view(window) {
                    let mut client_rect = RECT::default();
                    if unsafe { GetClientRect(window, &mut client_rect) } != 0 {
                        unsafe {
                            SetWindowPos(
                                content_view,
                                ptr::null_mut(),
                                0,
                                0,
                                client_rect.right - client_rect.left,
                                client_rect.bottom - client_rect.top,
                                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
                            );
                        }
                    }
                }
                0
            }
            WM_CLOSE => {
                let close_cb = take_close_callback(window as usize);
                if let Some(close_cb) = close_cb {
                    close_cb();
                }
                0
            }
            WM_NCDESTROY => {
                take_close_callback(window as usize);
                take_content_view(window);
                unsafe { DefWindowProcW(window, message, wparam, lparam) }
            }
            _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        }
    }

    unsafe fn ensure_window_class() -> Result<(), String> {
        let registration = CLASS_REGISTRATION.get_or_init(|| {
            let instance = unsafe { GetModuleHandleW(ptr::null()) };
            if instance.is_null() {
                return Err(unsafe { GetLastError() });
            }

            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: ptr::null_mut(),
                hCursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) },
                hbrBackground: ptr::null_mut(),
                lpszMenuName: ptr::null(),
                lpszClassName: class_name().as_ptr(),
                hIconSm: ptr::null_mut(),
            };

            if unsafe { RegisterClassExW(&class) } == 0 {
                Err(unsafe { GetLastError() })
            } else {
                Ok(())
            }
        });

        registration
            .as_ref()
            .map_err(|error| format!("RegisterClassExW failed with error {error}"))
            .copied()
    }

    fn adjusted_window_size(width: u32, height: u32) -> Result<(i32, i32), String> {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        let style = host_window_style();
        if unsafe { AdjustWindowRectEx(&mut rect, style, 0, 0) } == 0 {
            return Err(format!("AdjustWindowRectEx failed with error {}", unsafe {
                GetLastError()
            }));
        }
        Ok((rect.right - rect.left, rect.bottom - rect.top))
    }

    fn host_window_style() -> u32 {
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN | WS_VISIBLE
    }

    pub(super) unsafe fn create_window(
        title: &str,
        width: f64,
        height: f64,
        close_cb: Box<dyn Fn() + Send>,
    ) -> Result<PluginGuiWindow, String> {
        unsafe { ensure_window_class()? };

        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        if instance.is_null() {
            return Err(format!("GetModuleHandleW failed with error {}", unsafe {
                GetLastError()
            }));
        }

        let (content_width, content_height) = native_pixel_size(width, height);
        let (window_width, window_height) = adjusted_window_size(content_width, content_height)?;
        let title = wide_null(title);
        let style = host_window_style();
        let window = unsafe {
            CreateWindowExW(
                0,
                class_name().as_ptr(),
                title.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                window_width,
                window_height,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(format!("CreateWindowExW failed with error {}", unsafe {
                GetLastError()
            }));
        }

        let static_class = wide_null("STATIC");
        let content_view = unsafe {
            CreateWindowExW(
                0,
                static_class.as_ptr(),
                ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                0,
                0,
                content_width as i32,
                content_height as i32,
                window,
                ptr::null_mut(),
                instance,
                ptr::null(),
            )
        };
        if content_view.is_null() {
            let error = unsafe { GetLastError() };
            unsafe {
                DestroyWindow(window);
            }
            return Err(format!("content HWND creation failed with error {error}"));
        }

        register_content_view(window, content_view);
        register_close_callback(window as usize, close_cb);
        unsafe {
            ShowWindow(window, SW_SHOW);
        }

        Ok(PluginGuiWindow {
            window: window.cast(),
            content_view: content_view.cast(),
        })
    }

    pub(super) unsafe fn set_content_size(
        window: *mut c_void,
        content_view: *mut c_void,
        width: f64,
        height: f64,
    ) {
        if window.is_null() || content_view.is_null() {
            return;
        }

        let (content_width, content_height) = native_pixel_size(width, height);
        let Ok((window_width, window_height)) = adjusted_window_size(content_width, content_height)
        else {
            return;
        };
        unsafe {
            SetWindowPos(
                window.cast(),
                ptr::null_mut(),
                0,
                0,
                window_width,
                window_height,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
            );
            SetWindowPos(
                content_view.cast(),
                ptr::null_mut(),
                0,
                0,
                content_width as i32,
                content_height as i32,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
            );
        }
    }

    pub(super) unsafe fn destroy_window(window: *mut c_void) {
        if window.is_null() {
            return;
        }

        let window: HWND = window.cast();
        take_close_callback(window as usize);
        let owned = take_content_view(window).is_some();
        if owned && unsafe { IsWindow(window) } != 0 {
            unsafe {
                DestroyWindow(window);
            }
        }
    }

    pub(super) unsafe fn pump_events() -> bool {
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_QUIT {
                return false;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        true
    }

    unsafe fn plugin_surface(content_view: HWND) -> Result<HWND, String> {
        if content_view.is_null() || unsafe { IsWindow(content_view) } == 0 {
            return Err("Windows plugin content HWND is unavailable".into());
        }
        let mut child = unsafe { GetWindow(content_view, GW_CHILD) };
        let mut largest = ptr::null_mut();
        let mut largest_area = 0_i64;
        while !child.is_null() {
            if unsafe { IsWindowVisible(child) } != 0 {
                let mut rect = RECT::default();
                if unsafe { GetClientRect(child, &mut rect) } != 0 {
                    let area = i64::from((rect.right - rect.left).max(0))
                        * i64::from((rect.bottom - rect.top).max(0));
                    if area > largest_area {
                        largest = child;
                        largest_area = area;
                    }
                }
            }
            child = unsafe { GetWindow(child, GW_HWNDNEXT) };
        }
        if largest.is_null() {
            Err("Windows host content has no visible plugin child HWND".into())
        } else {
            Ok(largest)
        }
    }

    fn window_class(window: HWND) -> Option<String> {
        let mut class = [0_u16; 256];
        let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
        if length <= 0 {
            None
        } else {
            Some(String::from_utf16_lossy(&class[..length as usize]))
        }
    }

    fn is_wry_webview(window: HWND) -> bool {
        window_class(window).is_some_and(|class| class == "WRY_WEBVIEW")
    }

    unsafe fn deepest_visible_surface(mut window: HWND) -> (HWND, usize, bool) {
        let mut depth = 0;
        let mut is_webview = is_wry_webview(window);
        loop {
            let mut child = unsafe { GetWindow(window, GW_CHILD) };
            let mut largest = ptr::null_mut();
            let mut largest_area = 0_i64;
            while !child.is_null() {
                if unsafe { IsWindowVisible(child) } != 0 {
                    let mut rect = RECT::default();
                    if unsafe { GetClientRect(child, &mut rect) } != 0 {
                        let area = i64::from((rect.right - rect.left).max(0))
                            * i64::from((rect.bottom - rect.top).max(0));
                        if area > largest_area {
                            largest = child;
                            largest_area = area;
                        }
                    }
                }
                child = unsafe { GetWindow(child, GW_HWNDNEXT) };
            }
            if largest.is_null() {
                return (window, depth, is_webview);
            }
            window = largest;
            depth += 1;
            is_webview |= is_wry_webview(window);
        }
    }

    pub(super) unsafe fn verify_non_uniform_pixels(
        content_view: *mut c_void,
    ) -> Result<PixelEvidence, String> {
        let content_view = unsafe { plugin_surface(content_view.cast())? };
        let mut rect = RECT::default();
        if unsafe { GetClientRect(content_view, &mut rect) } == 0 {
            return Err(format!("GetClientRect failed with error {}", unsafe {
                GetLastError()
            }));
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err("captured Windows GUI frame is empty".into());
        }

        let mut origin = POINT { x: 0, y: 0 };
        if unsafe { ClientToScreen(content_view, &mut origin) } == 0 {
            return Err(format!("ClientToScreen failed with error {}", unsafe {
                GetLastError()
            }));
        }

        // Capture the composed desktop region. Parent HWND device contexts can
        // omit child, DirectComposition, and WebView2 pixels.
        let source = unsafe { GetDC(ptr::null_mut()) };
        if source.is_null() {
            return Err("GetDC returned null for the Windows desktop".into());
        }
        let memory = unsafe { CreateCompatibleDC(source) };
        let bitmap = unsafe { CreateCompatibleBitmap(source, width, height) };
        if memory.is_null() || bitmap.is_null() {
            if !bitmap.is_null() {
                unsafe { DeleteObject(bitmap) };
            }
            if !memory.is_null() {
                unsafe { DeleteDC(memory) };
            }
            unsafe { ReleaseDC(ptr::null_mut(), source) };
            return Err("failed to allocate Windows GUI capture surface".into());
        }
        let previous = unsafe { SelectObject(memory, bitmap) };
        let copied = unsafe {
            BitBlt(
                memory,
                0,
                0,
                width,
                height,
                source,
                origin.x,
                origin.y,
                SRCCOPY | CAPTUREBLT,
            )
        };
        let copied = if copied == 0 {
            unsafe { PrintWindow(content_view, memory, PW_RENDERFULLCONTENT) }
        } else {
            copied
        };
        let mut pixels = vec![0_u32; width as usize * height as usize];
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let rows = if copied != 0 {
            unsafe {
                GetDIBits(
                    memory,
                    bitmap,
                    0,
                    height as u32,
                    pixels.as_mut_ptr().cast(),
                    &mut info,
                    DIB_RGB_COLORS,
                )
            }
        } else {
            0
        };
        unsafe {
            SelectObject(memory, previous);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(ptr::null_mut(), source);
        }
        if rows != height {
            return Err(format!("Windows GUI capture copied {rows}/{height} rows"));
        }
        let step = (pixels.len() / 4096).max(1);
        non_uniform_pixel_evidence(
            width as u32,
            height as u32,
            pixels.into_iter().step_by(step),
        )
    }

    pub(super) unsafe fn drag_slider(
        content_view: *mut c_void,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    ) -> Result<InputDelivery, String> {
        let content_view = unsafe { plugin_surface(content_view.cast())? };
        let (input_view, input_depth, is_webview) =
            unsafe { deepest_visible_surface(content_view) };
        let mut bounds = RECT::default();
        if unsafe { GetClientRect(content_view, &mut bounds) } == 0 {
            return Err(format!("GetClientRect failed with error {}", unsafe {
                GetLastError()
            }));
        }
        let dpi = unsafe { GetDpiForWindow(content_view) };
        if dpi == 0 {
            return Err("GetDpiForWindow returned zero for plugin content".into());
        }
        let (from_x, from_y, to_x, to_y) = (
            windows_logical_to_client_pixel(from_x, dpi),
            windows_logical_to_client_pixel(from_y, dpi),
            windows_logical_to_client_pixel(to_x, dpi),
            windows_logical_to_client_pixel(to_y, dpi),
        );
        eprintln!(
            "Windows GUI input geometry: {dpi} DPI, client {}x{}, input depth {input_depth}, Wry WebView={is_webview}, drag ({from_x:.1},{from_y:.1}) -> ({to_x:.1},{to_y:.1})",
            bounds.right, bounds.bottom,
        );
        for (label, x, y) in [("start", from_x, from_y), ("end", to_x, to_y)] {
            if !x.is_finite()
                || !y.is_finite()
                || x < 0.0
                || y < 0.0
                || x >= f64::from(bounds.right)
                || y >= f64::from(bounds.bottom)
            {
                return Err(format!(
                    "Windows drag {label} ({x:.1},{y:.1}) at {dpi} DPI is outside plugin content {}x{}",
                    bounds.right, bounds.bottom,
                ));
            }
        }
        let root = unsafe { GetAncestor(content_view, GA_ROOT) };
        if root.is_null() {
            return Err("GetAncestor(GA_ROOT) returned null for plugin content".into());
        }
        let foreground_requested = unsafe { SetForegroundWindow(root) } != 0;
        let raised = unsafe { BringWindowToTop(root) } != 0;
        unsafe {
            SetActiveWindow(root);
            SetFocus(input_view);
            pump_events();
        }
        eprintln!(
            "Windows GUI input focus: foreground request={foreground_requested}, foreground active={}, raised={raised}, input focused={}",
            unsafe { GetForegroundWindow() } == root,
            unsafe { GetFocus() } == input_view,
        );
        let screen_point = |x: f64, y: f64| -> Result<POINT, String> {
            let mut point = POINT {
                x: x.round() as i32,
                y: y.round() as i32,
            };
            if unsafe { ClientToScreen(content_view, &mut point) } == 0 {
                Err(format!("ClientToScreen failed with error {}", unsafe {
                    GetLastError()
                }))
            } else {
                Ok(point)
            }
        };
        let from = screen_point(from_x, from_y)?;
        let to = screen_point(to_x, to_y)?;
        if is_webview {
            match ui_automation_range_drag(root, from, to) {
                Ok(target) => {
                    eprintln!("Windows WebView input set UI Automation range value to {target:.3}");
                    return Ok(InputDelivery::WindowsUiAutomation);
                }
                Err(error) => {
                    eprintln!("Windows WebView UI Automation fallback unavailable: {error}");
                }
            }
        }
        let input_point = |screen: POINT| -> Result<POINT, String> {
            let mut point = screen;
            if unsafe { ScreenToClient(input_view, &mut point) } == 0 {
                Err(format!("ScreenToClient failed with error {}", unsafe {
                    GetLastError()
                }))
            } else {
                Ok(point)
            }
        };
        let message_from = input_point(from)?;
        let message_to = input_point(to)?;
        let message_result = post_message_drag(
            input_view,
            f64::from(message_from.x),
            f64::from(message_from.y),
            f64::from(message_to.x),
            f64::from(message_to.y),
        );
        unsafe {
            pump_events();
        }
        let native_result = native_mouse_drag(from, to);
        match (message_result, native_result) {
            (Ok(()), Ok(())) => Ok(InputDelivery::WindowsMessageAndNativeMouse),
            (Ok(()), Err(native_error)) => {
                eprintln!("Windows native mouse injection unavailable: {native_error}");
                Ok(InputDelivery::WindowsMessage)
            }
            (Err(message_error), Ok(())) => {
                eprintln!("Win32 message injection unavailable: {message_error}");
                Ok(InputDelivery::NativeMouse)
            }
            (Err(message_error), Err(native_error)) => Err(format!(
                "Win32 message injection failed ({message_error}); native mouse injection failed ({native_error})"
            )),
        }
    }

    fn native_mouse_drag(from: POINT, to: POINT) -> Result<(), String> {
        position_cursor(from.x, from.y)?;
        unsafe {
            pump_events();
        }
        send_mouse_input(MOUSEEVENTF_LEFTDOWN)?;
        unsafe {
            pump_events();
        }
        let drag_result = (|| {
            for step in 1..=12 {
                let x = from.x + (to.x - from.x) * step / 12;
                let y = from.y + (to.y - from.y) * step / 12;
                position_cursor(x, y)?;
                send_mouse_input(MOUSEEVENTF_MOVE)?;
                unsafe {
                    pump_events();
                }
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            Ok(())
        })();
        let release_result = send_mouse_input(MOUSEEVENTF_LEFTUP);
        unsafe {
            pump_events();
        }
        drag_result.and(release_result)
    }
    #[derive(Clone, Copy)]
    struct UiAutomationDrag {
        from: AutomationPoint,
        to: AutomationPoint,
    }

    const UIA_HELPER_COMMAND: &str = "__windows-uia-range-drag";
    const UIA_HELPER_TARGET_PREFIX: &str = "SUNMAO_UIA_TARGET=";
    const UIA_HELPER_TIMEOUT: Duration = Duration::from_secs(5);

    fn collect_uia_helper_output(child: &mut std::process::Child) -> (String, String) {
        const MAX_OUTPUT: u64 = 16 * 1024;
        let stdout = child.stdout.take().map(|stream| {
            let mut limited = stream.take(MAX_OUTPUT);
            let mut output = String::new();
            let _ = limited.read_to_string(&mut output);
            output
        });
        let stderr = child.stderr.take().map(|stream| {
            let mut limited = stream.take(MAX_OUTPUT);
            let mut output = String::new();
            let _ = limited.read_to_string(&mut output);
            output
        });
        (stdout.unwrap_or_default(), stderr.unwrap_or_default())
    }

    fn ui_automation_range_drag(root: HWND, from: POINT, to: POINT) -> Result<f64, String> {
        // WebView2 exposes its remote accessibility fragments to an external
        // UIA client, but omits them when the hosting process queries itself.
        let executable = std::env::current_exe()
            .map_err(|error| format!("locating UI Automation helper executable failed: {error}"))?;
        let mut child = Command::new(executable)
            .arg(UIA_HELPER_COMMAND)
            .arg((root as usize).to_string())
            .arg(std::process::id().to_string())
            .arg(from.x.to_string())
            .arg(from.y.to_string())
            .arg(to.x.to_string())
            .arg(to.y.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("starting UI Automation helper failed: {error}"))?;
        let deadline = Instant::now() + UIA_HELPER_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let (_, stderr) = collect_uia_helper_output(&mut child);
                    let detail = stderr.trim();
                    return Err(if detail.is_empty() {
                        format!(
                            "external UI Automation helper timed out after {} ms",
                            UIA_HELPER_TIMEOUT.as_millis()
                        )
                    } else {
                        format!(
                            "external UI Automation helper timed out after {} ms: {detail}",
                            UIA_HELPER_TIMEOUT.as_millis()
                        )
                    });
                }
                Ok(None) => {
                    unsafe {
                        pump_events();
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("polling UI Automation helper failed: {error}"));
                }
            }
        };
        let (stdout, stderr) = collect_uia_helper_output(&mut child);
        if !stderr.trim().is_empty() {
            eprintln!("Windows UI Automation helper: {}", stderr.trim());
        }
        if !status.success() {
            return Err(format!(
                "external UI Automation helper exited with {status}: {}",
                stdout.trim()
            ));
        }
        let target = stdout.lines().find_map(|line| {
            line.strip_prefix(UIA_HELPER_TARGET_PREFIX)
                .and_then(|value| value.trim().parse::<f64>().ok())
        });
        target
            .filter(|value| value.is_finite())
            .ok_or_else(|| "external UI Automation helper returned no target value".into())
    }

    pub(super) fn run_ui_automation_helper(args: &[String]) -> Result<f64, String> {
        if args.len() != 6 {
            return Err(format!(
                "{UIA_HELPER_COMMAND} expects root, pid, from-x, from-y, to-x, to-y"
            ));
        }
        let parse = |index: usize, name: &str| {
            args[index]
                .parse::<i64>()
                .map_err(|error| format!("invalid {name} '{}': {error}", args[index]))
        };
        let root = usize::try_from(parse(0, "root HWND")?)
            .map_err(|_| "root HWND does not fit in usize".to_string())?;
        let process_id = u32::try_from(parse(1, "process id")?)
            .map_err(|_| "process id does not fit in u32".to_string())?;
        let point = |x_index: usize, y_index: usize| -> Result<AutomationPoint, String> {
            Ok(AutomationPoint {
                x: i32::try_from(parse(x_index, "x coordinate")?)
                    .map_err(|_| "x coordinate does not fit in i32".to_string())?,
                y: i32::try_from(parse(y_index, "y coordinate")?)
                    .map_err(|_| "y coordinate does not fit in i32".to_string())?,
            })
        };
        let drag = UiAutomationDrag {
            from: point(2, 3)?,
            to: point(4, 5)?,
        };
        ui_automation_set_range(process_id, root, drag)
    }

    fn ui_automation_set_range(
        process_id: u32,
        root: usize,
        drag: UiAutomationDrag,
    ) -> Result<f64, String> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if initialized.is_err() {
            return Err(format!(
                "CoInitializeEx(COINIT_MULTITHREADED) failed: {initialized:?}"
            ));
        }

        let result = (|| unsafe {
            let automation: IUIAutomation6 =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| format!("creating IUIAutomation failed: {error}"))?;
            // WebView2 exposes its DOM through a remote provider. Start at the
            // desktop and use the same PID lookup path as AutomationElement;
            // resolving the same HWND directly can omit remote fragments.
            let mut native_window_variant = VARIANT::default();
            let native_window_data = &mut *native_window_variant.Anonymous.Anonymous;
            native_window_data.vt = VT_I4;
            native_window_data.Anonymous.lVal = root as isize as i32;
            let native_window_condition = automation
                .CreatePropertyCondition(UIA_NativeWindowHandlePropertyId, &native_window_variant)
                .map_err(|error| {
                    format!("creating native-window UI Automation condition failed: {error}")
                })?;
            let mut process_variant = VARIANT::default();
            let process_data = &mut *process_variant.Anonymous.Anonymous;
            process_data.vt = VT_I4;
            process_data.Anonymous.lVal = process_id as i32;
            let process_condition = automation
                .CreatePropertyCondition(UIA_ProcessIdPropertyId, &process_variant)
                .map_err(|error| {
                    format!("creating process UI Automation condition failed: {error}")
                })?;
            let desktop = automation
                .GetRootElement()
                .map_err(|error| format!("IUIAutomation::GetRootElement failed: {error}"))?;
            let host = match desktop.FindFirst(TreeScope_Descendants, &process_condition) {
                Ok(host) => {
                    let returned_window = host
                        .CurrentNativeWindowHandle()
                        .ok()
                        .map(|window| window.0 as usize);
                    if returned_window.is_none() || returned_window == Some(root) {
                        host
                    } else {
                        eprintln!(
                            "Windows UI Automation PID lookup returned HWND {returned_window:?}, expected {root:#x}; trying exact HWND condition"
                        );
                        desktop
                            .FindFirst(TreeScope_Descendants, &native_window_condition)
                            .map_err(|error| {
                                format!(
                                    "PID UI Automation host HWND mismatch and exact HWND lookup failed: {error}"
                                )
                            })?
                    }
                }
                Err(process_error) => {
                    match desktop.FindFirst(TreeScope_Descendants, &native_window_condition) {
                        Ok(host) => host,
                        Err(native_error) => automation
                            .ElementFromHandle(AutomationHwnd(root as *mut c_void))
                            .map_err(|handle_error| {
                                format!(
                                    "process UI Automation search failed ({process_error}); native-window search failed ({native_error}); ElementFromHandle failed: {handle_error}"
                                )
                            })?,
                    }
                }
            };
            eprintln!(
                "Windows UI Automation host: requested HWND={root:#x}, returned HWND={:?}, pid={:?}",
                host.CurrentNativeWindowHandle()
                    .ok()
                    .map(|window| window.0 as usize),
                host.CurrentProcessId().ok(),
            );
            let mut slider_type_variant = VARIANT::default();
            let slider_type_data = &mut *slider_type_variant.Anonymous.Anonymous;
            slider_type_data.vt = VT_I4;
            slider_type_data.Anonymous.lVal = UIA_SliderControlTypeId.0;
            let slider_condition = automation
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &slider_type_variant)
                .map_err(|error| {
                    format!("creating slider UI Automation condition failed: {error}")
                })?;
            // Chromium's remote provider may connect its fragment during the
            // first targeted query. Keep that query as a preflight, then use
            // FindAll so drag-point scoring still considers every slider.
            let mut first_slider = match host.FindFirst(TreeScope_Descendants, &slider_condition) {
                Ok(element) => Some(element),
                Err(error) => {
                    eprintln!(
                        "Windows UI Automation FindFirst preflight returned no slider; continuing with FindAll ({error})"
                    );
                    None
                }
            };
            let mut slider_elements = match host.FindAll(TreeScope_Descendants, &slider_condition) {
                Ok(descendants) => {
                    let count = descendants.Length().map_err(|error| {
                        format!("reading UI Automation result count failed: {error}")
                    })?;
                    (0..count)
                        .filter_map(|index| descendants.GetElement(index).ok())
                        .collect::<Vec<_>>()
                }
                Err(all_error) => {
                    eprintln!("Windows UI Automation FindAll failed: {all_error}");
                    Vec::new()
                }
            };
            if slider_elements.is_empty() {
                slider_elements.extend(first_slider.take());
            }

            struct SliderCandidate {
                pattern: IUIAutomationRangeValuePattern,
                bounds: AutomationRect,
                minimum: f64,
                maximum: f64,
                current: f64,
                horizontal: bool,
                point_score: u8,
            }

            let mut candidates = Vec::new();
            let mut slider_errors = Vec::new();
            for (index, element) in slider_elements.into_iter().enumerate() {
                if element.CurrentControlType().ok() != Some(UIA_SliderControlTypeId) {
                    continue;
                }
                let pattern: IUIAutomationRangeValuePattern =
                    match element.GetCurrentPatternAs(UIA_RangeValuePatternId) {
                        Ok(pattern) => pattern,
                        Err(error) => {
                            slider_errors.push(format!(
                                "slider {index} RangeValue pattern unavailable: {error}"
                            ));
                            continue;
                        }
                    };
                let bounds = match element.CurrentBoundingRectangle() {
                    Ok(bounds) if bounds.right > bounds.left && bounds.bottom > bounds.top => {
                        bounds
                    }
                    Ok(bounds) => {
                        slider_errors.push(format!(
                            "slider {index} reported invalid rectangle ({},{})-({},{})",
                            bounds.left, bounds.top, bounds.right, bounds.bottom
                        ));
                        continue;
                    }
                    Err(error) => {
                        slider_errors
                            .push(format!("reading slider {index} rectangle failed: {error}"));
                        continue;
                    }
                };
                let (minimum, maximum, current) = match (
                    pattern.CurrentMinimum(),
                    pattern.CurrentMaximum(),
                    pattern.CurrentValue(),
                ) {
                    (Ok(minimum), Ok(maximum), Ok(current)) => (minimum, maximum, current),
                    values => {
                        slider_errors
                            .push(format!("reading slider {index} range failed: {values:?}"));
                        continue;
                    }
                };
                if !minimum.is_finite() || !maximum.is_finite() || maximum <= minimum {
                    slider_errors.push(format!(
                        "slider {index} reported invalid range {minimum}..{maximum}"
                    ));
                    continue;
                }
                let width = bounds.right - bounds.left;
                let height = bounds.bottom - bounds.top;
                let horizontal = match element.CurrentOrientation().ok() {
                    Some(OrientationType_Horizontal) => true,
                    Some(OrientationType_Vertical) => false,
                    _ => width >= height,
                };
                let point_score = drag_rectangle_score(
                    (bounds.left, bounds.top, bounds.right, bounds.bottom),
                    (drag.from.x, drag.from.y),
                    (drag.to.x, drag.to.y),
                );
                candidates.push(SliderCandidate {
                    pattern,
                    bounds,
                    minimum,
                    maximum,
                    current,
                    horizontal,
                    point_score,
                });
            }

            let candidate_count = candidates.len();
            let Some(candidate) = candidates
                .into_iter()
                .max_by_key(|candidate| candidate.point_score)
            else {
                let details = if slider_errors.is_empty() {
                    String::new()
                } else {
                    format!(": {}", slider_errors.join("; "))
                };
                return Err(format!(
                    "UI Automation found no usable slider under the WebView host{details}"
                ));
            };
            if candidate.point_score == 0 {
                return Err(format!(
                    "UI Automation found {candidate_count} usable slider(s), but none intersected drag ({},{})-({},{}); nearest slider was ({},{})-({},{})",
                    drag.from.x,
                    drag.from.y,
                    drag.to.x,
                    drag.to.y,
                    candidate.bounds.left,
                    candidate.bounds.top,
                    candidate.bounds.right,
                    candidate.bounds.bottom,
                ));
            }

            let delta = if candidate.horizontal {
                f64::from(drag.to.x - drag.from.x)
                    / f64::from(candidate.bounds.right - candidate.bounds.left)
            } else {
                f64::from(drag.from.y - drag.to.y)
                    / f64::from(candidate.bounds.bottom - candidate.bounds.top)
            };
            let target = (candidate.current + delta * (candidate.maximum - candidate.minimum))
                .clamp(candidate.minimum, candidate.maximum);
            if (target - candidate.current).abs() <= f64::EPSILON {
                return Err("UI Automation drag produced no slider value change".into());
            }
            candidate
                .pattern
                .SetValue(target)
                .map_err(|error| format!("setting slider value failed: {error}"))?;
            Ok(target)
        })();

        unsafe { CoUninitialize() };
        result
    }

    fn position_cursor(x: i32, y: i32) -> Result<(), String> {
        if unsafe { SetCursorPos(x, y) } == 0 {
            return Err(format!(
                "SetCursorPos({x},{y}) failed with error {}",
                unsafe { GetLastError() }
            ));
        }
        let mut actual = POINT::default();
        if unsafe { GetCursorPos(&mut actual) } == 0 {
            return Err(format!("GetCursorPos failed with error {}", unsafe {
                GetLastError()
            }));
        }
        if actual.x != x || actual.y != y {
            return Err(format!(
                "SetCursorPos requested ({x},{y}), but GetCursorPos returned ({},{})",
                actual.x, actual.y
            ));
        }
        Ok(())
    }

    fn post_message_drag(
        window: HWND,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    ) -> Result<(), String> {
        const MK_LBUTTON: WPARAM = 0x0001;
        let from = (from_x.round() as i32, from_y.round() as i32);
        let to = (to_x.round() as i32, to_y.round() as i32);
        post_mouse_message(window, WM_MOUSEMOVE, 0, from.0, from.1)?;
        post_mouse_message(window, WM_LBUTTONDOWN, MK_LBUTTON, from.0, from.1)?;
        for step in 1..=12 {
            let x = from.0 + (to.0 - from.0) * step / 12;
            let y = from.1 + (to.1 - from.1) * step / 12;
            post_mouse_message(window, WM_MOUSEMOVE, MK_LBUTTON, x, y)?;
        }
        post_mouse_message(window, WM_LBUTTONUP, 0, to.0, to.1)
    }

    fn post_mouse_message(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        x: i32,
        y: i32,
    ) -> Result<(), String> {
        let lparam = ((y as u32 & 0xffff) << 16 | (x as u32 & 0xffff)) as LPARAM;
        if unsafe { PostMessageW(window, message, wparam, lparam) } != 0 {
            Ok(())
        } else {
            Err(format!(
                "PostMessageW(0x{message:04x}, {x},{y}) failed with error {}",
                unsafe { GetLastError() }
            ))
        }
    }

    fn send_mouse_input(flags: u32) -> Result<(), String> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } == 1 {
            Ok(())
        } else {
            Err(format!("SendInput failed with error {}", unsafe {
                GetLastError()
            }))
        }
    }
}

// ---- Linux implementation ----

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
    use std::sync::OnceLock;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;
    use x11_dl::{xlib, xtest};

    const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(8);
    static X11_INITIALIZED: OnceLock<Result<xlib::Xlib, String>> = OnceLock::new();

    pub(super) fn initialize() -> Result<(), String> {
        match X11_INITIALIZED.get_or_init(|| {
            let xlib =
                xlib::Xlib::open().map_err(|error| format!("failed to load Xlib: {error}"))?;
            if unsafe { (xlib.XInitThreads)() } == 0 {
                Err("XInitThreads failed; X11 GUI hosting requires thread-safe Xlib".into())
            } else {
                Ok(xlib)
            }
        }) {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        }
    }

    enum WindowCommand {
        Resize(u32, u32),
        Close,
    }

    pub(super) struct LinuxWindow {
        window: xlib::Window,
        command_tx: Sender<WindowCommand>,
        event_thread: Option<JoinHandle<()>>,
    }

    pub(super) fn attachment_handle(window: xlib::Window) -> *mut c_void {
        window as usize as *mut c_void
    }

    unsafe fn plugin_surface(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        parent: xlib::Window,
    ) -> Result<(xlib::Window, u32, u32), String> {
        let mut surface = parent;
        let mut surface_size = None;

        // GL/WGPU usually draw in the first child. WebKitGTK adds a Wry
        // container and a native WebKit child below that, so follow the
        // largest visible branch until reaching the drawable content leaf.
        for _ in 0..32 {
            let mut root = 0;
            let mut returned_parent = 0;
            let mut children = ptr::null_mut();
            let mut child_count = 0;
            if unsafe {
                (xlib.XQueryTree)(
                    display,
                    surface,
                    &mut root,
                    &mut returned_parent,
                    &mut children,
                    &mut child_count,
                )
            } == 0
            {
                return Err("XQueryTree failed while locating the plugin surface".into());
            }

            let mut best = None;
            let mut best_area = 0_u64;
            for index in 0..child_count as usize {
                let child = unsafe { *children.add(index) };
                let mut attributes: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
                if unsafe { (xlib.XGetWindowAttributes)(display, child, &mut attributes) } == 0
                    || attributes.map_state != xlib::IsViewable
                    || attributes.class != xlib::InputOutput
                    || attributes.width <= 0
                    || attributes.height <= 0
                {
                    continue;
                }
                let area = attributes.width as u64 * attributes.height as u64;
                if area > best_area {
                    best = Some((child, attributes.width as u32, attributes.height as u32));
                    best_area = area;
                }
            }
            if !children.is_null() {
                unsafe { (xlib.XFree)(children.cast()) };
            }

            match best {
                Some((child, width, height)) => {
                    surface = child;
                    surface_size = Some((width, height));
                }
                None => {
                    return surface_size
                        .map(|(width, height)| (surface, width, height))
                        .ok_or_else(|| {
                            "X11 host content has no visible plugin child window".into()
                        });
                }
            }
        }

        Err("X11 plugin window hierarchy exceeds 32 levels".into())
    }

    impl LinuxWindow {
        pub(super) fn attachment_handle(&self) -> *mut c_void {
            attachment_handle(self.window)
        }

        pub(super) fn set_content_size(&self, width: f64, height: f64) {
            let (width, height) = native_pixel_size(width, height);
            let _ = self.command_tx.send(WindowCommand::Resize(width, height));
        }

        pub(super) fn verify_non_uniform_pixels(&self) -> Result<PixelEvidence, String> {
            let xlib =
                xlib::Xlib::open().map_err(|error| format!("failed to load Xlib: {error}"))?;
            let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
            if display.is_null() {
                return Err("XOpenDisplay failed during GUI capture".into());
            }
            let result = unsafe {
                (|| -> Result<PixelEvidence, String> {
                    let (surface, width, height) = plugin_surface(&xlib, display, self.window)?;
                    (xlib.XRaiseWindow)(display, self.window);
                    (xlib.XSync)(display, xlib::False);
                    let image = (xlib.XGetImage)(
                        display,
                        surface,
                        0,
                        0,
                        width,
                        height,
                        (xlib.XAllPlanes)(),
                        xlib::ZPixmap,
                    );
                    if image.is_null() {
                        Err("XGetImage returned null for the plugin GUI".into())
                    } else {
                        let step_x = (width / 64).max(1) as usize;
                        let step_y = (height / 64).max(1) as usize;
                        let pixels = (0..height as usize).step_by(step_y).flat_map(|y| {
                            (0..width as usize)
                                .step_by(step_x)
                                .map(move |x| (xlib.XGetPixel)(image, x as i32, y as i32) as u32)
                        });
                        let evidence = non_uniform_pixel_evidence(width, height, pixels);
                        (xlib.XDestroyImage)(image);
                        evidence
                    }
                })()
            };
            unsafe { (xlib.XCloseDisplay)(display) };
            result
        }

        pub(super) fn drag_slider(
            &self,
            from_x: f64,
            from_y: f64,
            to_x: f64,
            to_y: f64,
        ) -> Result<InputDelivery, String> {
            let xlib =
                xlib::Xlib::open().map_err(|error| format!("failed to load Xlib: {error}"))?;
            let xtest = xtest::Xf86vmode::open()
                .map_err(|error| format!("failed to load XTEST: {error}"))?;
            let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
            if display.is_null() {
                return Err("XOpenDisplay failed during GUI input injection".into());
            }
            let result = unsafe {
                (|| -> Result<(), String> {
                    let (surface, width, height) = plugin_surface(&xlib, display, self.window)?;
                    for (label, x, y) in [("start", from_x, from_y), ("end", to_x, to_y)] {
                        if !x.is_finite()
                            || !y.is_finite()
                            || x < 0.0
                            || y < 0.0
                            || x >= f64::from(width)
                            || y >= f64::from(height)
                        {
                            return Err(format!(
                                "X11 drag {label} ({x:.1},{y:.1}) is outside plugin content {}x{}",
                                width, height
                            ));
                        }
                    }

                    let mut event_base = 0;
                    let mut error_base = 0;
                    let mut major = 0;
                    let mut minor = 0;
                    if (xtest.XTestQueryExtension)(
                        display,
                        &mut event_base,
                        &mut error_base,
                        &mut major,
                        &mut minor,
                    ) == 0
                    {
                        return Err("the X11 server does not provide the XTEST extension".into());
                    }

                    let root = (xlib.XDefaultRootWindow)(display);
                    let root_point = |x: f64, y: f64| -> Result<(i32, i32), String> {
                        let mut root_x = 0;
                        let mut root_y = 0;
                        let mut child = 0;
                        if (xlib.XTranslateCoordinates)(
                            display,
                            surface,
                            root,
                            x.round() as i32,
                            y.round() as i32,
                            &mut root_x,
                            &mut root_y,
                            &mut child,
                        ) == 0
                        {
                            Err("XTranslateCoordinates failed during GUI input injection".into())
                        } else {
                            Ok((root_x, root_y))
                        }
                    };
                    let from = root_point(from_x, from_y)?;
                    let to = root_point(to_x, to_y)?;
                    let screen = (xlib.XDefaultScreen)(display);
                    (xlib.XRaiseWindow)(display, self.window);
                    (xlib.XSetInputFocus)(
                        display,
                        self.window,
                        xlib::RevertToParent,
                        xlib::CurrentTime,
                    );
                    (xtest.XTestFakeMotionEvent)(
                        display,
                        screen,
                        from.0,
                        from.1,
                        xlib::CurrentTime,
                    );
                    (xtest.XTestFakeButtonEvent)(display, 1, xlib::True, xlib::CurrentTime);
                    for step in 1..=12 {
                        let x = from.0 + (to.0 - from.0) * step / 12;
                        let y = from.1 + (to.1 - from.1) * step / 12;
                        (xtest.XTestFakeMotionEvent)(display, screen, x, y, xlib::CurrentTime);
                    }
                    (xtest.XTestFakeButtonEvent)(display, 1, xlib::False, xlib::CurrentTime);
                    (xlib.XSync)(display, xlib::False);
                    Ok(())
                })()
            };
            unsafe { (xlib.XCloseDisplay)(display) };
            result.map(|()| InputDelivery::NativeMouse)
        }

        fn close(&mut self) {
            let _ = self.command_tx.send(WindowCommand::Close);
            if let Some(event_thread) = self.event_thread.take() {
                let _ = event_thread.join();
            }
        }
    }

    impl Drop for LinuxWindow {
        fn drop(&mut self) {
            self.close();
        }
    }

    pub(super) fn create_window(
        title: &str,
        width: f64,
        height: f64,
        close_cb: Box<dyn Fn() + Send>,
    ) -> Result<LinuxWindow, String> {
        let title = CString::new(title).map_err(|_| "window title contains a NUL byte")?;
        let (width, height) = native_pixel_size(width, height);
        let (command_tx, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let event_thread = thread::Builder::new()
            .name("sunmao-runner-x11-window".into())
            .spawn(move || {
                run_event_loop(title, width, height, close_cb, command_rx, ready_tx);
            })
            .map_err(|error| format!("failed to start X11 window thread: {error}"))?;

        match ready_rx.recv() {
            Ok(Ok(window)) => Ok(LinuxWindow {
                window,
                command_tx,
                event_thread: Some(event_thread),
            }),
            Ok(Err(error)) => {
                let _ = event_thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = event_thread.join();
                Err("X11 window thread exited during initialization".into())
            }
        }
    }

    fn run_event_loop(
        title: CString,
        width: u32,
        height: u32,
        close_cb: Box<dyn Fn() + Send>,
        command_rx: Receiver<WindowCommand>,
        ready_tx: mpsc::SyncSender<Result<xlib::Window, String>>,
    ) {
        let xlib = match xlib::Xlib::open() {
            Ok(xlib) => xlib,
            Err(error) => {
                let _ = ready_tx.send(Err(format!("failed to load Xlib: {error}")));
                return;
            }
        };
        let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
        if display.is_null() {
            let _ = ready_tx.send(Err(
                "XOpenDisplay failed; ensure DISPLAY points to an X11 server".into(),
            ));
            return;
        }

        let screen = unsafe { (xlib.XDefaultScreen)(display) };
        let root = unsafe { (xlib.XRootWindow)(display, screen) };
        let black = unsafe { (xlib.XBlackPixel)(display, screen) };
        let white = unsafe { (xlib.XWhitePixel)(display, screen) };
        let window = unsafe {
            (xlib.XCreateSimpleWindow)(display, root, 0, 0, width, height, 0, black, white)
        };
        if window == 0 {
            unsafe {
                (xlib.XCloseDisplay)(display);
            }
            let _ = ready_tx.send(Err("XCreateSimpleWindow failed".into()));
            return;
        }

        let wm_protocols =
            unsafe { (xlib.XInternAtom)(display, b"WM_PROTOCOLS\0".as_ptr().cast(), xlib::False) };
        let wm_delete = unsafe {
            (xlib.XInternAtom)(display, b"WM_DELETE_WINDOW\0".as_ptr().cast(), xlib::False)
        };
        unsafe {
            (xlib.XStoreName)(display, window, title.as_ptr());
            (xlib.XSelectInput)(display, window, xlib::StructureNotifyMask);
            set_fixed_size_hints(&xlib, display, window, width, height);
            if wm_delete != 0 {
                let mut protocol = wm_delete;
                (xlib.XSetWMProtocols)(display, window, &mut protocol, 1);
            }
            (xlib.XMapWindow)(display, window);
            (xlib.XFlush)(display);
        }

        if ready_tx.send(Ok(window)).is_err() {
            unsafe {
                (xlib.XDestroyWindow)(display, window);
                (xlib.XCloseDisplay)(display);
            }
            return;
        }

        let mut close_cb = Some(close_cb);
        'event_loop: loop {
            match command_rx.recv_timeout(EVENT_POLL_INTERVAL) {
                Ok(WindowCommand::Resize(width, height)) => unsafe {
                    set_fixed_size_hints(&xlib, display, window, width, height);
                    (xlib.XResizeWindow)(display, window, width, height);
                    (xlib.XFlush)(display);
                },
                Ok(WindowCommand::Close) | Err(RecvTimeoutError::Disconnected) => {
                    break 'event_loop;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }

            loop {
                match command_rx.try_recv() {
                    Ok(WindowCommand::Resize(width, height)) => unsafe {
                        set_fixed_size_hints(&xlib, display, window, width, height);
                        (xlib.XResizeWindow)(display, window, width, height);
                    },
                    Ok(WindowCommand::Close) | Err(TryRecvError::Disconnected) => {
                        break 'event_loop;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }

            unsafe {
                while (xlib.XPending)(display) > 0 {
                    let mut event: xlib::XEvent = std::mem::zeroed();
                    (xlib.XNextEvent)(display, &mut event);
                    if event.get_type() == xlib::ClientMessage {
                        let client_message = event.client_message;
                        if client_message.window == window
                            && client_message.message_type == wm_protocols
                            && client_message.data.as_longs()[0] == wm_delete as libc::c_long
                        {
                            if let Some(close_cb) = close_cb.take() {
                                close_cb();
                            }
                        }
                    }
                }
            }
        }

        unsafe {
            (xlib.XDestroyWindow)(display, window);
            (xlib.XFlush)(display);
            (xlib.XCloseDisplay)(display);
        }
    }

    unsafe fn set_fixed_size_hints(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        window: xlib::Window,
        width: u32,
        height: u32,
    ) {
        let width = width.min(i32::MAX as u32) as i32;
        let height = height.min(i32::MAX as u32) as i32;
        let mut hints: xlib::XSizeHints = unsafe { std::mem::zeroed() };
        hints.flags = xlib::PMinSize | xlib::PMaxSize;
        hints.min_width = width;
        hints.max_width = width;
        hints.min_height = height;
        hints.max_height = height;
        unsafe {
            (xlib.XSetWMNormalHints)(display, window, &mut hints);
        }
    }
}

// ---- macOS implementation ----

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
    use core_graphics::display::{
        kCGWindowImageBoundsIgnoreFraming, kCGWindowImageNominalResolution,
        kCGWindowListOptionIncludingWindow, CGDisplay,
    };
    use objc_ffi::*;
    use std::time::Duration;

    const NS_LEFT_MOUSE_DOWN: u64 = 1;
    const NS_LEFT_MOUSE_UP: u64 = 2;
    const NS_MOUSE_MOVED: u64 = 5;
    const NS_LEFT_MOUSE_DRAGGED: u64 = 6;

    pub(super) fn pump_events() -> bool {
        let _ = CFRunLoop::run_in_mode(
            unsafe { kCFRunLoopDefaultMode },
            Duration::from_millis(1),
            false,
        );
        unsafe {
            let _pool = AutoreleasePool::new();
            let application_class = objc_getClass(b"NSApplication\0".as_ptr().cast());
            let shared_selector = sel_registerName(b"sharedApplication\0".as_ptr().cast());
            let application = msg_send0(application_class, shared_selector);
            if application.is_null() {
                return false;
            }

            let date_class = objc_getClass(b"NSDate\0".as_ptr().cast());
            let distant_past_selector = sel_registerName(b"distantPast\0".as_ptr().cast());
            let distant_past = msg_send0(date_class, distant_past_selector);
            let string_class = objc_getClass(b"NSString\0".as_ptr().cast());
            let string_selector = sel_registerName(b"stringWithUTF8String:\0".as_ptr().cast());
            let mode = msg_send1(
                string_class,
                string_selector,
                b"kCFRunLoopDefaultMode\0".as_ptr().cast_mut().cast(),
            );
            let next_selector = sel_registerName(
                b"nextEventMatchingMask:untilDate:inMode:dequeue:\0"
                    .as_ptr()
                    .cast(),
            );
            let next_event: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                u64,
                *mut c_void,
                *mut c_void,
                bool,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const c_void);
            let send_selector = sel_registerName(b"sendEvent:\0".as_ptr().cast());
            let send_event: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const c_void);
            for _ in 0..256 {
                let event = next_event(
                    application,
                    next_selector,
                    u64::MAX,
                    distant_past,
                    mode,
                    true,
                );
                if event.is_null() {
                    break;
                }
                send_event(application, send_selector, event);
            }
            let update_selector = sel_registerName(b"updateWindows\0".as_ptr().cast());
            msg_send_void(application, update_selector);
        }
        true
    }

    pub(super) fn set_content_size(window: *mut c_void, width: f64, height: f64) {
        if window.is_null() || !width.is_finite() || !height.is_finite() {
            return;
        }
        unsafe {
            let selector = sel_registerName(b"setContentSize:\0".as_ptr().cast());
            msg_send_size_void(
                window,
                selector,
                NSSize {
                    width: width.max(1.0),
                    height: height.max(1.0),
                },
            );
        }
    }

    unsafe fn plugin_surface(content_view: *mut c_void) -> Result<(*mut c_void, NSRect), String> {
        if content_view.is_null() {
            return Err("macOS plugin content view is null".into());
        }
        let subviews_selector = sel_registerName(b"subviews\0".as_ptr().cast());
        let subviews = msg_send0(content_view, subviews_selector);
        if subviews.is_null() {
            return Err("macOS host content has no subview collection".into());
        }
        let count_selector = sel_registerName(b"count\0".as_ptr().cast());
        let count_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const c_void);
        let object_selector = sel_registerName(b"objectAtIndex:\0".as_ptr().cast());
        let object_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        let frame_selector = sel_registerName(b"frame\0".as_ptr().cast());

        let mut largest = None;
        let mut largest_area = 0.0;
        for index in 0..count_fn(subviews, count_selector) {
            let view = object_fn(subviews, object_selector, index);
            if view.is_null() {
                continue;
            }
            let frame = msg_send_rect0(view, frame_selector);
            let area = frame.size.width.max(0.0) * frame.size.height.max(0.0);
            if area > largest_area {
                largest = Some((view, frame));
                largest_area = area;
            }
        }
        largest.ok_or_else(|| "macOS host content has no visible plugin child view".into())
    }

    unsafe fn find_javascript_webview(view: *mut c_void) -> *mut c_void {
        if view.is_null() {
            return ptr::null_mut();
        }
        let evaluate_selector =
            sel_registerName(b"evaluateJavaScript:completionHandler:\0".as_ptr().cast());
        let responds_selector = sel_registerName(b"respondsToSelector:\0".as_ptr().cast());
        let responds: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool =
            std::mem::transmute(objc_msgSend as *const c_void);
        let subviews_selector = sel_registerName(b"subviews\0".as_ptr().cast());
        let count_selector = sel_registerName(b"count\0".as_ptr().cast());
        let object_selector = sel_registerName(b"objectAtIndex:\0".as_ptr().cast());
        let count_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const c_void);
        let object_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);

        unsafe fn walk(
            view: *mut c_void,
            depth: usize,
            evaluate_selector: *mut c_void,
            responds_selector: *mut c_void,
            responds: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
            subviews_selector: *mut c_void,
            count_selector: *mut c_void,
            object_selector: *mut c_void,
            count_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize,
            object_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void,
        ) -> *mut c_void {
            if view.is_null() || depth > 24 {
                return ptr::null_mut();
            }
            if responds(view, responds_selector, evaluate_selector) {
                return view;
            }
            let subviews = msg_send0(view, subviews_selector);
            if subviews.is_null() {
                return ptr::null_mut();
            }
            for index in 0..count_fn(subviews, count_selector) {
                let child = object_fn(subviews, object_selector, index);
                let found = unsafe {
                    walk(
                        child,
                        depth + 1,
                        evaluate_selector,
                        responds_selector,
                        responds,
                        subviews_selector,
                        count_selector,
                        object_selector,
                        count_fn,
                        object_fn,
                    )
                };
                if !found.is_null() {
                    return found;
                }
            }
            ptr::null_mut()
        }

        unsafe {
            walk(
                view,
                0,
                evaluate_selector,
                responds_selector,
                responds,
                subviews_selector,
                count_selector,
                object_selector,
                count_fn,
                object_fn,
            )
        }
    }

    unsafe fn post_webview_dom_drag(
        webview: *mut c_void,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    ) -> Result<InputDelivery, String> {
        let script = format!(
            "(() => {{\n\
             const startX = {from_x:.3}; const startY = {from_y:.3}; const endX = {to_x:.3}; const endY = {to_y:.3};\n\
             let element = document.elementFromPoint(startX, startY);\n\
             if (!(element instanceof HTMLInputElement) || element.type !== 'range') {{\n\
               element = document.querySelector('input[type=range]');\n\
             }}\n\
             if (!(element instanceof HTMLInputElement) || element.type !== 'range') return;\n\
             const rect = element.getBoundingClientRect();\n\
             const minimum = Number(element.min || 0); const maximum = Number(element.max || 100);\n\
             const ratio = Math.max(0, Math.min(1, (endX - rect.left) / Math.max(rect.width, 1)));\n\
             element.dispatchEvent(new PointerEvent('pointerdown', {{ bubbles: true, pointerId: 1, clientX: startX, clientY: startY, buttons: 1 }}));\n\
             element.value = String(minimum + (maximum - minimum) * ratio);\n\
             element.dispatchEvent(new Event('input', {{ bubbles: true }}));\n\
             element.dispatchEvent(new PointerEvent('pointerup', {{ bubbles: true, pointerId: 1, clientX: endX, clientY: endY, buttons: 0 }}));\n\
             }})();"
        );
        let script = CString::new(script).map_err(|_| "WebView DOM gesture contains a NUL")?;
        let string_class = objc_getClass(b"NSString\0".as_ptr().cast());
        let alloc_selector = sel_registerName(b"alloc\0".as_ptr().cast());
        let init_selector = sel_registerName(b"initWithUTF8String:\0".as_ptr().cast());
        let string = msg_send1(
            msg_send0(string_class, alloc_selector),
            init_selector,
            script.as_ptr().cast_mut().cast(),
        );
        if string.is_null() {
            return Err("failed to create the WebView DOM gesture script".into());
        }
        let evaluate_selector =
            sel_registerName(b"evaluateJavaScript:completionHandler:\0".as_ptr().cast());
        let evaluate: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const c_void);
        evaluate(webview, evaluate_selector, string, ptr::null_mut());
        let release_selector = sel_registerName(b"release\0".as_ptr().cast());
        msg_send_void(string, release_selector);
        for _ in 0..20 {
            pump_events();
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(InputDelivery::WebViewDom)
    }

    pub(super) fn verify_non_uniform_pixels(
        window: *mut c_void,
        content_view: *mut c_void,
    ) -> Result<PixelEvidence, String> {
        let screenshot = screenshot_non_uniform_pixels(window, content_view);
        if screenshot.is_ok() {
            return screenshot;
        }
        match unsafe { bitmap_non_uniform_pixels(content_view) } {
            Ok(evidence) => Ok(evidence),
            Err(bitmap_error) => Err(format!(
                "{}; {}",
                screenshot
                    .err()
                    .unwrap_or_else(|| "CoreGraphics capture failed".into()),
                bitmap_error
            )),
        }
    }

    fn screenshot_non_uniform_pixels(
        window: *mut c_void,
        content_view: *mut c_void,
    ) -> Result<PixelEvidence, String> {
        if window.is_null() || content_view.is_null() {
            return Err("macOS GUI window or content view is null".into());
        }
        let window_id: u32 = unsafe {
            let selector = sel_registerName(b"windowNumber\0".as_ptr().cast());
            let function: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const c_void);
            u32::try_from(function(window, selector))
                .map_err(|_| "macOS window number is outside the CoreGraphics range")?
        };
        let image = CGDisplay::screenshot(
            unsafe { core_graphics::display::CGRectNull },
            kCGWindowListOptionIncludingWindow,
            window_id,
            kCGWindowImageBoundsIgnoreFraming | kCGWindowImageNominalResolution,
        )
        .ok_or("CoreGraphics could not capture the plugin GUI window")?;
        let width = u32::try_from(image.width()).map_err(|_| "captured GUI width is too large")?;
        let height =
            u32::try_from(image.height()).map_err(|_| "captured GUI height is too large")?;
        if width == 0 || height == 0 {
            return Err("captured macOS GUI frame is empty".into());
        }
        let (surface, frame) = unsafe { plugin_surface(content_view)? };
        let surface_bounds = unsafe {
            let selector = sel_registerName(b"bounds\0".as_ptr().cast());
            msg_send_rect0(surface, selector)
        };
        let content_width = surface_bounds.size.width.round().max(0.0) as u32;
        let content_height = surface_bounds.size.height.round().max(0.0) as u32;
        if content_width == 0 || content_height == 0 {
            return Err("macOS plugin content view is empty".into());
        }
        let content_x = frame.origin.x.round();
        let content_y = f64::from(height) - (frame.origin.y + frame.size.height).round();
        if content_x < 0.0 || content_y < 0.0 {
            return Err("macOS plugin surface lies outside the captured window".into());
        }
        let content_x = content_x as u32;
        let content_y = content_y as u32;
        let available_width = width.saturating_sub(content_x);
        let available_height = height.saturating_sub(content_y);
        if content_width > available_width || content_height > available_height {
            return Err(format!(
                "macOS plugin surface {}x{} overflows captured host content {}x{}",
                content_width, content_height, available_width, available_height
            ));
        }
        let bytes_per_row = image.bytes_per_row();
        let bytes_per_pixel = image.bits_per_pixel().div_ceil(8);
        if bytes_per_pixel < 3 {
            return Err(format!(
                "unsupported CoreGraphics pixel format: {} bits per pixel",
                image.bits_per_pixel()
            ));
        }
        let data = image.data();
        let step_x = (content_width / 64).max(1) as usize;
        let step_y = (content_height / 64).max(1) as usize;
        let pixels = (0..content_height as usize)
            .step_by(step_y)
            .flat_map(|content_row| {
                (0..content_width as usize).step_by(step_x).filter_map({
                    let data = data.bytes();
                    move |content_column| {
                        let y = content_y as usize + content_row;
                        let x = content_x as usize + content_column;
                        let offset = y
                            .checked_mul(bytes_per_row)?
                            .checked_add(x.checked_mul(bytes_per_pixel)?)?;
                        let bytes = data.get(offset..offset + 3)?;
                        Some(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], 0]))
                    }
                })
            });
        non_uniform_pixel_evidence(content_width, content_height, pixels)
    }

    unsafe fn bitmap_non_uniform_pixels(
        content_view: *mut c_void,
    ) -> Result<PixelEvidence, String> {
        let _pool = AutoreleasePool::new();
        let javascript_webview = find_javascript_webview(content_view);
        let surface = if javascript_webview.is_null() {
            plugin_surface(content_view)?.0
        } else {
            javascript_webview
        };
        let bounds_selector = sel_registerName(b"bounds\0".as_ptr().cast());
        let bounds = msg_send_rect0(surface, bounds_selector);
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return Err("macOS plugin surface bounds are empty".into());
        }
        let display_selector = sel_registerName(b"displayIfNeeded\0".as_ptr().cast());
        msg_send_void(surface, display_selector);
        let bitmap_selector =
            sel_registerName(b"bitmapImageRepForCachingDisplayInRect:\0".as_ptr().cast());
        let rep = msg_send_rect(surface, bitmap_selector, bounds);
        if rep.is_null() {
            return Err("NSView bitmapImageRepForCachingDisplayInRect returned null".into());
        }
        let cache_selector =
            sel_registerName(b"cacheDisplayInRect:toBitmapImageRep:\0".as_ptr().cast());
        let cache: unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const c_void);
        cache(surface, cache_selector, bounds, rep);

        let wide_selector = sel_registerName(b"pixelsWide\0".as_ptr().cast());
        let high_selector = sel_registerName(b"pixelsHigh\0".as_ptr().cast());
        let size_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
            std::mem::transmute(objc_msgSend as *const c_void);
        let width = u32::try_from(size_fn(rep, wide_selector).max(0))
            .map_err(|_| "cached GUI width is too large")?;
        let height = u32::try_from(size_fn(rep, high_selector).max(0))
            .map_err(|_| "cached GUI height is too large")?;
        if width == 0 || height == 0 {
            return Err("cached macOS GUI bitmap is empty".into());
        }
        let row_selector = sel_registerName(b"bytesPerRow\0".as_ptr().cast());
        let bpp_selector = sel_registerName(b"bitsPerPixel\0".as_ptr().cast());
        let bytes_per_row = usize::try_from(size_fn(rep, row_selector).max(0))
            .map_err(|_| "cached GUI bytes-per-row is too large")?;
        let bits_per_pixel = size_fn(rep, bpp_selector).max(0) as usize;
        let bytes_per_pixel = bits_per_pixel.saturating_add(7) / 8;
        if bytes_per_pixel < 3 {
            return Err(format!(
                "unsupported NSBitmapImageRep pixel format: {bits_per_pixel} bits per pixel"
            ));
        }
        let data_selector = sel_registerName(b"bitmapData\0".as_ptr().cast());
        let data = msg_send0(rep, data_selector) as *const u8;
        if data.is_null() {
            return Err("NSBitmapImageRep bitmapData returned null".into());
        }
        let step_x = (width / 64).max(1) as usize;
        let step_y = (height / 64).max(1) as usize;
        let pixels = (0..height as usize).step_by(step_y).flat_map(|row| {
            (0..width as usize)
                .step_by(step_x)
                .filter_map(move |column| {
                    let offset = row
                        .checked_mul(bytes_per_row)?
                        .checked_add(column.checked_mul(bytes_per_pixel)?)?;
                    let bytes = unsafe { std::slice::from_raw_parts(data.add(offset), 3) };
                    Some(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], 0]))
                })
        });
        non_uniform_pixel_evidence(width, height, pixels)
    }

    pub(super) fn drag_slider(
        window: *mut c_void,
        content_view: *mut c_void,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    ) -> Result<InputDelivery, String> {
        if window.is_null() || content_view.is_null() {
            return Err("macOS GUI window or content view is null".into());
        }
        let (surface, _) = unsafe { plugin_surface(content_view)? };
        let bounds = unsafe {
            let selector = sel_registerName(b"bounds\0".as_ptr().cast());
            msg_send_rect0(surface, selector)
        };
        for (label, x, y) in [("start", from_x, from_y), ("end", to_x, to_y)] {
            if !x.is_finite()
                || !y.is_finite()
                || x < 0.0
                || y < 0.0
                || x >= bounds.size.width
                || y >= bounds.size.height
            {
                return Err(format!(
                    "macOS drag {label} ({x:.1},{y:.1}) is outside plugin content {:.0}x{:.0}",
                    bounds.size.width, bounds.size.height
                ));
            }
        }

        unsafe {
            let _pool = AutoreleasePool::new();
            let event_class = objc_getClass(b"NSEvent\0".as_ptr().cast());
            if event_class.is_null() {
                return Err("NSEvent class not found".into());
            }
            let window_number_selector = sel_registerName(b"windowNumber\0".as_ptr().cast());
            let window_number_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const c_void);
            let window_number = window_number_fn(window, window_number_selector);
            let convert_selector = sel_registerName(b"convertPoint:toView:\0".as_ptr().cast());
            let convert_fn: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                NSPoint,
                *mut c_void,
            ) -> NSPoint = std::mem::transmute(objc_msgSend as *const c_void);
            let event_selector = sel_registerName(
                b"mouseEventWithType:location:modifierFlags:timestamp:windowNumber:context:eventNumber:clickCount:pressure:\0"
                    .as_ptr()
                    .cast(),
            );
            let event_fn: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                u64,
                NSPoint,
                u64,
                f64,
                isize,
                *mut c_void,
                isize,
                isize,
                f32,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const c_void);
            let key_selector = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr().cast());
            msg_send1(window, key_selector, ptr::null_mut());
            let flipped_selector = sel_registerName(b"isFlipped\0".as_ptr().cast());
            let flipped_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool =
                std::mem::transmute(objc_msgSend as *const c_void);
            let is_flipped = flipped_fn(surface, flipped_selector);
            let surface_point = |x: f64, y: f64| NSPoint {
                x,
                y: if is_flipped {
                    y
                } else {
                    bounds.size.height - y
                },
            };
            let hit_test_selector = sel_registerName(b"hitTest:\0".as_ptr().cast());
            let hit_test_fn: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                NSPoint,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const c_void);
            let hit_target = hit_test_fn(surface, hit_test_selector, surface_point(from_x, from_y));
            if hit_target.is_null() {
                return Err("macOS hit testing found no responder at the drag start".into());
            }
            let evaluate_selector =
                sel_registerName(b"evaluateJavaScript:completionHandler:\0".as_ptr().cast());
            let responds_selector = sel_registerName(b"respondsToSelector:\0".as_ptr().cast());
            let responds: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool =
                std::mem::transmute(objc_msgSend as *const c_void);
            let webview = {
                let mut candidate = hit_target;
                let superview_selector = sel_registerName(b"superview\0".as_ptr().cast());
                let mut found = ptr::null_mut();
                for _ in 0..24 {
                    if candidate.is_null() {
                        break;
                    }
                    if responds(candidate, responds_selector, evaluate_selector) {
                        found = candidate;
                        break;
                    }
                    candidate = msg_send0(candidate, superview_selector);
                }
                if found.is_null() {
                    find_javascript_webview(surface)
                } else {
                    found
                }
            };
            if !webview.is_null() {
                return post_webview_dom_drag(webview, from_x, from_y, to_x, to_y);
            }
            let gl_context_selector = sel_registerName(b"openGLContext\0".as_ptr().cast());
            let event_target = if responds(hit_target, responds_selector, gl_context_selector) {
                if surface.is_null() {
                    return Err("OpenGL plugin surface is null".into());
                }
                surface
            } else {
                hit_target
            };
            let first_responder_selector =
                sel_registerName(b"makeFirstResponder:\0".as_ptr().cast());
            msg_send1(window, first_responder_selector, event_target);
            let responder_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const c_void);
            let mouse_moved_selector = sel_registerName(b"mouseMoved:\0".as_ptr().cast());
            let mouse_down_selector = sel_registerName(b"mouseDown:\0".as_ptr().cast());
            let mouse_dragged_selector = sel_registerName(b"mouseDragged:\0".as_ptr().cast());
            let mouse_up_selector = sel_registerName(b"mouseUp:\0".as_ptr().cast());
            let process_info_class = objc_getClass(b"NSProcessInfo\0".as_ptr().cast());
            let process_info_selector = sel_registerName(b"processInfo\0".as_ptr().cast());
            let process_info = msg_send0(process_info_class, process_info_selector);
            let uptime_selector = sel_registerName(b"systemUptime\0".as_ptr().cast());
            let uptime_fn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> f64 =
                std::mem::transmute(objc_msgSend as *const c_void);
            let start_time = uptime_fn(process_info, uptime_selector);
            let mut event_number = 0_isize;

            let mut dispatch =
                |method: *mut c_void, event_type: u64, x: f64, y: f64, pressure: f32| {
                    event_number += 1;
                    let content_point = surface_point(x, y);
                    let window_point =
                        convert_fn(surface, convert_selector, content_point, ptr::null_mut());
                    let event = event_fn(
                        event_class,
                        event_selector,
                        event_type,
                        window_point,
                        0,
                        start_time + event_number as f64 * 0.01,
                        window_number,
                        ptr::null_mut(),
                        event_number,
                        1,
                        pressure,
                    );
                    if event.is_null() {
                        Err("NSEvent mouse event construction returned null".to_string())
                    } else {
                        responder_fn(event_target, method, event);
                        pump_events();
                        Ok(())
                    }
                };

            dispatch(mouse_moved_selector, NS_MOUSE_MOVED, from_x, from_y, 0.0)?;
            dispatch(mouse_down_selector, NS_LEFT_MOUSE_DOWN, from_x, from_y, 1.0)?;
            for step in 1..=12 {
                let amount = f64::from(step) / 12.0;
                dispatch(
                    mouse_dragged_selector,
                    NS_LEFT_MOUSE_DRAGGED,
                    from_x + (to_x - from_x) * amount,
                    from_y + (to_y - from_y) * amount,
                    1.0,
                )?;
            }
            dispatch(mouse_up_selector, NS_LEFT_MOUSE_UP, to_x, to_y, 0.0)?;
        }
        pump_events();
        Ok(InputDelivery::NativeMouse)
    }

    pub unsafe fn create_ns_window(
        title: &str,
        width: f64,
        height: f64,
        close_cb: Box<dyn Fn() + Send>,
    ) -> Result<PluginGuiWindow, String> {
        // Ensure an autorelease pool exists for all ObjC allocations
        let _pool = AutoreleasePool::new();

        let ns_window = objc_getClass(b"NSWindow\0".as_ptr() as *const _);
        if ns_window.is_null() {
            return Err("NSWindow class not found".into());
        }

        let sel_alloc = sel_registerName(b"alloc\0".as_ptr() as *const _);
        let sel_init_win = sel_registerName(
            b"initWithContentRect:styleMask:backing:defer:\0".as_ptr() as *const _,
        );
        let sel_content_view = sel_registerName(b"contentView\0".as_ptr() as *const _);
        let sel_order_front = sel_registerName(b"orderFront:\0".as_ptr() as *const _);
        let sel_make_key = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as *const _);
        let sel_order_front_regardless =
            sel_registerName(b"orderFrontRegardless\0".as_ptr() as *const _);
        let sel_set_title = sel_registerName(b"setTitle:\0".as_ptr() as *const _);
        let sel_set_delegate = sel_registerName(b"setDelegate:\0".as_ptr() as *const _);
        let sel_retain = sel_registerName(b"retain\0".as_ptr() as *const _);

        let style = macos_host_window_style();

        let rect = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize { width, height },
        };

        let alloc = msg_send0(ns_window, sel_alloc);
        if alloc.is_null() {
            return Err("NSWindow alloc failed".into());
        }

        let window = msg_send_init_window(
            alloc,
            sel_init_win,
            rect,
            style,
            NS_BACKING_STORE_BUFFERED,
            false,
        );
        if window.is_null() {
            return Err("NSWindow init failed".into());
        }

        // Set title — use alloc+initWithUTF8String: for a non-autoreleased string
        // that NSWindow will own via setTitle:'s retain
        let ns_string_class = objc_getClass(b"NSString\0".as_ptr() as *const _);
        let sel_alloc2 = sel_registerName(b"alloc\0".as_ptr() as *const _);
        let sel_init_utf8 = sel_registerName(b"initWithUTF8String:\0".as_ptr() as *const _);
        let c_title = CString::new(title).unwrap();
        let ns_title = msg_send1(
            msg_send0(ns_string_class, sel_alloc2),
            sel_init_utf8,
            c_title.as_ptr() as *mut c_void,
        );
        if !ns_title.is_null() {
            msg_send1(window, sel_set_title, ns_title);
            // NSWindow retains the title via setTitle:, so we release our alloc
            let sel_release = sel_registerName(b"release\0".as_ptr() as *const _);
            msg_send_void(ns_title, sel_release);
        }

        // Get content view
        let content_view = msg_send0(window, sel_content_view);
        if !content_view.is_null() {
            msg_send0(content_view, sel_retain);
        }

        // Create delegate for close callback
        let delegate = create_window_delegate(close_cb);
        if !delegate.is_null() {
            msg_send1(window, sel_set_delegate, delegate);
        }

        let sel_sharing = sel_registerName(b"setSharingType:\0".as_ptr() as *const _);
        let set_sharing: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
            std::mem::transmute(objc_msgSend as *const c_void);
        const NS_WINDOW_SHARING_READ_ONLY: usize = 1;
        set_sharing(window, sel_sharing, NS_WINDOW_SHARING_READ_ONLY);
        let sel_visible = sel_registerName(b"setIsVisible:\0".as_ptr() as *const _);
        let set_visible: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
            std::mem::transmute(objc_msgSend as *const c_void);
        set_visible(window, sel_visible, true);
        let sel_center = sel_registerName(b"center\0".as_ptr() as *const _);
        msg_send_void(window, sel_center);

        msg_send1(window, sel_make_key, ptr::null_mut());
        msg_send_void(window, sel_order_front_regardless);
        msg_send1(window, sel_order_front, ptr::null_mut());

        Ok(PluginGuiWindow {
            window,
            content_view,
            delegate,
        })
    }

    static mut DELEGATE_CLASS: *mut c_void = ptr::null_mut();
    static mut DELEGATE_CLASS_INIT: bool = false;

    unsafe fn ensure_delegate_class() {
        if DELEGATE_CLASS_INIT {
            return;
        }
        DELEGATE_CLASS_INIT = true;

        let ns_object = objc_getClass(b"NSObject\0".as_ptr() as *const _);
        let cls =
            objc_allocateClassPair(ns_object, b"SunMaoWindowDelegate\0".as_ptr() as *const _, 0);
        if cls.is_null() {
            return;
        }

        // Add windowWillClose: method
        extern "C" fn window_will_close(this: *mut c_void, _sel: *mut c_void, _notif: *mut c_void) {
            if let Some(cb) = take_close_callback(this as usize) {
                cb();
            }
        }

        let sel = sel_registerName(b"windowWillClose:\0".as_ptr() as *const _);
        class_addMethod(
            cls,
            sel,
            window_will_close as *mut c_void,
            b"v@:@\0".as_ptr() as *const _,
        );

        objc_registerClassPair(cls);
        DELEGATE_CLASS = cls;
    }

    unsafe fn create_window_delegate(close_cb: Box<dyn Fn() + Send>) -> *mut c_void {
        ensure_delegate_class();

        if DELEGATE_CLASS.is_null() {
            // Can't create delegate class, just call the callback now
            close_cb();
            return ptr::null_mut();
        }

        let sel_alloc = sel_registerName(b"alloc\0".as_ptr() as *const _);
        let sel_init = sel_registerName(b"init\0".as_ptr() as *const _);
        let instance = msg_send0(DELEGATE_CLASS, sel_alloc);
        let instance = msg_send0(instance, sel_init);

        register_close_callback(instance as usize, close_cb);

        instance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pixel_sizes_are_finite_positive_and_rounded() {
        assert_eq!(native_pixel_size(400.4, 299.6), (400, 300));
        assert_eq!(native_pixel_size(0.0, -10.0), (1, 1));
        assert_eq!(native_pixel_size(f64::NAN, f64::INFINITY), (1, 1));
        assert_eq!(
            native_pixel_size(f64::MAX, f64::MAX),
            (i32::MAX as u32, i32::MAX as u32)
        );
    }

    #[test]
    fn windows_logical_coordinates_scale_to_client_pixels() {
        assert_eq!(windows_logical_to_client_pixel(64.0, 96), 64.0);
        assert_eq!(windows_logical_to_client_pixel(64.0, 192), 128.0);
        assert_eq!(windows_logical_to_client_pixel(456.0, 144), 684.0);
    }

    #[test]
    fn ui_automation_drag_scoring_requires_a_rectangle_intersection() {
        let bounds = (100, 100, 200, 120);
        assert_eq!(drag_rectangle_score(bounds, (10, 10), (90, 90)), 0);
        assert_eq!(drag_rectangle_score(bounds, (10, 110), (250, 110)), 2);
        assert_eq!(
            drag_rectangle_score((100, 100, 120, 110), (0, 105), (300, 105)),
            1
        );
        assert_eq!(drag_rectangle_score(bounds, (110, 110), (190, 110)), 7);
        assert_eq!(
            drag_rectangle_score(bounds, (i32::MIN, i32::MIN), (i32::MAX, i32::MAX),),
            1
        );
    }

    #[test]
    fn pixel_evidence_rejects_uniform_and_low_contrast_frames() {
        let uniform = std::iter::repeat_n(0x0020_2020, 4096);
        assert!(non_uniform_pixel_evidence(64, 64, uniform).is_err());

        let low_contrast = (0..4096).map(|index| {
            if index % 2 == 0 {
                0x0020_2020
            } else {
                0x0028_2828
            }
        });
        assert!(non_uniform_pixel_evidence(64, 64, low_contrast).is_err());
    }

    #[test]
    fn pixel_evidence_accepts_a_visually_varied_frame() {
        let pixels = (0..4096).map(|index| {
            let x = (index % 64) as u32;
            let y = (index / 64) as u32;
            let red = x * 4;
            let green = y * 4;
            let blue = (x + y) * 2;
            red | (green << 8) | (blue << 16)
        });
        let evidence = non_uniform_pixel_evidence(64, 64, pixels).unwrap();
        assert_eq!(evidence.sampled_pixels, 4096);
        assert!(evidence.distinct_colors >= 8);
        assert!(evidence.intensity_range >= 16);
        assert!(evidence.intensity_std_dev >= 4.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_host_window_is_fixed_size() {
        const RESIZABLE: u64 = 1 << 3;
        assert_eq!(macos_host_window_style() & RESIZABLE, 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_strings_are_utf16_and_nul_terminated() {
        let value = windows::wide_null("SunMao");
        assert_eq!(value.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&value[..value.len() - 1]).unwrap(),
            "SunMao"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn x11_attachment_handle_contains_the_window_id() {
        let window = 0x1234 as x11_dl::xlib::Window;
        assert_eq!(linux::attachment_handle(window) as usize, window as usize);
    }
}
