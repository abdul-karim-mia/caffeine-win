use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};

/// Prevents Windows from sleeping (and optionally turning off the display).
/// Must be called from the thread whose execution state should be changed.
pub fn set_keep_awake(enabled: bool) -> bool {
    let flags = if enabled {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
    } else {
        ES_CONTINUOUS
    };

    unsafe { SetThreadExecutionState(flags) != 0 }
}
