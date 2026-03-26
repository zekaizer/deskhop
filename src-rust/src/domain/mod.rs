// Core domain logic — pure Rust, no HAL dependencies.
// Rule: `use crate::hal` is forbidden in this layer.

pub mod actions;
pub mod constants;
pub mod crc;
pub mod dispatch;
pub mod hid_classify;
pub mod hid_parser;
pub mod kbd_extract;
pub mod hid_report;
pub mod hid_routing;
pub mod hotkey_handlers;
#[cfg(test)]
mod integration_tests;
pub mod kbd_state;
pub mod keyboard;
pub mod blink;
pub mod mouse;
pub mod mouse_logic;
pub mod msg_handlers;
pub mod packet;
pub mod screensaver;
pub mod structs;
