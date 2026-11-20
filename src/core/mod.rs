//! Portable logic: rules, matching and the click engine. No platform code here.

pub mod clicklog;
mod config;
mod engine;
pub mod gate;
pub mod llm;
mod matcher;
pub mod requests;
pub mod transcript;

pub use config::{AddResult, Channel, Config, ConfigStore, LogSettings, Rule};
pub use engine::{ClickEvent, ClickResult, Engine, chats};
pub use matcher::{Hit, Node, Outcome, find_all, press};
