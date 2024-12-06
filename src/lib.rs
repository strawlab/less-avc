// Copyright 2022-2023 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! less Advanced Video Coding (H.264) encoding library
//!
//! This module contains a pure Rust implementation of an H.264 encoder. It is
//! simple ("less advanced"), and uses a small subset of the encoder features in
//! the H.264 specification. It was inspired by Ben Mesander's [World's Smallest
//! H.264
//! Encoder](https://www.cardinalpeak.com/blog/worlds-smallest-h-264-encoder).
//! In the present implementation, all data is encoded as a lossless PCM frame.
//! (Future updates could include other encoding possibilities.) Bit depths of 8
//! and 12 in monochrome and YCbCr colorspaces are supported. Tests ensure that
//! data is losslessly encoded.
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "backtrace", feature(error_generic_member_access))]
#![deny(unsafe_code)]

#[cfg(not(feature = "std"))]
extern crate core as std;

extern crate alloc;
use alloc::{vec, vec::Vec};

#[cfg(feature = "backtrace")]
use std::backtrace::Backtrace;

use bitvec::prelude::{BitVec, Msb0};

mod golomb;
use golomb::BitVecGolomb;

pub mod ycbcr_image;
use ycbcr_image::*;

pub mod nal_unit;
use nal_unit::*;

pub mod sei;

#[cfg(feature = "std")]
mod writer;
#[cfg(feature = "std")]
pub use writer::H264Writer;

mod encoder;
pub use encoder::LessEncoder;

// Error type ----------------------

/// An H.264 encoding error.
#[derive(Debug)]
pub enum Error {
    DataShapeProblem {
        msg: &'static str,
        #[cfg(feature = "backtrace")]
        backtrace: Backtrace,
    },
    UnsupportedFormat {
        #[cfg(feature = "backtrace")]
        backtrace: Backtrace,
    },
    UnsupportedImageSize {
        #[cfg(feature = "backtrace")]
        backtrace: Backtrace,
    },
    InconsistentState {
        #[cfg(feature = "backtrace")]
        backtrace: Backtrace,
    },
    #[cfg(feature = "std")]
    IoError {
        source: std::io::Error,
        #[cfg(feature = "backtrace")]
        backtrace: Backtrace,
    },
}
type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Error::IoError {
            source,
            #[cfg(feature = "backtrace")]
            backtrace: Backtrace::capture(),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::IoError {
                source,
                #[cfg(feature = "backtrace")]
                    backtrace: _,
            } => Some(source),
            _ => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            Error::DataShapeProblem {
                msg,
                #[cfg(feature = "backtrace")]
                    backtrace: _,
            } => {
                write!(f, "Image data shape is problematic: {msg}")
            }
            Error::UnsupportedFormat {
                #[cfg(feature = "backtrace")]
                    backtrace: _,
            } => {
                write!(f, "unsupported format")
            }
            Error::UnsupportedImageSize {
                #[cfg(feature = "backtrace")]
                    backtrace: _,
            } => {
                write!(f, "unsupported image size: even width and height required")
            }
            Error::InconsistentState {
                #[cfg(feature = "backtrace")]
                    backtrace: _,
            } => {
                write!(f, "internal error: inconsistent state")
            }
            #[cfg(feature = "std")]
            Error::IoError {
                source,
                #[cfg(feature = "backtrace")]
                    backtrace: _,
            } => {
                write!(f, "IO error: {source}")
            }
        }
    }
}

// Utility functions -------------------

#[inline]
fn next_multiple(a: u32, b: u32) -> u32 {
    a.div_ceil(b) * b
}

// H.264 definitions ------------------

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code, clippy::upper_case_acronyms)]
enum VideoFormat {
    Component,
    PAL,
    NTSC,
    SECAM,
    MAC,
    Unspecified,
    Reserved,
}

#[derive(Debug, PartialEq, Eq)]
struct TimingInfo {
    /// The number of time units of a clock operating at the frequency
    /// time_scale Hz that corresponds to one increment (called a clock tick) of
    /// a clock tick counter.
    num_units_in_tick: u32,

    /// The number of time units that pass in one second.
    time_scale: u32,

    /// If true, the temporal distance between the HRD output times of any two
    /// consecutive pictures in output order is constrained
    fixed_frame_rate_flag: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Vui {
    /// Whether intensity range in encoded signal uses full luma/chroma range.
    ///
    /// If false, the signal is "studio swing".
    full_range: bool,
    video_format: VideoFormat,
    timing_info: Option<TimingInfo>,
}

impl Vui {
    fn new(full_range: bool) -> Self {
        Self {
            full_range,
            video_format: VideoFormat::Unspecified,
            timing_info: None,
        }
    }

    fn append_to_rbsp(&self, bv: &mut BitVec<u8, Msb0>) {
        // vui_parameters( )
        // Annex E

        // aspect_ratio_info_present_flag 0
        bv.push(false);

        // overscan_info_present_flag 0
        bv.push(false);

        // video_signal_type_present_flag 1
        bv.push(true);

        // video_format
        let video_format_arr = match &self.video_format {
            VideoFormat::Component => [false, false, false],
            VideoFormat::PAL => [false, false, true],
            VideoFormat::NTSC => [false, true, false],
            VideoFormat::SECAM => [false, true, true],
            VideoFormat::MAC => [true, false, false],
            VideoFormat::Unspecified => [true, false, true],
            VideoFormat::Reserved => [true, true, true],
        };
        bv.extend(video_format_arr);

        // video_full_range_flag
        bv.push(self.full_range);

        // colour_description_present_flag 0
        bv.push(false);

        // chroma_loc_info_present_flag 0
        bv.push(false);

        // timing_info_present_flag
        if let Some(_timing_info) = &self.timing_info {
            todo!();
        } else {
            bv.push(false);
        }

        // nal_hrd_parameters_present_flag 0
        bv.push(false);

        // vcl_hrd_parameters_present_flag 0
        bv.push(false);

        // pic_struct_present_flag 0
        bv.push(false);

        // bitstream_restriction_flag 0
        bv.push(false);
    }
}

/// The dynamic range of the data, stored as number of bits.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BitDepth {
    /// 8 bit data
    Depth8,
    /// 12 bit data
    Depth12,
}

impl BitDepth {
    /// Return the number of bits
    pub fn num_bits(&self) -> u8 {
        match self {
            Self::Depth8 => 8,
            Self::Depth12 => 12,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ProfileIdc {
    Bare(u8),
    Extra((u8, ChromaFormatIdc)),
}

impl ProfileIdc {
    fn baseline() -> Self {
        Self::Bare(66)
    }
    fn high(chroma_format: ChromaFormatIdc) -> Self {
        Self::Extra((100, chroma_format))
    }
    fn high444pp(chroma_format: ChromaFormatIdc) -> Self {
        Self::Extra((244, chroma_format))
    }
    fn profile_idc_byte(&self) -> u8 {
        match self {
            Self::Bare(value) => *value,
            Self::Extra((value, _)) => *value,
        }
    }
    fn is_monochrome(&self) -> bool {
        match self {
            Self::Bare(_) | Self::Extra((_, ChromaFormatIdc::Chroma420(_))) => false,
            Self::Extra((_, ChromaFormatIdc::Monochrome(_))) => true,
        }
    }
    fn append_to_rbsp(&self, bv: &mut BitVec<u8, Msb0>) {
        match self {
            Self::Bare(_) => {}
            Self::Extra((_, chroma_format_idc)) => {
                let chroma_format_idc_value = chroma_format_idc.value();
                bv.extend_exp_golomb(chroma_format_idc_value);
                if chroma_format_idc_value == 3 {
                    // separate_colour_plane_flag 0
                    bv.push(false);
                }
                let bit_depth = match chroma_format_idc {
                    ChromaFormatIdc::Monochrome(bit_depth)
                    | ChromaFormatIdc::Chroma420(bit_depth) => bit_depth,
                };

                let bit_depth_luma_minus8 = bit_depth.num_bits() - 8;
                let bit_depth_chroma_minus8 = bit_depth.num_bits() - 8;
                bv.extend_exp_golomb(bit_depth_luma_minus8.into());
                bv.extend_exp_golomb(bit_depth_chroma_minus8.into());

                // qpprime_y_zero_transform_bypass_flag 0
                bv.push(false);
                // seq_scaling_matrix_present_flag 0
                bv.push(false);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum ChromaFormatIdc {
    Monochrome(BitDepth),
    // Theoretically, luma and chroma could have different bit depths, but
    // ffmpeg does not support this and thus it is difficult to test.
    Chroma420(BitDepth),
    // // Enabling these would require handling SubWidthC and SubHeightC not 2:
    // Chroma422,
    // Chroma444,
    // // separate planes would be handled separately.
}

impl ChromaFormatIdc {
    fn value(&self) -> u32 {
        match self {
            Self::Monochrome(_) => 0,
            Self::Chroma420(_) => 1,
            // Self::Chroma422 => 2,
            // Self::Chroma444 => 3,
        }
    }
}

/// Sequence parameter set
#[derive(Debug, PartialEq, Eq)]
struct Sps {
    profile_idc: ProfileIdc,
    pic_width_in_mbs_minus1: u32,
    pic_height_in_map_units_minus1: u32,
    frame_cropping: Option<[u32; 4]>,
    log2_max_frame_num_minus4: u32,
    pic_order_cnt_type: u32,
    log2_max_pic_order_cnt_lsb_minus4: u32,
    vui: Option<Vui>,
    // Future: expand with ability to set more parameters.
}

impl Sps {
    fn new(
        profile_idc: ProfileIdc,
        pic_width_in_mbs_minus1: u32,
        pic_height_in_map_units_minus1: u32,
        frame_cropping: Option<[u32; 4]>,
        vui: Option<Vui>,
    ) -> Self {
        Self {
            profile_idc,
            pic_width_in_mbs_minus1,
            pic_height_in_map_units_minus1,
            frame_cropping,
            log2_max_frame_num_minus4: 0,
            pic_order_cnt_type: 0,
            log2_max_pic_order_cnt_lsb_minus4: 0,
            vui,
        }
    }

    #[allow(dead_code)]
    fn log2_max_frame_num(&self) -> u32 {
        self.log2_max_frame_num_minus4 + 4
    }

    #[allow(dead_code)]
    fn log2_max_pic_order_cnt_lsb(&self) -> u32 {
        self.log2_max_pic_order_cnt_lsb_minus4 + 4
    }
    fn to_rbsp(&self) -> RbspData {
        // Payload
        // profile_idc
        let profile_idc = self.profile_idc.profile_idc_byte();

        // constraint_set0_flag = 0
        // constraint_set1_flag = 0
        // constraint_set2_flag = 0
        // constraint_set3_flag = 0
        // constraint_set4_flag = 0
        // constraint_set5_flag = 0
        // reserved_zero_2bits = 0
        let reserved = 0x00;

        // level_idc = 10
        let level_idc = 10;

        let start = vec![profile_idc, reserved, level_idc];
        let mut bv: BitVec<u8, Msb0> = BitVec::from_vec(start);

        // seq_parameter_set_id = 0
        bv.extend_exp_golomb(0);

        // chroma_format_idc etc if in the correct `profile_idc`.
        self.profile_idc.append_to_rbsp(&mut bv);

        bv.extend_exp_golomb(self.log2_max_frame_num_minus4);

        // pic_order_cnt_type
        bv.extend_exp_golomb(self.pic_order_cnt_type);

        // log2_max_pic_order_cnt_lsb_minus4
        bv.extend_exp_golomb(self.log2_max_pic_order_cnt_lsb_minus4);

        // max_num_ref_frames
        bv.extend_exp_golomb(0);

        // gaps_in_frame_num_value_allowed_flag = 0
        bv.push(false);

        // pic_width_in_mbs_minus1
        bv.extend_exp_golomb(self.pic_width_in_mbs_minus1);

        // pic_height_in_map_units_minus1
        bv.extend_exp_golomb(self.pic_height_in_map_units_minus1);

        // frame_mbs_only_flag = 1
        bv.push(true);

        // direct_8x8_inference_flag = 0
        bv.push(false);

        if let Some(lrtb) = &self.frame_cropping {
            // frame_cropping_flag = 1
            bv.push(true);
            for frame_crop_offset in lrtb.iter() {
                bv.extend_exp_golomb(*frame_crop_offset);
            }
        } else {
            // frame_cropping_flag = 0
            bv.push(false);
        }

        match &self.vui {
            None => {
                // vui_prameters_present_flag = 0
                bv.push(false);
            }
            Some(vui) => {
                bv.push(true);
                vui.append_to_rbsp(&mut bv);
            }
        }

        // rbsp_stop_one_bit = 1
        bv.push(true);

        RbspData::new(bv.into_vec())
    }
}

/// Picture parameter set
#[derive(PartialEq, Eq)]
struct Pps {
    pic_parameter_set_id: u32,
    // In the future: expand with ability to set some parameters.
}

impl Pps {
    fn new(pic_parameter_set_id: u32) -> Self {
        Self {
            pic_parameter_set_id,
        }
    }

    fn to_rbsp(&self) -> RbspData {
        // Payload

        let mut bv: BitVec<u8, Msb0> = BitVec::with_capacity(20 * 8); // 20 bytes should be enough

        bv.extend_exp_golomb(self.pic_parameter_set_id);

        // seq_parameter_set_id = 0
        bv.extend_exp_golomb(0);

        // entropy_coding_mode_flag = 0
        bv.push(false);

        // bottom_field_pic_order_in_frame_present_flag = 0
        bv.push(false);

        // num_slice_groups_minus1 = 0
        bv.extend_exp_golomb(0);

        // num_ref_idx_l0_default_active_minus1 = 0
        bv.extend_exp_golomb(0);

        // num_ref_idx_l1_default_active_minus1 = 0
        bv.extend_exp_golomb(0);

        // weighted_pred_flag = 0
        bv.push(false);

        // weighted_bipred_idc = 0
        bv.push(false);
        bv.push(false);

        // pic_init_qp_minus26 = 0
        bv.extend_signed_exp_golomb(0);

        // pic_init_qs_minus26 = 0
        bv.extend_signed_exp_golomb(0);

        // chroma_qp_index_offset
        bv.extend_signed_exp_golomb(0);

        // deblocking_filter_control_present_flag = 0
        bv.push(false);

        // constrained_intra_pred_flag = 0
        bv.push(false);

        // redundant_pic_cnt_present_flag = 0
        bv.push(false);

        // rbsp_trailing_bits( )
        bv.push(true);

        RbspData::new(bv.into_vec())
    }
}

struct SliceHeader {}

impl SliceHeader {
    fn new() -> Self {
        Self {}
    }

    fn to_rbsp(&self, sps: &Sps, pps: &Pps) -> RbspData {
        // We are `slice_layer_without_partitioning_rbsp` because we have
        // nal_unit_type 5 (NalUnitType::CodedSliceOfAnIDRPicture). Also
        // `IdrPicFlag` is 1 for the same reason.

        // Payload

        let mut bv: BitVec<u8, Msb0> = BitVec::with_capacity(20 * 8); // 20 bytes should be enough for slice header

        // first_mb_in_slice = 0
        bv.extend_exp_golomb(0);

        // slice_type = 7 (I)
        bv.extend_exp_golomb(7);

        bv.extend_exp_golomb(pps.pic_parameter_set_id);

        // colour_plane: None,

        // frame_num = 0
        let n_bits = sps.log2_max_frame_num();
        for _ in 0..n_bits {
            bv.push(false);
        }

        // idr_pic_id = 0
        bv.extend_exp_golomb(0);

        if sps.pic_order_cnt_type == 0 {
            // pic_order_cnt_lsb = 0
            let n_bits = sps.log2_max_pic_order_cnt_lsb();
            for _ in 0..n_bits {
                bv.push(false);
            }
        } else {
            todo!();
        }

        // dec_ref_pic_marking
        //   no_output_of_prior_pics_flag u(1)
        bv.push(true);

        //   long_term_reference_flag u(1)
        bv.push(false);

        // slice_qp_delta = 0
        bv.extend_signed_exp_golomb(0);

        // For the first macroblock, the macroblock type (mb_type) is read without
        // aligning to a byte boundary. This would explain why we must put this here
        // rather than in the first macroblock.
        bv.extend_exp_golomb(MacroblockType::I_PCM.mb_type());

        RbspData::new(bv.into_vec())
    }
}

#[allow(non_camel_case_types)]
enum MacroblockType {
    // I_NxN,
    I_PCM,
}

impl MacroblockType {
    #[inline]
    fn mb_type(&self) -> u32 {
        match self {
            // Self::I_NxN => 0,
            Self::I_PCM => 25,
        }
    }
    /// This is an opimization to compile time.
    #[inline]
    const fn as_encoded_macroblock_header(&self) -> &'static [u8] {
        match self {
            Self::I_PCM => &[0x0D, 0x00],
        }
    }
}

#[test]
fn test_macroblock_header() {
    {
        let typ = &MacroblockType::I_PCM;
        let mut bv: BitVec<u8, Msb0> = BitVec::new();
        bv.extend_exp_golomb(typ.mb_type());
        let macroblock_header_dynamic = bv.as_raw_slice();
        dbg!(macroblock_header_dynamic);
        let macroblock_header_static = typ.as_encoded_macroblock_header();
        assert_eq!(macroblock_header_static, macroblock_header_dynamic);
    }
}

#[inline]
fn copy_to_macroblock_8bit(
    mbs_row: usize,
    mbs_col: usize,
    src_plane: &DataPlane,
    dest: &mut Vec<u8>,
    dest_sz: usize,
) {
    // `dest_sz` will be 16 when copying luma block and 8 when copying 4:2:0
    // chroma block.
    let src_data = src_plane.data;
    let src_stride = src_plane.stride;
    for src_row in (mbs_row * dest_sz)..((mbs_row + 1) * dest_sz) {
        // This copies beyond end of source pixel data but still within source
        // buffer.
        let row_chunk = &src_data[src_row * src_stride..(src_row + 1) * src_stride];
        let chunk = &row_chunk[mbs_col * dest_sz..(mbs_col + 1) * dest_sz];
        dest.extend(chunk);
    }
}

#[inline]
fn copy_to_macroblock_12bit(
    mbs_row: usize,
    mbs_col: usize,
    src_plane: &DataPlane,
    dest: &mut Vec<u8>,
    dest_sz: usize,
) {
    // `dest_sz` will be 24 when copying luma block and 12 when copying 4:2:0
    // chroma block.
    let src_data = src_plane.data;
    let src_stride = src_plane.stride;
    let src_sz = dest_sz / 3 * 2;
    for src_row in (mbs_row * src_sz)..((mbs_row + 1) * src_sz) {
        // This copies beyond end of source pixel data but still within source
        // buffer.
        let row_chunk = &src_data[src_row * src_stride..(src_row + 1) * src_stride];
        let chunk = &row_chunk[mbs_col * dest_sz..(mbs_col + 1) * dest_sz];
        dest.extend(chunk);
    }
}

#[inline]
fn macroblock(
    mbs_row: usize,
    mbs_col: usize,
    result: &mut RbspData,
    y4m_frame: &YCbCrImage,
    luma_only: bool,
) {
    if !(mbs_row == 0 && mbs_col == 0) {
        result
            .data
            .extend(MacroblockType::I_PCM.as_encoded_macroblock_header());
    }

    match &y4m_frame.planes {
        Planes::Mono(y_plane) | Planes::YCbCr((y_plane, _, _)) => match y_plane.bit_depth {
            BitDepth::Depth8 => {
                copy_to_macroblock_8bit(mbs_row, mbs_col, y_plane, &mut result.data, 16);
            }
            BitDepth::Depth12 => {
                copy_to_macroblock_12bit(mbs_row, mbs_col, y_plane, &mut result.data, 24);
            }
        },
    }

    if luma_only {
        return;
    }

    match &y4m_frame.planes {
        Planes::Mono(y_plane) => {
            assert_eq!(y_plane.bit_depth, BitDepth::Depth8);
            // 2 macroblocks of chrominance at 8x8 each
            result.data.extend(vec![128u8; 2 * 8 * 8]);
        }
        Planes::YCbCr((_, u_plane, v_plane)) => {
            assert_eq!(u_plane.bit_depth, v_plane.bit_depth);
            match u_plane.bit_depth {
                BitDepth::Depth8 => {
                    copy_to_macroblock_8bit(mbs_row, mbs_col, u_plane, &mut result.data, 8);
                    copy_to_macroblock_8bit(mbs_row, mbs_col, v_plane, &mut result.data, 8);
                }
                BitDepth::Depth12 => {
                    copy_to_macroblock_12bit(mbs_row, mbs_col, u_plane, &mut result.data, 12);
                    copy_to_macroblock_12bit(mbs_row, mbs_col, v_plane, &mut result.data, 12);
                }
            }
        }
    }
}

/// Raw byte sequence payload (RBSP) data.
///
/// This is merely a newtype to indicate the type of data help within the
/// `Vec<u8>`.
#[derive(Clone)]
pub struct RbspData {
    /// Raw byte sequence payload (RBSP) data.
    pub data: Vec<u8>,
}

impl RbspData {
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Data from https://www.cardinalpeak.com/blog/worlds-smallest-h-264-encoder
    const HELLO_SPS: &[u8] = &[
        0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x0a, 0xf8, 0x41, 0xa2,
    ];
    const HELLO_PPS: &[u8] = &[0x00, 0x00, 0x00, 0x01, 0x68, 0xce, 0x38, 0x80];
    // Original slice header data had a bug in that it is not spec compliant.
    // Here I have fixed it.
    const FIXED_HELLO_SLICE_HEADER: &[u8] = &[0, 0, 0, 1, 37, 136, 132, 40, 104];
    // And here is the original slice header.
    const _HELLO_SLICE_HEADER: &[u8] = &[0x00, 0x00, 0x00, 0x01, 0x05, 0x88, 0x84, 0x21, 0xa0];
    const HELLO_MACROBLOCK_HEADER: &[u8] = &[0x0d, 0x00];

    use h264_reader::{
        nal::{pps::PicParameterSet, sps::SeqParameterSet, Nal, RefNal},
        rbsp::BitReader,
        Context,
    };

    #[test]
    fn test_next_multiple() {
        assert_eq!(next_multiple(10, 16), 16);
        assert_eq!(next_multiple(11, 16), 16);
        assert_eq!(next_multiple(15, 16), 16);
        assert_eq!(next_multiple(16, 16), 16);
        assert_eq!(next_multiple(17, 16), 32);
    }

    #[test]
    fn test_encode() {
        use h264_reader::rbsp::decode_nal;
        use std::ops::Deref;

        let nal_with_escape = &b"\x67\x64\x00\x0A\xAC\x72\x84\x44\x26\x84\x00\x00\x03\x00\x04\x00\x00\x03\x00\xCA\x3C\x48\x96\x11\x80"[..];
        let rbsp = &b"\x64\x00\x0a\xac\x72\x84\x44\x26\x84\x00\x00\x00\x04\x00\x00\x00\xca\x3c\x48\x96\x11\x80"[..];

        // `decode_nal` consumes the first byte as the NAL header
        assert_eq!(decode_nal(nal_with_escape).unwrap().deref(), rbsp);

        let mut nal2 = vec![0u8; rbsp.len() * 3];
        let sz = rbsp_to_ebsp(rbsp, &mut nal2);
        nal2.truncate(sz);
        nal2.insert(0, 0); // `decode_nal` needs first byte

        assert_eq!(decode_nal(&nal2).unwrap().deref(), rbsp);
    }

    #[test]
    fn test_sps() {
        let width = 128u32;
        let height = 96u32;

        let pic_width_in_mbs_minus1 = width.div_ceil(16) - 1;
        let pic_height_in_map_units_minus1 = height.div_ceil(16) - 1;

        let payload = Sps::new(
            ProfileIdc::baseline(),
            pic_width_in_mbs_minus1,
            pic_height_in_map_units_minus1,
            None,
            None,
        )
        .to_rbsp();
        let encoded = NalUnit::new(
            NalRefIdc::Three,
            NalUnitType::SequenceParameterSet,
            payload.clone(),
        )
        .to_annex_b_data();
        assert_eq!(&encoded, HELLO_SPS);

        let sps = SeqParameterSet::from_bits(BitReader::new(&payload.data[..])).unwrap();
        let sps2 = RefNal::new(&encoded[4..], &[], true);
        let sps2 = SeqParameterSet::from_bits(sps2.rbsp_bits()).unwrap();
        assert_eq!(format!("{sps:?}"), format!("{sps2:?}")); // compare debug representations

        assert_eq!(sps.pic_width_in_mbs_minus1, pic_width_in_mbs_minus1);
        assert_eq!(
            sps.pic_height_in_map_units_minus1,
            pic_height_in_map_units_minus1
        );
    }

    #[test]
    fn test_pps() {
        let payload = Pps::new(0).to_rbsp();
        let encoded = NalUnit::new(
            NalRefIdc::Three,
            NalUnitType::PictureParameterSet,
            payload.clone(),
        )
        .to_annex_b_data();

        assert_eq!(&encoded, HELLO_PPS);

        let sps = SeqParameterSet::from_bits(BitReader::new(&HELLO_SPS[5..])).unwrap();

        let mut ctx = Context::default();
        ctx.put_seq_param_set(sps);
        let _pps = PicParameterSet::from_bits(&ctx, BitReader::new(&payload.data[..])).unwrap();
    }

    #[test]
    fn test_slice_header() {
        let sps = Sps::new(ProfileIdc::baseline(), 5, 5, None, None);
        let pps = Pps::new(0);
        let payload = SliceHeader::new().to_rbsp(&sps, &pps);

        let sps = SeqParameterSet::from_bits(BitReader::new(&HELLO_SPS[5..])).unwrap();
        let mut ctx = Context::default();
        ctx.put_seq_param_set(sps);
        let pps = PicParameterSet::from_bits(&ctx, BitReader::new(&HELLO_PPS[5..])).unwrap();
        ctx.put_pic_param_set(pps);

        fn dbg_hex(vals: &[u8]) -> &[u8] {
            println!();
            for v in vals.iter() {
                println!("{:03} 0b{:08b} 0x{:02x}", v, v, v);
            }
            println!();
            vals
        }

        let encoded = NalUnit::new(
            NalRefIdc::One,
            NalUnitType::CodedSliceOfAnIDRPicture,
            payload,
        )
        .to_annex_b_data();

        let hello_slice_header = {
            let nal = RefNal::new(&FIXED_HELLO_SLICE_HEADER[4..], &[], true);
            let hello_slice_header = h264_reader::nal::slice::SliceHeader::from_bits(
                &ctx,
                &mut nal.rbsp_bits(),
                nal.header().unwrap(),
            )
            .unwrap()
            .0;
            hello_slice_header
        };

        let nal = RefNal::new(&encoded[4..], &[], true);
        let slice_header = h264_reader::nal::slice::SliceHeader::from_bits(
            &ctx,
            &mut nal.rbsp_bits(),
            nal.header().unwrap(),
        )
        .unwrap()
        .0;

        assert_eq!(
            format!("{:?}", hello_slice_header),
            format!("{:?}", slice_header)
        );
        assert_eq!(dbg_hex(&encoded), dbg_hex(FIXED_HELLO_SLICE_HEADER));
    }

    #[test]
    fn test_macroblock() {
        let mut bv: BitVec<u8, Msb0> = BitVec::new();
        bv.extend_exp_golomb(MacroblockType::I_PCM.mb_type());
        let macroblock_header = bv.as_raw_slice();

        assert_eq!(macroblock_header, HELLO_MACROBLOCK_HEADER);
    }
}
