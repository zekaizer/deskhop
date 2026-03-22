// FFI exports — all #[no_mangle] pub extern "C" functions callable from C.
// Split into sub-modules by domain for maintainability.

mod checksum;
mod handlers;
mod hid;
mod kbd_process;
mod keyboard;
mod mouse;
mod packet;
mod screensaver;
mod state;
