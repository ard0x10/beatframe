//! One copy of the app per Windows session. Starting it again, say from the
//! Start Menu while it already sits in the tray, opens the settings window of
//! the copy that runs instead of quietly doing nothing.

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

    const LOCK: &str = "Local\\beatframe";
    const SHOW_SETTINGS: &str = "Local\\beatframe-show-settings";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// True for the first copy. A second one, say started by hand after the
    /// sign-in entry, would draw a second light.
    pub fn first() -> bool {
        // The handle stays open until the process exits, which releases the name.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide(LOCK).as_ptr()) };
        !(handle.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS)
    }

    /// Called by a later copy before it exits.
    pub fn ask_for_settings() {
        let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide(SHOW_SETTINGS).as_ptr()) };
        if event.is_null() {
            // The first copy is still starting, or is on its way out.
            return;
        }
        // Windows lets the copy the user just started bring a window forward,
        // not the one already running. This hands the right over until the
        // next input.
        unsafe {
            AllowSetForegroundWindow(ASFW_ANY);
            SetEvent(event);
        }
    }

    /// Runs `show` on a thread of its own each time a later copy asks.
    pub fn listen(show: impl Fn() + Send + 'static) {
        // Auto reset: one start, one call.
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide(SHOW_SETTINGS).as_ptr()) };
        if event.is_null() {
            eprintln!("instance: a second start will not open the settings window");
            return;
        }
        let event = event as usize;
        std::thread::spawn(move || {
            while unsafe { WaitForSingleObject(event as _, INFINITE) } == WAIT_OBJECT_0 {
                show();
            }
        });
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn first() -> bool {
        true
    }

    pub fn ask_for_settings() {}

    pub fn listen(_show: impl Fn() + Send + 'static) {}
}

pub use platform::{ask_for_settings, first, listen};
