//! macOS only: a menu bar app has no Dock icon, so its window must be brought forward by hand.
//! While the window is open the app shows in the Dock (and the app switcher), like other apps.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSWindowCollectionBehavior};

/// The window opens on the desktop (Space) you are on, instead of switching to the one it was on.
pub fn open_on_this_desktop() {
    if let Some(mtm) = MainThreadMarker::new() {
        for window in NSApplication::sharedApplication(mtm).windows().iter() {
            window.setCollectionBehavior(window.collectionBehavior() | NSWindowCollectionBehavior::MoveToActiveSpace);
        }
    }
}

pub fn show_in_dock(show: bool) {
    if let Some(mtm) = MainThreadMarker::new() {
        let policy = if show { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
        NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
    }
}

#[allow(deprecated)]
pub fn activate() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}
