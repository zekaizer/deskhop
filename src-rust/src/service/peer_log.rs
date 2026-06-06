// Peer debug log ring buffer (DH_DEBUG only).
//
// Two rings per board:
//   OUT_RING — local logs + logs received from the peer; drained to this
//              board's own CDC. Sized at LINK TIME to fill the RAM left after
//              .bss (linker symbols __log_ring_start/__log_ring_end), so it
//              retains as much history as the free RAM allows.
//   TX_RING  — local logs only; drained over UART to the peer board. Small
//              fixed buffer (drained every tick), never carries received logs
//              so peer↔peer forwarding can't loop.
//
// push_local writes BOTH rings; push_received writes only OUT_RING. Both boards
// run the same drain (TX→UART, OUT→CDC), so EITHER board's CDC shows the full
// [A]+[B] stream — capture from whichever board sits on a readable host.
//
// Multi-producer / single-consumer, guarded by a Pico SDK hardware spinlock
// (peer_log_lock_acquire/release in hal_shim.c). Producers: dh_debug_printf (C,
// incl. TinyUSB CFG_TUSB_DEBUG_PRINTF), Rust traceln!(), and the packet receiver
// (push_received). Each line gets a "[A] "/"[B] " prefix at line-start.

#[cfg(feature = "dh_debug")]
use crate::domain::constants::{OUTPUT_A, OUTPUT_B};
#[cfg(feature = "dh_debug")]
use crate::domain::structs::GLOBAL_CFG;

#[cfg(any(feature = "dh_debug", test))]
const PREFIX_A: &[u8; 4] = b"[A] ";
#[cfg(any(feature = "dh_debug", test))]
const PREFIX_B: &[u8; 4] = b"[B] ";
const OVERFLOW_MSG: &[u8] = b"[peer_log overflow]\r\n";

/// Fixed size of the per-board UART forward ring (drained to the peer every
/// 1kHz tick, so it only needs to absorb a single burst).
#[cfg(feature = "dh_debug")]
const TX_RING_SIZE: usize = 4096;

// ============================================================
// Ring (pure logic — host-testable). Backed by a caller-provided buffer
// (pointer + capacity) so the OUT ring can be sized at link time.
// ============================================================

pub struct Ring {
    buf: *mut u8,
    cap: usize,
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
    /// Empty, unbacked ring (cap 0). All pushes are no-ops until `init`.
    pub const fn new() -> Self {
        Self {
            buf: core::ptr::null_mut(),
            cap: 0,
            head: 0,
            tail: 0,
            used: 0,
            overflow: false,
            last_was_newline: true,
        }
    }

    /// Attach a backing buffer and reset state. `buf` must be valid for `cap`
    /// bytes for the lifetime of all subsequent ring use.
    pub fn init(&mut self, buf: *mut u8, cap: usize) {
        self.buf = buf;
        self.cap = cap;
        self.head = 0;
        self.tail = 0;
        self.used = 0;
        self.overflow = false;
        self.last_was_newline = true;
    }

    fn write_byte(&mut self, b: u8) -> bool {
        if self.cap == 0 {
            return false; // unbacked — drop silently (not an overflow)
        }
        if self.used >= self.cap {
            self.overflow = true;
            return false;
        }
        unsafe { *self.buf.add(self.tail) = b };
        self.tail = (self.tail + 1) % self.cap;
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
        if self.overflow && self.cap != 0 && self.cap - self.used >= OVERFLOW_MSG.len() {
            for &b in OVERFLOW_MSG {
                let _ = self.write_byte(b);
            }
            self.last_was_newline = true;
            self.overflow = false;
        }
    }

    /// Push locally-produced bytes — injects role prefix at each line start and
    /// translates bare LF to CRLF for serial-terminal display (matches the C
    /// dh_debug_printf path; the `prev != '\r'` guard avoids doubling CR for
    /// input that already carries it).
    pub fn push_local(&mut self, bytes: &[u8], prefix: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.drain_overflow_sentinel_if_needed();
        let mut prev = 0u8;
        for &b in bytes {
            if self.last_was_newline {
                self.write_bytes(prefix);
            }
            if b == b'\n' && prev != b'\r' && !self.write_byte(b'\r') {
                break;
            }
            if !self.write_byte(b) {
                break;
            }
            self.last_was_newline = b == b'\n';
            prev = b;
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
        let b = unsafe { *self.buf.add(self.head) };
        self.head = (self.head + 1) % self.cap;
        self.used -= 1;
        Some(b)
    }

    /// Pop up to `out.len()` bytes into the slice. Returns count written.
    pub fn pop_into(&mut self, out: &mut [u8]) -> usize {
        let mut n = 0;
        while n < out.len() {
            match self.pop_byte() {
                Some(b) => {
                    out[n] = b;
                    n += 1;
                }
                None => break,
            }
        }
        n
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
// Global rings + push/pop API (DH_DEBUG only)
// ============================================================

#[cfg(feature = "dh_debug")]
static mut OUT_RING: Ring = Ring::new();
#[cfg(feature = "dh_debug")]
static mut TX_RING: Ring = Ring::new();
#[cfg(feature = "dh_debug")]
static mut TX_BUF: [u8; TX_RING_SIZE] = [0; TX_RING_SIZE];

// Link-time-sized OUT ring region (see misc/memory_map.ld).
#[cfg(all(feature = "dh_debug", target_os = "none"))]
extern "C" {
    static __log_ring_start: u8;
    static __log_ring_end: u8;
}

/// Attach backing buffers to the rings. Call once early at boot, before any
/// logging that must be retained. OUT_RING gets the link-time region (free
/// RAM); TX_RING gets the small fixed forward buffer.
///
/// # Safety
/// Call once at boot after peer_log_lock_init(), from a single thread. Relies
/// on the linker-provided __log_ring_start/__log_ring_end bounding valid RAM.
#[cfg(all(feature = "dh_debug", target_os = "none"))]
#[no_mangle]
pub unsafe extern "C" fn peer_log_init() {
    let start = core::ptr::addr_of!(__log_ring_start) as *mut u8;
    let end = core::ptr::addr_of!(__log_ring_end) as usize;
    let cap = end.saturating_sub(start as usize);
    let _g = lock();
    (*core::ptr::addr_of_mut!(OUT_RING)).init(start, cap);
    (*core::ptr::addr_of_mut!(TX_RING)).init(core::ptr::addr_of_mut!(TX_BUF) as *mut u8, TX_RING_SIZE);
}

/// No-op stub for release / host builds.
///
/// # Safety
/// No-op.
#[cfg(not(all(feature = "dh_debug", target_os = "none")))]
#[no_mangle]
pub unsafe extern "C" fn peer_log_init() {}

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
    unsafe {
        // Local logs go to BOTH rings: own CDC (OUT) and the peer (TX).
        (*core::ptr::addr_of_mut!(OUT_RING)).push_local(bytes, prefix);
        (*core::ptr::addr_of_mut!(TX_RING)).push_local(bytes, prefix);
    }
}

#[cfg(feature = "dh_debug")]
pub fn push_received(bytes: &[u8]) {
    // Received peer logs go to OUT only (own CDC) — never re-forwarded (no loop).
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(OUT_RING)).push_received(bytes) };
}

#[cfg(feature = "dh_debug")]
pub fn pop_byte() -> Option<u8> {
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(OUT_RING)).pop_byte() }
}

#[cfg(feature = "dh_debug")]
pub fn pop_chunk_8(allow_partial: bool) -> Option<[u8; 8]> {
    // UART forward path drains the TX ring.
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(TX_RING)).pop_chunk_8(allow_partial) }
}

#[cfg(feature = "dh_debug")]
pub fn pop_into(out: &mut [u8]) -> usize {
    // CDC output path drains the OUT ring.
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(OUT_RING)).pop_into(out) }
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
#[cfg(not(feature = "dh_debug"))]
pub fn pop_into(_out: &mut [u8]) -> usize {
    0
}

// ============================================================
// Drain task — runs at 1kHz on Core0. Both boards do both:
//   TX_RING  → packetize 8-byte chunks → UART (queue_packet) → peer
//   OUT_RING → bulk-pop bytes → own CDC (held while CDC disconnected)
// So either board's CDC shows the full [A]+[B] stream.
// ============================================================

#[cfg(all(feature = "dh_debug", target_os = "none"))]
extern "C" {
    fn peer_log_cdc_connected() -> bool;
    fn peer_log_cdc_write(data: *const u8, len: u32) -> u32;
    fn peer_log_cdc_flush();
}

#[cfg(feature = "dh_debug")]
fn flush_to_uart() {
    while let Some(chunk) = pop_chunk_8(true) {
        unsafe {
            crate::hal::device::queue_packet(
                chunk.as_ptr(),
                crate::domain::constants::PacketType::DebugLog as u8,
                8,
            );
        }
    }
}

#[cfg(all(feature = "dh_debug", target_os = "none"))]
fn flush_to_cdc() {
    if !unsafe { peer_log_cdc_connected() } {
        return;
    }
    let mut buf = [0u8; 64];
    let mut wrote = false;
    loop {
        let n = pop_into(&mut buf);
        if n == 0 {
            break;
        }
        unsafe { peer_log_cdc_write(buf.as_ptr(), n as u32) };
        wrote = true;
    }
    if wrote {
        unsafe { peer_log_cdc_flush() };
    }
}

#[cfg(all(feature = "dh_debug", not(target_os = "none")))]
fn flush_to_cdc() {}

/// Periodic drain task — register on Core0 at ~1kHz.
///
/// # Safety
/// Internally takes the peer_log spinlock; safe to call from main loop only.
#[cfg(feature = "dh_debug")]
#[no_mangle]
pub unsafe extern "C" fn debug_log_flush_task() {
    // Forward this board's own logs to the peer (TX→UART) AND output everything
    // (local + received) to this board's own CDC (OUT→CDC). Both boards do both.
    flush_to_uart();
    flush_to_cdc();
}

/// No-op stub when dh_debug is disabled.
///
/// # Safety
/// No-op.
#[cfg(not(feature = "dh_debug"))]
#[no_mangle]
pub unsafe extern "C" fn debug_log_flush_task() {}

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
// Tests (host target — exercises Ring directly over a stack buffer)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CAP: usize = 1024;

    /// Build a ring backed by the caller's buffer. `backing` must outlive the
    /// returned ring (raw-pointer backed).
    fn ring(backing: &mut [u8]) -> Ring {
        let mut r = Ring::new();
        r.init(backing.as_mut_ptr(), backing.len());
        r
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
    fn unbacked_ring_is_noop() {
        let mut r = Ring::new();
        r.push_local(b"x\n", PREFIX_A);
        r.push_received(b"y");
        assert_eq!(r.used(), 0);
        assert_eq!(r.pop_byte(), None);
    }

    #[test]
    fn empty_ring_pop_returns_none() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        assert_eq!(r.pop_byte(), None);
        assert_eq!(r.pop_chunk_8(true), None);
    }

    #[test]
    fn local_push_injects_prefix_at_line_start() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_local(b"hello\n", PREFIX_A);
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..n], b"[A] hello\r\n");
    }

    #[test]
    fn local_push_multiple_lines_each_prefixed() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_local(b"one\ntwo\n", PREFIX_B);
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..n], b"[B] one\r\n[B] two\r\n");
    }

    #[test]
    fn local_push_continues_unterminated_line() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_local(b"part1 ", PREFIX_A);
        r.push_local(b"part2\n", PREFIX_A);
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        // Second push must NOT add prefix because last byte was not '\n'.
        assert_eq!(&out[..n], b"[A] part1 part2\r\n");
    }

    #[test]
    fn received_push_strips_nul_padding() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_received(b"[A] x\n\0\0\0");
        let mut out = [0u8; 32];
        let n = drain(&mut r, &mut out);
        assert_eq!(&out[..n], b"[A] x\n");
    }

    #[test]
    fn pop_chunk_8_full_packet() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_received(b"abcdefgh");
        let chunk = r.pop_chunk_8(false).unwrap();
        assert_eq!(&chunk, b"abcdefgh");
    }

    #[test]
    fn pop_chunk_8_holds_until_full_when_not_partial() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_received(b"abc");
        assert_eq!(r.pop_chunk_8(false), None);
        // Allow partial → flushes with NUL padding.
        let chunk = r.pop_chunk_8(true).unwrap();
        assert_eq!(&chunk, b"abc\0\0\0\0\0");
    }

    #[test]
    fn overflow_sets_flag_and_emits_sentinel() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        // Fill the ring exactly using repeated 1-byte pushes.
        for _ in 0..TEST_CAP {
            r.push_received(b"x");
        }
        assert_eq!(r.used(), TEST_CAP);
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
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        r.push_local(b"", PREFIX_A);
        assert_eq!(r.used(), 0);
    }

    #[test]
    fn pop_byte_wraparound() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring(&mut b);
        // Push more than half so head+tail exercise wrap when popped/pushed.
        for _ in 0..(TEST_CAP - 2) {
            r.push_received(b"a");
        }
        for _ in 0..(TEST_CAP - 2) {
            r.pop_byte();
        }
        r.push_received(b"bc");
        assert_eq!(r.pop_byte(), Some(b'b'));
        assert_eq!(r.pop_byte(), Some(b'c'));
        assert_eq!(r.pop_byte(), None);
    }

    #[test]
    fn local_routes_to_both_received_to_out_only() {
        // Models the global routing: a local log goes to BOTH rings; a received
        // (peer) log goes to OUT only — so TX never re-forwards received logs.
        let mut ob = [0u8; TEST_CAP];
        let mut tb = [0u8; TEST_CAP];
        let mut out = ring(&mut ob);
        let mut tx = ring(&mut tb);

        // local: write to both
        out.push_local(b"local\n", PREFIX_A);
        tx.push_local(b"local\n", PREFIX_A);
        // received: out only
        out.push_received(b"[B] peer\n");

        let mut o = [0u8; 64];
        let no = drain(&mut out, &mut o);
        assert_eq!(&o[..no], b"[A] local\r\n[B] peer\n");

        let mut t = [0u8; 64];
        let nt = drain(&mut tx, &mut t);
        assert_eq!(&t[..nt], b"[A] local\r\n"); // peer log NOT forwarded
    }
}
