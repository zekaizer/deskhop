// HAL layer — C↔Rust boundary. All FFI declarations and exports live here.

pub mod device;
#[cfg(not(test))]
pub mod ffi;
#[cfg(test)]
pub mod mock;
#[cfg(not(test))]
pub mod pico;
pub mod scheduler;
pub mod trace;
pub mod traits;
