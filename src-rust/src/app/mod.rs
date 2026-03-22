// Application logic modules — pure logic, no direct C/hardware dependencies.

pub mod constants;
pub mod crc;
pub mod dispatch;
pub mod extract;
pub mod handlers;
pub mod hid_parser;
pub mod hid_report;
pub mod keyboard;
pub mod mouse;
pub mod mouse_logic;
pub mod packet;
pub mod screensaver;
pub mod state;
pub mod structs;
pub mod usb;
