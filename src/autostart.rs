use std::env;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY_CURRENT_USER,
    KEY_READ, KEY_WRITE, REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "CaffeineWin";

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_autostart_enabled() -> bool {
    let key_wide = to_wide(RUN_KEY);
    let app_wide = to_wide(APP_NAME);
    let mut hkey = std::ptr::null_mut();

    unsafe {
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_wide.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        ) != 0
        {
            return false;
        }

        let mut val_type: u32 = 0;
        let mut data_len: u32 = 0;
        let res = RegQueryValueExW(
            hkey,
            app_wide.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            std::ptr::null_mut(),
            &mut data_len,
        );

        RegCloseKey(hkey);
        res == 0
    }
}

pub fn toggle_autostart() -> bool {
    let enabled = is_autostart_enabled();
    let key_wide = to_wide(RUN_KEY);
    let app_wide = to_wide(APP_NAME);
    let mut hkey = std::ptr::null_mut();

    unsafe {
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_wide.as_ptr(),
            0,
            KEY_WRITE,
            &mut hkey,
        ) != 0
        {
            return false;
        }

        if enabled {
            RegDeleteValueW(hkey, app_wide.as_ptr());
            RegCloseKey(hkey);
            false
        } else {
            if let Ok(exe_path) = env::current_exe() {
                let exe_str = format!("\"{}\"", exe_path.to_string_lossy());
                let exe_wide = to_wide(&exe_str);
                let bytes_len = (exe_wide.len() * 2) as u32;

                RegSetValueExW(
                    hkey,
                    app_wide.as_ptr(),
                    0,
                    REG_SZ,
                    exe_wide.as_ptr() as _,
                    bytes_len,
                );
            }
            RegCloseKey(hkey);
            true
        }
    }
}
