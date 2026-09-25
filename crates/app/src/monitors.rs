//! The monitors connected right now, which of them the light is drawn on, and
//! a watch that tells when Windows changes them.

use crate::fullscreen::Rect;

/// The settings file's name for whichever monitor Windows makes the main display.
pub const PRIMARY: &str = "primary";

#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    /// The monitor's own name when it reports one, numbered when two share it.
    pub name: String,
    pub rect: Rect,
    pub primary: bool,
}

impl Monitor {
    pub fn width(&self) -> u32 {
        (self.rect.2 - self.rect.0).max(0) as u32
    }

    pub fn height(&self) -> u32 {
        (self.rect.3 - self.rect.1).max(0) as u32
    }
}

/// Orders monitors left to right and numbers the ones that report the same
/// name, so each of two identical monitors can be chosen on its own.
fn number_twins(monitors: &mut [Monitor]) {
    monitors.sort_by_key(|m| (m.rect.0, m.rect.1));
    let names: Vec<String> = monitors.iter().map(|m| m.name.clone()).collect();
    for (i, m) in monitors.iter_mut().enumerate() {
        let before = names[..i].iter().filter(|n| **n == names[i]).count();
        if before > 0 {
            m.name = format!("{} ({})", names[i], before + 1);
        }
    }
}

/// One line per monitor for the log: name, role, size and place.
pub fn describe(monitors: &[Monitor]) -> Vec<String> {
    monitors
        .iter()
        .map(|m| {
            let main = if m.primary { " (main)" } else { "" };
            format!("{}{main} {}x{} at {},{}", m.name, m.width(), m.height(), m.rect.0, m.rect.1)
        })
        .collect()
}

fn is_chosen(chosen: &[String], m: &Monitor) -> bool {
    chosen.iter().any(|c| (c == PRIMARY && m.primary) || *c == m.name)
}

/// The connected monitors the light is drawn on: the chosen ones, or the main
/// display when none of them is connected, so the light does not go dark while
/// the tray says it is on.
pub fn lit<'a>(chosen: &[String], connected: &'a [Monitor]) -> Vec<&'a Monitor> {
    let lit: Vec<&Monitor> = connected.iter().filter(|m| is_chosen(chosen, m)).collect();
    if !lit.is_empty() {
        return lit;
    }
    connected.iter().find(|m| m.primary).or(connected.first()).into_iter().collect()
}

/// `chosen` with `m` switched on or off. The main display is written as
/// "primary", so the light moves with it when Windows makes another monitor
/// the main one. Names of monitors that are not connected stay as they are.
pub fn choose(chosen: &[String], connected: &[Monitor], m: &Monitor, on: bool) -> Vec<String> {
    let mut out = chosen.to_vec();
    if !connected.iter().any(|c| is_chosen(chosen, c)) {
        // The main display is lit without being chosen; it stays lit.
        out.push(PRIMARY.into());
    }
    out.retain(|c| !(*c == m.name || (m.primary && c == PRIMARY)));
    if on {
        out.push(if m.primary { PRIMARY.into() } else { m.name.clone() });
    }
    out
}

pub use platform::{connected, watch};

#[cfg(windows)]
mod platform {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use windows_sys::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
        QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
    };
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, MONITORINFOF_PRIMARY, RegisterClassW, WM_DISPLAYCHANGE,
        WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
    };
    use windows_sys::core::BOOL;

    use super::{Monitor, number_twins};
    use crate::fullscreen::Rect;

    fn text(wide: &[u16]) -> String {
        let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..len])
    }

    /// The monitors' own names, keyed by the GDI name ("\\.\DISPLAY2") that
    /// the monitor list uses. A built-in panel often reports no name.
    fn names() -> HashMap<String, String> {
        let mut names = HashMap::new();
        let mut paths = Vec::new();
        // The layout can change between asking for the sizes and the query.
        for _ in 0..3 {
            let (mut np, mut nm) = (0, 0);
            if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) } != 0 {
                return names;
            }
            paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
            let status = unsafe {
                QueryDisplayConfig(
                    QDC_ONLY_ACTIVE_PATHS,
                    &mut np,
                    paths.as_mut_ptr(),
                    &mut nm,
                    modes.as_mut_ptr(),
                    std::ptr::null_mut(),
                )
            };
            if status == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if status != 0 {
                return names;
            }
            paths.truncate(np as usize);
            break;
        }
        for path in &paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source.header.adapterId = path.sourceInfo.adapterId;
            source.header.id = path.sourceInfo.id;
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            target.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            target.header.adapterId = path.targetInfo.adapterId;
            target.header.id = path.targetInfo.id;
            unsafe {
                if DisplayConfigGetDeviceInfo(&mut source.header) != 0
                    || DisplayConfigGetDeviceInfo(&mut target.header) != 0
                {
                    continue;
                }
            }
            let built_in = matches!(
                target.outputTechnology,
                DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                    | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                    | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
                    | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS
            );
            let name = match text(&target.monitorFriendlyDeviceName) {
                n if !n.trim().is_empty() => n.trim().to_string(),
                _ if built_in => "Built-in display".to_string(),
                _ => continue,
            };
            // A mirrored screen has two monitors on one source; the first names it.
            names.entry(text(&source.viewGdiDeviceName)).or_insert(name);
        }
        names
    }

    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, found: LPARAM) -> BOOL {
        let found = unsafe { &mut *(found as *mut Vec<(String, Rect, bool)>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) } != 0 {
            let r = info.monitorInfo.rcMonitor;
            found.push((
                text(&info.szDevice),
                Rect(r.left, r.top, r.right, r.bottom),
                info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            ));
        }
        1
    }

    /// The monitors as Windows lays them out now, in physical pixels, left to right.
    pub fn connected() -> Vec<Monitor> {
        let names = names();
        let mut found: Vec<(String, Rect, bool)> = Vec::new();
        unsafe {
            EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut found as *mut _ as LPARAM);
        }
        let mut monitors: Vec<Monitor> = found
            .into_iter()
            .map(|(gdi, rect, primary)| {
                let name = names.get(&gdi).cloned().unwrap_or_else(|| gdi.trim_start_matches("\\\\.\\").to_string());
                Monitor { name, rect, primary }
            })
            .collect();
        number_twins(&mut monitors);
        monitors
    }

    thread_local! {
        static ON_CHANGE: RefCell<Option<Box<dyn Fn()>>> = RefCell::new(None);
    }

    /// Keeps the listening window until dropped.
    pub struct Watch(HWND);

    impl Drop for Watch {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    /// Calls `changed` when a monitor is connected or removed, or its
    /// resolution, place or main display role changes. The call arrives on
    /// this thread, through its message loop.
    pub fn watch(changed: impl Fn() + 'static) -> Option<Watch> {
        ON_CHANGE.with(|c| *c.borrow_mut() = Some(Box::new(changed)));
        let class: Vec<u16> = "beatframe-displays".encode_utf16().chain(Some(0)).collect();
        // Windows sends the change to top-level windows only, so this is one,
        // never shown, rather than a message-only window.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(on_message);
            wc.hInstance = instance;
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            eprintln!("monitors: cannot watch for display changes");
            return None;
        }
        Some(Watch(hwnd))
    }

    unsafe extern "system" fn on_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_DISPLAYCHANGE {
            ON_CHANGE.with(|c| {
                if let Some(f) = &*c.borrow() {
                    f();
                }
            });
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Monitor;

    pub struct Watch;

    pub fn connected() -> Vec<Monitor> {
        Vec::new()
    }

    pub fn watch(_changed: impl Fn() + 'static) -> Option<Watch> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, left: i32, primary: bool) -> Monitor {
        Monitor { name: name.into(), rect: Rect(left, 0, left + 1920, 1080), primary }
    }

    fn names(lit: &[&Monitor]) -> Vec<String> {
        lit.iter().map(|m| m.name.clone()).collect()
    }

    #[test]
    fn two_monitors_with_one_name_get_numbers_left_to_right() {
        let mut ms = vec![monitor("Same", 1920, false), monitor("Built-in display", -1920, true), monitor("Same", 0, false)];
        number_twins(&mut ms);
        let got: Vec<(&str, i32)> = ms.iter().map(|m| (m.name.as_str(), m.rect.0)).collect();
        assert_eq!(got, [("Built-in display", -1920), ("Same", 0), ("Same (2)", 1920)]);
    }

    #[test]
    fn the_default_lights_the_main_display_only() {
        let ms = [monitor("Left", -1920, false), monitor("Main", 0, true), monitor("Right", 1920, false)];
        let default = crate::settings::Settings::default().monitors;
        assert_eq!(names(&lit(&default, &ms)), ["Main"]);
        let both = vec![PRIMARY.to_string(), "Right".into()];
        assert_eq!(names(&lit(&both, &ms)), ["Main", "Right"]);
        // The main display named outright as well is still one monitor.
        let twice = vec![PRIMARY.to_string(), "Main".into()];
        assert_eq!(names(&lit(&twice, &ms)), ["Main"]);
        // Only a monitor that is not connected: the main display stands in.
        let gone = vec!["Projector".to_string()];
        assert_eq!(names(&lit(&gone, &ms)), ["Main"]);
        assert!(lit(&default, &[]).is_empty());
    }

    #[test]
    fn choosing_a_monitor_keeps_the_others() {
        let ms = [monitor("Main", 0, true), monitor("Right", 1920, false)];
        let default = vec![PRIMARY.to_string()];
        let both = choose(&default, &ms, &ms[1], true);
        assert_eq!(both, [PRIMARY, "Right"]);
        assert_eq!(names(&lit(&both, &ms)), ["Main", "Right"]);
        assert_eq!(choose(&both, &ms, &ms[0], false), ["Right"]);
        // Checked again, the main display goes in as "primary", not by its name.
        assert_eq!(choose(&["Right".to_string()], &ms, &ms[0], true), ["Right", PRIMARY]);
        // A monitor chosen earlier and unplugged now keeps its place.
        let with_gone = vec!["Projector".to_string(), PRIMARY.into()];
        assert_eq!(choose(&with_gone, &ms, &ms[1], true), ["Projector", PRIMARY, "Right"]);
        // The main display lit only because nothing chosen is connected stays lit.
        let gone = vec!["Projector".to_string()];
        let added = choose(&gone, &ms, &ms[1], true);
        assert_eq!(names(&lit(&added, &ms)), ["Main", "Right"]);
    }
}
