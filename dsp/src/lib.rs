#![cfg_attr(not(test), no_std)]
#![cfg_attr(feature = "nightly", feature(asm, core_intrinsics))]

use core::ops::{Add, Mul, Neg};

fn abs<T>(x: T) -> T
where
    T: PartialOrd + Default + Neg<Output = T>,
{
    if x >= T::default() {
        x
    } else {
        -x
    }
}

// These are implemented here because core::f32 doesn't have them (yet).
// They are naive and don't handle inf/nan.
// `compiler-intrinsics`/llvm should have better (robust, universal, and
// faster) implementations.

fn copysign<T>(x: T, y: T) -> T
where
    T: PartialOrd + Default + Neg<Output = T>,
{
    if (x >= T::default() && y >= T::default())
        || (x <= T::default() && y <= T::default())
    {
        x
    } else {
        -x
    }
}

#[cfg(not(feature = "nightly"))]
fn max<T>(x: T, y: T) -> T
where
    T: PartialOrd,
{
    if x > y {
        x
    } else {
        y
    }
}

#[cfg(not(feature = "nightly"))]
fn min<T>(x: T, y: T) -> T
where
    T: PartialOrd,
{
    if x < y {
        x
    } else {
        y
    }
}

#[cfg(feature = "nightly")]
fn max(x: f32, y: f32) -> f32 {
    core::intrinsics::maxnumf32(x, y)
}

#[cfg(feature = "nightly")]
fn min(x: f32, y: f32) -> f32 {
    core::intrinsics::minnumf32(x, y)
}

// Multiply-accumulate vectors `x` and `a`.
//
// A.k.a. dot product.
// Rust/LLVM optimize this nicely.
fn macc<T>(y0: T, x: &[T], a: &[T]) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Copy,
{
    x.iter()
        .zip(a)
        .map(|(x, a)| *x * *a)
        .fold(y0, |y, xa| y + xa)
}

/// Combine high and low i32 into a single downscaled i32, saturating the type.
pub fn saturating_scale(lo: i32, hi: i32, shift: u32) -> i32 {
    debug_assert!(shift & 31 == shift);

    let shift_hi = 31 - shift;
    debug_assert!(shift_hi & 31 == shift_hi);

    let over = hi >> shift;
    if over < -1 {
        i32::MIN
    } else if over > 0 {
        i32::MAX
    } else {
        (lo >> shift) + (hi << shift_hi)
    }
}

mod atan2;
pub use atan2::*;
mod accu;
pub use accu::*;
mod complex;
pub use complex::*;
mod cossin;
pub use cossin::*;
pub mod iir;
pub mod iir_int;
mod lockin;
pub use lockin::*;
mod lowpass;
pub use lowpass::*;
mod pll;
pub use pll::*;
mod rpll;
pub use rpll::*;
mod unwrap;
pub use unwrap::*;

#[cfg(test)]
pub mod testing;
