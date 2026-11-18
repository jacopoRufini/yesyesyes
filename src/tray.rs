//! The menu bar / system tray icon, the same code on every OS (tray-icon; ksni over D-Bus on Linux).

use crate::icons;
use crate::state::State;
use std::cell::Cell;
use tray_icon::menu::{IconMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct Tray {
    icon: TrayIcon,
    toggle: IconMenuItem,
    shown: Cell<Option<bool>>,
}

impl Tray {
    pub fn new(state: State, ctx: eframe::egui::Context) -> Result<Tray, String> {
        let toggle = IconMenuItem::new("Pause", true, None, None);
        let configure = IconMenuItem::new("Configure…", true, None, None);
        set_menu_icon(&configure, icons::gear());
        let quit = MenuItem::new("Quit yesyesyes", true, None);
        let menu = Menu::new();
        menu.append_items(&[&toggle, &configure, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| e.to_string())?;

        let ids = (toggle.id().clone(), configure.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let (toggle, configure, quit) = &ids;
            if event.id == *toggle {
                state.update(|c| c.enabled = !c.enabled);
            } else if event.id == *configure {
                state.lock().show_window = true;
            } else if event.id == *quit {
                std::process::exit(0);
            }
            ctx.request_repaint();
        }));

        let builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("yesyesyes");
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icons::tray(true));
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(icons::tray(true));
        let icon = builder.build().map_err(|e| e.to_string())?;
        Ok(Tray { icon, toggle, shown: Cell::new(None) })
    }

    /// Shows the state in the icon and in the first menu item; only touches them when it changed.
    pub fn sync(&self, enabled: bool) {
        if self.shown.get() == Some(enabled) {
            return;
        }
        self.shown.set(Some(enabled));
        let (text, item_icon) = if enabled { ("Pause", icons::pause()) } else { ("Start", icons::play()) };
        self.toggle.set_text(text);
        set_menu_icon(&self.toggle, item_icon);
        set_tray_icon(&self.icon, icons::tray(enabled));
        let _ = self.icon.set_tooltip(Some(if enabled { "yesyesyes" } else { "yesyesyes (paused)" }));
    }
}

fn set_menu_icon(item: &IconMenuItem, icon: tray_icon::menu::Icon) {
    #[cfg(target_os = "macos")]
    item.set_icon_templated(Some(icon));
    #[cfg(not(target_os = "macos"))]
    item.set_icon(Some(icon));
}

fn set_tray_icon(tray: &TrayIcon, icon: Icon) {
    #[cfg(target_os = "macos")]
    let _ = tray.set_icon_templated(Some(icon));
    #[cfg(not(target_os = "macos"))]
    let _ = tray.set_icon(Some(icon));
}
