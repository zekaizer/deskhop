// FFI exports — all #[no_mangle] pub extern "C" functions callable from C.
// Split into sub-modules by domain for maintainability.

mod api_config;
mod checksum;
mod extract_data;
mod fw_handlers;
mod hid;
mod hotkey_dispatch;
mod hid_parser_ffi;
mod kbd_extract;
mod kbd_process;
mod keyboard;
mod mouse_process;
mod msg_dispatch;
mod packet;
mod screen_switch;
mod screensaver;
mod state;
mod tasks;
