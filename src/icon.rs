use std::f32::consts::PI;

use windows_sys::Win32::Foundation::COLORREF;
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateSolidBrush,
    DeleteDC, DeleteObject, Ellipse, GetDC, LineTo, MoveToEx, Pie, ReleaseDC, SelectObject,
    HGDIOBJ, PS_SOLID,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, HICON, ICONINFO,
};

pub const ICON_SIZE: i32 = 32;

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

unsafe fn select_pen(dc: windows_sys::Win32::Graphics::Gdi::HDC, color: COLORREF, width: i32,
                     objects: &mut Vec<HGDIOBJ>) {
    let pen = CreatePen(PS_SOLID, width, color);
    if !pen.is_null() {
        objects.push(pen as HGDIOBJ);
        SelectObject(dc, pen as HGDIOBJ);
    }
}

unsafe fn select_brush(dc: windows_sys::Win32::Graphics::Gdi::HDC, color: COLORREF,
                       objects: &mut Vec<HGDIOBJ>) {
    let brush = CreateSolidBrush(color);
    if !brush.is_null() {
        objects.push(brush as HGDIOBJ);
        SelectObject(dc, brush as HGDIOBJ);
    }
}

/// Builds a tray icon using GDI. Every created GDI object is deselected before
/// deletion, preventing the per-second icon refresh from exhausting GDI handles.
pub fn create_tray_icon(is_active: bool, is_infinite: bool, progress: f32) -> HICON {
    unsafe {
        let screen_dc = GetDC(std::ptr::null_mut());
        if screen_dc.is_null() {
            return std::ptr::null_mut();
        }

        let mem_dc = CreateCompatibleDC(screen_dc);
        let color_bmp = CreateCompatibleBitmap(screen_dc, ICON_SIZE, ICON_SIZE);
        let mask_bmp = CreateBitmap(ICON_SIZE, ICON_SIZE, 1, 1, std::ptr::null());

        if mem_dc.is_null() || color_bmp.is_null() || mask_bmp.is_null() {
            if !mem_dc.is_null() { DeleteDC(mem_dc); }
            if !color_bmp.is_null() { DeleteObject(color_bmp as HGDIOBJ); }
            if !mask_bmp.is_null() { DeleteObject(mask_bmp as HGDIOBJ); }
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return std::ptr::null_mut();
        }

        let old_bmp = SelectObject(mem_dc, color_bmp as HGDIOBJ);
        let mut objects: Vec<HGDIOBJ> = Vec::new();

        // Keep the original selected pen and brush so they can be restored
        // before deleting any GDI objects created below.
        let initial_brush = CreateSolidBrush(rgb(0, 0, 0));
        let initial_pen = CreatePen(PS_SOLID, 1, rgb(0, 0, 0));
        if initial_brush.is_null() || initial_pen.is_null() {
            if !initial_brush.is_null() { DeleteObject(initial_brush as HGDIOBJ); }
            if !initial_pen.is_null() { DeleteObject(initial_pen as HGDIOBJ); }
            SelectObject(mem_dc, old_bmp);
            DeleteDC(mem_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            DeleteObject(color_bmp as HGDIOBJ);
            DeleteObject(mask_bmp as HGDIOBJ);
            return std::ptr::null_mut();
        }
        let original_brush = SelectObject(mem_dc, initial_brush as HGDIOBJ);
        let original_pen = SelectObject(mem_dc, initial_pen as HGDIOBJ);
        objects.push(initial_brush as HGDIOBJ);
        objects.push(initial_pen as HGDIOBJ);
        windows_sys::Win32::Graphics::Gdi::Rectangle(
            mem_dc, 0, 0, ICON_SIZE + 1, ICON_SIZE + 1,
        );

        let center = ICON_SIZE / 2;
        let radius = center - 3;
        let left = center - radius;
        let top = center - radius;
        let right = center + radius;
        let bottom = center + radius;

        if !is_active {
            select_pen(mem_dc, rgb(120, 120, 120), 2, &mut objects);
            select_brush(mem_dc, rgb(40, 40, 40), &mut objects);
            Ellipse(mem_dc, left, top, right, bottom);

            select_pen(mem_dc, rgb(160, 160, 160), 1, &mut objects);
            select_brush(mem_dc, rgb(160, 160, 160), &mut objects);
            Ellipse(mem_dc, center - 2, center - 2, center + 2, center + 2);
        } else if is_infinite {
            select_pen(mem_dc, rgb(0, 220, 255), 2, &mut objects);
            select_brush(mem_dc, rgb(0, 80, 120), &mut objects);
            Ellipse(mem_dc, left, top, right, bottom);

            select_pen(mem_dc, rgb(255, 255, 255), 1, &mut objects);
            select_brush(mem_dc, rgb(255, 255, 255), &mut objects);
            Ellipse(mem_dc, center - 3, center - 3, center + 3, center + 3);
        } else {
            let p = progress.clamp(0.0, 1.0);
            select_pen(mem_dc, rgb(60, 60, 60), 1, &mut objects);
            select_brush(mem_dc, rgb(30, 30, 30), &mut objects);
            Ellipse(mem_dc, left, top, right, bottom);

            let fill_color = if p > 0.20 {
                rgb(0, 180, 255)
            } else if p > 0.05 {
                rgb(255, 170, 0)
            } else {
                rgb(255, 50, 50)
            };

            let angle = (1.0 - p) * 2.0 * PI;
            let end_x = center + (radius as f32 * angle.sin()) as i32;
            let end_y = center - (radius as f32 * angle.cos()) as i32;

            select_pen(mem_dc, fill_color, 1, &mut objects);
            select_brush(mem_dc, fill_color, &mut objects);
            Pie(mem_dc, left, top, right, bottom, center, top, end_x, end_y);

            select_pen(mem_dc, rgb(255, 255, 255), 1, &mut objects);
            MoveToEx(mem_dc, center, center, std::ptr::null_mut());
            LineTo(mem_dc, end_x, end_y);
        }

        // Restore the original selected objects before deleting the created ones.
        SelectObject(mem_dc, original_pen);
        SelectObject(mem_dc, original_brush);
        for object in objects {
            DeleteObject(object);
        }

        SelectObject(mem_dc, old_bmp);
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut(), screen_dc);

        // A monochrome icon mask uses 1 for transparent and 0 for opaque.
        let mask_dc = CreateCompatibleDC(std::ptr::null_mut());
        if mask_dc.is_null() {
            DeleteObject(color_bmp as HGDIOBJ);
            DeleteObject(mask_bmp as HGDIOBJ);
            return std::ptr::null_mut();
        }
        let old_mask_bmp = SelectObject(mask_dc, mask_bmp as HGDIOBJ);
        let white_brush = CreateSolidBrush(rgb(255, 255, 255));
        let black_brush = CreateSolidBrush(rgb(0, 0, 0));
        let no_pen = CreatePen(PS_SOLID, 1, rgb(0, 0, 0));

        if white_brush.is_null() || black_brush.is_null() || no_pen.is_null() {
            if !white_brush.is_null() { DeleteObject(white_brush as HGDIOBJ); }
            if !black_brush.is_null() { DeleteObject(black_brush as HGDIOBJ); }
            if !no_pen.is_null() { DeleteObject(no_pen as HGDIOBJ); }
            SelectObject(mask_dc, old_mask_bmp);
            DeleteDC(mask_dc);
            DeleteObject(color_bmp as HGDIOBJ);
            DeleteObject(mask_bmp as HGDIOBJ);
            return std::ptr::null_mut();
        }

        let original_mask_brush = SelectObject(mask_dc, white_brush as HGDIOBJ);
        let original_mask_pen = SelectObject(mask_dc, no_pen as HGDIOBJ);
        windows_sys::Win32::Graphics::Gdi::Rectangle(
            mask_dc, 0, 0, ICON_SIZE + 1, ICON_SIZE + 1,
        );

        SelectObject(mask_dc, black_brush as HGDIOBJ);
        Ellipse(mask_dc, left, top, right, bottom);

        // Restore original selections before deleting temporary GDI objects.
        SelectObject(mask_dc, original_mask_pen);
        SelectObject(mask_dc, original_mask_brush);
        DeleteObject(white_brush as HGDIOBJ);
        DeleteObject(black_brush as HGDIOBJ);
        DeleteObject(no_pen as HGDIOBJ);
        SelectObject(mask_dc, old_mask_bmp);
        DeleteDC(mask_dc);

        let mut icon_info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };
        let hicon = CreateIconIndirect(&mut icon_info);

        DeleteObject(color_bmp as HGDIOBJ);
        DeleteObject(mask_bmp as HGDIOBJ);
        hicon
    }
}

pub fn destroy_tray_icon(hicon: HICON) {
    if !hicon.is_null() {
        unsafe { DestroyIcon(hicon); }
    }
}
