#![windows_subsystem = "windows"]

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow,
    DispatchMessageW, GetClassNameW, GetCursorPos, GetMessageW, KillTimer, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetTimer,
    SetWindowsHookExW, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, HICON, HMENU,
    MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_RIGHTALIGN, TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP,
    WM_TIMER, WM_USER,
    WNDCLASSW,
};

use caffeine_win::icon::{create_tray_icon, destroy_tray_icon};
use caffeine_win::keepawake::set_keep_awake;
use caffeine_win::state::{AppState, TimerDuration};

const WM_TRAYICON: u32 = WM_USER + 1;
const TIMER_ID_SECOND: usize = 1001;
const CMD_AUTO_RESET: usize = 2010;

#[repr(C)]
struct LastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

type GetLastInputInfoFn = unsafe extern "system" fn(*mut LastInputInfo) -> i32;

/// Returns the tick count of the most recent keyboard or mouse input.
fn get_last_input_tick() -> Option<u32> {
    unsafe {
        let user32 = GetModuleHandleW(encode_wide("user32.dll").as_ptr());
        if user32.is_null() {
            return None;
        }
        let proc = GetProcAddress(user32, b"GetLastInputInfo\0".as_ptr())?;
        let get_last_input: GetLastInputInfoFn = std::mem::transmute(proc);
        let mut info = LastInputInfo {
            cb_size: std::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        if get_last_input(&mut info) != 0 {
            Some(info.dw_time)
        } else {
            None
        }
    }
}

const CMD_EXIT: usize = 2001;
const CMD_TOGGLE: usize = 2002;
const CMD_15M: usize = 2003;
const CMD_30M: usize = 2004;
const CMD_1H: usize = 2005;
const CMD_2H: usize = 2006;
const CMD_4H: usize = 2007;
const CMD_INFINITE: usize = 2008;
const CMD_AUTOSTART: usize = 2009;

const WH_CBT: i32 = 5;
const HCBT_CREATEWND: i32 = 3;
const PERSONALIZE_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

type AllowDarkModeForWindowFn = unsafe extern "system" fn(HWND, i32) -> i32;
type SetPreferredAppModeFn = unsafe extern "system" fn(i32) -> i32;
type RefreshImmersiveColorPolicyStateFn = unsafe extern "system" fn();
type FlushMenuThemesFn = unsafe extern "system" fn();
type SetWindowThemeFn =
    unsafe extern "system" fn(HWND, windows_sys::core::PCWSTR, windows_sys::core::PCWSTR) -> i32;

static mut UX_THEME: Option<HMODULE> = None;
static mut ALLOW_DARK_FOR_WINDOW: Option<AllowDarkModeForWindowFn> = None;
static mut SET_WINDOW_THEME: Option<SetWindowThemeFn> = None;
static mut SYSTEM_DARK: bool = false;

const APP_MODE_ALLOW_DARK: i32 = 1;

/// Reads the Windows "AppsUseLightTheme" DWORD. Zero means dark mode.
fn system_prefers_dark_mode() -> bool {
    let key: Vec<u16> = PERSONALIZE_KEY.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = "AppsUseLightTheme".encode_utf16().chain(Some(0)).collect();
    let mut hkey = null_mut();

    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return false;
        }

        let mut value = 1u32;
        let mut value_type = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let result = RegQueryValueExW(
            hkey,
            name.as_ptr(),
            null_mut(),
            &mut value_type,
            (&mut value as *mut u32).cast(),
            &mut size,
        );
        RegCloseKey(hkey);
        result == 0 && value_type == REG_DWORD && value == 0
    }
}

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn init_system_dark() {
    unsafe {
        SYSTEM_DARK = system_prefers_dark_mode();

        let lib = LoadLibraryW(encode_wide("uxtheme.dll").as_ptr());
        if lib.is_null() {
            return;
        }
        UX_THEME = Some(lib);

        // Windows 10 1903+: SetPreferredAppMode is ordinal 135.
        if let Some(proc) = GetProcAddress(lib, 135usize as *const u8) {
            let set_mode: SetPreferredAppModeFn = std::mem::transmute(proc);
            set_mode(APP_MODE_ALLOW_DARK);
        }

        // RefreshImmersiveColorPolicyState (ordinal 104).
        if let Some(proc) = GetProcAddress(lib, 104usize as *const u8) {
            let refresh: RefreshImmersiveColorPolicyStateFn = std::mem::transmute(proc);
            refresh();
        }

        // FlushMenuThemes (ordinal 136).
        if let Some(proc) = GetProcAddress(lib, 136usize as *const u8) {
            let flush: FlushMenuThemesFn = std::mem::transmute(proc);
            flush();
        }

        // AllowDarkModeForWindow (ordinal 133).
        if let Some(proc) = GetProcAddress(lib, 133usize as *const u8) {
            ALLOW_DARK_FOR_WINDOW = Some(std::mem::transmute(proc));
        }

        if let Some(proc) = GetProcAddress(lib, b"SetWindowTheme\0".as_ptr()) {
            SET_WINDOW_THEME = Some(std::mem::transmute(proc));
        }
    }
}

unsafe extern "system" fn cbt_hook(
    n_code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if n_code == HCBT_CREATEWND {
            let hwnd = wparam as HWND;
            let mut class_name = [0u16; 16];
            let length = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
            if length == 6 && class_name[..6] == ['#' as u16, '3' as u16, '2' as u16,
                '7' as u16, '6' as u16, '8' as u16]
            {
                if let Some(allow_dark) = ALLOW_DARK_FOR_WINDOW {
                    allow_dark(hwnd, 1);
                }
                if let Some(set_theme) = SET_WINDOW_THEME {
                    let theme = encode_wide("DarkMode_Explorer");
                    // The third argument is a sub-ID list; NULL lets Windows
                    // choose the default sub-ID list for this theme.
                    set_theme(hwnd, theme.as_ptr(), null_mut());
                }
            }
        }
        CallNextHookEx(null_mut(), n_code, wparam, lparam)
    }
}

struct TrayWindow {
    hwnd: HWND,
    app_state: AppState,
    current_icon: HICON,
    taskbar_created_message: u32,
    auto_reset_on_activity: bool,
    last_input_tick: Option<u32>,
}

static mut GLOBAL_WINDOW: Option<*mut TrayWindow> = None;

fn set_tooltip(nid: &mut NOTIFYICONDATAW, text: &str) {
    for (index, code_unit) in text.encode_utf16().take(nid.szTip.len().saturating_sub(1)).enumerate() {
        nid.szTip[index] = code_unit;
    }
}

impl TrayWindow {
    fn make_notify_data(&self, flags: u32, icon: HICON) -> NOTIFYICONDATAW {
        let mut nid = unsafe { std::mem::zeroed::<NOTIFYICONDATAW>() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid.uFlags = flags;
        nid.hIcon = icon;
        nid.uCallbackMessage = WM_TRAYICON;
        set_tooltip(&mut nid, &self.app_state.tooltip_text());
        nid
    }

    fn add_tray_icon(&mut self) -> bool {
        let nid = self.make_notify_data(NIF_MESSAGE | NIF_ICON | NIF_TIP, self.current_icon);
        unsafe { Shell_NotifyIconW(NIM_ADD, &nid) != 0 }
    }

    fn update_tray(&mut self) {
        let new_icon = create_tray_icon(
            self.app_state.current_mode.is_active(),
            self.app_state.current_mode == TimerDuration::Infinite,
            self.app_state.progress(),
        );
        if new_icon.is_null() {
            return;
        }

        let nid = self.make_notify_data(NIF_ICON | NIF_TIP, new_icon);
        let updated = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) != 0 };

        if updated {
            destroy_tray_icon(self.current_icon);
            self.current_icon = new_icon;
        } else {
            destroy_tray_icon(new_icon);
        }
    }

    fn sync_timer_and_power(&mut self) {
        let active = self.app_state.current_mode.is_active();
        let _ = set_keep_awake(active);
        unsafe {
            if active {
                SetTimer(self.hwnd, TIMER_ID_SECOND, 1000, None);
            } else {
                KillTimer(self.hwnd, TIMER_ID_SECOND);
            }
        }
    }

    fn on_left_click(&mut self) {
        self.app_state.cycle_next();
        self.sync_timer_and_power();
        self.update_tray();
    }

    fn toggle_active(&mut self) {
        self.app_state.toggle_active();
        self.sync_timer_and_power();
        self.update_tray();
    }

    fn select_duration(&mut self, mode: TimerDuration) {
        self.app_state.set_mode(mode);
        if self.auto_reset_on_activity {
            self.last_input_tick = get_last_input_tick();
        }
        self.sync_timer_and_power();
        self.update_tray();
    }

    fn toggle_auto_reset(&mut self) {
        self.auto_reset_on_activity = !self.auto_reset_on_activity;
        // Establish a baseline so enabling the feature does not treat old
        // keyboard/mouse activity as a fresh event.
        self.last_input_tick = if self.auto_reset_on_activity {
            get_last_input_tick()
        } else {
            None
        };
    }

    fn handle_activity_and_tick(&mut self) {
        if self.auto_reset_on_activity && self.app_state.current_mode.is_active() {
            if let Some(current_input_tick) = get_last_input_tick() {
                if let Some(previous_input_tick) = self.last_input_tick {
                    if current_input_tick != previous_input_tick {
                        self.app_state.reset_countdown();
                    }
                }
                self.last_input_tick = Some(current_input_tick);
            }
        }

        if self.app_state.tick() {
            let _ = set_keep_awake(false);
            unsafe { KillTimer(self.hwnd, TIMER_ID_SECOND); }
        }
        self.update_tray();
    }

    fn show_context_menu(&mut self) {
        unsafe {
            // Ask Explorer for the actual screen rectangle of our notification
            // icon. This remains correct when the taskbar is moved or the display
            // layout changes; the cursor position is only a fallback.
            let mut point = POINT { x: 0, y: 0 };
            let mut menu_flags = TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN;

            let identifier = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                guidItem: std::mem::zeroed(),
            };
            let mut icon_rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };

            if Shell_NotifyIconGetRect(&identifier, &mut icon_rect) == 0 {
                // Align the menu's right edge with the tray icon's right edge,
                // and its bottom edge with the icon's top edge. This gives the
                // familiar "menu grows upward from the tray icon" placement.
                point.x = icon_rect.right;
                point.y = icon_rect.top;
                menu_flags = TPM_RIGHTBUTTON | TPM_RIGHTALIGN | TPM_BOTTOMALIGN;
            } else {
                GetCursorPos(&mut point);
            }

            let menu: HMENU = CreatePopupMenu();
            if menu.is_null() {
                return;
            }

            let toggle_label = if self.app_state.current_mode.is_active() {
                "Deactivate (Turn Off)"
            } else {
                "Activate (15 min)"
            };
            let toggle_wide = encode_wide(toggle_label);
            AppendMenuW(menu, MF_STRING, CMD_TOGGLE, toggle_wide.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());

            let durations = [
                ("15 Minutes", CMD_15M, TimerDuration::Minutes(15)),
                ("30 Minutes", CMD_30M, TimerDuration::Minutes(30)),
                ("1 Hour", CMD_1H, TimerDuration::Hours(1)),
                ("2 Hours", CMD_2H, TimerDuration::Hours(2)),
                ("4 Hours", CMD_4H, TimerDuration::Hours(4)),
                ("Indefinite (Always Awake)", CMD_INFINITE, TimerDuration::Infinite),
            ];

            for (label, command, mode) in durations {
                let wide = encode_wide(label);
                let flags = if self.app_state.current_mode == mode {
                    MF_STRING | MF_CHECKED
                } else {
                    MF_STRING | MF_UNCHECKED
                };
                AppendMenuW(menu, flags, command, wide.as_ptr());
            }

            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());

            let reset_flags = if self.auto_reset_on_activity {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING | MF_UNCHECKED
            };
            let reset_label = encode_wide("Auto Reset on Activity (Mouse + Keyboard)");
            AppendMenuW(menu, reset_flags, CMD_AUTO_RESET, reset_label.as_ptr());

            let auto_flags = if caffeine_win::autostart::is_autostart_enabled() {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING | MF_UNCHECKED
            };
            let auto_label = encode_wide("Start with Windows");
            AppendMenuW(menu, auto_flags, CMD_AUTOSTART, auto_label.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
            let exit_label = encode_wide("Exit");
            AppendMenuW(menu, MF_STRING, CMD_EXIT, exit_label.as_ptr());

            SetForegroundWindow(self.hwnd);
            let hook = if SYSTEM_DARK {
                SetWindowsHookExW(
                    WH_CBT,
                    Some(cbt_hook),
                    GetModuleHandleW(null_mut()),
                    GetCurrentThreadId(),
                )
            } else {
                null_mut()
            };

            TrackPopupMenu(
                menu,
                menu_flags,
                point.x,
                point.y,
                0,
                self.hwnd,
                null_mut(),
            );

            if !hook.is_null() {
                UnhookWindowsHookEx(hook);
            }
            DestroyMenu(menu);
            // Required by the shell/Win32 popup-menu pattern to ensure the
            // foreground window receives the menu-dismissal notification.
            PostMessageW(self.hwnd, 0x0000, 0, 0); // WM_NULL
        }
    }

    fn handle_taskbar_recreated(&mut self) {
        let _ = self.add_tray_icon();
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if let Some(tray_ptr) = GLOBAL_WINDOW {
            let tray = &mut *tray_ptr;
            if msg == tray.taskbar_created_message {
                tray.handle_taskbar_recreated();
                return 0;
            }
        }

        match msg {
            WM_TRAYICON => {
                if let Some(tray_ptr) = GLOBAL_WINDOW {
                    let tray = &mut *tray_ptr;
                    match lparam as u32 {
                        WM_LBUTTONUP => tray.on_left_click(),
                        WM_RBUTTONUP => tray.show_context_menu(),
                        _ => {}
                    }
                }
                0
            }
            WM_TIMER => {
                if wparam == TIMER_ID_SECOND {
                    if let Some(tray_ptr) = GLOBAL_WINDOW {
                        let tray = &mut *tray_ptr;
                        tray.handle_activity_and_tick();
                    }
                }
                0
            }
            WM_COMMAND => {
                let command = (wparam & 0xffff) as usize;
                if let Some(tray_ptr) = GLOBAL_WINDOW {
                    let tray = &mut *tray_ptr;
                    match command {
                        CMD_TOGGLE => tray.toggle_active(),
                        CMD_15M => tray.select_duration(TimerDuration::Minutes(15)),
                        CMD_30M => tray.select_duration(TimerDuration::Minutes(30)),
                        CMD_1H => tray.select_duration(TimerDuration::Hours(1)),
                        CMD_2H => tray.select_duration(TimerDuration::Hours(2)),
                        CMD_4H => tray.select_duration(TimerDuration::Hours(4)),
                        CMD_INFINITE => tray.select_duration(TimerDuration::Infinite),
                        CMD_AUTOSTART => {
                            let _ = caffeine_win::autostart::toggle_autostart();
                        }
                        CMD_AUTO_RESET => {
                            tray.toggle_auto_reset();
                        }
                        CMD_EXIT => {
                            let _ = set_keep_awake(false);
                            PostQuitMessage(0);
                        }
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                let _ = set_keep_awake(false);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() {
    let _lock = match caffeine_win::single_instance::SingleInstance::try_acquire(
        "CaffeineWinSingleInstanceLock",
    ) {
        Some(lock) => lock,
        None => return,
    };

    init_system_dark();

    unsafe {
        let instance = GetModuleHandleW(null_mut());
        let class_name = encode_wide("CaffeineTrayWindowClass");
        let taskbar_created_message = RegisterWindowMessageW(encode_wide("TaskbarCreated").as_ptr());

        let window_class = windows_sys::Win32::UI::WindowsAndMessaging::WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
        };

        if windows_sys::Win32::UI::WindowsAndMessaging::RegisterClassW(&window_class) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            encode_wide("Caffeine").as_ptr(),
            0,
            0, 0, 0, 0,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            return;
        }

        let initial_icon = create_tray_icon(false, false, 0.0);
        if initial_icon.is_null() {
            return;
        }

        let mut tray_window = Box::new(TrayWindow {
            hwnd,
            app_state: AppState::new(),
            current_icon: initial_icon,
            taskbar_created_message,
            auto_reset_on_activity: false,
            last_input_tick: None,
        });
        GLOBAL_WINDOW = Some(tray_window.as_mut());

        if !tray_window.add_tray_icon() {
            GLOBAL_WINDOW = None;
            destroy_tray_icon(tray_window.current_icon);
            DestroyWindow(hwnd);
            return;
        }

        let mut msg: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let nid = tray_window.make_notify_data(NIF_MESSAGE | NIF_ICON | NIF_TIP, tray_window.current_icon);
        Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = set_keep_awake(false);
        GLOBAL_WINDOW = None;
        destroy_tray_icon(tray_window.current_icon);
        DestroyWindow(hwnd);
    }
}
