// FFI exports — all #[no_mangle] pub extern "C" functions callable from C.
// Split into sub-modules by domain for maintainability.

mod callbacks;
mod config;
mod tasks;
mod util;
