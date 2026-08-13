//! In-process GUI pixel probe for hosted CI.
//!
//! GitHub-hosted macOS/Windows runners often cannot capture another window
//! through CoreGraphics or GDI even when the plugin is in the same process.
//! The GL/WGPU views copy a downsampled frame into this slot after drawing;
//! `sunmao_unittest_runner` then `dlsym`s [`sunmao_debug_read_frame`].

use std::sync::Mutex;

static LAST_FRAME: Mutex<Option<(u32, u32, Vec<u32>)>> = Mutex::new(None);

pub fn enabled() -> bool {
    std::env::var_os("SUNMAO_GUI_PIXEL_PROBE").is_some()
}

pub fn store_sampled_rgba(
    width: u32,
    height: u32,
    bytes: &[u8],
    bytes_per_pixel: usize,
    bgra: bool,
) {
    if width == 0 || height == 0 || bytes_per_pixel < 3 {
        return;
    }
    let row_stride = (width as usize).saturating_mul(bytes_per_pixel);
    if row_stride == 0 {
        return;
    }
    store_sampled(width, height, |x, y| {
        let offset = (y as usize)
            .saturating_mul(row_stride)
            .saturating_add((x as usize).saturating_mul(bytes_per_pixel));
        let pixel = bytes.get(offset..offset + 3).unwrap_or(&[0, 0, 0]);
        if bgra {
            u32::from_ne_bytes([pixel[2], pixel[1], pixel[0], 0])
        } else {
            u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], 0])
        }
    });
}

pub fn store_sampled(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> u32) {
    let step_x = (width / 64).max(1);
    let step_y = (height / 64).max(1);
    let cols = width.div_ceil(step_x);
    let rows = height.div_ceil(step_y);
    let mut pixels = Vec::with_capacity((cols * rows) as usize);
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            pixels.push(pixel(x, y));
            x = x.saturating_add(step_x);
            if x == 0 {
                break;
            }
        }
        y = y.saturating_add(step_y);
        if y == 0 {
            break;
        }
    }
    if pixels.iter().all(|value| *value == 0) {
        return;
    }
    if let Ok(mut slot) = LAST_FRAME.lock() {
        *slot = Some((cols, rows, pixels));
    }
}

/// Copy the last rendered GUI thumbnail for the unittest runner.
///
/// Returns the number of packed `0x00BBGGRR` pixels written, or `0` when no
/// frame has been captured yet.
#[no_mangle]
pub unsafe extern "C" fn sunmao_debug_read_frame(
    width_out: *mut u32,
    height_out: *mut u32,
    pixels_out: *mut u32,
    max_pixels: usize,
) -> i32 {
    let Ok(guard) = LAST_FRAME.lock() else {
        return 0;
    };
    let Some((width, height, pixels)) = guard.as_ref() else {
        return 0;
    };
    if !width_out.is_null() {
        unsafe { *width_out = *width };
    }
    if !height_out.is_null() {
        unsafe { *height_out = *height };
    }
    if pixels_out.is_null() || max_pixels == 0 {
        return 0;
    }
    let count = pixels.len().min(max_pixels);
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), pixels_out, count);
    }
    i32::try_from(count).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_store_round_trips_through_the_exported_probe() {
        store_sampled(8, 8, |x, y| u32::from_ne_bytes([x as u8, y as u8, 40, 0]));
        let mut width = 0;
        let mut height = 0;
        let mut pixels = vec![0_u32; 16];
        let count = unsafe {
            sunmao_debug_read_frame(&mut width, &mut height, pixels.as_mut_ptr(), pixels.len())
        };
        assert!(count > 0);
        assert_eq!(width, 8);
        assert_eq!(height, 8);
        assert_ne!(pixels[0], pixels[count as usize - 1]);
    }
}
