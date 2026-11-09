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
    GetWindowRect, HCURSOR, HWND_TOPMOST, IDC_ARROW, KillTimer, LWA_ALPHA, LoadCursorW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetCursor, SetForegroundWindow, SetLayeredWindowAttributes, SetTimer,
    SetWindowPos, ShowWindow, TranslateMessage, WA_INACTIVE, WM_ACTIVATE, WM_CLOSE, WM_COMMAND,
    WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY,
    WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

const WM_TIMER_ANIM: usize = 3101;

const ANIM_DURATION_MS: u32 = 200;
const ANIM_FRAME_MS: u32 = 15;
// The fade finishes over this fraction of the slide, so the backdrop reads
// correctly before the window reaches full height.
const FADE_FRACTION: f32 = 0.35;
// Whole-window alpha. Layered windows are the only way DWM's blurred backdrop
// shows through the client area: without it the backdrop sits behind an opaque
// window and is invisible. 214 keeps rows readable while the desktop reads
// through, the same balance as a Windows context menu.
const FINAL_ALPHA: u8 = 214;
// Hover pill: it eases between rows instead of snapping, so the highlight
// slides when the cursor crosses rows.
const WM_TIMER_PILL: usize = 3102;
const PILL_FRAME_MS: u32 = 16;
const PILL_EASE: f32 = 0.4;
const PILL_INSET_X: i32 = 4;
const PILL_INSET_Y: i32 = 1;

/// Mica: the documented Windows 11 backdrop material, applied through
/// DwmSetWindowAttribute and themed by the system (light/dark + accent).
/// Transient is the variant Explorer uses for context menus and popups: it is
/// more transparent than the main-window material.
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMSBT_TRANSIENTWINDOW: u32 = 4;

const ITEM_PADDING_X: i32 = 12;
const CHECKBOX_WIDTH: i32 = 18;
const ICON_SIZE: i32 = 18;
const COLUMN_GAP: i32 = 8;
const ITEM_HEIGHT: i32 = 30;
const SEPARATOR_HEIGHT: i32 = 9;
const MENU_PADDING_Y: i32 = 5;
const MIN_WIDTH: i32 = 190;

/// Segoe Fluent Icons code points (the font Explorer uses for its menus).
pub const ICON_POWER: u16 = 0xE7E8;
pub const ICON_CLOCK: u16 = 0xE121;
pub const ICON_REFRESH: u16 = 0xE72C;
pub const ICON_HOME: u16 = 0xE80F;
pub const ICON_EXIT: u16 = 0xE711;
pub const ICON_MORE: u16 = 0xE712;

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
        let backdrop: u32 = DWMSBT_TRANSIENTWINDOW;
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
    /// Segoe Fluent Icons code point for the row icon, 0 for none.
    pub icon: u16,
}

impl MenuEntry {
    fn base(label: &str, icon: u16) -> MenuEntry {
        MenuEntry {
            label: label.to_string(),
            cmd: 0,
            checked: false,
            separator: false,
            radio: false,
            header: false,
            icon,
        }
    }

    pub fn item(label: &str, cmd: usize, checked: bool) -> MenuEntry {
        MenuEntry {
            cmd,
            checked,
            ..MenuEntry::base(label, 0)
        }
    }

    pub fn icon_item(label: &str, cmd: usize, checked: bool, icon: u16) -> MenuEntry {
        MenuEntry {
            cmd,
            checked,
            ..MenuEntry::base(label, icon)
        }
    }

    pub fn radio_item(label: &str, cmd: usize, checked: bool, icon: u16) -> MenuEntry {
        MenuEntry {
            cmd,
            checked,
            radio: true,
            ..MenuEntry::base(label, icon)
        }
    }

    pub fn header(label: &str, icon: u16) -> MenuEntry {
        MenuEntry {
            header: true,
            ..MenuEntry::base(label, icon)
        }
    }

    pub fn separator() -> MenuEntry {
        MenuEntry {
            separator: true,
            ..MenuEntry::base("", 0)
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
    /// Label font: Segoe UI Variable Text, like Explorer's menus.
    font: HFONT,
    /// Icon font for the left icon column: Segoe Fluent Icons.
    icon_font: HFONT,
    /// Mark font for ✓ ○ ●; Segoe UI lacks those.
    mark_font: HFONT,
    /// Index into `entries` of the highlighted row, if any.
    highlight: Option<usize>,
    /// Hover pill: its current rect and 0..255 opacity. Eases toward the
    /// highlighted row each frame, so the highlight moves between rows.
    pill: RECT,
    pill_alpha: f32,
    /// Slide animation progress, 0.0 ..= 1.0.
    progress: f32,
    dark: bool,
    scale: f32,
    /// Final client size in px: (width, height).
    final_size: (i32, i32),
    /// Accent tint for marks, from the DWM AccentColor registry value.
    accent: COLORREF,
    /// True when a DWM backdrop material (acrylic or Mica) sits behind the
    /// client area, so the background must not be painted opaque.
    translucent: bool,
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

    /// Row rect inset the way the hover pill is drawn.
    fn pill_rect(&self, index: Option<usize>) -> Option<RECT> {
        let index = index?;
        let rect = *self.item_rects.get(index)?;
        Some(RECT {
            left: rect.left + mulround(PILL_INSET_X, self.scale),
            right: rect.right - mulround(PILL_INSET_X, self.scale),
            top: rect.top + mulround(PILL_INSET_Y, self.scale),
            bottom: rect.bottom - mulround(PILL_INSET_Y, self.scale),
        })
    }

    /// One frame of the hover animation: ease the pill toward the highlighted
    /// row and its opacity toward visible or hidden. Returns true while the
    /// animation still needs frames.
    fn animate_pill(&mut self) -> bool {
        let target_alpha = if self.highlight.is_some() { 255.0 } else { 0.0 };
        self.pill_alpha += (target_alpha - self.pill_alpha) * PILL_EASE;

        if let Some(target) = self.pill_rect(self.highlight) {
            let lerp = |from: i32, to: i32| from + ((to - from) as f32 * PILL_EASE) as i32;
            self.pill.left = lerp(self.pill.left, target.left);
            self.pill.right = lerp(self.pill.right, target.right);
            self.pill.top = lerp(self.pill.top, target.top);
            self.pill.bottom = lerp(self.pill.bottom, target.bottom);
        }

        if self.highlight.is_none() && self.pill_alpha < 1.0 {
            self.pill_alpha = 0.0;
            return false;
        }
        self.pill_alpha > 0.01 || self.highlight.is_some()
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

            let fade = (t / FADE_FRACTION).min(1.0);
            let alpha = (FINAL_ALPHA as f32 * fade) as u8;
            SetLayeredWindowAttributes(self.hwnd, 0, alpha, LWA_ALPHA);

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

/// Draws one Segoe Fluent Icons code point centered in the cell.
unsafe fn draw_menu_icon(hdc: HDC, menu: &MenuWindow, icon: u16, rect: RECT, color: COLORREF) {
    unsafe {
        if menu.icon_font.is_null() || icon == 0 {
            return;
        }
        SetTextColor(hdc, color);
        let old_font = SelectObject(hdc, menu.icon_font);
        center_text(hdc, &[icon], &rect);
        SelectObject(hdc, old_font);
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
                if !menu.translucent {
                    // Opaque base for acrylic fallback; under Mica the material
                    // itself is the background, so an opaque fill would hide it.
                    let background = CreateSolidBrush(bg);
                    FillRect(hdc, &client, background);
                    DeleteObject(background);
                }

                // Hover pill: blended between the background and the hover
                // color by its animated opacity, so it eases in, moves, and
                // fades out instead of jumping.
                if menu.pill_alpha > 0.01 {
                    let mix = |base: u32, target: u32| -> COLORREF {
                        let t = (menu.pill_alpha / 255.0).clamp(0.0, 1.0);
                        let channel = |shift: u32| {
                            let b = (base >> shift) & 0xff;
                            let h = (target >> shift) & 0xff;
                            (b as f32 + ((h as f32 - b as f32) * t)) as u32
                        };
                        channel(0) | (channel(8) << 8) | (channel(16) << 16)
                    };
                    let brush = CreateSolidBrush(mix(bg, hover));
                    FillRect(hdc, &menu.pill, brush);
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

                    // Columns: mark, icon, label — Explorer's own spacing.
                    let text_rect = RECT {
                        left: rect.left
                            + mulround(
                                ITEM_PADDING_X
                                    + CHECKBOX_WIDTH
                                    + COLUMN_GAP
                                    + ICON_SIZE
                                    + COLUMN_GAP,
                                menu.scale,
                            ),
                        top: rect.top,
                        right: rect.right - mulround(ITEM_PADDING_X, menu.scale),
                        bottom: rect.bottom,
                    };

                    let icon_rect = RECT {
                        left: rect.left
                            + mulround(ITEM_PADDING_X + CHECKBOX_WIDTH + COLUMN_GAP, menu.scale),
                        top: rect.top,
                        right: rect.left
                            + mulround(
                                ITEM_PADDING_X + CHECKBOX_WIDTH + COLUMN_GAP + ICON_SIZE,
                                menu.scale,
                            ),
                        bottom: rect.bottom,
                    };

                    if entry.header {
                        // Status header: grayed, no mark column used.
                        SetTextColor(hdc, menu.colors().gray);
                        if entry.icon != 0 {
                            draw_menu_icon(hdc, menu, entry.icon, icon_rect, menu.colors().gray);
                        }
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
                        glyph_rect.right = text_rect.left
                            - mulround(COLUMN_GAP + ICON_SIZE + COLUMN_GAP, menu.scale);
                        SelectObject(hdc, menu.mark_font);
                        center_text(hdc, &glyph, &glyph_rect);
                        SelectObject(hdc, menu.font);
                    }

                    if entry.icon != 0 {
                        draw_menu_icon(hdc, menu, entry.icon, icon_rect, fg);
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
                        // Animate from the old position when the pill is
                        // already visible, so it slides to the new row.
                        if menu.pill_alpha < 1.0 {
                            if let Some(rect) = menu.pill_rect(hit) {
                                menu.pill = rect;
                            }
                        }
                        menu.highlight = hit;
                        SetTimer(hwnd, WM_TIMER_PILL, PILL_FRAME_MS, None);
                        InvalidateRect(hwnd, null_mut(), 0);
                    }
                }
                0
            }
            // Cursor left the menu: drop the hover highlight.
            0x02A3 => {
                if menu.highlight.is_some() {
                    menu.highlight = None;
                    SetTimer(hwnd, WM_TIMER_PILL, PILL_FRAME_MS, None);
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
                } else if wparam == WM_TIMER_PILL {
                    if !menu.animate_pill() {
                        KillTimer(hwnd, WM_TIMER_PILL);
                    }
                    InvalidateRect(hwnd, null_mut(), 0);
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_NCDESTROY => {
                // Notifies the modal loop in show_tray_menu to finish.
                MENU_OPEN = false;
                KillTimer(hwnd, WM_TIMER_ANIM);
                KillTimer(hwnd, WM_TIMER_PILL);
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

/// Applies the Twinkle Tray acrylic backdrop to the menu window. Returns
/// Applies the acrylic backdrop. Returns false when the composition API is
/// missing so Mica can be tried instead.
fn apply_acrylic(hwnd: HWND, dark: bool) -> bool {
    unsafe {
        let user32 = GetModuleHandleW(encode("user32.dll").as_ptr());
        if user32.is_null() {
            return false;
        }
        let Some(proc) = GetProcAddress(user32, b"SetWindowCompositionAttribute\0".as_ptr()) else {
            return false;
        };
        type SetWindowCompositionAttributeFn =
            unsafe extern "system" fn(HWND, *const WindowCompositionAttributeData) -> i32;
        let set_composition: SetWindowCompositionAttributeFn = std::mem::transmute(proc);

        // Explorer-style tint: mostly opaque with the blurred desktop showing
        // through. Alpha 0xA0 keeps text readable and the desktop visible,
        // like the real context menu.
        let (r, g, b) = if dark { (41, 41, 41) } else { (219, 219, 219) };
        let policy = AccentPolicy {
            accent_state: AccentState::AcrylicBlurBehind,
            accent_flags: 2,
            gradient_color: ((0xA0u32 << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32)
                as i32,
            animation_id: 0,
        };
        let data = WindowCompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            data: &policy as *const AccentPolicy as *mut std::ffi::c_void,
            size: std::mem::size_of::<AccentPolicy>() as u32,
        };
        // Reports success as an HRESULT (S_OK is 0), the documented shape of
        // this undocumented API.
        set_composition(hwnd, &data) == 0
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
        // Layered so the DWM backdrop behind it is actually visible; see
        // FINAL_ALPHA.
        let hwnd = CreateWindowExW(
            (WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as u32,
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

        // The acrylic blur behind the window reproduces the translucency of
        // Explorer's own context menus, where the wallpaper shows through.
        // Mica is the fallback when the composition API is unavailable.
        // The acrylic blur behind the window reproduces the translucency of
        // Explorer's own context menus, where the wallpaper shows through.
        // Mica is the fallback when the composition API is unavailable.
        let acrylic = apply_acrylic(hwnd, dark);
        let mica = !acrylic && apply_mica(hwnd, dark);
        // Any backdrop material means the background must not be painted:
        // the material provides it.
        let translucent = acrylic || mica;

        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let make_font = |face: &str, height: i32| -> HFONT {
            let f = CreateFontW(
                -mulround(height, scale),
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
                -height,
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
        // Windows 11 menus use the Variable Text face at 14px and Segoe Fluent
        // Icons at 16px; both fall back to plain Segoe UI when missing.
        let font = make_font("Segoe UI Variable Text", 14);
        let icon_font = make_font("Segoe Fluent Icons", 16);
        let mark_font = make_font("Segoe UI Symbol", 14);

        let mut menu = Box::new(MenuWindow {
            hwnd,
            parent,
            entries,
            item_rects: Vec::new(),
            font,
            icon_font,
            mark_font,
            highlight: None,
            pill: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            pill_alpha: 0.0,
            progress: 0.0,
            dark,
            scale,
            final_size: (0, 0),
            accent,
            translucent,
        });
        // Width from the longest label.
        {
            let columns = mulround(
                ITEM_PADDING_X
                    + CHECKBOX_WIDTH
                    + COLUMN_GAP
                    + ICON_SIZE
                    + COLUMN_GAP
                    + ITEM_PADDING_X,
                menu.scale,
            );
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
                    width = width.max(size.cx + columns);
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

        // Start fully invisible; the animation ramps the alpha up to
        // FINAL_ALPHA so the blurred backdrop fades in with the slide.
        SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);

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
