// Application logic modules — pure logic, no direct C/hardware dependencies.

pub mod constants;
pub mod crc;
pub mod dispatch;
pub mod extract;
pub mod handlers;
pub mod hid_parser;
#[cfg(test)]
mod integration_tests;
pub mod hotkey_handlers;
pub mod kbd_state;
pub mod hid_report;
pub mod keyboard;
pub mod mouse;
pub mod mouse_logic;
pub mod msg_handlers;
pub mod packet;
pub mod router;
pub mod screensaver;
pub mod structs;
pub mod usb;
