//! The notification area icon and its menu.

use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::icon;

pub enum Action {
    Toggle,
    OpenSettings,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    enabled: CheckMenuItem,
    settings: MenuId,
    quit: MenuId,
}

impl Tray {
    pub fn new(enabled: bool) -> Tray {
        let toggle = CheckMenuItem::new("On", true, enabled, None);
        let settings = MenuItem::new("Settings…", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        for item in [
            &toggle as &dyn tray_icon::menu::IsMenuItem,
            &settings,
            &PredefinedMenuItem::separator(),
            &quit,
        ] {
            menu.append(item).expect("building the tray menu");
        }
        let icon = TrayIconBuilder::new()
            .with_tooltip("BeatFrame")
            .with_icon(Icon::from_rgba(icon::rgba(ICON_SIZE), ICON_SIZE, ICON_SIZE).expect("icon size matches its pixels"))
            .with_menu(Box::new(menu))
            .build()
            .expect("creating the tray icon");
        Tray { _icon: icon, settings: settings.id().clone(), quit: quit.id().clone(), enabled: toggle }
    }

    pub fn action(&self, id: &MenuId) -> Option<Action> {
        if id == self.enabled.id() {
            Some(Action::Toggle)
        } else if *id == self.settings {
            Some(Action::OpenSettings)
        } else if *id == self.quit {
            Some(Action::Quit)
        } else {
            None
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.set_checked(on);
    }
}

pub const ICON_SIZE: u32 = 32;
