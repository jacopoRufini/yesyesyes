//! macOS only: a menu bar app has no Dock icon, so its window must be brought forward by hand.

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;

#[allow(deprecated)]
pub fn activate() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}
