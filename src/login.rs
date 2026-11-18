//! Launch at login, through the usual place of each OS (LaunchAgent, Run registry key, autostart).

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

fn launcher() -> Option<AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name("yesyesyes").set_app_path(exe.to_str()?);
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
    #[cfg(windows)]
    builder.set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    builder.build().ok()
}

pub fn is_enabled() -> bool {
    launcher().is_some_and(|l| l.is_enabled().unwrap_or(false))
}

pub fn set(enabled: bool) -> Result<(), String> {
    let launcher = launcher().ok_or("Cannot find the yesyesyes executable")?;
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| e.to_string())
}
