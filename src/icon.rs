use windows_sys::Win32::Foundation::COLORREF;
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateSolidBrush,
    DeleteDC, DeleteObject, Ellipse, GetDC, Pie, ReleaseDC, SelectObject, PS_SOLID,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};
use std::f32::consts::PI;

pub const ICON_SIZE: i32 = 32;

/// Creates an RGB COLORREF: 0x00BBGGRR
#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// Dynamic Icon Generator using native Win32 GDI:
/// Draws a circular clock progress bar:
/// - Gray ring background
/// - Colored pie wedge filling/draining clockwise from top
/// - Color shifts: Blue -> Orange (<20%) -> Red (<5%)
/// - When Off: Subtle hollow gray ring with dot
/// - When Infinite: Solid glowing cyan ring with infinity badge
pub fn create_tray_icon(is_active: bool, is_infinite: bool, progress: f32) -> HICON {
    unsafe {
        let screen_dc = GetDC(std::ptr::null_mut());
        let mem_dc = CreateCompatibleDC(screen_dc);
        let color_bmp = CreateCompatibleBitmap(screen_dc, ICON_SIZE, ICON_SIZE);
        let mask_bmp = CreateBitmap(ICON_SIZE, ICON_SIZE, 1, 1, std::ptr::null());

        let old_bmp = SelectObject(mem_dc, color_bmp as _);

        // Fill background with black (transparent mask color)
        let bg_brush = CreateSolidBrush(rgb(0, 0, 0));
        let old_brush = SelectObject(mem_dc, bg_brush as _);
        let null_pen = CreatePen(PS_SOLID, 0, rgb(0, 0, 0));
        let old_pen = SelectObject(mem_dc, null_pen as _);

        windows_sys::Win32::Graphics::Gdi::Rectangle(mem_dc, 0, 0, ICON_SIZE + 1, ICON_SIZE + 1);

        let center = ICON_SIZE / 2;
        let radius = (ICON_SIZE / 2) - 3;
        let left = center - radius;
        let top = center - radius;
        let right = center + radius;
        let bottom = center + radius;

        if !is_active {
            // OFF state: Dim grey circle outline
            let off_pen = CreatePen(PS_SOLID, 2, rgb(120, 120, 120));
            SelectObject(mem_dc, off_pen as _);
            let off_brush = CreateSolidBrush(rgb(40, 40, 40));
            SelectObject(mem_dc, off_brush as _);

            Ellipse(mem_dc, left, top, right, bottom);

            // Draw center dot
            let dot_pen = CreatePen(PS_SOLID, 1, rgb(160, 160, 160));
            SelectObject(mem_dc, dot_pen as _);
            let dot_brush = CreateSolidBrush(rgb(160, 160, 160));
            SelectObject(mem_dc, dot_brush as _);
            Ellipse(mem_dc, center - 2, center - 2, center + 2, center + 2);

            DeleteObject(off_pen as _);
            DeleteObject(off_brush as _);
            DeleteObject(dot_pen as _);
            DeleteObject(dot_brush as _);
        } else if is_infinite {
            // INFINITE state: Bright Cyan circle with glowing inner circle
            let cyan_pen = CreatePen(PS_SOLID, 2, rgb(0, 220, 255));
            SelectObject(mem_dc, cyan_pen as _);
            let cyan_brush = CreateSolidBrush(rgb(0, 80, 120));
            SelectObject(mem_dc, cyan_brush as _);

            Ellipse(mem_dc, left, top, right, bottom);

            // Inner solid eye
            let inner_pen = CreatePen(PS_SOLID, 1, rgb(255, 255, 255));
            SelectObject(mem_dc, inner_pen as _);
            let inner_brush = CreateSolidBrush(rgb(255, 255, 255));
            SelectObject(mem_dc, inner_brush as _);
            Ellipse(mem_dc, center - 3, center - 3, center + 3, center + 3);

            DeleteObject(cyan_pen as _);
            DeleteObject(cyan_brush as _);
            DeleteObject(inner_pen as _);
            DeleteObject(inner_brush as _);
        } else {
            // TIMED state: Circular clock progress
            // 1. Draw outer circle background track (dark gray)
            let track_pen = CreatePen(PS_SOLID, 1, rgb(60, 60, 60));
            SelectObject(mem_dc, track_pen as _);
            let track_brush = CreateSolidBrush(rgb(30, 30, 30));
            SelectObject(mem_dc, track_brush as _);
            Ellipse(mem_dc, left, top, right, bottom);

            // 2. Pick color based on remaining progress
            let fill_color = if progress > 0.20 {
                rgb(0, 180, 255) // Bright Sky Blue
            } else if progress > 0.05 {
                rgb(255, 170, 0) // Amber / Orange warning
            } else {
                rgb(255, 50, 50) // Red alert (<5%)
            };

            // 3. Draw clockwise pie wedge
            // Start at 12 o'clock: (center, top)
            let start_x = center;
            let start_y = top;

            // Angle in radians (0 at top, clockwise)
            let angle = (1.0 - progress) * 2.0 * PI;
            let end_x = center + (radius as f32 * angle.sin()) as i32;
            let end_y = center - (radius as f32 * angle.cos()) as i32;

            let pie_pen = CreatePen(PS_SOLID, 1, fill_color);
            SelectObject(mem_dc, pie_pen as _);
            let pie_brush = CreateSolidBrush(fill_color);
            SelectObject(mem_dc, pie_brush as _);

            Pie(
                mem_dc,
                left,
                top,
                right,
                bottom,
                start_x,
                start_y,
                end_x,
                end_y,
            );

            // 4. White central needle indicator
            let needle_pen = CreatePen(PS_SOLID, 1, rgb(255, 255, 255));
            SelectObject(mem_dc, needle_pen as _);
            windows_sys::Win32::Graphics::Gdi::MoveToEx(mem_dc, center, center, std::ptr::null_mut());
            windows_sys::Win32::Graphics::Gdi::LineTo(mem_dc, end_x, end_y);

            DeleteObject(track_pen as _);
            DeleteObject(track_brush as _);
            DeleteObject(pie_pen as _);
            DeleteObject(pie_brush as _);
            DeleteObject(needle_pen as _);
        }

        // Clean up DC
        SelectObject(mem_dc, old_pen as _);
        SelectObject(mem_dc, old_brush as _);
        SelectObject(mem_dc, old_bmp as _);
        DeleteObject(bg_brush as _);
        DeleteObject(null_pen as _);
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut(), screen_dc);

        // Generate circular transparency mask
        let mask_dc = CreateCompatibleDC(std::ptr::null_mut());
        let old_mask_bmp = SelectObject(mask_dc, mask_bmp as _);
        let white_brush = CreateSolidBrush(rgb(255, 255, 255));
        let black_brush = CreateSolidBrush(rgb(0, 0, 0));
        let no_pen = CreatePen(PS_SOLID, 0, rgb(0, 0, 0));

        // White everywhere (transparent for mask)
        let prev_brush = SelectObject(mask_dc, white_brush as _);
        let prev_pen = SelectObject(mask_dc, no_pen as _);
        windows_sys::Win32::Graphics::Gdi::Rectangle(mask_dc, 0, 0, ICON_SIZE + 1, ICON_SIZE + 1);

        // Black inside circle (visible for mask)
        SelectObject(mask_dc, black_brush as _);
        Ellipse(mask_dc, left, top, right, bottom);

        SelectObject(mask_dc, prev_pen as _);
        SelectObject(mask_dc, prev_brush as _);
        SelectObject(mask_dc, old_mask_bmp as _);
        DeleteObject(white_brush as _);
        DeleteObject(black_brush as _);
        DeleteObject(no_pen as _);
        DeleteDC(mask_dc);

        let mut icon_info = ICONINFO {
            fIcon: 1, // TRUE = Icon, FALSE = Cursor
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };

        let hicon = CreateIconIndirect(&mut icon_info);

        DeleteObject(color_bmp as _);
        DeleteObject(mask_bmp as _);

        hicon
    }
}

pub fn destroy_tray_icon(hicon: HICON) {
    if !hicon.is_null() {
        unsafe {
            DestroyIcon(hicon);
        }
    }
}
