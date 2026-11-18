mod app;
mod backend;
mod core;
mod gate_server;
mod icons;
mod login;
#[cfg(target_os = "macos")]
mod macos_app;
mod state;
mod tray;

use crate::core::ConfigStore;
use eframe::egui;

fn main() {
    if std::env::args().any(|a| a == "--dump") {
        std::process::exit(backend::dump());
    }
    let state = state::State::new(ConfigStore::default_location());
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("yesyesyes")
            .with_inner_size([560.0, 520.0])
            .with_visible(false),
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder.with_activation_policy(ActivationPolicy::Accessory);
        }));
    }
    if let Err(e) = eframe::run_native("yesyesyes", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, state))))) {
        eprintln!("yesyesyes could not start: {e}");
        std::process::exit(1);
    }
}
