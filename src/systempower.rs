//! The Windows screen-off timeout, shown as the app's resting state.
//!
//! The power setting is read on demand, so the label is always in sync with
//! whatever Windows is set to: the menu builds its header when it opens, and
//! the value follows the active power source (plugged in or on battery).

use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, PowerGetActiveScheme, PowerReadACValueIndex, PowerReadDCValueIndex,
    SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
use windows_sys::core::GUID;

/// {7516B95F-F776-4464-8C53-06167F40CC99} Display power subgroup.
fn display_subgroup() -> GUID {
    GUID::from_u128(0x7516b95f_f776_4464_8c53_06167f40cc99)
}

/// {3C0BC021-C8A8-4E07-A973-6B14CBCB2B7E} "Turn off display after".
fn video_idle_timeout() -> GUID {
    GUID::from_u128(0x3c0bc021_c8a8_4e07_a973_6b14cbcb2b7e)
}

/// True when the machine is running on battery, so the DC value is the one
/// Windows actually shows.
fn on_battery() -> bool {
    unsafe {
        let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
        GetSystemPowerStatus(&mut status) != 0 && status.ACLineStatus == 0
    }
}

/// Seconds before the display turns off, following the current power source.
/// `Some(0)` means the display never turns off.
pub fn screen_off_seconds() -> Option<u32> {
    unsafe {
        let mut scheme: *mut GUID = std::ptr::null_mut();
        if PowerGetActiveScheme(HKEY_CURRENT_USER, &mut scheme) != 0 || scheme.is_null() {
            return None;
        }
        let subgroup = display_subgroup();
        let setting = video_idle_timeout();
        let mut seconds = 0u32;
        let read = if on_battery() {
            PowerReadDCValueIndex(HKEY_CURRENT_USER, scheme, &subgroup, &setting, &mut seconds)
        } else {
            PowerReadACValueIndex(HKEY_CURRENT_USER, scheme, &subgroup, &setting, &mut seconds)
        };
        // PowerGetActiveScheme hands back an allocated GUID.
        LocalFree(scheme as HLOCAL);
        if read != 0 { None } else { Some(seconds) }
    }
}

/// `N min`, `N h`, or `Never`, matching how Windows words the setting.
fn format_duration(seconds: u32) -> String {
    if seconds == 0 {
        return "Never".to_string();
    }
    if seconds < 60 {
        return format!("{seconds} s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    format!("{hours} h")
}

/// Header text for the resting state: the system's own screen-off timeout.
pub fn screen_off_label() -> String {
    match screen_off_seconds() {
        Some(seconds) => format!("Screen off after {}", format_duration(seconds)),
        None => "Off".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_off_timeout_reads_a_number() {
        // Reads the real setting, so any machine returns something valid:
        // zero means the display never turns off.
        let seconds = screen_off_seconds();
        match seconds {
            Some(seconds) => assert!(seconds <= 86_400, "implausible timeout: {seconds}"),
            None => panic!("reading the screen-off timeout failed"),
        }
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "Never");
        assert_eq!(format_duration(45), "45 s");
        assert_eq!(format_duration(60), "1 min");
        assert_eq!(format_duration(300), "5 min");
        assert_eq!(format_duration(7200), "2 h");
    }
}
