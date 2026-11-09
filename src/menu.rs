//! Custom system-tray popup menu.
//!
//! Replaces the classic `TrackPopupMenu` HMENU with an owner-drawn popup
//! window so the menu can carry the Windows 11 Mica backdrop
//! (`DwmSetWindowAttribute`, documented) plus a slide-up animation. Older
//! builds fall back to acrylic blur (`SetWindowCompositionAttribute`,
//! undocumented, same technique Twinkle Tray uses in `acrylic.cc`). Commands
//! are still dispatched to the parent window as `WM_COMMAND`, keeping the
//! existing command IDs.

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FW_NORMAL, FillRect, GetMonitorInfoW, GetTextExtentPoint32W, HBRUSH, HDC, HFONT,
    InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect, PAINTSTRUCT, SRCCOPY,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::UI::Controls::MARGINS;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowRect, HCURSOR, HWND_TOPMOST, IDC_ARROW, KillTimer, LoadCursorW, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, WA_INACTIVE,
    WM_ACTIVATE, WM_CLOSE, WM_COMMAND, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const WM_TIMER_ANIM: usize = 3101;

const ANIM_DURATION_MS: u32 = 200;
const ANIM_FRAME_MS: u32 = 15;

/// Mica: the documented Windows 11 backdrop material, applied through
/// DwmSetWindowAttribute and themed by the system (light/dark + accent).
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMSBT_MAINWINDOW: u32 = 2;

const ITEM_PADDING_X: i32 = 14;
const CHECKBOX_WIDTH: i32 = 24;
const ITEM_HEIGHT: i32 = 34;
const SEPARATOR_HEIGHT: i32 = 9;
const MENU_PADDING_Y: i32 = 6;
const MIN_WIDTH: i32 = 210;

#[repr(i32)]
enum AccentState {
    #[allow(dead_code)]
    Disabled = 0,
    AcrylicBlurBehind = 4,
}

// Undocumented DWM composition structures, mirrored from Twinkle Tray's
// src/modules/acrylic/acrylic.cc.
#[repr(C)]
struct AccentPolicy {
    accent_state: AccentState,
    accent_flags: i32,
    gradient_color: i32,
    animation_id: i32,
}

#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: i32,
    data: *mut std::ffi::c_void,
    size: u32,
}

const WCA_ACCENT_POLICY: i32 = 19;

/// Applies the Mica material. Windows 11 22000+ only; returns false elsewhere
/// so the caller can fall back to acrylic blur.
fn apply_mica(hwnd: HWND, dark: bool) -> bool {
    unsafe {
        // The frame must cover the client area for the material to show here.
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        if DwmExtendFrameIntoClientArea(hwnd, &margins) != 0 {
            return false;
        }
        let backdrop: u32 = DWMSBT_MAINWINDOW;
        if DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        ) != 0
        {
            return false;
        }
        // Mica itself is theme-aware; the dark flag only tints our own text.
        let _ = dark;
        true
    }
}

fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    r | (g << 8) | (b << 16)
}

fn encode(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// One row of the menu. Separator rows ignore the other fields.
pub struct MenuEntry {
    pub label: String,
    pub cmd: usize,
    pub checked: bool,
    pub separator: bool,
    /// Duration-style row: draws a radio bullet instead of a tick.
    pub radio: bool,
    /// Status row: grayed text, never selectable.
    pub header: bool,
}

impl MenuEntry {
    pub fn item(label: &str, cmd: usize, checked: bool) -> MenuEntry {
        MenuEntry {
            label: label.to_string(),
            cmd,
            checked,
            separator: false,
            radio: false,
            header: false,
        }
    }

    pub fn radio_item(label: &str, cmd: usize, checked: bool) -> MenuEntry {
        MenuEntry {
            label: label.to_string(),
            cmd,
            checked,
            separator: false,
            radio: true,
            header: false,
        }
    }

    pub fn header(label: &str) -> MenuEntry {
        MenuEntry {
            label: label.to_string(),
            cmd: 0,
            checked: false,
            separator: false,
            radio: false,
            header: true,
        }
    }

    /// Rows the user cannot pick: separators and the status header.
    fn selectable(&self) -> bool {
        !self.separator && !self.header
    }
}

/// Windows 11 shell palette; `accent` tints the checked marks.
struct MenuColors {
    background: COLORREF,
    text: COLORREF,
    hover: COLORREF,
    separator_line: COLORREF,
    gray: COLORREF,
    accent: COLORREF,
}

impl MenuColors {
    fn new(dark: bool, accent: COLORREF) -> Self {
        if dark {
            MenuColors {
                background: rgb(32, 32, 32),
                text: rgb(255, 255, 255),
                hover: rgb(69, 69, 69),
                separator_line: rgb(94, 94, 94),
                gray: rgb(120, 120, 120),
                accent,
            }
        } else {
            MenuColors {
                background: rgb(249, 249, 249),
                text: rgb(26, 26, 26),
                hover: rgb(229, 229, 229),
                separator_line: rgb(199, 199, 199),
                gray: rgb(120, 120, 120),
                accent,
            }
        }
    }
}

struct MenuWindow {
    hwnd: HWND,
    parent: HWND,
    entries: Vec<MenuEntry>,
    item_rects: Vec<RECT>,
    font: HFONT,
    /// Second font for ✓ ○ ● marks; Segoe UI lacks them.
    mark_font: HFONT,
    /// Index into `entries` of the highlighted row, if any.
    highlight: Option<usize>,
    /// Slide animation progress, 0.0 ..= 1.0.
    progress: f32,
    dark: bool,
    scale: f32,
    /// Final client size in px: (width, height).
    final_size: (i32, i32),
    /// Accent tint for marks, from the DWM AccentColor registry value.
    accent: COLORREF,
    /// True when the Windows 11 Mica material sits behind the client area.
    mica: bool,
}

impl MenuWindow {
    fn colors(&self) -> MenuColors {
        MenuColors::new(self.dark, self.accent)
    }

    /// Accent tint for checked marks, from the DWM AccentColor value.
    #[allow(dead_code)]
    fn accent(&self) -> COLORREF {
        self.accent
    }

    /// Row under the given client-space point, or None. Both axes matter:
    /// rows span the full width, and the point must fall inside one.
    fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        self.item_rects
            .iter()
            .position(|rect| rect.left <= x && x < rect.right && rect.top <= y && y < rect.bottom)
            .filter(|&index| self.entries[index].selectable())
    }

    /// Send the selected command to the parent window and close.
    unsafe fn select(&mut self, index: usize) {
        unsafe {
            let cmd = self.entries[index].cmd;
            PostMessageW(self.parent, WM_COMMAND, cmd as WPARAM, 0);
            PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
        }
    }

    /// One animation frame: grow the window upward from the anchor point and
    /// ramp the layered alpha with easeOutQuad easing.
    unsafe fn animate(&mut self) {
        unsafe {
            let (width, height) = self.final_size;
            self.progress =
                (self.progress + ANIM_FRAME_MS as f32 / ANIM_DURATION_MS as f32).min(1.0);
            let t = self.progress;
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            let current_height = ((height as f32 * eased).ceil() as i32).clamp(1, height);

            // The bottom edge stays fixed at the final anchor so the menu appears
            // to slide upward out of the tray icon.
            let mut current = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetWindowRect(self.hwnd, &mut current);
            let bottom = current.bottom;
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                current.left,
                bottom - current_height,
                width,
                current_height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );

            if self.progress >= 1.0 {
                KillTimer(self.hwnd, WM_TIMER_ANIM);
            }
        }
    }
}

fn mulround(px: i32, scale: f32) -> i32 {
    ((px as f32) * scale).round() as i32
}

static mut MENU_WINDOW: *mut MenuWindow = null_mut();

/// True while the nested modal loop should keep pumping.
static mut MENU_OPEN: bool = false;

unsafe fn center_text(hdc: HDC, text: &[u16], rect: &RECT) {
    unsafe {
        let mut target = *rect;
        DrawTextW(
            hdc,
            text.as_ptr(),
            text.len() as i32,
            &mut target,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER,
        );
    }
}

unsafe extern "system" fn menu_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let ptr = MENU_WINDOW;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let menu = &mut *ptr;

        match msg {
            WM_PAINT => {
                let (bg, fg, hover, line_color) = {
                    let c = menu.colors();
                    (c.background, c.text, c.hover, c.separator_line)
                };
                let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
                let paint_hdc = BeginPaint(hwnd, &mut ps);
                let mut client = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                GetClientRect(hwnd, &mut client);

                // Double-buffer: every hover change repaints the whole window,
                // and painting a layered acrylic surface directly flickers.
                let width = client.right - client.left;
                let height = client.bottom - client.top;
                let mem_dc = CreateCompatibleDC(paint_hdc);
                let mem_bitmap = CreateCompatibleBitmap(paint_hdc, width, height);
                let old_bitmap = SelectObject(mem_dc, mem_bitmap);
                let hdc = mem_dc;
                if !menu.mica {
                    // Opaque base for acrylic fallback; under Mica the material
                    // itself is the background, so an opaque fill would hide it.
                    let background = CreateSolidBrush(bg);
                    FillRect(hdc, &client, background);
                    DeleteObject(background);
                }

                if let Some(index) = menu.highlight {
                    let mut rect = menu.item_rects[index];
                    // Inset so the highlight is an inset pill, like native menus.
                    rect.left += mulround(4, menu.scale);
                    rect.right -= mulround(4, menu.scale);
                    rect.top += mulround(2, menu.scale);
                    rect.bottom -= mulround(2, menu.scale);
                    let brush = CreateSolidBrush(hover);
                    FillRect(hdc, &rect, brush);
                    DeleteObject(brush);
                }

                SetBkMode(hdc, TRANSPARENT as i32);
                SelectObject(hdc, menu.font);

                for (index, entry) in menu.entries.iter().enumerate() {
                    let rect = menu.item_rects[index];
                    if entry.separator {
                        let line_y = (rect.top + rect.bottom) / 2;
                        let line_rect = RECT {
                            left: rect.left + mulround(ITEM_PADDING_X, menu.scale),
                            top: line_y,
                            right: rect.right - mulround(ITEM_PADDING_X, menu.scale),
                            bottom: line_y + 1,
                        };
                        let brush: HBRUSH = CreateSolidBrush(line_color);
                        FillRect(hdc, &line_rect, brush);
                        DeleteObject(brush);
                        continue;
                    }

                    let text_rect = RECT {
                        left: rect.left + mulround(CHECKBOX_WIDTH, menu.scale),
                        top: rect.top,
                        right: rect.right - mulround(ITEM_PADDING_X, menu.scale),
                        bottom: rect.bottom,
                    };

                    if entry.header {
                        // Status header: grayed, no mark column used.
                        SetTextColor(hdc, menu.colors().gray);
                        let label: Vec<u16> = entry.label.encode_utf16().collect();
                        let mut target = text_rect;
                        DrawTextW(
                            hdc,
                            label.as_ptr(),
                            label.len() as i32,
                            &mut target,
                            DT_SINGLELINE | DT_VCENTER,
                        );
                        continue;
                    }

                    let mark = if entry.radio {
                        if entry.checked {
                            "\u{25CF}"
                        } else {
                            "\u{25CB}"
                        }
                    } else if entry.checked {
                        "\u{2713}"
                    } else {
                        ""
                    };
                    if !mark.is_empty() {
                        let mark_color = if entry.radio && entry.checked {
                            menu.colors().accent
                        } else {
                            fg
                        };
                        SetTextColor(hdc, mark_color);
                        let glyph: Vec<u16> = mark.encode_utf16().collect();
                        let mut glyph_rect = rect;
                        glyph_rect.right = text_rect.left;
                        SelectObject(hdc, menu.mark_font);
                        center_text(hdc, &glyph, &glyph_rect);
                        SelectObject(hdc, menu.font);
                    }

                    SetTextColor(hdc, fg);
                    let label: Vec<u16> = entry.label.encode_utf16().collect();
                    let mut target = text_rect;
                    DrawTextW(
                        hdc,
                        label.as_ptr(),
                        label.len() as i32,
                        &mut target,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                    );
                }

                // Blit the finished frame in one shot, then release the buffer.
                // The old bitmap goes back only after the copy: selecting it
                // first would point mem_dc at a default 1x1 bitmap.
                BitBlt(paint_hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);
                SelectObject(mem_dc, old_bitmap);
                DeleteObject(mem_bitmap);
                DeleteDC(mem_dc);

                EndPaint(hwnd, &ps);
                0
            }
            WM_ERASEBKGND => 1,
            WM_MOUSEMOVE => {
                let x = (lparam & 0xffff) as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as i16 as i32;
                // Re-arm leave tracking so hover clears when the cursor exits.
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
                if x > 0 || y > 0 {
                    let hit = menu.hit_test(x, y);
                    if hit != menu.highlight {
                        menu.highlight = hit;
                        InvalidateRect(hwnd, null_mut(), 0);
                    }
                }
                0
            }
            // Cursor left the menu: drop the hover highlight.
            0x02A3 => {
                if menu.highlight.is_some() {
                    menu.highlight = None;
                    InvalidateRect(hwnd, null_mut(), 0);
                }
                0
            }
            WM_LBUTTONUP => {
                let x = (lparam & 0xffff) as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as i16 as i32;
                if let Some(index) = menu.hit_test(x, y) {
                    menu.select(index);
                }
                0
            }
            WM_KEYDOWN => match wparam {
                0x1B => {
                    // Escape
                    PostMessageW(menu.hwnd, WM_CLOSE, 0, 0);
                    0
                }
                0x0D => {
                    // Enter
                    let _ = wparam;
                    let _ = lparam;
                    if let Some(index) = menu.highlight {
                        menu.select(index);
                    } else {
                        PostMessageW(menu.hwnd, WM_CLOSE, 0, 0);
                    }
                    0
                }
                0x26 => {
                    // Up arrow
                    let _ = lparam;
                    let mut index = menu.highlight.unwrap_or(menu.entries.len());
                    while index > 0 {
                        index -= 1;
                        if !menu.entries[index].separator {
                            menu.highlight = Some(index);
                            InvalidateRect(hwnd, null_mut(), 0);
                            break;
                        }
                    }
                    0
                }
                0x28 => {
                    // Down arrow
                    let _ = lparam;
                    let start = menu.highlight.map_or(0, |index| index + 1);
                    for index in start..menu.entries.len() {
                        if !menu.entries[index].separator {
                            menu.highlight = Some(index);
                            InvalidateRect(hwnd, null_mut(), 0);
                            break;
                        }
                    }
                    0
                }
                _ => 0,
            },
            WM_KEYUP => 0,
            WM_TIMER => {
                if wparam == WM_TIMER_ANIM {
                    menu.animate();
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_NCDESTROY => {
                // Notifies the modal loop in show_tray_menu to finish.
                MENU_OPEN = false;
                KillTimer(hwnd, WM_TIMER_ANIM);
                0
            }
            WM_ACTIVATE => {
                // User clicked elsewhere: dismiss like a native menu would.
                if (wparam & 0xffff) == WA_INACTIVE as usize {
                    PostMessageW(menu.hwnd, WM_CLOSE, 0, 0);
                }
                0
            }
            WM_KILLFOCUS => {
                PostMessageW(menu.hwnd, WM_CLOSE, 0, 0);
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_SETCURSOR => {
                let cursor: HCURSOR = LoadCursorW(null_mut(), IDC_ARROW);
                SetCursor(cursor);
                1
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Applies the Twinkle Tray acrylic backdrop to the menu window.
fn apply_acrylic(hwnd: HWND, dark: bool) {
    unsafe {
        let user32 = GetModuleHandleW(encode("user32.dll").as_ptr());
        if user32.is_null() {
            return;
        }
        let Some(proc) = GetProcAddress(user32, b"SetWindowCompositionAttribute\0".as_ptr()) else {
            return;
        };
        type SetWindowCompositionAttributeFn =
            unsafe extern "system" fn(HWND, *const WindowCompositionAttributeData) -> i32;
        let set_composition: SetWindowCompositionAttributeFn = std::mem::transmute(proc);

        let (r, g, b) = if dark { (41, 41, 41) } else { (219, 219, 219) };
        let policy = AccentPolicy {
            accent_state: AccentState::AcrylicBlurBehind,
            accent_flags: 2,
            gradient_color: ((0xD0u32 << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32)
                as i32,
            animation_id: 0,
        };
        let data = WindowCompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            data: &policy as *const AccentPolicy as *mut std::ffi::c_void,
            size: std::mem::size_of::<AccentPolicy>() as u32,
        };
        set_composition(hwnd, &data);
    }
}

/// Menu position: above the icon when it fits, otherwise below, clamped to
/// the monitor work area so multi-monitor setups never clip it.
fn menu_position(anchor: RECT, work: RECT, width: i32, height: i32) -> (i32, i32) {
    let max_x = (work.right - width).max(work.left);
    let x = (anchor.right - width).clamp(work.left, max_x);
    let above = anchor.top - height;
    let below = anchor.bottom;
    let max_y = (work.bottom - height).max(work.top);
    let y = if above >= work.top {
        above
    } else if below + height <= work.bottom {
        below
    } else {
        above.clamp(work.top, max_y)
    };
    (x, y)
}

/// Shows the popup menu anchored to the top-right of `anchor` (the tray icon
/// rect) and blocks until the menu is dismissed or an item is chosen.
/// `dark` follows the system theme; `accent` tints checked marks; commands
/// arrive at `parent` via WM_COMMAND.
pub fn show_tray_menu(
    parent: HWND,
    entries: Vec<MenuEntry>,
    anchor: RECT,
    dark: bool,
    accent: COLORREF,
) {
    unsafe {
        if MENU_OPEN {
            return;
        }
        MENU_OPEN = true;

        let instance = GetModuleHandleW(null_mut());
        let class_name = encode("CaffeineMenuWindow");
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(menu_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
        };
        // Re-registration of an identical class fails harmlessly; safe to call
        // every time the menu opens.
        RegisterClassW(&class);

        // Created hidden at zero size; the animation grows it into place.
        // Not layered: layered windows bypass the DWM backdrop, so Mica and
        // acrylic would not render.
        let hwnd = CreateWindowExW(
            (WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as u32,
            class_name.as_ptr(),
            encode("Caffeine Menu").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            MENU_OPEN = false;
            return;
        }

        // Windows 11 rounded corners, matching native tray menus.
        let preference: i32 = DWMWCP_ROUND as i32;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );

        // Keep the window frame dark in dark mode; the shell only themes
        // Explorer's own menus, not ours.
        if dark {
            let value: i32 = 1;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                &value as *const i32 as *const std::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );
        }

        // Mica on Windows 11; acrylic blur on anything older.
        let mica = apply_mica(hwnd, dark);
        if !mica {
            apply_acrylic(hwnd, dark);
        }

        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let make_font = |face: &str| -> HFONT {
            let height = -mulround(15, scale);
            let f = CreateFontW(
                height,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                encode(face).as_ptr(),
            );
            if !f.is_null() {
                return f;
            }
            CreateFontW(
                -15,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                encode("Segoe UI").as_ptr(),
            )
        };
        let font = make_font("Segoe UI");
        let mark_font = make_font("Segoe UI Symbol");

        let mut menu = Box::new(MenuWindow {
            hwnd,
            parent,
            entries,
            item_rects: Vec::new(),
            font,
            mark_font,
            highlight: None,
            progress: 0.0,
            dark,
            scale,
            final_size: (0, 0),
            accent,
            mica,
        });

        // Width from the longest label.
        {
            let hdc = CreateCompatibleDC(null_mut());
            let old_font = SelectObject(hdc, menu.font);
            let mut width = mulround(MIN_WIDTH, menu.scale);
            for entry in menu.entries.iter() {
                if entry.separator {
                    continue;
                }
                let text: Vec<u16> = entry.label.encode_utf16().collect();
                let mut size = SIZE { cx: 0, cy: 0 };
                if GetTextExtentPoint32W(hdc, text.as_ptr(), text.len() as i32, &mut size) != 0 {
                    let needed =
                        size.cx + mulround(CHECKBOX_WIDTH + ITEM_PADDING_X * 2, menu.scale);
                    width = width.max(needed);
                }
            }
            SelectObject(hdc, old_font);
            DeleteDC(hdc);
            menu.final_size.0 = width;
        }

        // Lay out rows top to bottom.
        let mut y = mulround(MENU_PADDING_Y, menu.scale);
        let item_height = mulround(ITEM_HEIGHT, menu.scale);
        let separator_height = mulround(SEPARATOR_HEIGHT, menu.scale);
        for entry in menu.entries.iter() {
            let height = if entry.separator {
                separator_height
            } else {
                item_height
            };
            menu.item_rects.push(RECT {
                left: 0,
                top: y,
                right: menu.final_size.0,
                bottom: y + height,
            });
            y += height;
        }
        let height = y + mulround(MENU_PADDING_Y, menu.scale);
        menu.final_size.1 = height;

        // The menu's bottom edge lines up with the top of the tray icon and
        // the right edges are aligned ΓÇö the native menu placement.
        let width = menu.final_size.0;

        // Clamp to the monitor's work area so multi-monitor setups never clip
        // the menu; falls back to a huge rect when the lookup fails.
        let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: anchor,
            rcWork: anchor,
            dwFlags: 0,
        };
        let work = if !monitor.is_null() && GetMonitorInfoW(monitor, &mut monitor_info) != 0 {
            monitor_info.rcWork
        } else {
            RECT {
                left: i32::MIN / 2,
                top: i32::MIN / 2,
                right: i32::MAX / 2,
                bottom: i32::MAX / 2,
            }
        };
        let (x, y) = menu_position(anchor, work, width, height);
        MENU_WINDOW = Box::into_raw(menu);

        SetWindowPos(hwnd, HWND_TOPMOST, x, y + height, width, 0, SWP_NOACTIVATE);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        SetTimer(hwnd, WM_TIMER_ANIM, ANIM_FRAME_MS, None);

        // Nested modal loop: pump until the menu window is destroyed. If
        // WM_QUIT arrives here, hand it back to the outer loop in main().
        loop {
            let mut msg = std::mem::zeroed::<MSG>();
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                if result == 0 {
                    PostQuitMessage(0);
                }
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !MENU_OPEN {
                break;
            }
        }

        // Clean up the heap MenuWindow, its fonts, and reset the menu class
        // brush so the next open starts clean.
        let ptr = MENU_WINDOW;
        MENU_WINDOW = null_mut();
        if !ptr.is_null() {
            let menu = &mut *ptr;
            DeleteObject(menu.font);
            if !menu.mark_font.is_null() {
                DeleteObject(menu.mark_font);
            }
            drop(Box::from_raw(ptr));
        }
        MENU_OPEN = false;
    }
}
