//! Small safe wrappers over the registry API, shared by the readers. Read-only:
//! every key is opened with `KEY_READ`.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_READ, RRF_RT_REG_DWORD, RRF_RT_REG_QWORD, RRF_RT_REG_SZ, RegCloseKey, RegEnumValueW,
    RegGetValueW, RegOpenKeyExW,
};

pub use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// An open key, closed on drop.
pub struct Key(HKEY);

impl Key {
    pub fn open(root: HKEY, path: &str) -> Option<Key> {
        let path = wide(path);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated and outlives the call; `key` is a valid out pointer.
        let rc = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ, &mut key) };
        (rc == ERROR_SUCCESS).then_some(Key(key))
    }

    pub fn dword(&self, name: &str) -> Option<u32> {
        let name = wide(name);
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: `value` is a writable u32 and `size` holds its byte length.
        let rc = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut size,
            )
        };
        (rc == ERROR_SUCCESS).then_some(value)
    }

    pub fn qword(&self, name: &str) -> Option<u64> {
        let name = wide(name);
        let mut value: u64 = 0;
        let mut size = std::mem::size_of::<u64>() as u32;
        // SAFETY: `value` is a writable u64 and `size` holds its byte length.
        let rc = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                RRF_RT_REG_QWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u64).cast(),
                &mut size,
            )
        };
        (rc == ERROR_SUCCESS).then_some(value)
    }

    /// `REG_SZ` (and `REG_EXPAND_SZ` unexpanded is not requested).
    pub fn string(&self, name: &str) -> Option<String> {
        let name = wide(name);
        let mut size: u32 = 0;
        // SAFETY: a null data pointer asks only for the size in bytes.
        let rc = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: `buf` has `size` writable bytes.
        let rc = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    /// Names of values (paths can be long, so the name buffer is 32k u16).
    pub fn value_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = vec![0u16; 32_768];
        for index in 0.. {
            let mut len = buf.len() as u32;
            // SAFETY: `buf` has `len` u16 slots; type and data out pointers are null.
            let rc = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if rc != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        out
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful RegOpenKeyExW and is closed once.
        unsafe { RegCloseKey(self.0) };
    }
}
