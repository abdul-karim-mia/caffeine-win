//! Custom system-tray popup menu.
//!
//! Replaces the classic `TrackPopupMenu` HMENU with an owner-drawn popup
//! window so we can apply an acrylic blur (`SetWindowCompositionAttribute`,
//! undocumented, same technique Twinkle Tray uses in `acrylic.cc`) and a
//! slide-up animation. Commands are still dispatched to the parent window as
//! `WM_COMMAND`, keeping the existing command IDs.

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, FillRect, GetTextExtentPoint32W, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_NORMAL, HBRUSH, HDC, HFONT,
    PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetWindowRect, KillTimer, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassW, SetCursor, SetForegroundWindow,
    SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, HCURSOR,
    HWND_TOPMOST, IDC_ARROW, LWA_ALPHA, MSG, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    WA_INACTIVE, WM_ACTIVATE, WM_CLOSE, WM_COMMAND, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP,
    WM_KILLFOCUS, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR,
    WM_TIMER,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WNDCLASSW,
};

const WM_TIMER_ANIM: usize = 3101;

const ANIM_DURATION_MS: u32 = 200;
const ANIM_FRAME_MS: u32 = 15;
// Opacity fade finishes over the first fraction of the slide, so the acrylic
// backdrop reads correctly before the window reaches full height.
const FADE_FRACTION: f32 = 0.35;
// Whole-window alpha; keeps the acrylic backdrop slightly visible through the
// content (Twinkle Tray uses #D0/#DD tints with a similar ramp).
const FINAL_ALPHA: u32 = 214;

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
}

impl MenuEntry {
    pub fn item(label: &str, cmd: usize, checked: bool) -> MenuEntry {
        MenuEntry { label: label.to_string(), cmd, checked, separator: false }
    }

    pub fn separator() -> MenuEntry {
        MenuEntry { label: String::new(), cmd: 0, checked: false, separator: true }
    }
}

struct MenuWindow {
    hwnd: HWND,
    parent: HWND,
    entries: Vec<MenuEntry>,
    item_rects: Vec<RECT>,
    font: HFONT,
    /// Index into `entries` of the highlighted row, if any.
    highlight: Option<usize>,
    /// Slide animation progress, 0.0 ..= 1.0.
    progress: f32,
    dark: bool,
    scale: f32,
    /// Final client size in px: (width, height).
    final_size: (i32, i32),
}

impl MenuWindow {
    fn colors(&self) -> (COLORREF, COLORREF, COLORREF, COLORREF) {
        // (background, text, hover highlight, separator line)
        if self.dark {
            (rgb(32, 32, 32), rgb(240, 240, 240), rgb(58, 58, 58), rgb(94, 94, 94))
        } else {
            (rgb(243, 243, 243), rgb(17, 17, 17), rgb(229, 229, 229), rgb(199, 199, 199))
        }
    }

    /// Row under the given client-space point, or None.
    fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 {
            return None;
        }
        for (index, rect) in self.item_rects.iter().enumerate() {
            if rect.top <= y && y < rect.bottom {
                if self.entries[index].separator {
                    return None;
                }
                return Some(index);
            }
        }
        None
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
            self.progress = (self.progress + ANIM_FRAME_MS as f32 / ANIM_DURATION_MS as f32).min(1.0);
            let t = self.progress;
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            let current_height = ((height as f32 * eased).ceil() as i32).clamp(1, height);

            // The bottom edge stays fixed at the final anchor so the menu appears
            // to slide upward out of the tray icon.
            let mut current = RECT { left: 0, top: 0, right: 0, bottom: 0 };
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
    (px as f32 * scale).round() as i32
}

static mut MENU_WINDOW: *mut MenuWindow = null_mut();

/// True while the nested modal loop should keep pumping.
static mut MENU_OPEN: bool = false;

unsafe fn center_text(hdc: HDC, text: &[u16], rect: &RECT) {
    unsafe {
        let mut target = *rect;
        DrawTextW(hdc, text.as_ptr(), text.len() as i32, &mut target, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
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
                let (bg, fg, hover, line_color) = menu.colors();
                let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd, &mut client);
                let background = CreateSolidBrush(bg);
                FillRect(hdc, &client, background);
                DeleteObject(background);

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
                SetTextColor(hdc, fg);
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
                    if entry.checked {
                        let glyph: Vec<u16> = "\u{2713}".encode_utf16().collect();
                        let mut glyph_rect = rect;
                        glyph_rect.right = text_rect.left;
                        center_text(hdc, &glyph, &glyph_rect);
                    }
                    let label: Vec<u16> = entry.label.encode_utf16().collect();
                    center_text(hdc, &label, &text_rect);
                }

                EndPaint(hwnd, &ps);
                0
            }
            WM_ERASEBKGND => 1,
            WM_MOUSEMOVE => {
                let x = (lparam & 0xffff) as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as i16 as i32;
                if x > 0 || y > 0 {
                    let hit = menu.hit_test(x, y);
                    if hit != menu.highlight {
                        menu.highlight = hit;
                        InvalidateRect(hwnd, null_mut(), 0);
                    }
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
                0x1B => { // Escape
                    PostMessageW(menu.hwnd, WM_CLOSE, 0, 0);
                    0
                }
                0x0D => { // Enter
                    let _ = wparam;
                    let _ = lparam;
                    if let Some(index) = menu.highlight {
                        menu.select(index);
                    } else {
                        PostMessageW(menu.hwnd, WM_CLOSE, 0, 0);
                    }
                    0
                }
                0x26 => { // Up arrow
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
                0x28 => { // Down arrow
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
            gradient_color: ((0xD0u32 << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32) as i32,
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

/// Shows the popup menu anchored to the top-right of `anchor` (the tray icon
/// rect) and blocks until the menu is dismissed or an item is chosen.
/// `dark` follows the system theme; commands arrive at `parent` via WM_COMMAND.
pub fn show_tray_menu(parent: HWND, entries: Vec<MenuEntry>, anchor: RECT, dark: bool) {
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

        apply_acrylic(hwnd, dark);

        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let mut font = CreateFontW(
            -mulround(15, scale),
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
        );
        if font.is_null() {
            font = CreateFontW(
                -15, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, 0, 0, 0, 0, 0, encode("Segoe UI").as_ptr(),
            );
        }

        let mut menu = Box::new(MenuWindow {
            hwnd,
            parent,
            entries,
            item_rects: Vec::new(),
            font,
            highlight: None,
            progress: 0.0,
            dark,
            scale,
            final_size: (0, 0),
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
            let height = if entry.separator { separator_height } else { item_height };
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
        // the right edges are aligned — the native menu placement.
        let width = menu.final_size.0;
        let x = anchor.right - width;
        let y = anchor.bottom - height;

        // Start fully invisible; the first animation frame resets the alpha.
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

        // Clean up the heap MenuWindow and its font.
        let ptr = MENU_WINDOW;
        MENU_WINDOW = null_mut();
        if !ptr.is_null() {
            let menu = &mut *ptr;
            DeleteObject(menu.font);
            drop(Box::from_raw(ptr));
        }
        MENU_OPEN = false;
    }
}
