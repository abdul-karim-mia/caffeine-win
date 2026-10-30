use std::env;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "CaffeineWin";

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Returns true only when the per-user Run value exists and can be queried.
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

        let mut value_type = 0u32;
        let mut data_len = 0u32;
        let result = RegQueryValueExW(
            hkey,
            app_wide.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            std::ptr::null_mut(),
            &mut data_len,
        );

        RegCloseKey(hkey);
        result == 0 && value_type == REG_SZ && data_len >= 2
    }
}

/// Toggles the Run entry. Returns `Ok(new_state)` only after the registry
/// operation succeeds; errors contain the Win32 error code.
pub fn toggle_autostart() -> Result<bool, u32> {
    let enabled = is_autostart_enabled();
    let key_wide = to_wide(RUN_KEY);
    let app_wide = to_wide(APP_NAME);
    let mut hkey = std::ptr::null_mut();

    unsafe {
        let open_result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_wide.as_ptr(),
            0,
            KEY_READ | KEY_SET_VALUE,
            &mut hkey,
        );
        if open_result != 0 {
            return Err(open_result);
        }

        let result = if enabled {
            let code = RegDeleteValueW(hkey, app_wide.as_ptr());
            if code == ERROR_FILE_NOT_FOUND || code == ERROR_PATH_NOT_FOUND {
                0
            } else {
                code
            }
        } else {
            match env::current_exe() {
                Ok(exe_path) => {
                    let exe_str = format!("\"{}\"", exe_path.to_string_lossy());
                    let exe_wide = to_wide(&exe_str);
                    let bytes_len = (exe_wide.len() * std::mem::size_of::<u16>()) as u32;
                    RegSetValueExW(
                        hkey,
                        app_wide.as_ptr(),
                        0,
                        REG_SZ,
                        exe_wide.as_ptr().cast(),
                        bytes_len,
                    )
                }
                Err(_) => {
                    RegCloseKey(hkey);
                    return Err(1); // ERROR_INVALID_FUNCTION
                }
            }
        };

        RegCloseKey(hkey);
        if result != 0 {
            Err(result)
        } else {
            Ok(!enabled)
        }
    }
}
