//! One backend per OS: where the notifications are and how to reach them.

use crate::core::Node;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::Platform;
#[cfg(target_os = "macos")]
pub use macos::Platform;
#[cfg(windows)]
pub use windows::Platform;

/// Whether the backend can read notifications, and if not what the user has to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Access {
    Unknown,
    Ready,
    Missing(String),
}

pub trait Backend {
    type Node: Node;
    /// `prompt`: ask the system for the permission if it supports asking (macOS).
    fn access(&self, prompt: bool) -> Access;
    /// The elements that hold the notifications currently on screen.
    fn roots(&self) -> Vec<Self::Node>;
}

/// `yesyesyes --dump`: prints what the backend sees, to find app names and button labels.
pub fn dump() -> i32 {
    let platform = Platform::new();
    if let Access::Missing(hint) = platform.access(true) {
        eprintln!("{hint}");
        return 1;
    }
    let roots = platform.roots();
    if roots.is_empty() {
        println!("No notifications found. Show one and run this again while it is on screen.");
    }
    for root in &roots {
        print_node(root, 0);
    }
    0
}

fn print_node<N: Node>(node: &N, depth: usize) {
    let kind = if node.is_button() { "button" } else { "element" };
    let actions = node.named_actions();
    let actions = if actions.is_empty() { String::new() } else { format!(" actions {actions:?}") };
    println!("{}{kind} {:?}{actions}", "  ".repeat(depth), node.labels());
    for child in node.children() {
        print_node(&child, depth + 1);
    }
}
