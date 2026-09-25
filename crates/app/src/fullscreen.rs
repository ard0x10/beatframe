//! Tells when the foreground window covers a monitor the light is on, so the
//! light can step aside there for games and full screen video.

use std::time::{Duration, Instant};

/// How long the screen has to stay uncovered before the light comes back, so a
/// quick Alt+Tab out of a game does not flash it.
pub const RETURN_DELAY: Duration = Duration::from_secs(1);

/// A video going full screen resizes its window without changing the
/// foreground, so the window is also checked on this interval.
pub const POLL: Duration = Duration::from_millis(500);

/// Screen rectangle in physical pixels: left, top, right, bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect(pub i32, pub i32, pub i32, pub i32);

/// A window covers the monitor when its visible frame reaches every edge. A
/// maximized window stops at the taskbar and does not.
pub fn covers(window: Rect, monitor: Rect) -> bool {
    window.0 <= monitor.0 && window.1 <= monitor.1 && window.2 >= monitor.2 && window.3 >= monitor.3
}

/// For each monitor, whether the foreground window's `frame` covers it. A game
/// on one monitor leaves the others alone.
pub fn covered(frame: Option<Rect>, monitors: &[Rect]) -> Vec<bool> {
    monitors.iter().map(|&m| frame.is_some_and(|f| covers(f, m))).collect()
}

/// Hides at once, shows again only after `RETURN_DELAY` of uncovered readings.
#[derive(Default)]
pub struct Gate {
    hidden: bool,
    clear_since: Option<Instant>,
}

impl Gate {
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    pub fn update(&mut self, covered: bool, now: Instant) {
        if covered {
            self.hidden = true;
            self.clear_since = None;
        } else if self.hidden {
            let since = *self.clear_since.get_or_insert(now);
            if now - since >= RETURN_DELAY {
                self.hidden = false;
                self.clear_since = None;
            }
        }
    }

    /// When the next reading is due: the regular poll, or the moment the light
    /// may come back if that is sooner.
    pub fn next_check(&self, now: Instant) -> Instant {
        let poll = now + POLL;
        self.clear_since.map_or(poll, |s| poll.min(s + RETURN_DELAY))
    }
}

pub use platform::{foreground_frame, watch};

#[cfg(windows)]
mod platform {
    use std::cell::RefCell;

    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindowRect,
        GetWindowThreadProcessId, WINEVENT_OUTOFCONTEXT,
    };

    use super::Rect;

    thread_local! {
        static ON_CHANGE: RefCell<Option<Box<dyn Fn()>>> = RefCell::new(None);
    }

    /// Keeps the foreground hook installed until dropped.
    pub struct Watch(HWINEVENTHOOK);

    impl Drop for Watch {
        fn drop(&mut self) {
            unsafe {
                UnhookWinEvent(self.0);
            }
        }
    }

    /// Calls `changed` whenever another window comes to the foreground. The
    /// call arrives on this thread, through its message loop.
    pub fn watch(changed: impl Fn() + 'static) -> Option<Watch> {
        ON_CHANGE.with(|c| *c.borrow_mut() = Some(Box::new(changed)));
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if hook.is_null() {
            eprintln!("fullscreen: cannot watch the foreground window, checking on a timer only");
            return None;
        }
        Some(Watch(hook))
    }

    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
        ON_CHANGE.with(|c| {
            if let Some(f) = &*c.borrow() {
                f();
            }
        });
    }

    fn rect(r: RECT) -> Rect {
        Rect(r.left, r.top, r.right, r.bottom)
    }

    /// The foreground window's frame, or `None` when it is the desktop, the
    /// shell or one of this app's own windows, which never hide the light.
    pub fn foreground_frame() -> Option<Rect> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() || hwnd == GetShellWindow() {
                return None;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == GetCurrentProcessId() {
                return None;
            }
            // The desktop behind the icons spans the screen too.
            let mut class = [0u16; 16];
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..len]);
            if class == "Progman" || class == "WorkerW" {
                return None;
            }

            // The visible frame, without the invisible resize border of normal windows.
            let mut frame: RECT = std::mem::zeroed();
            let got_frame = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut frame as *mut RECT).cast(),
                size_of::<RECT>() as u32,
            ) == 0;
            if !got_frame && GetWindowRect(hwnd, &mut frame) == 0 {
                return None;
            }
            Some(rect(frame))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub struct Watch;

    pub fn watch(_changed: impl Fn() + 'static) -> Option<Watch> {
        None
    }

    pub fn foreground_frame() -> Option<super::Rect> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect(0, 0, 1920, 1200);

    #[test]
    fn only_a_window_reaching_every_edge_covers() {
        assert!(covers(SCREEN, SCREEN));
        // Larger than the screen, as when only the window rectangle is known.
        assert!(covers(Rect(-8, -8, 1928, 1208), SCREEN));
        // Maximized: stops at the taskbar.
        assert!(!covers(Rect(0, 0, 1920, 1152), SCREEN));
        // Maximized with an auto-hidden taskbar keeps a thin strip free.
        assert!(!covers(Rect(0, 0, 1920, 1198), SCREEN));
        // Full screen on the other monitor.
        assert!(!covers(Rect(1920, 0, 3840, 1080), SCREEN));
    }

    #[test]
    fn only_the_monitor_under_the_window_is_covered() {
        // A 1920x1200 main display with a 1440x900 monitor to its right.
        let right = Rect(1920, 0, 3360, 900);
        let both = [SCREEN, right];
        assert_eq!(covered(Some(right), &both), [false, true]);
        assert_eq!(covered(Some(SCREEN), &both), [true, false]);
        // One window stretched over both covers both.
        assert_eq!(covered(Some(Rect(0, 0, 3360, 1200)), &both), [true, true]);
        // The desktop or this app's own window in front covers nothing.
        assert_eq!(covered(None, &both), [false, false]);
    }

    #[test]
    fn hides_at_once_and_returns_after_the_delay() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut g = Gate::default();
        g.update(false, t0);
        assert!(!g.hidden());
        g.update(true, ms(10));
        assert!(g.hidden());

        // A short Alt+Tab: uncovered for 400 ms, then covered again.
        g.update(false, ms(100));
        g.update(false, ms(500));
        assert!(g.hidden());
        g.update(true, ms(500));
        g.update(false, ms(600));
        assert!(g.hidden(), "the clock restarts after the window covers again");
        assert_eq!(g.next_check(ms(1400)), ms(1600));
        g.update(false, ms(1599));
        assert!(g.hidden());
        g.update(false, ms(1600));
        assert!(!g.hidden());
        assert_eq!(g.next_check(ms(1600)), ms(1600) + POLL);
    }
}
