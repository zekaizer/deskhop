// Peer debug log ring buffer (DH_DEBUG only).
//
// Two rings per board:
//   OUT_RING — local logs + logs received from the peer; output to this board's
//              own CDC. A non-destructive SCROLLBACK: reads advance a cursor but
//              don't free bytes (the oldest are dropped only when the ring is
//              full), so a terminal that attaches late — or reattaches after the
//              passthrough re-enumeration — can rewind and replay the boot
//              history instead of catching the stream mid-line. Sized at LINK
//              TIME to fill the RAM left after .bss (linker symbols
//              __log_ring_start/__log_ring_end), so it holds as much history as
//              free RAM allows.
//   TX_RING  — local logs only; drained (destructively) over UART to the peer
//              board. Small fixed buffer (drained every tick), never carries
//              received logs so peer↔peer forwarding can't loop.
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

/// Fixed size of the per-board UART forward ring. Must absorb a burst (the boot
/// HID descriptor dump is a few KB emitted at once) while it drains to the peer
/// at the UART rate, throttled by the TX queue's free space.
#[cfg(feature = "dh_debug")]
const TX_RING_SIZE: usize = 16384;

// ============================================================
// Ring (pure logic — host-testable). Backed by a caller-provided buffer
// (pointer + capacity) so the OUT ring can be sized at link time.
// ============================================================

pub struct Ring {
    buf: *mut u8,
    cap: usize,
    tail: usize,    // next write position
    stored: usize,  // retained bytes; head = tail - stored (mod cap), <= cap
    unread: usize,  // bytes not yet handed to the reader; read = tail - unread, <= stored
    /// Full-buffer policy. true = overwrite the oldest byte (scrollback, used by
    /// OUT_RING so a fresh CDC connection can replay history); false = drop new
    /// bytes and raise `overflow` (used by TX_RING, where the UART forward must
    /// not silently rewrite already-queued bytes).
    overwrite: bool,
    overflow: bool,
    last_was_newline: bool,
}

impl Default for Ring {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Ring {
    /// Empty, unbacked ring (cap 0). All pushes are no-ops until `init`.
    /// `overwrite` selects the full-buffer policy (see the field docs).
    pub const fn new(overwrite: bool) -> Self {
        Self {
            buf: core::ptr::null_mut(),
            cap: 0,
            tail: 0,
            stored: 0,
            unread: 0,
            overwrite,
            overflow: false,
            last_was_newline: true,
        }
    }

    /// Attach a backing buffer and reset state. `buf` must be valid for `cap`
    /// bytes for the lifetime of all subsequent ring use. The `overwrite`
    /// policy chosen at construction is preserved.
    pub fn init(&mut self, buf: *mut u8, cap: usize) {
        self.buf = buf;
        self.cap = cap;
        self.tail = 0;
        self.stored = 0;
        self.unread = 0;
        self.overflow = false;
        self.last_was_newline = true;
    }

    /// Index of the oldest retained byte. Only valid when stored > 0.
    fn head(&self) -> usize {
        (self.tail + self.cap - self.stored) % self.cap
    }

    /// Index of the next byte the reader will consume. Only valid when cap > 0.
    fn read_pos(&self) -> usize {
        (self.tail + self.cap - self.unread) % self.cap
    }

    fn write_byte(&mut self, b: u8) -> bool {
        if self.cap == 0 {
            return false; // unbacked — drop silently (not an overflow)
        }
        if self.stored >= self.cap {
            if !self.overwrite {
                self.overflow = true;
                return false;
            }
            // Scrollback: overwrite the oldest byte. stored stays at cap (head
            // advances implicitly with tail); the displaced byte pushes the
            // read cursor forward (unread clamped to stored).
            unsafe { *self.buf.add(self.tail) = b };
            self.tail = (self.tail + 1) % self.cap;
            self.unread = (self.unread + 1).min(self.stored);
            return true;
        }
        unsafe { *self.buf.add(self.tail) = b };
        self.tail = (self.tail + 1) % self.cap;
        self.stored += 1;
        self.unread += 1;
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
        if self.overflow && self.cap != 0 && self.cap - self.stored >= OVERFLOW_MSG.len() {
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

    /// Destructively remove the oldest retained byte (advances head). Used by
    /// the UART forward path (TX_RING) where consumed bytes are gone for good.
    pub fn pop_byte(&mut self) -> Option<u8> {
        if self.stored == 0 {
            return None;
        }
        let b = unsafe { *self.buf.add(self.head()) };
        self.stored -= 1;
        // The removed byte was the oldest; if the read cursor was sitting on it
        // (unread == old stored), it advances too.
        if self.unread > self.stored {
            self.unread = self.stored;
        }
        Some(b)
    }

    /// Copy up to `out.len()` unread bytes into the slice WITHOUT removing them
    /// from the ring — only the read cursor advances. Returns count written.
    /// Retained bytes stay until overwritten, so `rewind_read` can replay them.
    pub fn read_into(&mut self, out: &mut [u8]) -> usize {
        if self.cap == 0 {
            return 0;
        }
        let mut n = 0;
        let mut rp = self.read_pos();
        while n < out.len() && self.unread > 0 {
            out[n] = unsafe { *self.buf.add(rp) };
            rp = (rp + 1) % self.cap;
            self.unread -= 1;
            n += 1;
        }
        n
    }

    /// Rewind the read cursor to the oldest retained byte so the next
    /// `read_into` replays the full scrollback. Call on a fresh CDC connection.
    pub fn rewind_read(&mut self) {
        self.unread = self.stored;
    }

    /// Pop up to 8 bytes for a DebugLog packet. If `allow_partial` is false,
    /// returns None when fewer than 8 bytes are queued (waiting for a full
    /// chunk). Partial packets are NUL-padded.
    pub fn pop_chunk_8(&mut self, allow_partial: bool) -> Option<[u8; 8]> {
        if self.stored == 0 {
            return None;
        }
        if !allow_partial && self.stored < 8 {
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

    /// Retained byte count (history depth), not the unread count.
    pub fn used(&self) -> usize {
        self.stored
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

// OUT_RING is a scrollback (overwrite oldest) so a fresh CDC connection can
// rewind and replay the retained boot history; TX_RING drops on full (the UART
// forward must not silently rewrite already-queued bytes).
#[cfg(feature = "dh_debug")]
static mut OUT_RING: Ring = Ring::new(true);
#[cfg(feature = "dh_debug")]
static mut TX_RING: Ring = Ring::new(false);
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
pub fn read_into(out: &mut [u8]) -> usize {
    // CDC output path: non-destructive read of the OUT scrollback.
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(OUT_RING)).read_into(out) }
}

#[cfg(feature = "dh_debug")]
pub fn rewind_read() {
    // Replay the full OUT scrollback (call on a fresh CDC connection).
    let _g = lock();
    unsafe { (*core::ptr::addr_of_mut!(OUT_RING)).rewind_read() }
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
pub fn read_into(_out: &mut [u8]) -> usize {
    0
}
#[cfg(not(feature = "dh_debug"))]
pub fn rewind_read() {}

// ============================================================
// Drain task — runs at 1kHz on Core0. Both boards do both:
//   TX_RING  → packetize 8-byte chunks → UART (queue_packet) → peer
//   OUT_RING → read cursor → own CDC (held while disconnected; rewound on a
//              fresh connect to replay the retained scrollback)
// So either board's CDC shows the full [A]+[B] stream.
// ============================================================

#[cfg(all(feature = "dh_debug", target_os = "none"))]
extern "C" {
    fn peer_log_cdc_connected() -> bool;
    fn peer_log_cdc_write(data: *const u8, len: u32) -> u32;
    fn peer_log_cdc_write_avail() -> u32;
    fn peer_log_cdc_flush();
}

/// Max DebugLog chunks forwarded to the peer per 1kHz tick. Kept BELOW the UART
/// drain rate (921600 baud ≈ 8.4 packets/tick) so the link runs with idle gaps,
/// giving the peer's 1KB UART RX ring time to drain between bursts. Without this
/// a boot burst (HID descriptor dump) streams at full UART rate and overruns the
/// peer's RX ring, garbling the forwarded stream. TX_RING absorbs the backlog.
#[cfg(all(feature = "dh_debug", target_os = "none"))]
const FORWARD_CHUNKS_PER_TICK: u32 = 4;

#[cfg(all(feature = "dh_debug", target_os = "none"))]
fn flush_to_uart() {
    // Drain at most FORWARD_CHUNKS_PER_TICK, and never more than the UART TX
    // queue can accept; the rest stays in TX_RING for the next tick.
    let mut budget = unsafe { crate::hal::device::hal_uart_tx_free() }
        .min(FORWARD_CHUNKS_PER_TICK);
    while budget > 0 {
        match pop_chunk_8(true) {
            Some(chunk) => unsafe {
                crate::hal::device::queue_packet(
                    chunk.as_ptr(),
                    crate::domain::constants::PacketType::DebugLog as u8,
                    8,
                );
            },
            None => break,
        }
        budget -= 1;
    }
}

#[cfg(all(feature = "dh_debug", not(target_os = "none")))]
fn flush_to_uart() {}

/// Tracks the CDC connection (DTR) so we can detect a fresh open. Read/written
/// only from flush_to_cdc on Core0 — single-consumer, no lock needed.
#[cfg(all(feature = "dh_debug", target_os = "none"))]
static mut CDC_WAS_CONNECTED: bool = false;

#[cfg(all(feature = "dh_debug", target_os = "none"))]
fn flush_to_cdc() {
    let connected = unsafe { peer_log_cdc_connected() };
    let was = unsafe { core::ptr::read(core::ptr::addr_of!(CDC_WAS_CONNECTED)) };
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(CDC_WAS_CONNECTED), connected) };
    if !connected {
        // Held while disconnected — the scrollback retains history (overwriting
        // oldest) so it can replay when a terminal finally attaches.
        return;
    }
    if !was {
        // DTR rising edge (a terminal just opened): rewind the read cursor so
        // the full retained scrollback replays from the oldest byte. This is
        // why the big buffer matters — a late-attaching terminal still sees the
        // boot history instead of catching the stream mid-line.
        rewind_read();
    }
    let mut buf = [0u8; 64];
    let mut wrote = false;
    loop {
        // Read only what the CDC TX FIFO can accept this pass. read_into is
        // non-destructive (advances only the read cursor), so bytes the FIFO
        // can't take this tick stay queued for the next one and the scrollback
        // is preserved for a future reconnect.
        let avail = unsafe { peer_log_cdc_write_avail() } as usize;
        if avail == 0 {
            break;
        }
        let want = avail.min(buf.len());
        let n = read_into(&mut buf[..want]);
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

    /// Build a drop-on-full ring (TX semantics). `backing` must outlive it.
    fn ring(backing: &mut [u8]) -> Ring {
        let mut r = Ring::new(false);
        r.init(backing.as_mut_ptr(), backing.len());
        r
    }

    /// Build an overwrite-oldest scrollback ring (OUT semantics).
    fn ring_ow(backing: &mut [u8]) -> Ring {
        let mut r = Ring::new(true);
        r.init(backing.as_mut_ptr(), backing.len());
        r
    }

    /// Non-destructively read all currently-unread bytes into `out`.
    fn read_all(r: &mut Ring, out: &mut [u8]) -> usize {
        r.read_into(out)
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
        let mut r = Ring::new(false);
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

    // ---- Scrollback (OUT_RING) semantics ----

    #[test]
    fn scrollback_read_is_non_destructive_and_replays_on_rewind() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring_ow(&mut b);
        r.push_received(b"[A] one\n");

        // First read drains the unread bytes...
        let mut o = [0u8; 32];
        let n = read_all(&mut r, &mut o);
        assert_eq!(&o[..n], b"[A] one\n");
        // ...but they are retained (history depth unchanged) and not re-read.
        assert_eq!(r.used(), 8);
        assert_eq!(read_all(&mut r, &mut [0u8; 32]), 0);

        // Rewind (fresh CDC connect) replays the full retained history.
        r.rewind_read();
        let mut o2 = [0u8; 32];
        let n2 = read_all(&mut r, &mut o2);
        assert_eq!(&o2[..n2], b"[A] one\n");
    }

    #[test]
    fn scrollback_overwrites_oldest_when_full_no_overflow_flag() {
        const CAP: usize = 8;
        let mut b = [0u8; CAP];
        let mut r = ring_ow(&mut b);
        // Fill exactly, then push 3 more — oldest 3 are overwritten.
        r.push_received(b"01234567");
        r.push_received(b"89A");
        assert_eq!(r.used(), CAP); // capped at cap
        assert!(!r.overflow); // scrollback never raises overflow
        let mut o = [0u8; CAP];
        let n = read_all(&mut r, &mut o);
        // Retains the newest CAP bytes: "345678" + "9A" = "3456789A".
        assert_eq!(&o[..n], b"3456789A");
    }

    #[test]
    fn scrollback_rewind_after_overwrite_replays_retained_window() {
        const CAP: usize = 8;
        let mut b = [0u8; CAP];
        let mut r = ring_ow(&mut b);
        r.push_received(b"01234567"); // fills
        let mut o = [0u8; CAP];
        read_all(&mut r, &mut o); // reader caught up
        r.push_received(b"89"); // overwrites "01", read cursor pushed forward
        // Rewind replays the whole retained window, not just the 2 new bytes.
        r.rewind_read();
        let mut o2 = [0u8; CAP];
        let n2 = read_all(&mut r, &mut o2);
        assert_eq!(&o2[..n2], b"23456789");
    }

    #[test]
    fn scrollback_writes_while_disconnected_then_replays_in_order() {
        let mut b = [0u8; TEST_CAP];
        let mut r = ring_ow(&mut b);
        // Simulate boot logs accumulating while no terminal is attached
        // (reader never called). They are all retained.
        r.push_local(b"boot\n", PREFIX_A);
        r.push_received(b"[B] peer\n");
        r.push_local(b"ready\n", PREFIX_A);
        // Fresh connect: rewind then read everything from the start, in order.
        r.rewind_read();
        let mut o = [0u8; 64];
        let n = read_all(&mut r, &mut o);
        assert_eq!(&o[..n], b"[A] boot\r\n[B] peer\n[A] ready\r\n");
    }
}
