mod app;
mod autooff;
mod backend;
mod core;
mod gate_server;
mod llm_bridge;
mod llm_server;
mod memory;
mod icons;
mod login;
#[cfg(target_os = "macos")]
mod macos_app;
mod state;
mod tray;

use crate::core::ConfigStore;
use eframe::egui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, dir] = &args[..]
        && flag == "--iconset"
    {
        // Used by scripts/build-macos-app.sh for the icon of the app bundle.
        if let Err(e) = icons::write_iconset(std::path::Path::new(dir)) {
            eprintln!("yesyesyes: cannot write the icons: {e}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().any(|a| a == "--dump") {
        std::process::exit(backend::dump());
    }
    let state = state::State::new(ConfigStore::default_location());
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("yesyesyes")
            .with_inner_size([1200.0, 780.0])
            .with_icon(icons::app())
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
