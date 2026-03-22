// Tracing infrastructure for debug builds.
//
// When the `trace` feature is enabled, trace!() writes raw bytes
// to UART via the C HAL shim. When disabled, it compiles to nothing.

#[cfg(feature = "trace")]
extern "C" {
    pub fn hal_trace_write(buf: *const u8, len: u32);
}

/// Write a static string to the trace output (UART).
/// Compiles to zero code when `trace` feature is disabled.
#[macro_export]
macro_rules! trace {
    ($msg:expr) => {
        #[cfg(feature = "trace")]
        {
            let bytes = $msg.as_bytes();
            unsafe { $crate::trace::hal_trace_write(bytes.as_ptr(), bytes.len() as u32) };
        }
    };
}

/// Write a static string followed by a newline.
#[macro_export]
macro_rules! traceln {
    ($msg:expr) => {
        $crate::trace!($msg);
        $crate::trace!("\r\n");
    };
}
