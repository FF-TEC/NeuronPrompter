// =============================================================================
// Platform-specific transparent splash screen rendering for the NeuronPrompter
// startup sequence.
//
// Decodes an embedded PNG (960x360 RGBA, pre-rendered by tools/gen/generate_splash.py)
// and paints it onto a tao window using native OS APIs so the desktop is visible
// behind the logo text. The PNG contains the "NeuronPrompter" brand text with a
// purple-to-cyan gradient on "Prompter", gaussian blur glow, and fully transparent
// background.
//
// Platform rendering:
// - **Windows**: Win32 layered window -- `UpdateLayeredWindow` with premultiplied
//   BGRA pixel data in a 32-bit DIB section. The window receives `WS_EX_LAYERED`
//   after creation, and the bitmap is applied as a one-shot operation (no WM_PAINT).
// - **macOS**: `NSImage` loaded from the raw PNG bytes, displayed in an `NSImageView`
//   added to the `NSWindow`'s content view. The window is set to non-opaque with a
//   clear background color. AppKit handles Retina 2x scaling automatically.
// - **Linux**: Cairo `ImageSurface` from premultiplied ARGB data, painted in a GTK
//   `draw` signal handler. The window uses an RGBA visual and `app_paintable` mode
//   (both set by tao's `with_transparent(true)` + `with_transparent_draw(false)`).
//
// This module is compiled only when the `gui` feature flag is enabled.
// =============================================================================

#![allow(unsafe_code)]

use tao::window::Window;

/// Embedded splash screen PNG (960x360 pixels, RGBA, pre-rendered by
/// tools/gen/generate_splash.py). Contains the "NeuronPrompter" logo text with
/// purple-to-cyan gradient on "Prompter", gaussian blur glow, and fully
/// transparent background. The 2x resolution provides crisp rendering
/// on HiDPI / Retina displays.
const SPLASH_PNG: &[u8] = include_bytes!("../assets/splash.png");

/// Decodes the embedded PNG to an RGBA pixel buffer using the `image` crate.
///
/// Returns a tuple of (pixels, width, height) where pixels is a Vec<u8> in
/// row-major RGBA byte order (4 bytes per pixel: R, G, B, A).
///
/// Panics if the embedded PNG is corrupted or not a valid image (this is a
/// compile-time-embedded asset, so corruption indicates a build toolchain
/// problem).
fn decode_png() -> (Vec<u8>, u32, u32) {
    // The PNG is a compile-time-embedded asset, so decoding failure indicates
    // a build toolchain problem. Unwrap is safe here: the asset is validated
    // by the splash_png_decodes_to_expected_dimensions test.
    #[allow(clippy::expect_used)]
    let img = image::load_from_memory(SPLASH_PNG)
        .expect("embedded splash.png is a valid PNG")
        .into_rgba8();
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}

/// Renders the transparent splash PNG onto the given tao window using
/// platform-native APIs. After this call, the window displays the PNG
/// with per-pixel alpha transparency -- the desktop is visible behind
/// areas where the PNG alpha channel is zero.
///
/// # Errors
///
/// Returns a `String` describing the failure if the platform-specific
/// rendering API call fails (e.g., `CreateDIBSection` or
/// `UpdateLayeredWindow` on Windows, `NSImage::initWithData` on macOS,
/// Cairo surface creation on Linux).
pub fn render_splash_on_window(window: &Window) -> Result<(), String> {
    let (rgba_pixels, width, height) = decode_png();
    render_platform(window, &rgba_pixels, width, height)
}

// ---------------------------------------------------------------------------
// Windows: Win32 layered window with UpdateLayeredWindow
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_lines)]
fn render_platform(window: &Window, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    use image::RgbaImage;
    use image::imageops::FilterType;
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
        RGBQUAD, ReleaseDC, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, ULW_ALPHA, UpdateLayeredWindow,
        WS_EX_LAYERED,
    };

    let phys_size = window.inner_size();
    let target_w = phys_size.width;
    let target_h = phys_size.height;

    let (final_rgba, final_w, final_h) = if target_w == width && target_h == height {
        (rgba.to_vec(), width, height)
    } else {
        let src = RgbaImage::from_raw(width, height, rgba.to_vec())
            .ok_or("failed to construct RgbaImage from decoded PNG")?;
        let resized = image::imageops::resize(&src, target_w, target_h, FilterType::CatmullRom);
        let (rw, rh) = resized.dimensions();
        (resized.into_raw(), rw, rh)
    };

    let hwnd_raw = window.hwnd();
    let hwnd = HWND(hwnd_raw as *mut _);

    // Convert RGBA to premultiplied BGRA
    let pixel_count = (final_w * final_h) as usize;
    let mut bgra = Vec::with_capacity(pixel_count * 4);
    for pixel in final_rgba.chunks_exact(4) {
        let (r, g, b, a) = (
            u32::from(pixel[0]),
            u32::from(pixel[1]),
            u32::from(pixel[2]),
            u32::from(pixel[3]),
        );
        #[allow(clippy::cast_possible_truncation)]
        {
            bgra.push(((b * a / 255) & 0xFF) as u8);
            bgra.push(((g * a / 255) & 0xFF) as u8);
            bgra.push(((r * a / 255) & 0xFF) as u8);
            bgra.push(a as u8);
        }
    }

    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        #[allow(clippy::cast_possible_wrap)]
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED.0 as isize);

        let screen_dc = GetDC(Some(hwnd));
        let mem_dc = CreateCompatibleDC(Some(screen_dc));

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: final_w as i32,
                biHeight: -(final_h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD::default()],
        };

        let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(
            Some(mem_dc),
            &raw const bmi,
            DIB_RGB_COLORS,
            &raw mut bits_ptr,
            None,
            0,
        )
        .map_err(|e| format!("CreateDIBSection failed: {e}"))?;

        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits_ptr.cast::<u8>(), bgra.len());

        let old_bitmap = SelectObject(mem_dc, dib.into());

        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let size = SIZE {
            cx: target_w as i32,
            cy: target_h as i32,
        };
        let src_point = POINT { x: 0, y: 0 };
        #[allow(clippy::cast_possible_truncation)]
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            None,
            Some(&raw const size),
            Some(mem_dc),
            Some(&raw const src_point),
            COLORREF(0),
            Some(&raw const blend),
            ULW_ALPHA,
        )
        .map_err(|e| format!("UpdateLayeredWindow failed: {e}"))?;

        SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(Some(hwnd), screen_dc);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// macOS: NSWindow transparency + NSImageView with the PNG
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn render_platform(window: &Window, _rgba: &[u8], _width: u32, _height: u32) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadOnly};
    use objc2_app_kit::{NSColor, NSImage, NSImageView, NSView, NSWindow};
    use objc2_foundation::{NSData, NSSize};
    use tao::platform::macos::WindowExtMacOS;

    unsafe {
        let ns_window_ptr = window.ns_window();
        let ns_view_ptr = window.ns_view();

        let ns_window: &NSWindow = &*(ns_window_ptr as *const NSWindow);
        let ns_view: &NSView = &*(ns_view_ptr as *const NSView);

        ns_window.setOpaque(false);
        ns_window.setHasShadow(false);
        let clear_color = NSColor::clearColor();
        ns_window.setBackgroundColor(Some(&clear_color));

        let png_data = NSData::with_bytes(SPLASH_PNG);
        let image: Retained<NSImage> = NSImage::initWithData(NSImage::alloc(), &png_data)
            .ok_or_else(|| "NSImage::initWithData returned nil".to_string())?;

        image.setSize(NSSize {
            width: 480.0,
            height: 180.0,
        });

        let mtm = MainThreadMarker::new_unchecked();
        let frame = ns_view.bounds();
        let image_view: Retained<NSImageView> =
            NSImageView::initWithFrame(NSImageView::alloc(mtm), frame);
        image_view.setImage(Some(&image));
        ns_view.addSubview(&image_view);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Linux: GTK + Cairo draw handler with RGBA visual
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn render_platform(window: &Window, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    use cairo::Format;
    use gtk::prelude::*;
    use tao::platform::unix::WindowExtUnix;

    let stride = Format::ARgb32
        .stride_for_width(width)
        .map_err(|_| "cairo stride calculation failed".to_string())?;

    // Convert RGBA to premultiplied BGRA (Cairo ARgb32 on little-endian)
    let mut argb_data = vec![0u8; (stride * height as i32) as usize];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let src = (y * width as usize + x) * 4;
            let dst = y * stride as usize + x * 4;
            let (r, g, b, a) = (
                rgba[src] as u32,
                rgba[src + 1] as u32,
                rgba[src + 2] as u32,
                rgba[src + 3] as u32,
            );
            argb_data[dst] = ((b * a / 255) & 0xFF) as u8;
            argb_data[dst + 1] = ((g * a / 255) & 0xFF) as u8;
            argb_data[dst + 2] = ((r * a / 255) & 0xFF) as u8;
            argb_data[dst + 3] = a as u8;
        }
    }

    let surface = cairo::ImageSurface::create_for_data(
        argb_data,
        Format::ARgb32,
        width as i32,
        height as i32,
        stride,
    )
    .map_err(|e| format!("cairo ImageSurface creation failed: {e}"))?;

    let gtk_window = window.gtk_window();
    let img_width = width as f64;
    let img_height = height as f64;

    gtk_window.connect_draw(move |widget, cr| {
        let alloc = widget.allocation();

        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        let _ = cr.paint();

        let sx = alloc.width() as f64 / img_width;
        let sy = alloc.height() as f64 / img_height;
        cr.scale(sx, sy);

        cr.set_operator(cairo::Operator::Over);
        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
        let _ = cr.paint();

        gtk::glib::Propagation::Stop
    });

    gtk_window.queue_draw();

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn splash_png_decodes_to_expected_dimensions() {
        let (pixels, w, h) = super::decode_png();
        assert_eq!(w, 960, "splash PNG width must be 960 pixels (2x of 480)");
        assert_eq!(h, 360, "splash PNG height must be 360 pixels (2x of 180)");
        assert_eq!(
            pixels.len(),
            (w * h * 4) as usize,
            "pixel buffer must contain width * height * 4 bytes (RGBA)"
        );
    }
}
