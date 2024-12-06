// Copyright 2022-2023 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Network Abstraction Layer (NAL) encoding

use super::*;

/// Data to save a NAL unit
///
/// The data is in the raw byte sequence payload (RBSP) representation and gets
/// converted to a NAL unit by the [Self::to_annex_b_data] method.
pub struct NalUnit {
    ref_idc: NalRefIdc,
    unit_type: NalUnitType,
    rbsp_data: RbspData,
}

impl NalUnit {
    /// Create new [NalUnit].
    pub fn new(ref_idc: NalRefIdc, unit_type: NalUnitType, rbsp_data: RbspData) -> Self {
        Self {
            ref_idc,
            unit_type,
            rbsp_data,
        }
    }

    fn to_buf(&self, with_frame: bool) -> Vec<u8> {
        #[allow(clippy::identity_op)]
        // forbidden_zero_bit = 0
        let nal_byte = 0x00 | (self.ref_idc.nal_ref_idc() << 5) | self.unit_type.nal_unit_type();

        let rbsp_buf = &self.rbsp_data.data;
        let rbsp_size = rbsp_buf.len();
        let max_nal_buf_size = calc_max_nal_buf_size(rbsp_size);

        let n_start = if with_frame { 5 } else { 1 };
        let mut result = vec![0u8; n_start + max_nal_buf_size];
        if with_frame {
            result[..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        }
        result[n_start - 1] = nal_byte;

        let nal_buf_sz = rbsp_to_ebsp(&self.rbsp_data.data, &mut result[n_start..]);
        let final_sz = n_start + nal_buf_sz;
        result.truncate(final_sz);

        result
    }

    /// Return a single "naked" NAL unit.
    ///
    /// This is the encapsulated byte sequence payload (EBSP) without NALU
    /// Header.
    pub fn to_nal_unit(&self) -> Vec<u8> {
        self.to_buf(false)
    }
    /// Return a single NAL unit encoded for direct saving to `.h264` file.
    pub fn to_annex_b_data(&self) -> Vec<u8> {
        self.to_buf(true)
    }
}

/// Calculate the maximum possible NAL buffer size for a given RBSP size.
#[inline]
fn calc_max_nal_buf_size(rbsp_size: usize) -> usize {
    (div_ceil(rbsp_size as u32 * 3, 2) * 2).try_into().unwrap()
}

/// Convert Raw byte sequence payload (RBSP) data to Encapsulated Byte Sequence
/// Payload (EBSP) bytes.
pub(crate) fn rbsp_to_ebsp(rbsp_buf: &[u8], nal_buf: &mut [u8]) -> usize {
    let rbsp_size = rbsp_buf.len();
    let max_nal_buf_size = calc_max_nal_buf_size(rbsp_size);
    assert!(nal_buf.len() >= max_nal_buf_size);
    let mut dest_len = 0;

    let mut input_buf = rbsp_buf;

    while let Some(first_idx) = memchr::memchr(0x00, input_buf) {
        if first_idx + 1 < input_buf.len() {
            // more input exists
            if input_buf[first_idx + 1] == 0x00 {
                // two nulls in a row
                if first_idx + 2 < input_buf.len() {
                    // it is longer
                    let pos3 = input_buf[first_idx + 2];
                    if needs_protecting_in_pos3(pos3) {
                        let src = &input_buf[..first_idx + 2];
                        nal_buf[dest_len..dest_len + src.len()].copy_from_slice(src);
                        dest_len += src.len();
                        nal_buf[dest_len] = 0x03;
                        dest_len += 1;
                        input_buf = &input_buf[src.len()..];
                    } else {
                        let src = &input_buf[..first_idx + 2];
                        nal_buf[dest_len..dest_len + src.len()].copy_from_slice(src);
                        dest_len += src.len();
                        input_buf = &input_buf[src.len()..];
                    }
                } else {
                    // no more input
                    break;
                }
            } else {
                // next index is not null, use input up to and including null
                let src = &input_buf[..first_idx + 1];
                nal_buf[dest_len..dest_len + src.len()].copy_from_slice(src);
                dest_len += src.len();
                input_buf = &input_buf[src.len()..];
            }
        } else {
            // no more input
            break;
        }
    }

    if !input_buf.is_empty() {
        nal_buf[dest_len..dest_len + input_buf.len()].copy_from_slice(input_buf);
        dest_len += input_buf.len();
    }

    dest_len
}

#[inline]
/// Returns true if byte is 0x00, 0x01, 0x02 or 0x03.
fn needs_protecting_in_pos3(byte: u8) -> bool {
    matches!(byte, 0x00..=0x03)
}

#[test]
fn test_bad_byte() {
    assert!(needs_protecting_in_pos3(0x00));
    assert!(needs_protecting_in_pos3(0x01));
    assert!(needs_protecting_in_pos3(0x02));
    assert!(needs_protecting_in_pos3(0x03));
    assert!(!needs_protecting_in_pos3(0x04));
    for byte in 4..=255 {
        assert!(!needs_protecting_in_pos3(byte));
    }
}

#[test]
fn test_nal_encoding_roundtrip() {
    // `h264_reader::rbsp::decode_nal` trims the first byte.
    let test_vecs = [
        vec![0x68, 0x00],
        vec![0x68, 0x01],
        vec![0x68, 0x02],
        vec![0x68, 0x03],
        vec![0x68, 0x04],
        vec![0x68, 0x00, 0x00],
        vec![0x68, 0x00, 0x01],
        vec![0x68, 0x00, 0x02],
        vec![0x68, 0x00, 0x03],
        vec![0x68, 0x00, 0x04],
        vec![0x68, 0x00, 0x00, 0x00],
        vec![0x68, 0x00, 0x00, 0x01],
        vec![0x68, 0x00, 0x00, 0x02],
        vec![0x68, 0x00, 0x00, 0x03],
        vec![0x68, 0x00, 0x00, 0x04],
        vec![0x68, 0x00, 0x00, 0x00, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x01],
        vec![0x68, 0x00, 0x00, 0x00, 0x02],
        vec![0x68, 0x00, 0x00, 0x00, 0x03],
        vec![0x68, 0x00, 0x00, 0x00, 0x04],
        vec![0x68, 0x00, 0x00, 0x00, 0x05],
        vec![0x68, 0x03, 0x03, 0x03, 0x03],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x01, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x02, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x03, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x04, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x05, 0x00],
        vec![0x68, 0x03, 0x03, 0x03, 0x03, 0x03],
        vec![0x68, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00],
        vec![0x68, 0x00, 0x00, 0x03, 0x00, 0x00, 0x01],
        vec![0x68, 0x00, 0x00, 0x03, 0x00, 0x00, 0x02],
        vec![0x68, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03],
        vec![0x68, 0x00, 0x00, 0x03, 0x00, 0x00, 0x04],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
        vec![0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
    ];
    for orig in test_vecs.iter() {
        let mut encoded = vec![0u8; calc_max_nal_buf_size(orig.len())];
        let sz = rbsp_to_ebsp(orig, &mut encoded);
        encoded.truncate(sz);

        let decoded = h264_reader::rbsp::decode_nal(&encoded).unwrap();
        assert_eq!(&orig.as_slice()[1..], decoded.as_ref());
    }
}

/// Possible values for the `nal_ref_idc` field in the `nal_unit`.
///
/// Encodes to 2 bits.
pub enum NalRefIdc {
    // TODO: could these have better names?
    Zero,
    One,
    Two,
    Three,
}

impl NalRefIdc {
    pub(crate) fn nal_ref_idc(&self) -> u8 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// Possible values for the `nal_unit_type` field in `nal_unit`.
///
/// Encodes to 5 bits.
#[allow(dead_code)]
#[derive(PartialEq, Eq)]
#[non_exhaustive]
pub enum NalUnitType {
    /// Unspecified
    Unspecified,
    /// Coded slice of a non-IDR picture
    CodedSliceOfANonIDRPicture,
    /// Coded slice data partition A
    CodedSliceDataPartitionA,
    /// Coded slice data partition B
    CodedSliceDataPartitionB,
    /// Coded slice data partition C
    CodedSliceDataPartitionC,
    /// Coded slice of an IDR picture
    CodedSliceOfAnIDRPicture,
    /// Supplemental enhancement information (SEI)
    SupplementalEnhancementInformation,
    /// Sequence parameter set
    SequenceParameterSet,
    /// Picture parameter set
    PictureParameterSet,
    // There are more, which is why this is marked `non_exhaustive`.
}

impl NalUnitType {
    pub(crate) fn nal_unit_type(&self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::CodedSliceOfANonIDRPicture => 1,
            Self::CodedSliceDataPartitionA => 2,
            Self::CodedSliceDataPartitionB => 3,
            Self::CodedSliceDataPartitionC => 4,
            Self::CodedSliceOfAnIDRPicture => 5,
            Self::SupplementalEnhancementInformation => 6,
            Self::SequenceParameterSet => 7,
            Self::PictureParameterSet => 8,
        }
    }
}

/// The initial [NalUnit] returned when starting a [LessEncoder].
pub struct InitialNalUnits {
    /// sequence parameter set NAL unit
    pub sps: NalUnit,
    /// picture parameter set NAL unit
    pub pps: NalUnit,
    /// frame NAL unit
    pub frame: NalUnit,
}

impl InitialNalUnits {
    /// Return an [Iterator] over the NAL units generated at the start of encoding.
    pub fn into_iter(self) -> impl Iterator<Item = NalUnit> {
        vec![self.sps, self.pps, self.frame].into_iter()
    }
}
