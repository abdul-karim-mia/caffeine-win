#![windows_subsystem = "windows"]

use caffeine_win::icon::{create_tray_icon, destroy_tray_icon};
use caffeine_win::keepawake::set_keep_awake;
use caffeine_win::menu::MenuEntry;
use caffeine_win::state::{AppState, TimerDuration};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, REG_DWORD, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    HICON, IDYES, KillTimer, MB_ICONQUESTION, MB_SETFOREGROUND, MB_YESNO, MSG, MessageBoxW,
    PostQuitMessage, RegisterWindowMessageW, SetTimer, TranslateMessage, WM_COMMAND, WM_DESTROY,
    WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER, WM_USER,
};

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
/// Row identifier for the Options submenu parent; never dispatched.
const PERSONALIZE_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

static mut SYSTEM_DARK: bool = false;

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
    }
}

/// DWM accent color as an 0xAARRGGBB DWORD, converted to a COLORREF and
/// cached after the first read. Falls back to the Windows blue accent.
fn accent_color() -> windows_sys::Win32::Foundation::COLORREF {
    static ACCENT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let value = *ACCENT.get_or_init(|| unsafe {
        let key = encode_wide("Software\\Microsoft\\Windows\\DWM");
        let name = encode_wide("AccentColor");
        let mut hkey = null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return 0xff_00_78_d7;
        }
        let mut value = 0u32;
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
        if result != 0 || value_type != REG_DWORD {
            0xff_00_78_d7
        } else {
            value
        }
    });
    value & 0x00ff_ffff
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
    for (index, code_unit) in text
        .encode_utf16()
        .take(nid.szTip.len().saturating_sub(1))
        .enumerate()
    {
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
            unsafe {
                KillTimer(self.hwnd, TIMER_ID_SECOND);
            }
            self.show_balloon("Keep-awake timer finished. Caffeine is now Off.");
        }
        self.update_tray();
    }

    fn show_context_menu(&mut self) {
        unsafe {
            // Ask Explorer for the actual screen rectangle of our notification
            // icon. This stays correct when the taskbar moves or the display
            // layout changes; the cursor position is only a fallback.
            let identifier = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                guidItem: std::mem::zeroed(),
            };
            let mut anchor = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if Shell_NotifyIconGetRect(&identifier, &mut anchor) != 0 {
                let mut point = POINT { x: 0, y: 0 };
                GetCursorPos(&mut point);
                anchor = RECT {
                    left: point.x,
                    top: point.y,
                    right: point.x,
                    bottom: point.y,
                };
            }

            let active = self.app_state.current_mode.is_active();
            let mut rows = vec![
                MenuEntry::header(
                    &self.app_state.status_label(),
                    caffeine_win::menu::ICON_CLOCK,
                ),
                MenuEntry::icon_item(
                    if active { "Deactivate" } else { "Activate" },
                    CMD_TOGGLE,
                    active,
                    caffeine_win::menu::ICON_POWER,
                ),
            ];
            let durations = [
                ("15 min", CMD_15M, TimerDuration::Minutes(15)),
                ("30 min", CMD_30M, TimerDuration::Minutes(30)),
                ("1 h", CMD_1H, TimerDuration::Hours(1)),
                ("2 h", CMD_2H, TimerDuration::Hours(2)),
                ("4 h", CMD_4H, TimerDuration::Hours(4)),
                ("Always awake", CMD_INFINITE, TimerDuration::Infinite),
            ];
            for (label, command, mode) in durations {
                rows.push(MenuEntry::radio_item(
                    label,
                    command,
                    self.app_state.current_mode == mode,
                    caffeine_win::menu::ICON_CLOCK,
                ));
            }
            rows.push(MenuEntry::icon_item(
                "Auto reset on activity",
                CMD_AUTO_RESET,
                self.auto_reset_on_activity,
                caffeine_win::menu::ICON_REFRESH,
            ));
            rows.push(MenuEntry::icon_item(
                "Start with Windows",
                CMD_AUTOSTART,
                caffeine_win::autostart::is_autostart_enabled(),
                caffeine_win::menu::ICON_HOME,
            ));
            rows.push(MenuEntry::icon_item(
                "Exit",
                CMD_EXIT,
                false,
                caffeine_win::menu::ICON_EXIT,
            ));

            caffeine_win::menu::show_tray_menu(
                self.hwnd,
                rows,
                anchor,
                SYSTEM_DARK,
                accent_color(),
            );
        }
    }
    /// Balloon tip; used when the keep-awake timer runs out.
    fn show_balloon(&self, info: &str) {
        unsafe {
            let mut nid = self.make_notify_data(NIF_INFO | NIF_TIP, self.current_icon);
            for (index, unit) in info.encode_utf16().take(nid.szInfo.len()).enumerate() {
                nid.szInfo[index] = unit;
            }
            for (index, unit) in "Caffeine"
                .encode_utf16()
                .take(nid.szInfoTitle.len())
                .enumerate()
            {
                nid.szInfoTitle[index] = unit;
            }
            nid.dwInfoFlags = NIIF_INFO;
            nid.Anonymous.uTimeout = 5000;
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
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
                            if tray.app_state.current_mode.is_active()
                                && MessageBoxW(
                                    hwnd,
                                    encode_wide(
                                        "A keep-awake timer is still running.\nExit and stop keeping the PC awake?",
                                    )
                                    .as_ptr(),
                                    encode_wide("Caffeine").as_ptr(),
                                    MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND,
                                ) != IDYES
                            {
                                return 0;
                            }
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
        let taskbar_created_message =
            RegisterWindowMessageW(encode_wide("TaskbarCreated").as_ptr());

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

        let nid = tray_window
            .make_notify_data(NIF_MESSAGE | NIF_ICON | NIF_TIP, tray_window.current_icon);
        Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = set_keep_awake(false);
        GLOBAL_WINDOW = None;
        destroy_tray_icon(tray_window.current_icon);
        DestroyWindow(hwnd);
    }
}
