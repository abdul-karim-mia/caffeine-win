use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};

pub fn set_keep_awake(enabled: bool) -> bool {
    let flags = if enabled {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    unsafe {
        let prev = SetThreadExecutionState(flags);
        prev != 0
    }
}
