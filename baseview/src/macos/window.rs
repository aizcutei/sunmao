#[cfg(any(feature = "opengl", feature = "metal"))]
use std::cell::UnsafeCell;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr;
use std::rc::Rc;
use std::sync::Mutex;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyRegular, NSBackingStoreBuffered, NSEvent,
    NSEventModifierFlags, NSEventSubtype, NSEventType, NSPasteboard, NSView, NSWindow,
    NSWindowStyleMask,
};
use cocoa::base::{id, nil, BOOL, NO, YES};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
use core_foundation::base::TCFType;
use core_foundation::runloop::{
    __CFRunLoopTimer, kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFRunLoop, CFRunLoopSource,
    CFRunLoopSourceContext, CFRunLoopSourceCreate, CFRunLoopSourceInvalidate,
    CFRunLoopSourceSignal, CFRunLoopTimer, CFRunLoopTimerContext, CFRunLoopWakeUp,
};
use keyboard_types::KeyboardEvent;
use objc::class;
use objc::{msg_send, runtime::Object, sel, sel_impl};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle as RwhWindowHandle,
};

use crate::{
    Event, EventStatus, MouseCursor, Size, WindowHandler, WindowInfo, WindowOpenOptions,
    WindowScalePolicy,
};

use super::keyboard::KeyboardState;
use super::view::{create_view, BASEVIEW_STATE_IVAR};

#[cfg(feature = "opengl")]
use crate::gl::{GlConfig, GlContext};

#[cfg(feature = "metal")]
use crate::metal_layer::MetalLayer;

pub struct WindowHandle {
    state: Rc<WindowState>,
}

static BLOCKING_EVENT_LOOP_STOP_SOURCE: Mutex<Option<usize>> = Mutex::new(None);

fn stop_application_event_loop() {
    unsafe {
        let app = NSApp();
        app.stop_(app);

        // AppKit applies stop: after processing another event. Post one so an
        // otherwise idle WebKit window cannot leave NSApplication::run stuck.
        let wake_event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2_(
            nil,
            NSEventType::NSApplicationDefined,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::empty(),
            0.0,
            0,
            nil,
            NSEventSubtype::NSWindowExposedEventType,
            0,
            0,
        );
        if wake_event != nil {
            app.postEvent_atStart_(wake_event, YES);
        }
    }
}

struct BlockingEventLoopRegistration {
    source: CFRunLoopSource,
}

impl BlockingEventLoopRegistration {
    fn new() -> Self {
        extern "C" fn stop_application(_: *const c_void) {
            stop_application_event_loop();
        }

        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: stop_application,
        };
        let source = unsafe {
            let source = CFRunLoopSourceCreate(ptr::null(), 0, &mut context);
            CFRunLoopSource::wrap_under_create_rule(source)
        };
        let run_loop = CFRunLoop::get_main();
        run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });

        let source_ptr = source.as_concrete_TypeRef() as usize;
        let mut registered = BLOCKING_EVENT_LOOP_STOP_SOURCE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            registered.replace(source_ptr).is_none(),
            "multiple blocking macOS event loops are not supported"
        );

        Self { source }
    }
}

impl Drop for BlockingEventLoopRegistration {
    fn drop(&mut self) {
        let source_ptr = self.source.as_concrete_TypeRef() as usize;
        let mut registered = BLOCKING_EVENT_LOOP_STOP_SOURCE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *registered == Some(source_ptr) {
            *registered = None;
        }

        let run_loop = CFRunLoop::get_main();
        run_loop.remove_source(&self.source, unsafe { kCFRunLoopCommonModes });
        unsafe {
            CFRunLoopSourceInvalidate(self.source.as_concrete_TypeRef());
        }
    }
}

/// Ask the main AppKit event loop to stop from a watchdog thread.
pub fn request_event_loop_stop() {
    let registered = BLOCKING_EVENT_LOOP_STOP_SOURCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(source_ptr) = *registered else {
        return;
    };
    let run_loop = CFRunLoop::get_main();
    unsafe {
        CFRunLoopSourceSignal(source_ptr as _);
        CFRunLoopWakeUp(run_loop.as_concrete_TypeRef());
    }
}

impl WindowHandle {
    pub fn close(&mut self) {
        self.state.window_inner.close();
    }

    pub fn is_open(&self) -> bool {
        self.state.window_inner.open.get()
    }

    pub fn resize(&mut self, size: Size) {
        let mut window = Window {
            inner: &self.state.window_inner,
        };
        window.resize(size);
    }
}

impl HasWindowHandle for WindowHandle {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        self.state.window_inner.window_handle()
    }
}

/// A native rendering resource that is initialized only after AppKit has
/// attached the baseview NSView to its parent window.
///
/// macOS windows are `Rc`-owned and confined to the main thread. The slot is
/// populated before the handler is built, read only while the handler is
/// building or active, and taken only after the handler has been dropped.
/// Those lifecycle rules make the interior mutation below non-aliasing.
#[cfg(any(feature = "opengl", feature = "metal"))]
struct WindowResource<T> {
    value: UnsafeCell<Option<T>>,
}

#[cfg(any(feature = "opengl", feature = "metal"))]
impl<T> WindowResource<T> {
    fn empty() -> Self {
        Self {
            value: UnsafeCell::new(None),
        }
    }

    /// # Safety
    /// No references returned by `get` may exist, and this slot must not be
    /// accessed concurrently or reentrantly while it is being populated.
    unsafe fn set(&self, value: T) {
        unsafe {
            let slot = &mut *self.value.get();
            assert!(slot.is_none(), "native window resource initialized twice");
            *slot = Some(value);
        }
    }

    fn get(&self) -> Option<&T> {
        unsafe { (&*self.value.get()).as_ref() }
    }

    /// # Safety
    /// No references returned by `get` may exist, and the slot must remain
    /// inaccessible until the returned resource has been dropped.
    unsafe fn take(&self) -> Option<T> {
        unsafe { (&mut *self.value.get()).take() }
    }
}

pub(super) struct WindowInner {
    open: Cell<bool>,

    /// Only set if we created the parent window, i.e. we are running in
    /// parentless mode
    ns_app: Cell<Option<id>>,
    /// Only set if we created the parent window, i.e. we are running in
    /// parentless mode
    ns_window: Cell<Option<id>>,
    /// Our subclassed NSView
    ns_view: id,

    #[cfg(feature = "opengl")]
    gl_context: WindowResource<GlContext>,

    #[cfg(feature = "metal")]
    metal_layer: WindowResource<MetalLayer>,
}

impl WindowInner {
    pub(super) fn close(&self) {
        if !self.open.replace(false) {
            return;
        }

        unsafe {
            // Clone the NSView's retained state without consuming its raw Rc.
            // Native teardown can synchronously call back through this ivar.
            let Some(window_state) = WindowState::try_from_view(&*self.ns_view) else {
                return;
            };

            // A handler may synchronously request close from on_event/on_frame,
            // or from its builder before it has been installed in the state.
            // Native teardown must wait until that handler can be dropped first.
            if window_state.handler_active.get() || window_state.handler_building.get() {
                window_state.close_pending.set(true);
                return;
            }

            self.finish_close(window_state);
        }
    }

    unsafe fn finish_close(&self, window_state: Rc<WindowState>) {
        // This is the exact pointer created by `Rc::into_raw` in `prepare`.
        // Keep its ownership in the ivar until all detach callbacks finish.
        let retained_state_ptr: *const c_void = *(*self.ns_view).get_ivar(BASEVIEW_STATE_IVAR);

        // Cancel the frame timer.
        if let Some(frame_timer) = window_state.frame_timer.take() {
            CFRunLoop::get_main().remove_timer(&frame_timer, kCFRunLoopDefaultMode);
        }

        // Deregister NSView from NotificationCenter.
        let notification_center: id = msg_send![class!(NSNotificationCenter), defaultCenter];
        let () = msg_send![notification_center, removeObserver:self.ns_view];

        // Drop child renderers (including Wry/WebKit) while their parent
        // NSView and NSWindow are still alive and attached.
        let window_handler = window_state.window_handler.borrow_mut().take();
        drop_window_handler(window_handler);

        // Rust-side render resources can refer to views/layers parented to
        // `ns_view`. Drop their handler-side owners first, then clear these
        // slots before detaching or releasing the AppKit parent.
        #[cfg(feature = "opengl")]
        {
            // SAFETY: no handler is building or active, and no `Window` can
            // retain a resource reference beyond a handler callback.
            let gl_context = unsafe { self.gl_context.take() };
            drop(gl_context);
        }
        #[cfg(feature = "metal")]
        {
            // SAFETY: the handler has been dropped and the main-thread-only
            // window state cannot be accessed concurrently during teardown.
            let metal_layer = unsafe { self.metal_layer.take() };
            drop(metal_layer);
        }

        // Close the window if in non-parented mode.
        if let Some(ns_window) = self.ns_window.take() {
            ns_window.close();
        }

        // Ensure that the NSView is detached from the parent window.
        self.ns_view.removeFromSuperview();

        // Detachment callbacks are complete. Clear the ivar before releasing the
        // NSView so an unexpected external retain cannot later expose a stale Rc.
        let null_state: *const c_void = std::ptr::null();
        (*self.ns_view).set_ivar(BASEVIEW_STATE_IVAR, null_state);
        let () = msg_send![self.ns_view as id, release];

        // If in non-parented mode, quit the app altogether.
        if self.ns_app.take().is_some() {
            stop_application_event_loop();
        }

        // Consume the ivar's raw ownership exactly once. The local clone keeps
        // WindowState alive through the rest of this function.
        if !retained_state_ptr.is_null() {
            drop(Rc::from_raw(retained_state_ptr as *const WindowState));
        }
        drop(window_state);
    }

    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        if self.open.get() {
            let ns_view = std::ptr::NonNull::new(self.ns_view as *mut std::ffi::c_void)
                .ok_or(HandleError::Unavailable)?;
            let handle = AppKitWindowHandle::new(ns_view);
            return Ok(unsafe { RwhWindowHandle::borrow_raw(RawWindowHandle::AppKit(handle)) });
        }
        Err(HandleError::Unavailable)
    }
}

pub struct Window<'a> {
    inner: &'a WindowInner,
}

impl<'a> Window<'a> {
    pub fn open_parented<P, H, B>(parent: &P, options: WindowOpenOptions, build: B) -> WindowHandle
    where
        P: HasWindowHandle,
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let pool = unsafe { NSAutoreleasePool::new(nil) };

        let scaling = match options.scale {
            WindowScalePolicy::ScaleFactor(scale) => scale,
            WindowScalePolicy::SystemScaleFactor => 1.0,
        };

        let window_info = WindowInfo::from_logical_size(options.size, scaling);
        let handle = parent
            .window_handle()
            .expect("Failed to get parent window handle");
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            panic!("Not a macOS window");
        };
        let ns_view = unsafe { create_view(&options) };

        let window_inner = WindowInner {
            open: Cell::new(true),
            ns_app: Cell::new(None),
            ns_window: Cell::new(None),
            ns_view,

            #[cfg(feature = "opengl")]
            gl_context: WindowResource::empty(),

            #[cfg(feature = "metal")]
            metal_layer: WindowResource::empty(),
        };
        let window_state = Self::prepare(window_inner, window_info);
        let mut initialization_guard = WindowInitializationGuard::new(Rc::clone(&window_state));

        unsafe {
            let _: id = msg_send![handle.ns_view.as_ptr() as *mut Object, addSubview: ns_view];

            // Auto-resize to fill parent view. NSViewWidthSizable | NSViewHeightSizable = 6.
            // This ensures the baseview child fills the AU host's view.
            let _: () = msg_send![ns_view, setAutoresizingMask: 0x6_u64];

            // Set initial frame to match parent's bounds so we fill it immediately.
            let parent_bounds: cocoa::foundation::NSRect =
                msg_send![handle.ns_view.as_ptr() as *mut Object, bounds];
            let _: () = msg_send![ns_view, setFrame: parent_bounds];
        }

        #[cfg(feature = "opengl")]
        if let Some(gl_config) = options.gl_config {
            let gl_context = Self::create_gl_context(ns_view, gl_config);
            // SAFETY: attachment is complete, the handler has not been built,
            // and this main-thread-only slot is still empty and unborrowed.
            unsafe {
                window_state.window_inner.gl_context.set(gl_context);
            }
        }
        #[cfg(feature = "metal")]
        if options.metal_layer {
            let metal_layer = unsafe { MetalLayer::new(ns_view) };
            // SAFETY: attachment is complete, the handler has not been built,
            // and this main-thread-only slot is still empty and unborrowed.
            unsafe {
                window_state.window_inner.metal_layer.set(metal_layer);
            }
        }

        let window_handle = Self::finish(window_state, build);
        initialization_guard.disarm();

        unsafe {
            let () = msg_send![pool, drain];
        }
        window_handle
    }

    pub fn open_blocking<H, B>(options: WindowOpenOptions, build: B)
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let _blocking_event_loop = BlockingEventLoopRegistration::new();
        let pool = unsafe { NSAutoreleasePool::new(nil) };

        // It seems prudent to run NSApp() here before doing other
        // work. It runs [NSApplication sharedApplication], which is
        // what is run at the very start of the Xcode-generated main
        // function of a cocoa app according to:
        // https://developer.apple.com/documentation/appkit/nsapplication
        let app = unsafe { NSApp() };

        unsafe {
            app.setActivationPolicy_(NSApplicationActivationPolicyRegular);
            // Command-line standalone binaries do not receive the normal
            // application bootstrap performed by an AppKit app bundle. Finish
            // launching before installing the window and frame timer so the
            // main run loop actually services AppKit and CF callbacks.
            app.finishLaunching();
            app.activateIgnoringOtherApps_(YES);
        }

        let scaling = match options.scale {
            WindowScalePolicy::ScaleFactor(scale) => scale,
            WindowScalePolicy::SystemScaleFactor => 1.0,
        };

        let window_info = WindowInfo::from_logical_size(options.size, scaling);

        let rect = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(
                window_info.logical_size().width,
                window_info.logical_size().height,
            ),
        );

        let ns_window = unsafe {
            let ns_window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                rect,
                NSWindowStyleMask::NSTitledWindowMask
                    | NSWindowStyleMask::NSClosableWindowMask
                    | NSWindowStyleMask::NSMiniaturizableWindowMask,
                NSBackingStoreBuffered,
                NO,
            );
            ns_window.center();

            let title = NSString::alloc(nil).init_str(&options.title).autorelease();
            ns_window.setTitle_(title);

            ns_window.makeKeyAndOrderFront_(nil);

            ns_window
        };

        let ns_view = unsafe { create_view(&options) };

        let window_inner = WindowInner {
            open: Cell::new(true),
            ns_app: Cell::new(Some(app)),
            ns_window: Cell::new(Some(ns_window)),
            ns_view,

            #[cfg(feature = "opengl")]
            gl_context: WindowResource::empty(),

            #[cfg(feature = "metal")]
            metal_layer: WindowResource::empty(),
        };

        let window_state = Self::prepare(window_inner, window_info);
        let mut initialization_guard = WindowInitializationGuard::new(Rc::clone(&window_state));

        unsafe {
            ns_window.setContentView_(ns_view);
            ns_window.setDelegate_(ns_view);
        }

        #[cfg(feature = "opengl")]
        if let Some(gl_config) = options.gl_config {
            let gl_context = Self::create_gl_context(ns_view, gl_config);
            // SAFETY: the NSView is installed in its NSWindow, the handler has
            // not been built, and the slot is still empty and unborrowed.
            unsafe {
                window_state.window_inner.gl_context.set(gl_context);
            }
        }
        #[cfg(feature = "metal")]
        if options.metal_layer {
            let metal_layer = unsafe { MetalLayer::new(ns_view) };
            // SAFETY: the NSView is installed in its NSWindow, the handler has
            // not been built, and the slot is still empty and unborrowed.
            unsafe {
                window_state.window_inner.metal_layer.set(metal_layer);
            }
        }

        let mut window_handle = Self::finish(window_state, build);
        initialization_guard.disarm();

        let should_run = window_handle.is_open();
        unsafe {
            let () = msg_send![pool, drain];

            // A handler is allowed to reject initialization and close the
            // window from its builder. Calling `run` after `stop_` was sent
            // during that close starts a fresh AppKit loop with no live
            // window, leaving callers blocked until an external wake-up.
            if should_run {
                app.run();
            }
        }

        // A watchdog can stop the CFRunLoop before AppKit delivers a close
        // callback. Complete native teardown after the blocking loop returns;
        // normal user/frame-driven closes make this a no-op.
        window_handle.close();
    }

    fn prepare(window_inner: WindowInner, window_info: WindowInfo) -> Rc<WindowState> {
        let ns_view = window_inner.ns_view;
        let window_state = Rc::new(WindowState {
            window_inner,
            window_handler: RefCell::new(None),
            handler_active: Cell::new(false),
            handler_building: Cell::new(true),
            close_pending: Cell::new(false),
            keyboard_state: KeyboardState::new(),
            frame_timer: Cell::new(None),
            window_info: Cell::new(window_info),
            deferred_events: RefCell::default(),
        });

        let window_state_ptr = Rc::into_raw(Rc::clone(&window_state));
        unsafe {
            (*ns_view).set_ivar(BASEVIEW_STATE_IVAR, window_state_ptr as *const c_void);
        }

        window_state
    }

    fn finish<H, B>(window_state: Rc<WindowState>, build: B) -> WindowHandle
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let mut window = crate::Window::new(Window {
            inner: &window_state.window_inner,
        });
        let window_handler = Box::new(build(&mut window));
        window_state.handler_building.set(false);

        if window_state.window_inner.open.get() {
            *window_state.window_handler.borrow_mut() = Some(window_handler);
            window_state.drain_deferred_events();
        } else {
            drop_window_handler(Some(window_handler));
            window_state.complete_pending_close();
        }

        if window_state.window_inner.open.get() {
            unsafe {
                let state_ptr: *const c_void =
                    *(*window_state.window_inner.ns_view).get_ivar(BASEVIEW_STATE_IVAR);
                debug_assert!(!state_ptr.is_null());
                WindowState::setup_timer(state_ptr as *const WindowState);
            }
        }

        WindowHandle {
            state: window_state,
        }
    }

    pub fn close(&mut self) {
        self.inner.close();
    }

    pub fn has_focus(&mut self) -> bool {
        unsafe {
            let view = self.inner.ns_view.as_mut().unwrap();
            let window: id = msg_send![view, window];
            if window == nil {
                return false;
            };
            let first_responder: id = msg_send![window, firstResponder];
            let is_key_window: BOOL = msg_send![window, isKeyWindow];
            let is_focused: BOOL = msg_send![view, isEqual: first_responder];
            is_key_window == YES && is_focused == YES
        }
    }

    pub fn focus(&mut self) {
        unsafe {
            let view = self.inner.ns_view.as_mut().unwrap();
            let window: id = msg_send![view, window];
            if window != nil {
                msg_send![window, makeFirstResponder:view]
            }
        }
    }

    pub fn resize(&mut self, size: Size) {
        if self.inner.open.get() {
            // NOTE: macOS gives you a personal rave if you pass in fractional pixels here. Even
            // though the size is in fractional pixels.
            let size = NSSize::new(size.width.round(), size.height.round());

            unsafe { NSView::setFrameSize(self.inner.ns_view, size) };
            unsafe {
                let _: () = msg_send![self.inner.ns_view, setNeedsDisplay: YES];
            }

            // When using OpenGL the `NSOpenGLView` needs to be resized separately? Why? Because
            // macOS.
            #[cfg(feature = "opengl")]
            if let Some(gl_context) = self.inner.gl_context.get() {
                gl_context.resize(size);
            }

            #[cfg(feature = "metal")]
            if let Some(metal_layer) = self.inner.metal_layer.get() {
                metal_layer.resize(self.inner.ns_view);
            }

            // If this is a standalone window then we'll also need to resize the window itself
            if let Some(ns_window) = self.inner.ns_window.get() {
                unsafe { NSWindow::setContentSize_(ns_window, size) };
            }
        }
    }

    pub fn set_mouse_cursor(&mut self, mouse_cursor: MouseCursor) {
        unsafe {
            let cursor: id = match mouse_cursor {
                MouseCursor::Hidden => {
                    let _: () = msg_send![class!(NSCursor), setHiddenUntilMouseMoves: YES];
                    return;
                }
                MouseCursor::Hand => msg_send![class!(NSCursor), pointingHandCursor],
                MouseCursor::HandGrabbing | MouseCursor::Move | MouseCursor::AllScroll => {
                    msg_send![class!(NSCursor), closedHandCursor]
                }
                MouseCursor::Text | MouseCursor::VerticalText => {
                    msg_send![class!(NSCursor), IBeamCursor]
                }
                MouseCursor::Crosshair | MouseCursor::Cell => {
                    msg_send![class!(NSCursor), crosshairCursor]
                }
                MouseCursor::EResize
                | MouseCursor::WResize
                | MouseCursor::EwResize
                | MouseCursor::ColResize => {
                    msg_send![class!(NSCursor), resizeLeftRightCursor]
                }
                MouseCursor::NResize
                | MouseCursor::SResize
                | MouseCursor::NsResize
                | MouseCursor::RowResize => {
                    msg_send![class!(NSCursor), resizeUpDownCursor]
                }
                _ => msg_send![class!(NSCursor), arrowCursor],
            };
            let _: () = msg_send![cursor, set];
        }
    }

    #[cfg(feature = "opengl")]
    pub fn gl_context(&self) -> Option<&GlContext> {
        self.inner.gl_context.get()
    }

    #[cfg(feature = "metal")]
    pub fn metal_layer(&self) -> Option<&MetalLayer> {
        self.inner.metal_layer.get()
    }

    #[cfg(feature = "opengl")]
    fn create_gl_context(ns_view: id, config: GlConfig) -> GlContext {
        let ns_view_ptr = std::ptr::NonNull::new(ns_view as *mut c_void).expect("ns_view is null");
        let handle = AppKitWindowHandle::new(ns_view_ptr);
        let handle = RawWindowHandle::AppKit(handle);
        unsafe { GlContext::create(&handle, config).expect("Could not create OpenGL context") }
    }
}

pub(super) struct WindowState {
    pub(super) window_inner: WindowInner,
    window_handler: RefCell<Option<Box<dyn WindowHandler>>>,
    handler_active: Cell<bool>,
    handler_building: Cell<bool>,
    close_pending: Cell<bool>,
    keyboard_state: KeyboardState,
    frame_timer: Cell<Option<CFRunLoopTimer>>,
    /// The last known window info for this window.
    pub window_info: Cell<WindowInfo>,

    /// Events that will be triggered at the end of `window_handler`'s borrow.
    deferred_events: RefCell<VecDeque<Event>>,
}

impl WindowState {
    /// Tries to get the `WindowState` held by an initialized `NSView`.
    ///
    /// AppKit can invoke geometry callbacks while `initWithFrame:` is still
    /// running, before baseview has installed the state pointer in the view.
    pub(super) unsafe fn try_from_view(view: &Object) -> Option<Rc<WindowState>> {
        let state_ptr: *const c_void = *view.get_ivar(BASEVIEW_STATE_IVAR);
        if state_ptr.is_null() {
            return None;
        }

        let state_rc = Rc::from_raw(state_ptr as *const WindowState);
        let state = Rc::clone(&state_rc);
        let _ = Rc::into_raw(state_rc);
        Some(state)
    }

    /// Gets the `WindowState` held by a given `NSView`.
    ///
    /// This method returns a cloned `Rc<WindowState>` rather than just a `&WindowState`, since the
    /// original `Rc<WindowState>` owned by the `NSView` can be dropped at any time
    /// (including during an event handler).
    pub(super) unsafe fn from_view(view: &Object) -> Rc<WindowState> {
        Self::try_from_view(view).expect("baseview state is not initialized")
    }

    /// Trigger the event immediately and return the event status.
    pub(super) fn trigger_event(&self, event: Event) -> EventStatus {
        if !self.handler_available() {
            self.deferred_events.borrow_mut().push_back(event);
            return EventStatus::Ignored;
        }

        let status = self.dispatch_event(event);
        self.drain_deferred_events();
        status
    }

    /// Trigger the event immediately if `window_handler` can be borrowed mutably,
    /// otherwise add the event to a queue that will be cleared once `window_handler`'s mutable borrow ends.
    /// As this method might result in the event triggering asynchronously, it can't reliably return the event status.
    pub(super) fn trigger_deferrable_event(&self, event: Event) {
        let _ = self.trigger_event(event);
    }

    pub(super) fn trigger_frame(&self) {
        if !self.handler_available() {
            return;
        }

        let Some(mut window_handler) = self.window_handler.borrow_mut().take() else {
            return;
        };
        self.handler_active.set(true);
        let mut window = crate::Window::new(Window {
            inner: &self.window_inner,
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            window_handler.on_frame(&mut window);
        }));
        self.handler_active.set(false);

        if result.is_err() {
            eprintln!("baseview window handler panicked during on_frame");
            drop_window_handler(Some(window_handler));
            self.close_after_handler();
            return;
        }

        if self.window_inner.open.get() {
            *self.window_handler.borrow_mut() = Some(window_handler);
            self.drain_deferred_events();
        } else {
            drop_window_handler(Some(window_handler));
            self.complete_pending_close();
        }
    }

    pub(super) fn keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub(super) fn process_native_key_event(&self, event: *mut Object) -> Option<KeyboardEvent> {
        self.keyboard_state.process_native_event(event)
    }

    unsafe fn setup_timer(window_state_ptr: *const WindowState) {
        extern "C" fn timer_callback(_: *mut __CFRunLoopTimer, window_state_ptr: *mut c_void) {
            unsafe {
                let window_state_ptr = window_state_ptr as *const WindowState;
                // `setup_timer` passes the exact pointer retained in the NSView
                // ivar. Own a strong count for the entire callback because
                // `trigger_frame` may synchronously close the window and remove
                // the timer.
                Rc::increment_strong_count(window_state_ptr);
                let window_state = Rc::from_raw(window_state_ptr);
                window_state.trigger_frame();
            }
        }

        let mut timer_context = CFRunLoopTimerContext {
            version: 0,
            info: window_state_ptr as *mut c_void,
            retain: None,
            release: None,
            copyDescription: None,
        };

        let timer = CFRunLoopTimer::new(0.0, 0.015, 0, 0, timer_callback, &mut timer_context);

        // Use the main run loop explicitly. CFRunLoop::get_current() would add the timer
        // to the current thread's run loop, which may not be pumped if we're on a
        // background thread or dispatched via GCD.
        CFRunLoop::get_main().add_timer(&timer, kCFRunLoopDefaultMode);

        (*window_state_ptr).frame_timer.set(Some(timer));
    }

    fn handler_available(&self) -> bool {
        !self.handler_active.get()
            && self.window_inner.open.get()
            && self.window_handler.borrow().is_some()
    }

    fn dispatch_event(&self, event: Event) -> EventStatus {
        let Some(mut window_handler) = self.window_handler.borrow_mut().take() else {
            self.deferred_events.borrow_mut().push_back(event);
            return EventStatus::Ignored;
        };
        self.handler_active.set(true);
        let mut window = crate::Window::new(Window {
            inner: &self.window_inner,
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            window_handler.on_event(&mut window, event)
        }));
        self.handler_active.set(false);

        let status = match result {
            Ok(status) => status,
            Err(_) => {
                eprintln!("baseview window handler panicked during on_event");
                drop_window_handler(Some(window_handler));
                self.close_after_handler();
                return EventStatus::Ignored;
            }
        };

        if self.window_inner.open.get() {
            *self.window_handler.borrow_mut() = Some(window_handler);
        } else {
            drop_window_handler(Some(window_handler));
            self.complete_pending_close();
        }
        status
    }

    fn drain_deferred_events(&self) {
        while self.handler_available() {
            let Some(event) = self.deferred_events.borrow_mut().pop_front() else {
                break;
            };
            let _ = self.dispatch_event(event);
        }
    }

    fn close_after_handler(&self) {
        if self.window_inner.open.get() {
            self.window_inner.close();
        } else {
            self.complete_pending_close();
        }
    }

    fn complete_pending_close(&self) {
        if !self.close_pending.replace(false) {
            return;
        }

        unsafe {
            if let Some(window_state) = WindowState::try_from_view(&*self.window_inner.ns_view) {
                self.window_inner.finish_close(window_state);
            }
        }
    }
}

fn drop_window_handler(window_handler: Option<Box<dyn WindowHandler>>) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(window_handler))).is_err() {
        eprintln!("baseview window handler panicked during drop");
    }
}

struct WindowInitializationGuard {
    state: Rc<WindowState>,
    armed: bool,
}

impl WindowInitializationGuard {
    fn new(state: Rc<WindowState>) -> Self {
        Self { state, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for WindowInitializationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.state.handler_building.set(false);
            if self.state.window_inner.open.get() {
                self.state.window_inner.close();
            } else {
                self.state.complete_pending_close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSApplicationLoad() -> bool;
    }

    struct ReentrantHandler {
        event_count: Arc<AtomicUsize>,
        drop_count: Arc<AtomicUsize>,
    }

    impl WindowHandler for ReentrantHandler {
        fn on_frame(&mut self, window: &mut crate::Window) {
            window.close();
        }

        fn on_event(&mut self, window: &mut crate::Window, _event: Event) -> EventStatus {
            if self.event_count.fetch_add(1, Ordering::SeqCst) == 0 {
                // setFrameSize: synchronously re-enters the NSView callback. The
                // resulting resize event must be queued until this call returns.
                window.resize(Size::new(321.0, 181.0));
            }
            EventStatus::Ignored
        }
    }

    impl Drop for ReentrantHandler {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[cfg(any(feature = "opengl", feature = "metal"))]
    #[test]
    fn window_resource_take_clears_the_slot_before_drop() {
        struct DropProbe(Arc<AtomicUsize>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let drop_count = Arc::new(AtomicUsize::new(0));
        let resource = WindowResource::empty();
        unsafe {
            resource.set(DropProbe(Arc::clone(&drop_count)));
        }
        assert!(resource.get().is_some());

        let value = unsafe { resource.take() };
        assert!(resource.get().is_none());
        assert_eq!(drop_count.load(Ordering::SeqCst), 0);

        drop(value);
        assert_eq!(drop_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn deferred_resize_reentry_and_handler_requested_close_are_safe() {
        unsafe {
            let _ = NSApplicationLoad();
        }
        let options = WindowOpenOptions::new(
            "baseview reentry test",
            Size::new(320.0, 180.0),
            WindowScalePolicy::ScaleFactor(1.0),
        );
        let window_info = WindowInfo::from_logical_size(options.size, 1.0);
        let ns_view = unsafe { create_view(&options) };
        let window_inner = WindowInner {
            open: Cell::new(true),
            ns_app: Cell::new(None),
            ns_window: Cell::new(None),
            ns_view,
            #[cfg(feature = "opengl")]
            gl_context: WindowResource::empty(),
            #[cfg(feature = "metal")]
            metal_layer: WindowResource::empty(),
        };
        let state = Window::prepare(window_inner, window_info);
        state
            .deferred_events
            .borrow_mut()
            .push_back(Event::Window(crate::WindowEvent::Focused));

        let event_count = Arc::new(AtomicUsize::new(0));
        let drop_count = Arc::new(AtomicUsize::new(0));
        let mut handle = Window::finish(Rc::clone(&state), {
            let event_count = Arc::clone(&event_count);
            let drop_count = Arc::clone(&drop_count);
            move |_| ReentrantHandler {
                event_count,
                drop_count,
            }
        });

        assert_eq!(event_count.load(Ordering::SeqCst), 2);
        assert!(handle.is_open());

        state.trigger_frame();
        assert!(!handle.is_open());
        assert_eq!(drop_count.load(Ordering::SeqCst), 1);

        // A second close is idempotent and must not consume the raw Rc twice.
        handle.close();
        assert_eq!(drop_count.load(Ordering::SeqCst), 1);
    }
}

impl<'a> HasWindowHandle for Window<'a> {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        self.inner.window_handle()
    }
}

impl<'a> HasDisplayHandle for Window<'a> {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe {
            DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(AppKitDisplayHandle::new()))
        })
    }
}

pub fn copy_to_clipboard(string: &str) {
    unsafe {
        let pb = NSPasteboard::generalPasteboard(nil);

        let ns_str = NSString::alloc(nil).init_str(string);

        pb.clearContents();
        pb.setString_forType(ns_str, cocoa::appkit::NSPasteboardTypeString);
    }
}
