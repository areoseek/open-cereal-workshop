//! Reader for WavPack files (lossless integer audio, mono or stereo), so samples and impulse files kept in that
//! format can be loaded like any other audio.
//!
//! This file is a Rust port of the decoding path of the WavPack reference library and stays under its licence:
//!
//! Copyright (c) 1998 - 2025 David Bryant
//! All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without modification, are permitted provided that
//! the following conditions are met:
//!
//!   * Redistributions of source code must retain the above copyright notice, this list of conditions and the
//!     following disclaimer.
//!   * Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//!     following disclaimer in the documentation and/or other materials provided with the distribution.
//!   * Neither the name of the copyright holder nor the names of its contributors may be used to endorse or
//!     promote products derived from this software without specific prior written permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
//! WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
//! PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT,
//! INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
//! PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
//! CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
//! OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
//! DAMAGE.

const MONO_FLAG: u32 = 4;
const HYBRID_FLAG: u32 = 8;
const JOINT_STEREO: u32 = 0x10;
const FLOAT_DATA: u32 = 0x80;
const INT32_DATA: u32 = 0x100;
const FALSE_STEREO: u32 = 0x4000_0000;
const DSD_FLAG: u32 = 0x8000_0000;
const RATES: [u32; 15] = [6000, 8000, 9600, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 192000];
const LIMIT_ONES: u32 = 16;
const HYBRID_BITRATE: u32 = 0x200;
const HYBRID_BALANCE: u32 = 0x400;
const SLS: u32 = 8;
const SLO: u32 = 1 << (SLS - 1);

const EXP2: [u8; 256] = [
    0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x08, 0x09, 0x0a, 0x0b,
    0x0b, 0x0c, 0x0d, 0x0e, 0x0e, 0x0f, 0x10, 0x10, 0x11, 0x12, 0x13, 0x13, 0x14, 0x15, 0x16, 0x16,
    0x17, 0x18, 0x19, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1d, 0x1e, 0x1f, 0x20, 0x20, 0x21, 0x22, 0x23,
    0x24, 0x24, 0x25, 0x26, 0x27, 0x28, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3a, 0x3b, 0x3c, 0x3d,
    0x3e, 0x3f, 0x40, 0x41, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x48, 0x49, 0x4a, 0x4b,
    0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a,
    0x5b, 0x5c, 0x5d, 0x5e, 0x5e, 0x5f, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
    0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x87, 0x88, 0x89, 0x8a,
    0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b,
    0x9c, 0x9d, 0x9f, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad,
    0xaf, 0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0,
    0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc8, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf, 0xd0, 0xd2, 0xd3, 0xd4,
    0xd6, 0xd7, 0xd8, 0xd9, 0xdb, 0xdc, 0xdd, 0xde, 0xe0, 0xe1, 0xe2, 0xe4, 0xe5, 0xe6, 0xe8, 0xe9,
    0xea, 0xec, 0xed, 0xee, 0xf0, 0xf1, 0xf2, 0xf4, 0xf5, 0xf6, 0xf8, 0xf9, 0xfa, 0xfc, 0xfd, 0xff,
];

const LOG2: [u8; 256] = [
    0x00, 0x01, 0x03, 0x04, 0x06, 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x10, 0x11, 0x12, 0x14, 0x15,
    0x16, 0x18, 0x19, 0x1a, 0x1c, 0x1d, 0x1e, 0x20, 0x21, 0x22, 0x24, 0x25, 0x26, 0x28, 0x29, 0x2a,
    0x2c, 0x2d, 0x2e, 0x2f, 0x31, 0x32, 0x33, 0x34, 0x36, 0x37, 0x38, 0x39, 0x3b, 0x3c, 0x3d, 0x3e,
    0x3f, 0x41, 0x42, 0x43, 0x44, 0x45, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4d, 0x4e, 0x4f, 0x50, 0x51,
    0x52, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x61, 0x62, 0x63,
    0x64, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x74, 0x75,
    0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85,
    0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95,
    0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4,
    0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb0, 0xb1, 0xb2, 0xb2,
    0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0, 0xc0,
    0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcb, 0xcc, 0xcd, 0xce,
    0xcf, 0xd0, 0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd8, 0xd9, 0xda, 0xdb,
    0xdc, 0xdc, 0xdd, 0xde, 0xdf, 0xe0, 0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe4, 0xe5, 0xe6, 0xe7, 0xe7,
    0xe8, 0xe9, 0xea, 0xea, 0xeb, 0xec, 0xed, 0xee, 0xee, 0xef, 0xf0, 0xf1, 0xf1, 0xf2, 0xf3, 0xf4,
    0xf4, 0xf5, 0xf6, 0xf7, 0xf7, 0xf8, 0xf9, 0xf9, 0xfa, 0xfb, 0xfc, 0xfc, 0xfd, 0xfe, 0xff, 0xff,
];

/// What a file decodes to, interleaved when there are two channels.
pub struct Decoded {
    pub rate: u32,
    pub channels: usize,
    /// Bytes each sample was stored in (1 to 4); full scale is `1 << (8 * bytes - 1)`.
    pub bytes: u32,
    pub samples: Vec<i32>,
    /// True when `samples` holds the bit patterns of 32-bit floats instead of whole numbers.
    pub float: bool,
    /// Blocks whose check value did not match what was decoded.
    pub bad_blocks: usize,
}

fn exp2s(log: i32) -> i32 {
    if log < 0 {
        return -exp2s(-log);
    }
    let value = EXP2[(log & 0xff) as usize] as u32 | 0x100;
    let log = log >> 8;
    if log <= 9 {
        (value >> (9 - log)) as i32
    } else {
        value.wrapping_shl((log - 9).min(31) as u32) as i32
    }
}

fn log2(mut v: u32) -> i32 {
    v = v.wrapping_add(v >> 9);
    let bits = (32 - v.leading_zeros()) as i32;
    if v < 256 {
        (bits << 8) + LOG2[((v << (9 - bits)) & 0xff) as usize] as i32
    } else {
        (bits << 8) + LOG2[((v >> (bits - 9)) & 0xff) as usize] as i32
    }
}

fn restore_weight(w: i8) -> i32 {
    let mut r = (w as i32) << 3;
    if r > 0 {
        r += (r + 64) >> 7;
    }
    r
}

#[inline]
fn apply_weight(w: i32, s: i32) -> i32 {
    if s == s as i16 as i32 {
        (w.wrapping_mul(s) + 512) >> 10
    } else {
        (((s & 0xffff).wrapping_mul(w) >> 9).wrapping_add(((s & !0xffff) >> 9).wrapping_mul(w)).wrapping_add(1)) >> 1
    }
}

#[inline]
fn update_weight(w: &mut i32, delta: i32, source: i32, result: i32) {
    if source != 0 && result != 0 {
        if (source ^ result) < 0 {
            *w -= delta;
        } else {
            *w += delta;
        }
    }
}

#[inline]
fn update_weight_clip(w: &mut i32, delta: i32, source: i32, result: i32) {
    if source != 0 && result != 0 {
        if (source ^ result) < 0 {
            *w = (*w - delta).max(-1024);
        } else {
            *w = (*w + delta).min(1024);
        }
    }
}

#[derive(Default, Clone)]
struct Pass {
    term: i32,
    delta: i32,
    wa: i32,
    wb: i32,
    a: [i32; 8],
    b: [i32; 8],
}

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    /// The next bit, lowest bit of each byte first; ones once the data has run out, which ends the block.
    #[inline]
    fn bit(&mut self) -> u32 {
        let v = match self.d.get(self.pos >> 3) {
            Some(b) => (*b as u32 >> (self.pos & 7)) & 1,
            None => 1,
        };
        self.pos += 1;
        v
    }

    fn bits(&mut self, n: u32) -> u32 {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit() << i;
        }
        v
    }

    fn code(&mut self, maxcode: u32) -> u32 {
        if maxcode < 2 {
            return if maxcode != 0 { self.bit() } else { 0 };
        }
        let bitcount = 32 - maxcode.leading_zeros();
        let extras = (1u32 << bitcount).wrapping_sub(maxcode).wrapping_sub(1);
        let mut code = self.bits(bitcount - 1);
        if code >= extras {
            code = (code << 1).wrapping_sub(extras).wrapping_add(self.bit());
        }
        code
    }

    /// A count written as a run of ones and then that many bits less one; `None` at the end of the data.
    fn escape(&mut self) -> Option<u32> {
        let mut cbits = 0;
        while cbits < 33 && self.bit() != 0 {
            cbits += 1;
        }
        if cbits == 33 {
            return None;
        }
        if cbits < 2 {
            return Some(cbits);
        }
        let mut mask = 1u32;
        let mut v = 0u32;
        for _ in 1..cbits {
            if self.bit() != 0 {
                v |= mask;
            }
            mask = mask.wrapping_shl(1);
        }
        Some(v | mask)
    }
}

#[derive(Default)]
struct Words {
    med: [[u32; 3]; 2],
    holding_one: u32,
    holding_zero: bool,
    zeros_acc: u32,
    slow_level: [u32; 2],
    bitrate_acc: [u32; 2],
    bitrate_delta: [i32; 2],
    error_limit: [u32; 2],
}

const DIV: [u32; 3] = [128, 64, 32];

impl Words {
    #[inline]
    fn get(&self, c: usize, n: usize) -> u32 {
        (self.med[c][n] >> 4) + 1
    }
    #[inline]
    fn inc(&mut self, c: usize, n: usize) {
        let m = self.med[c][n];
        self.med[c][n] = m.wrapping_add(((m.wrapping_add(DIV[n])) / DIV[n]) * 5);
    }
    #[inline]
    fn dec(&mut self, c: usize, n: usize) {
        let m = self.med[c][n];
        self.med[c][n] = m.wrapping_sub(((m.wrapping_add(DIV[n] - 2)) / DIV[n]) * 2);
    }

    fn update_error_limit(&mut self, flags: u32, mono: bool) {
        self.bitrate_acc[0] = self.bitrate_acc[0].wrapping_add(self.bitrate_delta[0] as u32);
        let mut b0 = (self.bitrate_acc[0] >> 16) as i32;
        let limit = |slow: i32, rate: i32| if slow - rate > -0x100 { exp2s(slow - rate + 0x100) as u32 } else { 0 };
        if mono {
            if flags & HYBRID_BITRATE != 0 {
                let s0 = ((self.slow_level[0].wrapping_add(SLO)) >> SLS) as i32;
                self.error_limit[0] = limit(s0, b0);
            } else {
                self.error_limit[0] = exp2s(b0) as u32;
            }
            return;
        }
        self.bitrate_acc[1] = self.bitrate_acc[1].wrapping_add(self.bitrate_delta[1] as u32);
        let mut b1 = (self.bitrate_acc[1] >> 16) as i32;
        if flags & HYBRID_BITRATE != 0 {
            let s0 = ((self.slow_level[0].wrapping_add(SLO)) >> SLS) as i32;
            let s1 = ((self.slow_level[1].wrapping_add(SLO)) >> SLS) as i32;
            if flags & HYBRID_BALANCE != 0 {
                let balance = (s1 - s0 + b1 + 1) >> 1;
                if balance > b0 {
                    b1 = b0 * 2;
                    b0 = 0;
                } else if -balance > b0 {
                    b0 *= 2;
                    b1 = 0;
                } else {
                    b1 = b0 + balance;
                    b0 -= balance;
                }
            }
            self.error_limit[0] = limit(s0, b0);
            self.error_limit[1] = limit(s1, b1);
        } else {
            self.error_limit[0] = exp2s(b0) as u32;
            self.error_limit[1] = exp2s(b1) as u32;
        }
    }

    /// One value of a lossy block, read to within the error the block allows; `None` at the end of the data.
    fn word(&mut self, bs: &mut Bits, c: usize, flags: u32, mono: bool) -> Option<i32> {
        let decay = |s: &mut Self| s.slow_level[c] = s.slow_level[c].wrapping_sub(s.slow_level[c].wrapping_add(SLO) >> SLS);
        if self.med[0][0] < 2 && !self.holding_zero && self.holding_one == 0 && self.med[1][0] < 2 {
            if self.zeros_acc != 0 {
                self.zeros_acc -= 1;
                if self.zeros_acc != 0 {
                    decay(self);
                    return Some(0);
                }
            } else {
                self.zeros_acc = bs.escape()?;
                if self.zeros_acc != 0 {
                    decay(self);
                    self.med = [[0; 3]; 2];
                    return Some(0);
                }
            }
        }
        let mut ones = 0u32;
        if self.holding_zero {
            self.holding_zero = false;
        } else {
            while ones < LIMIT_ONES + 1 && bs.bit() != 0 {
                ones += 1;
            }
            if ones >= LIMIT_ONES {
                if ones == LIMIT_ONES + 1 {
                    return None;
                }
                ones = bs.escape()?.wrapping_add(LIMIT_ONES);
            }
            if self.holding_one != 0 {
                self.holding_one = ones & 1;
                ones = (ones >> 1) + 1;
            } else {
                self.holding_one = ones & 1;
                ones >>= 1;
            }
            self.holding_zero = self.holding_one == 0;
        }
        if c == 0 {
            self.update_error_limit(flags, mono);
        }
        let (mut low, mut high) = self.range(c, ones);
        low &= 0x7fff_ffff;
        high &= 0x7fff_ffff;
        if low > high {
            high = low;
        }
        let mut mid = (high + low + 1) >> 1;
        let limit = self.error_limit[c];
        if limit == 0 {
            mid = bs.code(high - low).wrapping_add(low);
        } else {
            while high - low > limit {
                if bs.bit() != 0 {
                    low = mid;
                    mid = (high + low + 1) >> 1;
                } else {
                    high = mid - 1;
                    mid = (high + low + 1) >> 1;
                }
            }
        }
        let sign = bs.bit();
        if flags & HYBRID_BITRATE != 0 {
            decay(self);
            self.slow_level[c] = self.slow_level[c].wrapping_add(log2(mid) as u32);
        }
        Some(if sign != 0 { !(mid as i32) } else { mid as i32 })
    }

    /// The span of values that `ones` selects, moving the running medians as it goes.
    fn range(&mut self, c: usize, ones: u32) -> (u32, u32) {
        let mut low;
        let high;
        if ones == 0 {
            low = 0u32;
            high = self.get(c, 0) - 1;
            self.dec(c, 0);
        } else {
            low = self.get(c, 0);
            self.inc(c, 0);
            if ones == 1 {
                high = low.wrapping_add(self.get(c, 1)).wrapping_sub(1);
                self.dec(c, 1);
            } else {
                low = low.wrapping_add(self.get(c, 1));
                self.inc(c, 1);
                if ones == 2 {
                    high = low.wrapping_add(self.get(c, 2)).wrapping_sub(1);
                    self.dec(c, 2);
                } else {
                    low = low.wrapping_add((ones - 2).wrapping_mul(self.get(c, 2)));
                    high = low.wrapping_add(self.get(c, 2)).wrapping_sub(1);
                    self.inc(c, 2);
                }
            }
        }
        (low, high)
    }

    /// Read `buf.len()` residuals (alternating channels when not mono); gives how many were read.
    fn read(&mut self, bs: &mut Bits, buf: &mut [i32], mono: bool) -> usize {
        let n = buf.len();
        let mut i = 0;
        while i < n {
            let mut c = if mono { 0 } else { i & 1 };
            if self.holding_zero {
                self.holding_zero = false;
                let low = bs.code(self.get(c, 0) - 1);
                self.dec(c, 0);
                buf[i] = if bs.bit() != 0 { !(low as i32) } else { low as i32 };
                i += 1;
                if i == n {
                    break;
                }
                c = if mono { 0 } else { i & 1 };
            }
            if self.med[0][0] < 2 && self.holding_one == 0 && self.med[1][0] < 2 {
                if self.zeros_acc != 0 {
                    self.zeros_acc -= 1;
                    if self.zeros_acc != 0 {
                        buf[i] = 0;
                        i += 1;
                        continue;
                    }
                } else {
                    match bs.escape() {
                        Some(v) => self.zeros_acc = v,
                        None => break,
                    }
                    if self.zeros_acc != 0 {
                        self.med = [[0; 3]; 2];
                        buf[i] = 0;
                        i += 1;
                        continue;
                    }
                }
            }
            let mut ones = 0u32;
            while ones < LIMIT_ONES + 1 && bs.bit() != 0 {
                ones += 1;
            }
            if ones >= LIMIT_ONES {
                if ones == LIMIT_ONES + 1 {
                    break;
                }
                match bs.escape() {
                    Some(v) => ones = v.wrapping_add(LIMIT_ONES),
                    None => break,
                }
            }
            if self.holding_one != 0 {
                self.holding_one = ones & 1;
                ones = (ones >> 1) + 1;
            } else {
                self.holding_one = ones & 1;
                ones >>= 1;
            }
            self.holding_zero = self.holding_one == 0;
            let (mut low, high) = self.range(c, ones);
            low = low.wrapping_add(bs.code(high.wrapping_sub(low)));
            buf[i] = if bs.bit() != 0 { !(low as i32) } else { low as i32 };
            i += 1;
        }
        i
    }
}

fn mono_pass(p: &mut Pass, buf: &mut [i32]) {
    let delta = p.delta;
    if p.term > 8 {
        for s in buf.iter_mut() {
            let sam = if p.term == 17 { p.a[0].wrapping_mul(2).wrapping_sub(p.a[1]) } else { (p.a[0].wrapping_mul(3).wrapping_sub(p.a[1])) >> 1 };
            p.a[1] = p.a[0];
            p.a[0] = apply_weight(p.wa, sam).wrapping_add(*s);
            update_weight(&mut p.wa, delta, sam, *s);
            *s = p.a[0];
        }
    } else {
        let mut m = 0usize;
        let mut k = (p.term & 7) as usize;
        for s in buf.iter_mut() {
            let sam = p.a[m];
            p.a[k] = apply_weight(p.wa, sam).wrapping_add(*s);
            update_weight(&mut p.wa, delta, sam, *s);
            *s = p.a[k];
            m = (m + 1) & 7;
            k = (k + 1) & 7;
        }
    }
}

fn stereo_pass(p: &mut Pass, buf: &mut [i32]) {
    let delta = p.delta;
    match p.term {
        17 | 18 => {
            for f in buf.chunks_exact_mut(2) {
                for (c, s) in f.iter_mut().enumerate() {
                    let (h, w) = if c == 0 { (&mut p.a, &mut p.wa) } else { (&mut p.b, &mut p.wb) };
                    let sam = if p.term == 17 { h[0].wrapping_mul(2).wrapping_sub(h[1]) } else { h[0].wrapping_add(h[0].wrapping_sub(h[1]) >> 1) };
                    h[1] = h[0];
                    h[0] = apply_weight(*w, sam).wrapping_add(*s);
                    update_weight(w, delta, sam, *s);
                    *s = h[0];
                }
            }
        }
        -1 => {
            for f in buf.chunks_exact_mut(2) {
                let sam = f[0].wrapping_add(apply_weight(p.wa, p.a[0]));
                update_weight_clip(&mut p.wa, delta, p.a[0], f[0]);
                f[0] = sam;
                p.a[0] = f[1].wrapping_add(apply_weight(p.wb, sam));
                update_weight_clip(&mut p.wb, delta, sam, f[1]);
                f[1] = p.a[0];
            }
        }
        -2 => {
            for f in buf.chunks_exact_mut(2) {
                let sam = f[1].wrapping_add(apply_weight(p.wb, p.b[0]));
                update_weight_clip(&mut p.wb, delta, p.b[0], f[1]);
                f[1] = sam;
                p.b[0] = f[0].wrapping_add(apply_weight(p.wa, sam));
                update_weight_clip(&mut p.wa, delta, sam, f[0]);
                f[0] = p.b[0];
            }
        }
        -3 => {
            for f in buf.chunks_exact_mut(2) {
                let sa = f[0].wrapping_add(apply_weight(p.wa, p.a[0]));
                update_weight_clip(&mut p.wa, delta, p.a[0], f[0]);
                let sb = f[1].wrapping_add(apply_weight(p.wb, p.b[0]));
                update_weight_clip(&mut p.wb, delta, p.b[0], f[1]);
                f[0] = sa;
                p.b[0] = sa;
                f[1] = sb;
                p.a[0] = sb;
            }
        }
        _ => {
            let mut m = 0usize;
            let mut k = (p.term & 7) as usize;
            for f in buf.chunks_exact_mut(2) {
                let sam = p.a[m];
                p.a[k] = apply_weight(p.wa, sam).wrapping_add(f[0]);
                update_weight(&mut p.wa, delta, sam, f[0]);
                f[0] = p.a[k];
                let sam = p.b[m];
                p.b[k] = apply_weight(p.wb, sam).wrapping_add(f[1]);
                update_weight(&mut p.wb, delta, sam, f[1]);
                f[1] = p.b[k];
                m = (m + 1) & 7;
                k = (k + 1) & 7;
            }
        }
    }
}

fn le16(d: &[u8], at: usize) -> i32 {
    u16::from_le_bytes([d[at], d[at + 1]]) as i32
}

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// True when the bytes start like a WavPack file.
pub fn is_wavpack(bytes: &[u8]) -> bool {
    bytes.len() >= 32 && &bytes[0..4] == b"wvpk"
}

/// Decode a whole file held in memory.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let mut out = Decoded { rate: 44_100, channels: 0, bytes: 2, samples: Vec::new(), float: false, bad_blocks: 0 };
    let mut off = 0usize;
    while off + 32 <= bytes.len() {
        if &bytes[off..off + 4] != b"wvpk" {
            // Tags and other trailing data end the audio.
            break;
        }
        let size = le32(bytes, off + 4) as usize;
        let end = off + 8 + size;
        if size < 24 || end > bytes.len() {
            break;
        }
        let count = le32(bytes, off + 20) as usize;
        let flags = le32(bytes, off + 24);
        let crc = le32(bytes, off + 28);
        let body = &bytes[off + 32..end];
        off = end;
        if count == 0 {
            continue;
        }
        if flags & DSD_FLAG != 0 {
            return Err("DSD WavPack files are not read".into());
        }
        let hybrid = flags & HYBRID_FLAG != 0;
        let channels = if flags & MONO_FLAG != 0 { 1 } else { 2 };
        if out.channels == 0 {
            out.channels = channels;
            out.bytes = (flags & 3) + 1;
            out.float = flags & FLOAT_DATA != 0;
            let ri = ((flags >> 23) & 15) as usize;
            if ri < 15 {
                out.rate = RATES[ri];
            }
        } else if out.channels != channels {
            return Err("WavPack files with more than two channels are not read yet".into());
        }
        let mono = flags & (MONO_FLAG | FALSE_STEREO) != 0;
        let mut passes: Vec<Pass> = Vec::new();
        let mut words = Words::default();
        let mut stream: &[u8] = &[];
        let mut int32 = [0u8; 4];
        let mut float = [0u8; 4];
        let mut at = 0usize;
        while at + 2 <= body.len() {
            let id = body[at];
            let mut len = body[at + 1] as usize;
            let mut head = 2;
            if id & 0x80 != 0 {
                if at + 4 > body.len() {
                    break;
                }
                len |= (body[at + 2] as usize) << 8 | (body[at + 3] as usize) << 16;
                head = 4;
            }
            len *= 2;
            let start = at + head;
            if start + len > body.len() {
                break;
            }
            let d = &body[start..start + len - if id & 0x40 != 0 && len > 0 { 1 } else { 0 }];
            at = start + len;
            match id & 0x3f {
                0x2 => {
                    passes = vec![Pass::default(); d.len().min(16)];
                    for (p, b) in passes.iter_mut().rev().zip(d) {
                        p.term = (*b & 0x1f) as i32 - 5;
                        p.delta = ((*b >> 5) & 7) as i32;
                        let ok = matches!(p.term, 1..=8 | 17 | 18) || (!mono && matches!(p.term, -3..=-1));
                        if !ok {
                            return Err("bad WavPack block".into());
                        }
                    }
                }
                0x3 => {
                    let mut it = d.iter();
                    for p in passes.iter_mut().rev() {
                        let Some(a) = it.next() else { break };
                        p.wa = restore_weight(*a as i8);
                        if !mono {
                            let Some(b) = it.next() else { break };
                            p.wb = restore_weight(*b as i8);
                        }
                    }
                }
                0x4 => {
                    let mut q = 0usize;
                    let next = |q: &mut usize| -> Option<i32> {
                        if *q + 2 > d.len() {
                            return None;
                        }
                        let v = exp2s(le16(d, *q) as i16 as i32);
                        *q += 2;
                        Some(v)
                    };
                    'fill: for p in passes.iter_mut().rev() {
                        if q >= d.len() {
                            break;
                        }
                        if p.term > 8 {
                            for i in 0..2 {
                                let Some(v) = next(&mut q) else { break 'fill };
                                p.a[i] = v;
                            }
                            if !mono {
                                for i in 0..2 {
                                    let Some(v) = next(&mut q) else { break 'fill };
                                    p.b[i] = v;
                                }
                            }
                        } else if p.term < 0 {
                            let Some(v) = next(&mut q) else { break 'fill };
                            p.a[0] = v;
                            let Some(v) = next(&mut q) else { break 'fill };
                            p.b[0] = v;
                        } else {
                            for m in 0..p.term as usize {
                                let Some(v) = next(&mut q) else { break 'fill };
                                p.a[m] = v;
                                if !mono {
                                    let Some(v) = next(&mut q) else { break 'fill };
                                    p.b[m] = v;
                                }
                            }
                        }
                    }
                }
                0x5 => {
                    let chans = if mono { 1 } else { 2 };
                    if d.len() >= 6 * chans {
                        for c in 0..chans {
                            for n in 0..3 {
                                words.med[c][n] = exp2s(le16(d, c * 6 + n * 2)) as u32;
                            }
                        }
                    }
                }
                0x6 => {
                    let chans = if mono { 1 } else { 2 };
                    let mut q = 0;
                    if flags & HYBRID_BITRATE != 0 && d.len() >= q + 2 * chans {
                        for c in 0..chans {
                            words.slow_level[c] = exp2s(le16(d, q)) as u32;
                            q += 2;
                        }
                    }
                    if d.len() >= q + 2 * chans {
                        for c in 0..chans {
                            words.bitrate_acc[c] = (le16(d, q) as u32) << 16;
                            q += 2;
                        }
                    }
                    if d.len() >= q + 2 * chans {
                        for c in 0..chans {
                            words.bitrate_delta[c] = exp2s(le16(d, q) as i16 as i32);
                            q += 2;
                        }
                    }
                }
                0x8 if d.len() == 4 => float.copy_from_slice(d),
                0x9 if d.len() == 4 => int32.copy_from_slice(d),
                0xa => stream = d,
                0x27 if d.len() >= 3 => out.rate = d[0] as u32 | (d[1] as u32) << 8 | (d[2] as u32) << 16,
                _ => {}
            }
        }
        let n = if mono { count } else { count * 2 };
        let mut buf = vec![0i32; n];
        let mut bs = Bits { d: stream, pos: 0 };
        let got = if hybrid {
            let mut i = 0;
            while i < n {
                match words.word(&mut bs, if mono { 0 } else { i & 1 }, flags, mono) {
                    Some(v) => buf[i] = v,
                    None => break,
                }
                i += 1;
            }
            i
        } else {
            words.read(&mut bs, &mut buf, mono)
        };
        let mut sum = 0xffff_ffffu32;
        if mono {
            for p in passes.iter_mut() {
                mono_pass(p, &mut buf);
            }
            for s in &buf {
                sum = sum.wrapping_mul(3).wrapping_add(*s as u32);
            }
        } else {
            for p in passes.iter_mut() {
                stereo_pass(p, &mut buf);
            }
            for f in buf.chunks_exact_mut(2) {
                if flags & JOINT_STEREO != 0 {
                    f[1] = f[1].wrapping_sub(f[0] >> 1);
                    f[0] = f[0].wrapping_add(f[1]);
                }
                sum = sum.wrapping_mul(9).wrapping_add((f[0] as u32).wrapping_mul(3)).wrapping_add(f[1] as u32);
            }
        }
        if got != n || sum != crc {
            out.bad_blocks += 1;
        }
        let mut shift = (flags >> 13) & 0x1f;
        if flags & FLOAT_DATA != 0 {
            // Without the extra stream the lowest bits of quiet values are filled in, which is inaudible.
            let (fl, fshift, max_exp, norm) = (float[0], float[1] as u32 & 0x1f, float[2] as i32, float[3] as i32);
            for s in buf.iter_mut() {
                let mut out = 0u32;
                if *s != 0 {
                    let mut v = ((*s as u32) << fshift) as i32;
                    if v < 0 {
                        v = v.wrapping_neg();
                        out |= 0x8000_0000;
                    }
                    let mut v = v as u32;
                    let mut exp = max_exp;
                    if v >= 0x100_0000 {
                        while v & 0xf00_0000 != 0 {
                            v >>= 1;
                            exp += 1;
                        }
                    } else if exp != 0 {
                        let mut count = 0u32;
                        while v & 0x80_0000 == 0 {
                            exp -= 1;
                            if exp == 0 {
                                break;
                            }
                            count += 1;
                            v <<= 1;
                        }
                        count &= 0x1f;
                        if count != 0 && fl & 1 != 0 {
                            v |= (1u32 << count) - 1;
                        }
                    }
                    // Bring the file's own full scale to one.
                    let exp = if norm != 0 && exp != 0 { (exp + 127 - norm).clamp(1, 254) } else { exp };
                    out |= (v & 0x7f_ffff) | ((exp as u32 & 0xff) << 23);
                }
                *s = out as i32;
            }
            shift = 0;
        } else if flags & INT32_DATA != 0 {
            let (sent, zeros, ones, dups) = (int32[0] as u32 & 31, int32[1] as u32 & 31, int32[2] as u32 & 31, int32[3] as u32 & 31);
            if sent == 0 {
                for s in buf.iter_mut() {
                    if zeros != 0 {
                        *s = ((*s as u32) << zeros) as i32;
                    } else if ones != 0 {
                        *s = (((*s as u32).wrapping_add(1)) << ones).wrapping_sub(1) as i32;
                    } else if dups != 0 {
                        let low = (*s & 1) as u32;
                        *s = (((*s as u32).wrapping_add(low)) << dups).wrapping_sub(low) as i32;
                    }
                }
            } else {
                shift = (shift + zeros + sent + ones + dups) & 0x1f;
            }
        }
        if shift != 0 {
            for s in buf.iter_mut() {
                *s = ((*s as u32) << shift) as i32;
            }
        }
        if mono && channels == 2 {
            out.samples.extend(buf.iter().flat_map(|s| [*s, *s]));
        } else {
            out.samples.extend_from_slice(&buf);
        }
    }
    if out.samples.is_empty() {
        return Err("no audio decoded".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_small_helpers_match_their_definitions() {
        assert_eq!(exp2s(0), 0);
        assert_eq!(exp2s(256), 1);
        assert_eq!(exp2s(9 << 8), 256);
        assert_eq!(exp2s(-(10 << 8)), -512);
        assert_eq!(restore_weight(0), 0);
        assert_eq!(restore_weight(-4), -32);
        assert_eq!(restore_weight(127), 1016 + ((1016 + 64) >> 7));
        assert_eq!(apply_weight(1024, 1000), 1000);
        assert_eq!(apply_weight(512, 100_000), 50_000);
    }

    #[test]
    fn codes_are_read_low_bit_first() {
        // maxcode 5 needs 3 bits with 2 short codes: 0 and 1 take two bits, the rest three.
        let d = [0b0000_0110u8];
        let mut bs = Bits { d: &d, pos: 0 };
        assert_eq!(bs.code(5), 3);
        let mut bs = Bits { d: &d, pos: 3 };
        assert_eq!(bs.code(5), 0);
    }

    /// With a content folder at hand, every block of some real files must pass its own check value.
    #[test]
    fn real_files_pass_their_block_checks() {
        let Some(root) = std::env::var_os("CEREAL_FLP_CORPUS") else { return };
        let mut seen = 0;
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().map(|x| x.eq_ignore_ascii_case("wv")).unwrap_or(false) && seen < 150 {
                    let bytes = std::fs::read(&p).unwrap();
                    let d = decode(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                    assert_eq!(d.bad_blocks, 0, "{}", p.display());
                    seen += 1;
                }
            }
        }
    }
}
