use std::ffi::CStr;
use std::fmt::{Debug, Display, Formatter};
use x11::xlib;

use std::cell::Cell;
use std::cell::RefCell;
use std::error::Error;
use std::os::raw::{c_int, c_uchar, c_ulong};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

type XLibErrorHandler = unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int;

// XSetErrorHandler changes process-global state. Multiple plug-in instances may
// create or render editors on different threads, so every temporary handler
// installation must be serialized.
static XLIB_ERROR_HANDLER_LOCK: Mutex<()> = Mutex::new(());
static PREVIOUS_XLIB_ERROR_HANDLER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Used as part of [`XErrorHandler::handle()`]. When an X11 error occurs during this function,
    /// the error gets copied to this RefCell after which the program is allowed to resume. The
    /// error can then be converted to a regular Rust Result value afterward.
    static CURRENT_X11_ERROR: RefCell<Option<XLibError>> = const { RefCell::new(None) };
    static ACTIVE_X11_DISPLAY: Cell<*mut xlib::Display> = const { Cell::new(std::ptr::null_mut()) };
}

/// A helper struct for safe X11 error handling
pub struct XErrorHandler<'a> {
    display: *mut xlib::Display,
    error: &'a RefCell<Option<XLibError>>,
}

impl<'a> XErrorHandler<'a> {
    /// Syncs and checks if any previous X11 calls from the given display returned an error
    pub fn check(&mut self) -> Result<(), XLibError> {
        // Flush all possible previous errors
        unsafe {
            xlib::XSync(self.display, 0);
        }
        let error = self.error.borrow_mut().take();

        match error {
            None => Ok(()),
            Some(inner) => Err(inner),
        }
    }

    /// Sets up a temporary X11 error handler for the duration of the given closure, and allows
    /// that closure to check on the latest X11 error at any time.
    ///
    /// # Safety
    ///
    /// The given display pointer *must* be and remain valid for the duration of this function, as
    /// well as for the duration of the given `handler` closure.
    pub unsafe fn handle<T, F: FnOnce(&mut XErrorHandler) -> T>(
        display: *mut xlib::Display,
        handler: F,
    ) -> T {
        /// # Safety
        /// The given display and error pointers *must* be valid for the duration of this function.
        unsafe extern "C" fn error_handler(
            dpy: *mut xlib::Display,
            err: *mut xlib::XErrorEvent,
        ) -> i32 {
            let handled = ACTIVE_X11_DISPLAY.with(|active_display| {
                if active_display.get() != dpy || dpy.is_null() || err.is_null() {
                    return false;
                }

                // Xlib invokes the handler synchronously while processing the
                // error, so the event pointer is valid for this callback.
                let err = unsafe { &*err };
                CURRENT_X11_ERROR.with(|error| {
                    let mut error = error.borrow_mut();
                    if error.is_none() {
                        *error = Some(unsafe { XLibError::from_event(err) });
                    }
                });
                true
            });
            if handled {
                return 0;
            }

            // Do not swallow unrelated Xlib errors from a host thread while
            // our temporary process-global handler is installed.
            let previous = PREVIOUS_XLIB_ERROR_HANDLER.load(Ordering::Acquire);
            if previous != 0 && previous != error_handler as usize {
                let previous: XLibErrorHandler = unsafe { std::mem::transmute(previous) };
                return unsafe { previous(dpy, err) };
            }
            0
        }

        let lock = XLIB_ERROR_HANDLER_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Flush all possible previous errors
        unsafe {
            xlib::XSync(display, 0);
        }

        let result = CURRENT_X11_ERROR.with(|error| {
            // Make sure to clear any errors from the last call to this function
            {
                *error.borrow_mut() = None;
            }

            ACTIVE_X11_DISPLAY.with(|active_display| active_display.set(display));
            let old_handler = unsafe { xlib::XSetErrorHandler(Some(error_handler)) };
            PREVIOUS_XLIB_ERROR_HANDLER.store(
                old_handler.map_or(0, |handler| handler as usize),
                Ordering::Release,
            );
            let panic_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let mut h = XErrorHandler { display, error };
                handler(&mut h)
            }));
            // Whatever happened, restore old error handler
            unsafe { xlib::XSetErrorHandler(old_handler) };
            ACTIVE_X11_DISPLAY.with(|active_display| active_display.set(std::ptr::null_mut()));
            PREVIOUS_XLIB_ERROR_HANDLER.store(0, Ordering::Release);
            panic_result
        });
        drop(lock);

        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
}

pub struct XLibError {
    type_: c_int,
    resourceid: xlib::XID,
    serial: c_ulong,
    error_code: c_uchar,
    request_code: c_uchar,
    minor_code: c_uchar,

    display_name: Box<str>,
}

impl XLibError {
    /// # Safety
    /// The display pointer inside error must be valid for the duration of this call
    unsafe fn from_event(error: &xlib::XErrorEvent) -> Self {
        Self {
            type_: error.type_,
            resourceid: error.resourceid,
            serial: error.serial,

            error_code: error.error_code,
            request_code: error.request_code,
            minor_code: error.minor_code,

            display_name: Self::get_display_name(error),
        }
    }

    /// # Safety
    /// The display pointer inside error must be valid for the duration of this call
    unsafe fn get_display_name(error: &xlib::XErrorEvent) -> Box<str> {
        let mut buf = [0; 255];
        unsafe {
            xlib::XGetErrorText(
                error.display,
                error.error_code.into(),
                buf.as_mut_ptr().cast(),
                (buf.len() - 1) as i32,
            );
        }

        buf[buf.len() - 1] = 0;
        // SAFETY: whatever XGetErrorText did or not, we guaranteed there is a nul byte at the end of the buffer
        let cstr = unsafe { CStr::from_ptr(buf.as_mut_ptr().cast()) };

        cstr.to_string_lossy().into()
    }
}

impl Debug for XLibError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XLibError")
            .field("error_code", &self.error_code)
            .field("error_message", &self.display_name)
            .field("minor_code", &self.minor_code)
            .field("request_code", &self.request_code)
            .field("type", &self.type_)
            .field("resource_id", &self.resourceid)
            .field("serial", &self.serial)
            .finish()
    }
}

impl Display for XLibError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "XLib error: {} (error code {})",
            &self.display_name, self.error_code
        )
    }
}

impl Error for XLibError {}
