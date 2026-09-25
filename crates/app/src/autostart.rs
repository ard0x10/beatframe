//! Starting with Windows through the user's Run key: no administrator rights,
//! and it shows up under Startup apps where it can be turned off as well.

/// Adds or removes the entry. When on, the path is written every time, so an
/// exe that was moved is found again at the next sign-in.
pub fn set(on: bool) {
    let result = if on { platform::add() } else { platform::remove() };
    if let Err(e) = result {
        eprintln!("autostart: could not {} the sign-in entry: {e}", if on { "write" } else { "remove" });
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "BeatFrame";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn add() -> std::io::Result<()> {
        let exe = std::env::current_exe()?;
        let command = wide(&format!("\"{}\"", exe.display()));
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(KEY).as_ptr(),
                wide(NAME).as_ptr(),
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        };
        match status {
            ERROR_SUCCESS => Ok(()),
            e => Err(std::io::Error::from_raw_os_error(e as i32)),
        }
    }

    pub fn remove() -> std::io::Result<()> {
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(NAME).as_ptr()) };
        match status {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            e => Err(std::io::Error::from_raw_os_error(e as i32)),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn add() -> std::io::Result<()> {
        Err(std::io::Error::other("starting at sign-in is not implemented on this platform yet"))
    }

    pub fn remove() -> std::io::Result<()> {
        Ok(())
    }
}
