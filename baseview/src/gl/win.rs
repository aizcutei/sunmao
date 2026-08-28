use std::ffi::{c_void, CString, OsStr};
use std::os::windows::ffi::OsStrExt;

use raw_window_handle::RawWindowHandle;

use winapi::shared::minwindef::{HINSTANCE, HMODULE, PROC};
use winapi::shared::ntdef::WCHAR;
use winapi::shared::windef::{HDC, HGLRC, HWND};
use winapi::um::libloaderapi::{FreeLibrary, GetProcAddress, LoadLibraryA};
use winapi::um::wingdi::{
    wglCreateContext, wglDeleteContext, wglGetProcAddress, wglMakeCurrent, ChoosePixelFormat,
    DescribePixelFormat, SetPixelFormat, SwapBuffers, PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW,
    PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR,
};
use winapi::um::winnt::IMAGE_DOS_HEADER;
use winapi::um::winuser::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetDC, RegisterClassW, ReleaseDC,
    UnregisterClassW, CS_OWNDC, CW_USEDEFAULT, WNDCLASSW,
};

use super::{GlConfig, GlError, Profile};

// See https://www.khronos.org/registry/OpenGL/extensions/ARB/WGL_ARB_create_context.txt

type WglCreateContextAttribsARB = extern "system" fn(HDC, HGLRC, *const i32) -> HGLRC;

const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
const WGL_CONTEXT_PROFILE_MASK_ARB: i32 = 0x9126;

const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: i32 = 0x00000001;
const WGL_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB: i32 = 0x00000002;

// See https://www.khronos.org/registry/OpenGL/extensions/ARB/WGL_ARB_pixel_format.txt

type WglChoosePixelFormatARB =
    extern "system" fn(HDC, *const i32, *const f32, u32, *mut i32, *mut u32) -> i32;

const WGL_DRAW_TO_WINDOW_ARB: i32 = 0x2001;
const WGL_ACCELERATION_ARB: i32 = 0x2003;
const WGL_SUPPORT_OPENGL_ARB: i32 = 0x2010;
const WGL_DOUBLE_BUFFER_ARB: i32 = 0x2011;
const WGL_PIXEL_TYPE_ARB: i32 = 0x2013;
const WGL_RED_BITS_ARB: i32 = 0x2015;
const WGL_GREEN_BITS_ARB: i32 = 0x2017;
const WGL_BLUE_BITS_ARB: i32 = 0x2019;
const WGL_ALPHA_BITS_ARB: i32 = 0x201B;
const WGL_DEPTH_BITS_ARB: i32 = 0x2022;
const WGL_STENCIL_BITS_ARB: i32 = 0x2023;

const WGL_FULL_ACCELERATION_ARB: i32 = 0x2027;
const WGL_TYPE_RGBA_ARB: i32 = 0x202B;

// See https://www.khronos.org/registry/OpenGL/extensions/ARB/ARB_multisample.txt

const WGL_SAMPLE_BUFFERS_ARB: i32 = 0x2041;
const WGL_SAMPLES_ARB: i32 = 0x2042;

// See https://www.khronos.org/registry/OpenGL/extensions/ARB/ARB_framebuffer_sRGB.txt

const WGL_FRAMEBUFFER_SRGB_CAPABLE_ARB: i32 = 0x20A9;

// See https://www.khronos.org/registry/OpenGL/extensions/EXT/WGL_EXT_swap_control.txt

type WglSwapIntervalEXT = extern "system" fn(i32) -> i32;

// `wglGetProcAddress` uses the values 1, 2, 3, and -1 as failure sentinels on
// some implementations. They are non-null, so checking only for null before
// transmuting/calling the result can jump to an invalid address.
fn is_valid_wgl_proc_address(address: PROC) -> bool {
    let address = address as usize;
    address > 3 && address != usize::MAX
}

pub type CreationFailedError = ();
pub struct GlContext {
    hwnd: HWND,
    hdc: HDC,
    hglrc: HGLRC,
    gl_library: HMODULE,
}

extern "C" {
    static __ImageBase: IMAGE_DOS_HEADER;
}

impl GlContext {
    pub unsafe fn create(parent: &RawWindowHandle, config: GlConfig) -> Result<GlContext, GlError> {
        let handle = if let RawWindowHandle::Win32(handle) = parent {
            handle
        } else {
            return Err(GlError::InvalidWindowHandle);
        };
        let hwnd = handle.hwnd.get() as HWND;

        // Create temporary window and context to load function pointers

        let class_name_str = format!("raw-gl-context-window-{}", uuid::Uuid::new_v4().to_simple());
        let mut class_name: Vec<WCHAR> = OsStr::new(&class_name_str).encode_wide().collect();
        class_name.push(0);

        let hinstance = &__ImageBase as *const IMAGE_DOS_HEADER as HINSTANCE;

        let wnd_class = WNDCLASSW {
            style: CS_OWNDC,
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: hinstance,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };

        let class = RegisterClassW(&wnd_class);
        if class == 0 {
            return Err(GlError::CreationFailed(()));
        }

        let hwnd_tmp = CreateWindowExW(
            0,
            class as *const WCHAR,
            [0].as_ptr(),
            0,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null_mut(),
        );

        if hwnd_tmp.is_null() {
            UnregisterClassW(class_name.as_ptr(), hinstance);
            return Err(GlError::CreationFailed(()));
        }

        let hdc_tmp = GetDC(hwnd_tmp);
        if hdc_tmp.is_null() {
            DestroyWindow(hwnd_tmp);
            UnregisterClassW(class_name.as_ptr(), hinstance);
            return Err(GlError::CreationFailed(()));
        }

        let pfd_tmp = PIXELFORMATDESCRIPTOR {
            nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
            iPixelType: PFD_TYPE_RGBA,
            cColorBits: 32,
            cAlphaBits: 8,
            cDepthBits: 24,
            cStencilBits: 8,
            iLayerType: PFD_MAIN_PLANE,
            ..std::mem::zeroed()
        };

        let pixel_format_tmp = ChoosePixelFormat(hdc_tmp, &pfd_tmp);
        if pixel_format_tmp == 0 || SetPixelFormat(hdc_tmp, pixel_format_tmp, &pfd_tmp) == 0 {
            ReleaseDC(hwnd_tmp, hdc_tmp);
            DestroyWindow(hwnd_tmp);
            UnregisterClassW(class_name.as_ptr(), hinstance);
            return Err(GlError::CreationFailed(()));
        }

        let hglrc_tmp = wglCreateContext(hdc_tmp);
        if hglrc_tmp.is_null() {
            ReleaseDC(hwnd_tmp, hdc_tmp);
            DestroyWindow(hwnd_tmp);
            UnregisterClassW(class_name.as_ptr(), hinstance);
            return Err(GlError::CreationFailed(()));
        }

        if wglMakeCurrent(hdc_tmp, hglrc_tmp) == 0 {
            wglDeleteContext(hglrc_tmp);
            ReleaseDC(hwnd_tmp, hdc_tmp);
            DestroyWindow(hwnd_tmp);
            UnregisterClassW(class_name.as_ptr(), hinstance);
            return Err(GlError::CreationFailed(()));
        }

        let wgl_create_context_attribs_arb: Option<WglCreateContextAttribsARB> = {
            let symbol = b"wglCreateContextAttribsARB\0";
            let addr = wglGetProcAddress(symbol.as_ptr() as *const i8);
            if is_valid_wgl_proc_address(addr) {
                #[allow(clippy::missing_transmute_annotations)]
                Some(std::mem::transmute(addr))
            } else {
                None
            }
        };

        let wgl_choose_pixel_format_arb: Option<WglChoosePixelFormatARB> = {
            let symbol = b"wglChoosePixelFormatARB\0";
            let addr = wglGetProcAddress(symbol.as_ptr() as *const i8);
            if is_valid_wgl_proc_address(addr) {
                #[allow(clippy::missing_transmute_annotations)]
                Some(std::mem::transmute(addr))
            } else {
                None
            }
        };

        let wgl_swap_interval_ext: Option<WglSwapIntervalEXT> = {
            let symbol = b"wglSwapIntervalEXT\0";
            let addr = wglGetProcAddress(symbol.as_ptr() as *const i8);
            if is_valid_wgl_proc_address(addr) {
                #[allow(clippy::missing_transmute_annotations)]
                Some(std::mem::transmute(addr))
            } else {
                None
            }
        };

        wglMakeCurrent(hdc_tmp, std::ptr::null_mut());
        wglDeleteContext(hglrc_tmp);
        ReleaseDC(hwnd_tmp, hdc_tmp);
        DestroyWindow(hwnd_tmp);
        UnregisterClassW(class_name.as_ptr(), hinstance);

        let (Some(wgl_choose_pixel_format_arb), Some(wgl_create_context_attribs_arb)) =
            (wgl_choose_pixel_format_arb, wgl_create_context_attribs_arb)
        else {
            return Err(GlError::VersionNotSupported);
        };

        // Create actual context

        let hdc = GetDC(hwnd);
        if hdc.is_null() {
            return Err(GlError::CreationFailed(()));
        }

        #[rustfmt::skip]
        let pixel_format_attribs = [
            WGL_DRAW_TO_WINDOW_ARB, 1,
            WGL_ACCELERATION_ARB, WGL_FULL_ACCELERATION_ARB,
            WGL_SUPPORT_OPENGL_ARB, 1,
            WGL_DOUBLE_BUFFER_ARB, config.double_buffer as i32,
            WGL_PIXEL_TYPE_ARB, WGL_TYPE_RGBA_ARB,
            WGL_RED_BITS_ARB, config.red_bits as i32,
            WGL_GREEN_BITS_ARB, config.green_bits as i32,
            WGL_BLUE_BITS_ARB, config.blue_bits as i32,
            WGL_ALPHA_BITS_ARB, config.alpha_bits as i32,
            WGL_DEPTH_BITS_ARB, config.depth_bits as i32,
            WGL_STENCIL_BITS_ARB, config.stencil_bits as i32,
            WGL_SAMPLE_BUFFERS_ARB, config.samples.is_some() as i32,
            WGL_SAMPLES_ARB, config.samples.unwrap_or(0) as i32,
            WGL_FRAMEBUFFER_SRGB_CAPABLE_ARB, config.srgb as i32,
            0,
        ];

        let mut pixel_format = 0;
        let mut num_formats = 0;
        if wgl_choose_pixel_format_arb(
            hdc,
            pixel_format_attribs.as_ptr(),
            std::ptr::null(),
            1,
            &mut pixel_format,
            &mut num_formats,
        ) == 0
            || num_formats == 0
            || pixel_format == 0
        {
            ReleaseDC(hwnd, hdc);
            return Err(GlError::CreationFailed(()));
        }

        let mut pfd: PIXELFORMATDESCRIPTOR = std::mem::zeroed();
        if DescribePixelFormat(
            hdc,
            pixel_format,
            std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32,
            &mut pfd,
        ) == 0
            || SetPixelFormat(hdc, pixel_format, &pfd) == 0
        {
            ReleaseDC(hwnd, hdc);
            return Err(GlError::CreationFailed(()));
        }

        let profile_mask = match config.profile {
            Profile::Core => WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
            Profile::Compatibility => WGL_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
        };

        #[rustfmt::skip]
        let ctx_attribs = [
            WGL_CONTEXT_MAJOR_VERSION_ARB, config.version.0 as i32,
            WGL_CONTEXT_MINOR_VERSION_ARB, config.version.1 as i32,
            WGL_CONTEXT_PROFILE_MASK_ARB, profile_mask,
            0
        ];

        let hglrc = wgl_create_context_attribs_arb(hdc, std::ptr::null_mut(), ctx_attribs.as_ptr());
        if hglrc.is_null() {
            ReleaseDC(hwnd, hdc);
            return Err(GlError::CreationFailed(()));
        }

        let gl_library = LoadLibraryA(b"opengl32.dll\0".as_ptr() as *const i8);
        if gl_library.is_null() {
            wglDeleteContext(hglrc);
            ReleaseDC(hwnd, hdc);
            return Err(GlError::CreationFailed(()));
        }

        if wglMakeCurrent(hdc, hglrc) == 0 {
            FreeLibrary(gl_library);
            wglDeleteContext(hglrc);
            ReleaseDC(hwnd, hdc);
            return Err(GlError::CreationFailed(()));
        }
        if let Some(wgl_swap_interval_ext) = wgl_swap_interval_ext {
            wgl_swap_interval_ext(config.vsync as i32);
        }
        wglMakeCurrent(hdc, std::ptr::null_mut());

        Ok(GlContext {
            hwnd,
            hdc,
            hglrc,
            gl_library,
        })
    }

    pub unsafe fn make_current(&self) -> Result<(), GlError> {
        if wglMakeCurrent(self.hdc, self.hglrc) == 0 {
            Err(GlError::CreationFailed(()))
        } else {
            Ok(())
        }
    }

    pub unsafe fn make_not_current(&self) -> Result<(), GlError> {
        if wglMakeCurrent(self.hdc, std::ptr::null_mut()) == 0 {
            Err(GlError::CreationFailed(()))
        } else {
            Ok(())
        }
    }

    pub fn get_proc_address(&self, symbol: &str) -> *const c_void {
        let Ok(symbol) = CString::new(symbol) else {
            return std::ptr::null();
        };
        unsafe {
            let addr = wglGetProcAddress(symbol.as_ptr()) as PROC;
            if is_valid_wgl_proc_address(addr) {
                addr as *const c_void
            } else {
                GetProcAddress(self.gl_library, symbol.as_ptr()) as *const c_void
            }
        }
    }

    pub fn swap_buffers(&self) -> Result<(), GlError> {
        if unsafe { SwapBuffers(self.hdc) } == 0 {
            Err(GlError::CreationFailed(()))
        } else {
            Ok(())
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            wglMakeCurrent(std::ptr::null_mut(), std::ptr::null_mut());
            wglDeleteContext(self.hglrc);
            ReleaseDC(self.hwnd, self.hdc);
            FreeLibrary(self.gl_library);
        }
    }
}
