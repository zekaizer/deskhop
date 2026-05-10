// Peer debug log ring buffer (DH_DEBUG only).
//
// Multi-producer / single-consumer byte ring guarded by a Pico SDK hardware
// spinlock (peer_log_lock_acquire/release exposed by hal_shim.c). Producers:
// dh_debug_printf (C, including TinyUSB's CFG_TUSB_DEBUG_PRINTF), Rust
// traceln!(), and the packet receiver on B (push_received). Consumer: a
// single drain task on Core0 (added in a later commit).
//
// Each line gets a "[A] " or "[B] " prefix injected at line-start so the
// receiving board can attribute lines once they reach its CDC.

#[cfg(any(feature = "dh_debug", test))]
use crate::domain::constants::{OUTPUT_A, OUTPUT_B};
#[cfg(feature = "dh_debug")]
use crate::domain::structs::GLOBAL_CFG;

const RING_SIZE: usize = 1024;
#[cfg(any(feature = "dh_debug", test))]
const PREFIX_A: &[u8; 4] = b"[A] ";
#[cfg(any(feature = "dh_debug", test))]
const PREFIX_B: &[u8; 4] = b"[B] ";
const OVERFLOW_MSG: &[u8] = b"[peer_log overflow]\n";

// ============================================================
// Ring (pure logic — host-testable, no spinlock dependency)
// ============================================================

pub struct Ring {
    buf: [u8; RING_SIZE],
    head: usize,
    tail: usize,
    used: usize,
    overflow: bool,
    last_was_newline: bool,
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

impl Ring {
    pub const fn new() -> Self {
        Self {
            buf: [0; RING_SIZE],
            head: 0,
            tail: 0,
            used: 0,
            overflow: false,
            last_was_newline: true,
        }
    }

    fn write_byte(&mut self, b: u8) -> bool {
        if self.used >= RING_SIZE {
            self.overflow = true;
            return false;
        }
        self.buf[self.tail] = b;
        self.tail = (self.tail + 1) % RING_SIZE;
        self.used += 1;
        true
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if !self.write_byte(b) {
                return;
            }
        }
    }

    fn drain_overflow_sentinel_if_needed(&mut self) {
        if self.overflow && RING_SIZE - self.used >= OVERFLOW_MSG.len() {
            for &b in OVERFLOW_MSG {
                let _ = self.write_byte(b);
            }
            self.last_was_newline = true;
            self.overflow = false;
        }
    }

    /// Push locally-produced bytes — injects role prefix at each line start.
    pub fn push_local(&mut self, bytes: &[u8], prefix: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.drain_overflow_sentinel_if_needed();
        for &b in bytes {
            if self.last_was_newline {
                self.write_bytes(prefix);
            }
            if !self.write_byte(b) {
                break;
            }
            self.last_was_newline = b == b'\n';
        }
    }

    /// Push bytes received over UART from the peer board. The sender already
    /// embedded its role prefix — we only strip NUL padding.
    pub fn push_received(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == 0 {
                continue;
            }
            if !self.write_byte(b) {
                break;
            }
            self.last_was_newline = b == b'\n';
        }
    }

    pub fn pop_byte(&mut self) -> Option<u8> {
        if self.used == 0 {
            return None;
        }
        let b = self.buf[self.head];
        self.head = (self.head + 1) % RING_SIZE;
        self.used -= 1;
        Some(b)
    }

    /// Pop up to 8 bytes for a DebugLog packet. If `allow_partial` is false,
    /// returns None when fewer than 8 bytes are queued (waiting for a full
    /// chunk). Partial packets are NUL-padded.
    pub fn pop_chunk_8(&mut self, allow_partial: bool) -> Option<[u8; 8]> {
        if self.used == 0 {
            return None;
        }
        if !allow_partial && self.used < 8 {
            return None;
        }
        let mut out = [0u8; 8];
        for slot in out.iter_mut() {
            match self.pop_byte() {
                Some(b) => *slot = b,
                None => break,
            }
        }
        Some(out)
    }

    pub fn used(&self) -> usize {
        self.used
    }
}

// ============================================================
// Spinlock guard (cross-core MPSC). Only compiled when dh_debug is on.
// ============================================================

#[cfg(all(feature = "dh_debug", target_os = "none"))]
extern "C" {
    fn peer_log_lock_acquire() -> u32;
    fn peer_log_lock_release(saved: u32);
}

#[cfg(all(feature = "dh_debug", not(target_os = "none")))]
unsafe fn peer_log_lock_acquire() -> u32 {
    0
}
#[cfg(all(feature = "dh_debug", not(target_os = "none")))]
unsafe fn peer_log_lock_release(_saved: u32) {}

#[cfg(feature = "dh_debug")]
struct LockGuard(u32);
#[cfg(feature = "dh_debug")]
impl Drop for LockGuard {
    fn drop(&mut self) {
        unsafe { peer_log_lock_release(self.0) };
    }
}
#[cfg(feature = "dh_debug")]
fn lock() -> LockGuard {
    LockGuard(unsafe { peer_log_lock_acquire() })
}

// ============================================================
// Global ring + push/pop API (DH_DEBUG only)
// ============================================================

#[cfg(feature = "dh_debug")]
static mut RING: Ring = Ring::new();

#[cfg(feature = "dh_debug")]
fn role_prefix() -> &'static [u8] {
    let role = unsafe { (*core::ptr::addr_of!(GLOBAL_CFG)).board_role };
    if role == OUTPUT_B {
        PREFIX_B
    } else if role == OUTPUT_A {
        PREFIX_A
    } else {
        // Pre-role default: treat as A (board_role hasn't been written yet).
        PREFIX_A
    }
}

#[cfg(feature = "dh_debug")]
pub fn push(bytes: &[u8]) {
    let prefix = role_prefix();
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(RING)).push_local(bytes, prefix) };
}

#[cfg(feature = "dh_debug")]
pub fn push_received(bytes: &[u8]) {
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(RING)).push_received(bytes) };
}

#[cfg(feature = "dh_debug")]
pub fn pop_byte() -> Option<u8> {
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(RING)).pop_byte() }
}

#[cfg(feature = "dh_debug")]
pub fn pop_chunk_8(allow_partial: bool) -> Option<[u8; 8]> {
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(RING)).pop_chunk_8(allow_partial) }
}

#[cfg(not(feature = "dh_debug"))]
pub fn push(_bytes: &[u8]) {}
#[cfg(not(feature = "dh_debug"))]
pub fn push_received(_bytes: &[u8]) {}
#[cfg(not(feature = "dh_debug"))]
pub fn pop_byte() -> Option<u8> {
    None
}
#[cfg(not(feature = "dh_debug"))]
pub fn pop_chunk_8(_allow_partial: bool) -> Option<[u8; 8]> {
    None
}

// ============================================================
// C-callable FFI export
// ============================================================

/// Push raw bytes from C. Called from dh_debug_printf in hal_util.c.
///
/// # Safety
/// `data` must be either null or point to at least `len` valid bytes.
/// Safe to call from any context including ISR (uses spinlock + IRQ disable).
#[no_mangle]
pub unsafe extern "C" fn peer_log_push(data: *const u8, len: usize) {
    if data.is_null() || len == 0 {
        return;
    }
    let slice = core::slice::from_raw_parts(data, len);
    push(slice);
}

// ============================================================
// Tests (host target — exercises Ring directly)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring_pop_returns_none() {
        let mut r = Ring::new();
        assert_eq!(r.pop_byte(), None);
        assert_eq!(r.pop_chunk_8(true), None);
    }

    fn drain(r: &mut Ring, out: &mut [u8]) -> usize {
        let mut n = 0;
        while n < out.len() {
            match r.pop_byte() {
                Some(b) => {
                    out[n] = b;
                    n += 1;
                }
                None => break,
            }
        }
        n
    }

    #[test]
    fn local_push_injects_prefix_at_line_start() {
        let mut r = Ring::new();
        r.push_local(b"hello\n", PREFIX_A);
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..n], b"[A] hello\n");
    }

    #[test]
    fn local_push_multiple_lines_each_prefixed() {
        let mut r = Ring::new();
        r.push_local(b"one\ntwo\n", PREFIX_B);
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..n], b"[B] one\n[B] two\n");
    }

    #[test]
    fn local_push_continues_unterminated_line() {
        let mut r = Ring::new();
        r.push_local(b"part1 ", PREFIX_A);
        r.push_local(b"part2\n", PREFIX_A);
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        // Second push must NOT add prefix because last byte was not '\n'.
        assert_eq!(&out[..n], b"[A] part1 part2\n");
    }

    #[test]
    fn received_push_strips_nul_padding() {
        let mut r = Ring::new();
        r.push_received(b"[A] x\n\0\0\0");
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..n], b"[A] x\n");
    }

    #[test]
    fn pop_chunk_8_full_packet() {
        let mut r = Ring::new();
        // Push raw via received to skip prefix logic.
        r.push_received(b"abcdefgh");
        let chunk = r.pop_chunk_8(false).unwrap();
        assert_eq!(&chunk, b"abcdefgh");
    }

    #[test]
    fn pop_chunk_8_holds_until_full_when_not_partial() {
        let mut r = Ring::new();
        r.push_received(b"abc");
        assert_eq!(r.pop_chunk_8(false), None);
        // Allow partial → flushes with NUL padding.
        let chunk = r.pop_chunk_8(true).unwrap();
        assert_eq!(&chunk, b"abc\0\0\0\0\0");
    }

    #[test]
    fn overflow_sets_flag_and_emits_sentinel() {
        let mut r = Ring::new();
        // Fill the ring exactly using repeated 1-byte pushes.
        for _ in 0..RING_SIZE {
            r.push_received(b"x");
        }
        assert_eq!(r.used(), RING_SIZE);
        // One more byte triggers overflow.
        r.push_received(b"y");
        assert!(r.overflow);
        // Drain everything.
        while r.pop_byte().is_some() {}
        // Next local push must emit the sentinel before its own bytes.
        r.push_local(b"hi\n", PREFIX_A);
        let mut out = [0u8; 64];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..OVERFLOW_MSG.len()], OVERFLOW_MSG);
        assert!(n > OVERFLOW_MSG.len());
        assert!(!r.overflow);
    }

    #[test]
    fn push_local_empty_no_op() {
        let mut r = Ring::new();
        r.push_local(b"", PREFIX_A);
        assert_eq!(r.used(), 0);
    }

    #[test]
    fn pop_byte_wraparound() {
        let mut r = Ring::new();
        // Push more than half so head+tail exercise wrap when popped/pushed.
        for _ in 0..(RING_SIZE - 2) {
            r.push_received(b"a");
        }
        for _ in 0..(RING_SIZE - 2) {
            r.pop_byte();
        }
        r.push_received(b"bc");
        assert_eq!(r.pop_byte(), Some(b'b'));
        assert_eq!(r.pop_byte(), Some(b'c'));
        assert_eq!(r.pop_byte(), None);
    }
}
