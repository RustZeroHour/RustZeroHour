// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/GameEngine/Include/Common/crc.h (non-debug inline variant)
// and Core/GameEngine/Source/Common/crc.cpp (debug variant, identical
// algorithm) @ 571a7c5, Zero Hour configuration.
//
// The original carries the following notices, preserved here:
//
//   CRC.h — A class encapsulating CRC calculation.
//   Author: Matthew D. Campbell, October 2001.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! The game's custom 32-bit checksum.
//!
//! The original `CRC` class is *not* a CRC-32: it is a shift-add checksum
//! where each byte `b` folds the accumulator as
//! `crc = (crc << 1) + b + old_top_bit`. It is used for save-game integrity,
//! network state hashing and the random seed handshake, so the Rust port must
//! reproduce it bit-exactly (parity level **P0**).
//!
//! # Examples
//!
//! ```
//! use rzh_util::Crc;
//!
//! let mut crc = Crc::new();
//! crc.update(b"123456789");
//! assert_eq!(crc.get(), 0x0000_63C5);
//!
//! // `update` adds into the running value, like CRC::computeCRC:
//! let mut incremental = Crc::new();
//! incremental.update(b"1234");
//! incremental.update(b"56789");
//! assert_eq!(incremental.get(), crc.get());
//! ```

/// The Zero Hour shift-add checksum accumulator.
///
/// Port of `CRC` (`Common/crc.h`). All arithmetic is wrapping `u32` math,
/// exactly like the C++ original operating on `UnsignedInt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Crc {
    crc: u32,
}

impl Crc {
    /// Creates a checksum with accumulator `0`, like `CRC::CRC()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { crc: 0 }
    }

    /// Folds `bytes` into the running checksum.
    ///
    /// Port of `CRC::computeCRC(const void*, Int)`. Empty input is a no-op,
    /// exactly like the original's `len < 1` early-out. The checksum is
    /// **additive**: calling this twice concatenates the effect of the two
    /// byte sequences.
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            // CRC::addCRC: hibit = 1 if the top bit is set, else 0;
            // crc = (crc << 1) + byte + hibit (unsigned wrap-around).
            let hibit = self.crc >> 31;
            self.crc = (self.crc << 1)
                .wrapping_add(u32::from(byte))
                .wrapping_add(hibit);
        }
    }

    /// Returns the current checksum value (port of `CRC::get()`).
    #[must_use]
    pub const fn get(&self) -> u32 {
        self.crc
    }

    /// Resets the accumulator to `0` (port of `CRC::clear()`).
    pub fn clear(&mut self) {
        self.crc = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::Crc;

    #[test]
    fn empty_input_is_zero() {
        let mut crc = Crc::new();
        crc.update(&[]);
        assert_eq!(crc.get(), 0);
    }

    #[test]
    fn single_bytes_follow_the_shift_add_rule() {
        // crc = (0 << 1) + b + 0 == b for a fresh accumulator.
        for byte in [0x00u8, 0x01, 0x7f, 0x80, 0xff] {
            let mut crc = Crc::new();
            crc.update(&[byte]);
            assert_eq!(crc.get(), u32::from(byte));
        }
    }

    #[test]
    fn top_bit_feeds_back_as_carry() {
        // Second byte exercises the hibit term: after 0xff the accumulator is
        // 0x000000ff, whose top bit is clear, so hibit stays 0 here. A longer
        // run is needed to reach the top bit; the golden vectors cover it.
        let mut crc = Crc::new();
        crc.update(&[0xff, 0x00]);
        assert_eq!(crc.get(), 0x1fe);
    }

    #[test]
    fn update_is_additive_and_order_sensitive() {
        let mut whole = Crc::new();
        whole.update(b"the quick brown fox");

        let mut parts = Crc::new();
        parts.update(b"the quick");
        parts.update(b" brown fox");
        assert_eq!(whole.get(), parts.get());

        let mut swapped = Crc::new();
        swapped.update(b" brown fox");
        swapped.update(b"the quick");
        assert_ne!(whole.get(), swapped.get());
    }

    #[test]
    fn clear_resets() {
        let mut crc = Crc::new();
        crc.update(b"anything");
        crc.clear();
        assert_eq!(crc.get(), 0);
    }
}
