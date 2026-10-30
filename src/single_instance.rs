use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};

pub struct SingleInstance {
    _handle: windows_sys::Win32::Foundation::HANDLE,
}

impl SingleInstance {
    pub fn try_acquire(name: &str) -> Option<Self> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
            if handle.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                windows_sys::Win32::Foundation::CloseHandle(handle);
                return None;
            }
            Some(Self { _handle: handle })
        }
    }
}
