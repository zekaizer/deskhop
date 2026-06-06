// Unified debug log formatter (DH_DEBUG). One line builder with a normalized,
// column-aligned, bracketed metadata group so every subsystem logs the same
// shape. The board "[A] "/"[B] " tag is prepended later by peer_log at line
// start; this module produces everything after it:
//
//   <sec:04>.<ms:03> <LVL> <subsys:<4>: <event/keys...>
//   e.g.  "0012.345 I pt  : cap addr=01 inst=00 proto=01 len=3b n=03 ok"
//
// Full line (with peer_log board tag): "[A] 0012.345 I pt  : cap addr=01 ...".
// Levels: I(nfo) / W(arn) / E(rror). subsys is a short subsystem/module tag
// (pt, hb, heap, usb, rmp, kbd, mse, boot, ...). On non-firmware/host builds the
// timestamp reads 0; on non-DH_DEBUG builds done() is a no-op (peer_log stub).

const LINE_MAX: usize = 128;

/// Subsystem tag column width (left-aligned, space-padded).
const SUBSYS_W: usize = 4;
/// Seconds column width (right-aligned, space-padded).
const SEC_W: usize = 4;

pub const I: u8 = b'I';
pub const W: u8 = b'W';
pub const E: u8 = b'E';

pub struct DLog {
    buf: [u8; LINE_MAX],
    len: usize,
}

impl DLog {
    /// Begin a line: writes the normalized "<sec>.<ms> <LVL> <subsys> " prefix.
    pub fn new(level: u8, subsys: &[u8]) -> Self {
        let mut d = DLog { buf: [0; LINE_MAX], len: 0 };
        let us = now_us32();
        let sec = us / 1_000_000;
        let ms = (us % 1_000_000) / 1000;
        d.uzp(sec, SEC_W).raw(b'.').u3(ms).raw(b' ');
        d.raw(level).raw(b' ');
        d.s(subsys);
        for _ in 0..SUBSYS_W.saturating_sub(subsys.len()) {
            d.raw(b' ');
        }
        d.raw(b':').raw(b' ');
        d
    }

    fn raw(&mut self, b: u8) -> &mut Self {
        if self.len < LINE_MAX {
            self.buf[self.len] = b;
            self.len += 1;
        }
        self
    }

    /// Append a raw byte string.
    pub fn s(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            self.raw(b);
        }
        self
    }

    /// Append a u32 in decimal.
    pub fn u(&mut self, v: u32) -> &mut Self {
        if v == 0 {
            return self.raw(b'0');
        }
        let mut tmp = [0u8; 10];
        let mut n = 0;
        let mut x = v;
        while x > 0 {
            tmp[n] = b'0' + (x % 10) as u8;
            x /= 10;
            n += 1;
        }
        while n > 0 {
            n -= 1;
            self.raw(tmp[n]);
        }
        self
    }

    /// Append a u64 in decimal (heartbeat loop counters).
    pub fn u64(&mut self, v: u64) -> &mut Self {
        if v == 0 {
            return self.raw(b'0');
        }
        let mut tmp = [0u8; 20];
        let mut n = 0;
        let mut x = v;
        while x > 0 {
            tmp[n] = b'0' + (x % 10) as u8;
            x /= 10;
            n += 1;
        }
        while n > 0 {
            n -= 1;
            self.raw(tmp[n]);
        }
        self
    }

    /// Right-aligned decimal in a fixed width, zero-padded.
    fn uzp(&mut self, v: u32, width: usize) -> &mut Self {
        let mut digits = 1;
        let mut x = v;
        while x >= 10 {
            x /= 10;
            digits += 1;
        }
        for _ in 0..width.saturating_sub(digits) {
            self.raw(b'0');
        }
        self.u(v)
    }

    /// Zero-padded 3-digit decimal (milliseconds).
    fn u3(&mut self, v: u32) -> &mut Self {
        self.raw(b'0' + ((v / 100) % 10) as u8);
        self.raw(b'0' + ((v / 10) % 10) as u8);
        self.raw(b'0' + (v % 10) as u8)
    }

    /// Append a byte as two hex digits.
    pub fn hx(&mut self, v: u8) -> &mut Self {
        const H: &[u8; 16] = b"0123456789abcdef";
        self.raw(H[(v >> 4) as usize]).raw(H[(v & 0x0F) as usize])
    }

    /// Append a u16 as four hex digits (big-endian).
    pub fn hx16(&mut self, v: u16) -> &mut Self {
        self.hx((v >> 8) as u8).hx(v as u8)
    }

    /// Terminate the line (newline) and push to the peer log ring.
    pub fn done(&mut self) {
        self.raw(b'\n');
        crate::service::peer_log::push(&self.buf[..self.len]);
    }
}

/// Start an Info line for `subsys`.
pub fn i(subsys: &[u8]) -> DLog {
    DLog::new(I, subsys)
}
/// Start a Warn line for `subsys`.
pub fn w(subsys: &[u8]) -> DLog {
    DLog::new(W, subsys)
}
/// Start an Error line for `subsys`.
pub fn e(subsys: &[u8]) -> DLog {
    DLog::new(E, subsys)
}

#[cfg(target_os = "none")]
fn now_us32() -> u32 {
    unsafe { crate::hal::device::hal_time_us_32() }
}
#[cfg(not(target_os = "none"))]
fn now_us32() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Drain helper: DLog has no buffer accessor, so reconstruct via a peek by
    // formatting into a fresh buffer through the public builders and checking
    // the byte content we can observe — here we test the numeric/format helpers
    // by building lines and inspecting the internal buffer through `as_bytes`.
    impl DLog {
        fn as_bytes(&self) -> &[u8] {
            &self.buf[..self.len]
        }
    }

    #[test]
    fn prefix_is_normalized_width() {
        // ts=0 on host → "0000.000 I pt  : "
        let d = i(b"pt");
        assert_eq!(d.as_bytes(), b"0000.000 I pt  : ");
    }

    #[test]
    fn subsys_padded_to_four() {
        let d = i(b"heap");
        assert_eq!(d.as_bytes(), b"0000.000 I heap: ");
        let d2 = i(b"hb");
        assert_eq!(d2.as_bytes(), b"0000.000 I hb  : ");
    }

    #[test]
    fn level_char() {
        assert_eq!(w(b"heap").as_bytes(), b"0000.000 W heap: ");
        assert_eq!(e(b"pt").as_bytes(), b"0000.000 E pt  : ");
    }

    #[test]
    fn decimal_u() {
        let mut d = i(b"pt");
        d.s(b"n=").u(1234);
        assert!(d.as_bytes().ends_with(b"n=1234"));
        let mut z = i(b"pt");
        z.u(0);
        assert!(z.as_bytes().ends_with(b" 0"));
    }

    #[test]
    fn hex_helpers() {
        let mut d = i(b"pt");
        d.s(b"x=").hx(0x0e).s(b" v=").hx16(0x046d);
        assert!(d.as_bytes().ends_with(b"x=0e v=046d"));
    }

    #[test]
    fn u64_decimal() {
        let mut d = i(b"hb");
        d.s(b"c0=").u64(44504890);
        assert!(d.as_bytes().ends_with(b"c0=44504890"));
    }
}
