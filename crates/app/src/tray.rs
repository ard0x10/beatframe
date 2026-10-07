//! The notification area icon and its menu.

use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::icon;
use crate::monitors::{self, Monitor};

pub enum Action {
    Toggle,
    /// Light this monitor, or stop lighting it.
    Monitor(Monitor, bool),
    OpenSettings,
    Quit,
}

/// One line of the monitor section, as the menu shows it.
#[derive(Clone, PartialEq)]
struct Row {
    monitor: Monitor,
    lit: bool,
    /// False for the last monitor with the light, which keeps it.
    enabled: bool,
}

/// The monitor section: none with a single monitor, one row per monitor
/// otherwise, worded and locked as in the settings window.
fn rows(chosen: &[String], connected: &[Monitor]) -> Vec<Row> {
    if connected.len() < 2 {
        return Vec::new();
    }
    let lit: Vec<&Monitor> = monitors::lit(chosen, connected);
    connected
        .iter()
        .map(|m| {
            let on = lit.contains(&m);
            Row { monitor: m.clone(), lit: on, enabled: !(on && lit.len() == 1) }
        })
        .collect()
}

fn label(m: &Monitor) -> String {
    if m.primary { format!("{} (main display)", m.name) } else { m.name.clone() }
}

pub struct Tray {
    icon: TrayIcon,
    enabled: CheckMenuItem,
    settings: MenuId,
    quit: MenuId,
    rows: Vec<Row>,
    row_ids: Vec<MenuId>,
}

impl Tray {
    pub fn new(enabled: bool) -> Tray {
        let icon = TrayIconBuilder::new()
            .with_tooltip("BeatFrame")
            .with_icon(Icon::from_rgba(icon::rgba(ICON_SIZE), ICON_SIZE, ICON_SIZE).expect("icon size matches its pixels"))
            .build()
            .expect("creating the tray icon");
        let mut tray = Tray {
            icon,
            enabled: CheckMenuItem::new("On", true, enabled, None),
            settings: MenuId::new(""),
            quit: MenuId::new(""),
            rows: Vec::new(),
            row_ids: Vec::new(),
        };
        tray.build();
        tray
    }

    /// Puts the whole menu together again, for a new set of monitor rows.
    fn build(&mut self) {
        let settings = MenuItem::new("Settings…", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let rows: Vec<CheckMenuItem> =
            self.rows.iter().map(|r| CheckMenuItem::new(label(&r.monitor), r.enabled, r.lit, None)).collect();
        let separator = PredefinedMenuItem::separator();
        let mut items: Vec<&dyn IsMenuItem> = vec![&self.enabled];
        if !rows.is_empty() {
            items.push(&separator);
            items.extend(rows.iter().map(|r| r as &dyn IsMenuItem));
        }
        let separator_2 = PredefinedMenuItem::separator();
        let separator_3 = PredefinedMenuItem::separator();
        items.extend([&separator_2 as &dyn IsMenuItem, &settings, &separator_3, &quit]);
        let menu = Menu::new();
        for item in items {
            menu.append(item).expect("building the tray menu");
        }
        self.icon.set_menu(Some(Box::new(menu)));
        self.settings = settings.id().clone();
        self.quit = quit.id().clone();
        self.row_ids = rows.iter().map(|r| r.id().clone()).collect();
    }

    pub fn action(&self, id: &MenuId) -> Option<Action> {
        if id == self.enabled.id() {
            Some(Action::Toggle)
        } else if *id == self.settings {
            Some(Action::OpenSettings)
        } else if *id == self.quit {
            Some(Action::Quit)
        } else {
            let i = self.row_ids.iter().position(|r| r == id)?;
            let row = &self.rows[i];
            Some(Action::Monitor(row.monitor.clone(), !row.lit))
        }
    }

    /// Shows the switches and the monitors as they are now. A click has
    /// already flipped its own check mark, so every mark is set again.
    pub fn show(&mut self, enabled: bool, chosen: &[String], connected: &[Monitor]) {
        self.enabled.set_checked(enabled);
        let rows = rows(chosen, connected);
        if rows != self.rows {
            self.rows = rows;
            self.build();
        }
    }
}

pub const ICON_SIZE: u32 = 32;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitors::PRIMARY;

    fn monitor(name: &str, x: i32, primary: bool) -> Monitor {
        Monitor { name: name.into(), rect: crate::fullscreen::Rect(x, 0, x + 1920, 1080), primary }
    }

    #[test]
    fn one_monitor_gets_no_section() {
        assert!(rows(&[PRIMARY.into()], &[monitor("Laptop", 0, true)]).is_empty());
        assert!(rows(&[PRIMARY.into()], &[]).is_empty());
    }

    #[test]
    fn each_monitor_gets_a_row_and_the_last_lit_one_is_locked() {
        let connected = [monitor("Laptop", 0, true), monitor("DELL", 1920, false)];
        let only_main = rows(&[PRIMARY.into()], &connected);
        let state: Vec<(&str, bool, bool)> = only_main.iter().map(|r| (r.monitor.name.as_str(), r.lit, r.enabled)).collect();
        assert_eq!(state, [("Laptop", true, false), ("DELL", false, true)]);

        let both = rows(&[PRIMARY.into(), "DELL".into()], &connected);
        assert!(both.iter().all(|r| r.lit && r.enabled), "two lit, either may go dark");

        // A chosen monitor that is unplugged leaves the main display lit, and locked.
        let gone = rows(&["Other".into()], &connected);
        assert!(gone[0].lit && !gone[0].enabled && !gone[1].lit);
        assert_eq!(label(&connected[0]), "Laptop (main display)");
        assert_eq!(label(&connected[1]), "DELL");
    }
}
