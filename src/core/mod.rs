//! Portable logic: rules, matching and the click engine. No platform code here.

pub mod clicklog;
mod config;
mod engine;
pub mod gate;
mod matcher;
pub mod requests;

pub use config::{AddResult, Channel, Config, ConfigStore, LogSettings, Rule};
pub use engine::{ClickEvent, ClickResult, Engine};
pub use matcher::{Hit, Node, Outcome, find_all, press};
