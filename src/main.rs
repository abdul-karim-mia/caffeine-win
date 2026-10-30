#![windows_subsystem = "windows"]

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, KillTimer, PostQuitMessage, RegisterClassW, SetForegroundWindow,
    SetTimer, TrackPopupMenu, TranslateMessage, HICON, HMENU, MF_CHECKED, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY,
    WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER, WM_USER, WNDCLASSW,
};

use caffeine_win::icon::{create_tray_icon, destroy_tray_icon};
use caffeine_win::keepawake::set_keep_awake;
use caffeine_win::state::{AppState, TimerDuration};

const WM_TRAYICON: u32 = WM_USER + 1;
const TIMER_ID_SECOND: usize = 1001;

// Menu Command IDs
const CMD_EXIT: usize = 2001;
const CMD_TOGGLE: usize = 2002;
const CMD_15M: usize = 2003;
const CMD_30M: usize = 2004;
const CMD_1H: usize = 2005;
const CMD_2H: usize = 2006;
const CMD_4H: usize = 2007;
const CMD_INFINITE: usize = 2008;
const CMD_AUTOSTART: usize = 2009;

struct TrayWindow {
    hwnd: HWND,
    app_state: AppState,
    current_icon: HICON,
}

static mut GLOBAL_WINDOW: Option<*mut TrayWindow> = None;

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_tooltip(nid: &mut NOTIFYICONDATAW, text: &str) {
    let wide = text.encode_utf16().take(127);
    for (i, c) in wide.enumerate() {
        nid.szTip[i] = c;
    }
}

impl TrayWindow {
    fn update_tray(&mut self) {
        let is_active = self.app_state.current_mode.is_active();
        let is_infinite = self.app_state.current_mode == TimerDuration::Infinite;
        let progress = self.app_state.progress();

        let new_icon = create_tray_icon(is_active, is_infinite, progress);

        let mut nid = unsafe { std::mem::zeroed::<NOTIFYICONDATAW>() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_TIP;
        nid.hIcon = new_icon;
        set_tooltip(&mut nid, &self.app_state.tooltip_text());

        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }

        destroy_tray_icon(self.current_icon);
        self.current_icon = new_icon;
    }

    fn on_left_click(&mut self) {
        self.app_state.cycle_next();
        let active = self.app_state.current_mode.is_active();
        set_keep_awake(active);

        unsafe {
            if active {
                SetTimer(self.hwnd, TIMER_ID_SECOND, 1000, None);
            } else {
                KillTimer(self.hwnd, TIMER_ID_SECOND);
            }
        }
        self.update_tray();
    }

    fn show_context_menu(&mut self) {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);

            let menu: HMENU = CreatePopupMenu();
            let state = &self.app_state;

            let toggle_label = if state.current_mode.is_active() {
                "Deactivate (Turn Off)"
            } else {
                "Activate (15 min)"
            };
            let w_toggle = encode_wide(toggle_label);
            AppendMenuW(menu, MF_STRING, CMD_TOGGLE, w_toggle.as_ptr());

            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());

            let durations = [
                ("15 Minutes", CMD_15M, TimerDuration::Minutes(15)),
                ("30 Minutes", CMD_30M, TimerDuration::Minutes(30)),
                ("1 Hour", CMD_1H, TimerDuration::Hours(1)),
                ("2 Hours", CMD_2H, TimerDuration::Hours(2)),
                ("4 Hours", CMD_4H, TimerDuration::Hours(4)),
                ("Indefinite (Always Awake)", CMD_INFINITE, TimerDuration::Infinite),
            ];

            for (label, cmd_id, mode) in durations {
                let w_label = encode_wide(label);
                let flags = if state.current_mode == mode {
                    MF_STRING | MF_CHECKED
                } else {
                    MF_STRING | MF_UNCHECKED
                };
                AppendMenuW(menu, flags, cmd_id, w_label.as_ptr());
            }

            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());

            let auto_flags = if caffeine_win::autostart::is_autostart_enabled() {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING | MF_UNCHECKED
            };
            let w_auto = encode_wide("Start with Windows");
            AppendMenuW(menu, auto_flags, CMD_AUTOSTART, w_auto.as_ptr());

            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
            let w_exit = encode_wide("Exit");
            AppendMenuW(menu, MF_STRING, CMD_EXIT, w_exit.as_ptr());

            SetForegroundWindow(self.hwnd);
            TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                null_mut(),
            );
            DestroyMenu(menu);
        }
    }

    fn select_duration(&mut self, mode: TimerDuration) {
        self.app_state.set_mode(mode);
        let active = mode.is_active();
        set_keep_awake(active);

        unsafe {
            if active {
                SetTimer(self.hwnd, TIMER_ID_SECOND, 1000, None);
            } else {
                KillTimer(self.hwnd, TIMER_ID_SECOND);
            }
        }
        self.update_tray();
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAYICON => {
                let event = lparam as u32;
                if let Some(tray_ptr) = GLOBAL_WINDOW {
                    let tray = &mut *tray_ptr;
                    match event {
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
                        let expired = tray.app_state.tick();
                        if expired {
                            set_keep_awake(false);
                            KillTimer(hwnd, TIMER_ID_SECOND);
                        }
                        tray.update_tray();
                    }
                }
                0
            }
            WM_COMMAND => {
                let cmd = wparam as usize;
                if let Some(tray_ptr) = GLOBAL_WINDOW {
                    let tray = &mut *tray_ptr;
                    match cmd {
                        CMD_TOGGLE => tray.on_left_click(),
                        CMD_15M => tray.select_duration(TimerDuration::Minutes(15)),
                        CMD_30M => tray.select_duration(TimerDuration::Minutes(30)),
                        CMD_1H => tray.select_duration(TimerDuration::Hours(1)),
                        CMD_2H => tray.select_duration(TimerDuration::Hours(2)),
                        CMD_4H => tray.select_duration(TimerDuration::Hours(4)),
                        CMD_INFINITE => tray.select_duration(TimerDuration::Infinite),
                        CMD_AUTOSTART => {
                            caffeine_win::autostart::toggle_autostart();
                        }
                        CMD_EXIT => {
                            set_keep_awake(false);
                            PostQuitMessage(0);
                        }
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                set_keep_awake(false);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() {
    let _lock = match caffeine_win::single_instance::SingleInstance::try_acquire("CaffeineWinSingleInstanceLock") {
        Some(lock) => lock,
        None => return, // Another instance is already running
    };

    unsafe {
        let instance = GetModuleHandleW(null_mut());
        let class_name = encode_wide("CaffeineTrayWindowClass");

        let wc = WNDCLASSW {
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

        RegisterClassW(&wc);

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

        let initial_state = AppState::new();
        let initial_icon = create_tray_icon(false, false, 0.0);

        let mut tray_window = Box::new(TrayWindow {
            hwnd,
            app_state: initial_state,
            current_icon: initial_icon,
        });

        GLOBAL_WINDOW = Some(tray_window.as_mut());

        // Add NotifyIcon to System Tray
        let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = initial_icon;
        set_tooltip(&mut nid, &tray_window.app_state.tooltip_text());

        Shell_NotifyIconW(NIM_ADD, &nid);

        // Win32 Message Loop
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        Shell_NotifyIconW(NIM_DELETE, &nid);
        destroy_tray_icon(tray_window.current_icon);
    }
}
