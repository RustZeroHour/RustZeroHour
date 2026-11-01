// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/vector2i.h and
// Core/Libraries/Source/WWVegas/WWMath/Vector3i.h @ 571a7c5, Zero Hour
// configuration. The originals carry the following notices, preserved here:
//
//   VECTOR2I.h / VECTOR3I.h — Westwood 3D integer vector types.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! Integer vectors (`Vector2i`, `Vector3i`).
//!
//! Small value types used by the partition manager and the terrain/grid
//! code. `Vector3i16` (u16 components) is not ported: the game engine
//! itself does not use it.
//!
//! The default constructors of the C++ leave the fields uninitialized;
//! the Rust [`Default`] implementations are all-zero.

use core::ops::{Index, IndexMut};

/// A 2D integer vector (`Vector2i`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Vector2i {
    /// I component (`I`).
    pub i: i32,
    /// J component (`J`).
    pub j: i32,
}

impl Vector2i {
    /// Constructs a vector from its components.
    #[must_use]
    pub const fn new(i: i32, j: i32) -> Self {
        Self { i, j }
    }

    /// Sets both components (`Set`).
    pub fn set(&mut self, i: i32, j: i32) {
        self.i = i;
        self.j = j;
    }

    /// Swaps with `other` (`Swap`; the original used a xor-swap, which is
    /// the same observable behaviour).
    pub fn swap(&mut self, other: &mut Self) {
        core::mem::swap(&mut self.i, &mut other.i);
        core::mem::swap(&mut self.j, &mut other.j);
    }
}

impl Index<usize> for Vector2i {
    type Output = i32;

    #[inline]
    fn index(&self, n: usize) -> &i32 {
        match n {
            0 => &self.i,
            1 => &self.j,
            _ => panic!("Vector2i index out of range: {n}"),
        }
    }
}

impl IndexMut<usize> for Vector2i {
    #[inline]
    fn index_mut(&mut self, n: usize) -> &mut i32 {
        match n {
            0 => &mut self.i,
            1 => &mut self.j,
            _ => panic!("Vector2i index out of range: {n}"),
        }
    }
}

/// A 3D integer vector (`Vector3i`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Vector3i {
    /// I component (`I`).
    pub i: i32,
    /// J component (`J`).
    pub j: i32,
    /// K component (`K`).
    pub k: i32,
}

impl Vector3i {
    /// Constructs a vector from its components.
    #[must_use]
    pub const fn new(i: i32, j: i32, k: i32) -> Self {
        Self { i, j, k }
    }

    /// Swaps with `other` (`Swap`; the original used a xor-swap).
    pub fn swap(&mut self, other: &mut Self) {
        core::mem::swap(&mut self.i, &mut other.i);
        core::mem::swap(&mut self.j, &mut other.j);
        core::mem::swap(&mut self.k, &mut other.k);
    }
}

impl Index<usize> for Vector3i {
    type Output = i32;

    #[inline]
    fn index(&self, n: usize) -> &i32 {
        match n {
            0 => &self.i,
            1 => &self.j,
            2 => &self.k,
            _ => panic!("Vector3i index out of range: {n}"),
        }
    }
}

impl IndexMut<usize> for Vector3i {
    #[inline]
    fn index_mut(&mut self, n: usize) -> &mut i32 {
        match n {
            0 => &mut self.i,
            1 => &mut self.j,
            2 => &mut self.k,
            _ => panic!("Vector3i index out of range: {n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Vector2i, Vector3i};

    #[test]
    fn equality_is_componentwise() {
        assert_eq!(Vector2i::new(1, 2), Vector2i::new(1, 2));
        assert_ne!(Vector2i::new(1, 2), Vector2i::new(2, 1));
        assert_eq!(Vector3i::new(1, 2, 3), Vector3i::new(1, 2, 3));
        assert_ne!(Vector3i::new(1, 2, 3), Vector3i::new(1, 2, 4));
    }

    #[test]
    fn swap_exchanges_the_values() {
        let mut a = Vector2i::new(1, 2);
        let mut b = Vector2i::new(3, 4);
        a.swap(&mut b);
        assert_eq!(a, Vector2i::new(3, 4));
        assert_eq!(b, Vector2i::new(1, 2));

        let mut c = Vector3i::new(1, 2, 3);
        let mut d = Vector3i::new(4, 5, 6);
        c.swap(&mut d);
        assert_eq!(c, Vector3i::new(4, 5, 6));
        assert_eq!(d, Vector3i::new(1, 2, 3));
    }

    #[test]
    fn set_and_index() {
        let mut v = Vector2i::new(0, 0);
        v.set(7, 8);
        assert_eq!(v[0], 7);
        assert_eq!(v[1], 8);
        v[1] = 9;
        assert_eq!(v.j, 9);

        let w = Vector3i::new(1, 2, 3);
        assert_eq!(w[2], 3);
    }
}
