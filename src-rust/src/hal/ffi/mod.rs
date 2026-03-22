// FFI exports — all #[no_mangle] pub extern "C" functions callable from C.
// Split into sub-modules by domain for maintainability.

mod checksum;
mod handlers;
mod hid;
mod hid_parser_ffi;
mod kbd_extract;
mod kbd_process;
mod keyboard;
mod mouse;
mod mouse_process;
mod packet;
mod screen_switch;
mod screensaver;
mod state;
mod tasks;
