// Copyright 2022 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Variable Length Coding (VLC) using Exponential Golomb codes.

use bitvec::prelude::{BitVec, Msb0};

#[inline]
fn num_bits(mut x: u32) -> u8 {
    if x == 0 {
        return 1;
    }
    let mut value = 0;
    while x != 0 {
        x >>= 1;
        value += 1;
    }
    value
}

#[test]
fn test_num_bits() {
    for x in 0..100000 {
        let nbits1 = format!("{:b}", x).len();
        let nbits2 = num_bits(x);
        assert_eq!(nbits1, nbits2 as usize);
    }
}

/// Exponential-Golomb coding, for unsigned numbers.
///
/// See https://en.wikipedia.org/wiki/Exponential-Golomb_coding
#[inline]
fn bv_exp_golomb(bv: &mut BitVec<u8, Msb0>, x: u32) {
    let v = x + 1;
    let nbits = num_bits(v);

    let nbits_m_1 = nbits - 1;
    bv.extend((0..nbits_m_1).map(|_| false));

    for i in 0..nbits {
        let shift = nbits - 1 - i;
        let mask = 1u32 << shift;
        bv.push(v & mask != 0);
    }
}

#[test]
fn test_exp_goloumb() {
    fn exp_golomb(x: u32) -> Vec<bool> {
        let mut bv = BitVec::new();
        bv_exp_golomb(&mut bv, x);
        let mut result = vec![];
        for b in bv {
            result.push(b);
        }
        result
    }

    // tests from https://en.wikipedia.org/wiki/Exponential-Golomb_coding
    assert_eq!(exp_golomb(0), vec![true]);
    assert_eq!(exp_golomb(1), vec![false, true, false]);
    assert_eq!(exp_golomb(2), vec![false, true, true]);
    assert_eq!(exp_golomb(3), vec![false, false, true, false, false]);
    assert_eq!(exp_golomb(4), vec![false, false, true, false, true]);
    assert_eq!(exp_golomb(5), vec![false, false, true, true, false]);
    assert_eq!(exp_golomb(6), vec![false, false, true, true, true]);
    assert_eq!(
        exp_golomb(7),
        vec![false, false, false, true, false, false, false]
    );
    assert_eq!(
        exp_golomb(8),
        vec![false, false, false, true, false, false, true]
    );
}

/// Exponential-Golomb coding, with extension to negative numbers
///
/// See https://en.wikipedia.org/wiki/Exponential-Golomb_coding
#[inline]
fn bv_signed_exp_golomb(bv: &mut BitVec<u8, Msb0>, x: i32) {
    // this implementation is not very efficient.
    let code = if x > 0 { 2 * x - 1 } else { -2 * x };
    let code: u32 = code.try_into().unwrap();
    bv_exp_golomb(bv, code);
}

#[test]
fn test_signed_exp_goloumb() {
    fn signed_exp_golomb(x: i32) -> Vec<bool> {
        let mut bv = BitVec::new();
        bv_signed_exp_golomb(&mut bv, x);
        let mut result = vec![];
        for b in bv {
            result.push(b);
        }
        result
    }

    // tests from https://en.wikipedia.org/wiki/Exponential-Golomb_coding
    assert_eq!(signed_exp_golomb(0), vec![true]);
    assert_eq!(signed_exp_golomb(1), vec![false, true, false]);
    assert_eq!(signed_exp_golomb(-1), vec![false, true, true]);
    assert_eq!(signed_exp_golomb(2), vec![false, false, true, false, false]);
    assert_eq!(signed_exp_golomb(-2), vec![false, false, true, false, true]);
    assert_eq!(signed_exp_golomb(3), vec![false, false, true, true, false]);
    assert_eq!(signed_exp_golomb(-3), vec![false, false, true, true, true]);
    assert_eq!(
        signed_exp_golomb(4),
        vec![false, false, false, true, false, false, false]
    );
    assert_eq!(
        signed_exp_golomb(-4),
        vec![false, false, false, true, false, false, true]
    );
}

pub(crate) trait BitVecGolomb {
    fn extend_exp_golomb(&mut self, value: u32);
    fn extend_signed_exp_golomb(&mut self, value: i32);
}

impl BitVecGolomb for BitVec<u8, Msb0> {
    fn extend_exp_golomb(&mut self, value: u32) {
        bv_exp_golomb(self, value)
    }
    fn extend_signed_exp_golomb(&mut self, value: i32) {
        bv_signed_exp_golomb(self, value)
    }
}
