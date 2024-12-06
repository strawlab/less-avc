// Copyright 2022-2023 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! High-level encoder to take frames into and emit NAL units.

#[cfg(feature = "backtrace")]
use std::backtrace::Backtrace;

use super::nal_unit::*;
use super::*;

/// Convert input images [YCbCrImage] into H.264 NAL units [NalUnit].
///
/// This high-level type brings together the steps of initiating an h.264
/// encoding session with a sequence parameter set and a picture parameter set
/// and then repeatedly appending pictures.
pub struct LessEncoder {
    width: u32,
    height: u32,
    mbs_width: usize,
    mbs_height: usize,
    sps: Sps,
    pps: Pps,
}

impl LessEncoder {
    /// Initialize an encoder and encode first frame.
    ///
    /// The sequence parameter set and picture parameter set are inferred from
    /// the input [YCbCrImage].
    pub fn new(y4m_frame: &YCbCrImage) -> Result<(InitialNalUnits, Self)> {
        let width = y4m_frame.width;
        let height = y4m_frame.height;

        let bit_depth = y4m_frame.luma_bit_depth();

        if let (Planes::YCbCr(_), BitDepth::Depth12, false) =
            (&y4m_frame.planes, &bit_depth, width % 4 == 0)
        {
            return Err(Error::DataShapeProblem {
                msg: "for bit depth 12 color, width must be divisible by 4",
                #[cfg(feature = "backtrace")]
                backtrace: Backtrace::capture(),
            });
        }

        match (&y4m_frame.planes, &bit_depth, width % 2 == 0) {
            (Planes::Mono(_), BitDepth::Depth8, false) | (_, _, true) => {}
            (_, _, false) => {
                return Err(Error::DataShapeProblem {
                    msg: "width must be divisible by 2 (except mono8)",
                    #[cfg(feature = "backtrace")]
                    backtrace: Backtrace::capture(),
                });
            }
        }

        #[allow(non_snake_case)]
        let (profile_idc, SubWidthC, SubHeightC) = match (&y4m_frame.planes, bit_depth) {
            (Planes::YCbCr(_), BitDepth::Depth8) => (ProfileIdc::baseline(), 2, 2),
            (Planes::Mono(_), BitDepth::Depth8) => (
                ProfileIdc::high(ChromaFormatIdc::Monochrome(bit_depth)),
                1,
                1,
            ),
            (Planes::Mono(_), BitDepth::Depth12) => (
                ProfileIdc::high444pp(ChromaFormatIdc::Monochrome(bit_depth)),
                1,
                1,
            ),
            (Planes::YCbCr(_), BitDepth::Depth12) => (
                ProfileIdc::high444pp(ChromaFormatIdc::Chroma420(bit_depth)),
                2,
                2,
            ),
        };

        let pic_width_in_mbs_minus1 = width.div_ceil(16) - 1;
        let pic_height_in_map_units_minus1 = height.div_ceil(16) - 1;

        let frame_cropping = if ((pic_width_in_mbs_minus1 + 1) * 16 != width)
            || ((pic_height_in_map_units_minus1 + 1) * 16 != height)
        {
            // full size of allocated space
            let padded_width = (pic_width_in_mbs_minus1 + 1) * 16;
            let padded_height = (pic_height_in_map_units_minus1 + 1) * 16;

            let lr_pad = padded_width - width;
            let tb_pad = padded_height - height;

            let lpad = 0;
            let tpad = 0;
            let rpad = lr_pad / SubWidthC;
            let bpad = tb_pad / SubHeightC;

            if (lpad * SubWidthC + width + rpad * SubWidthC != padded_width)
                || (tpad * SubHeightC + bpad * SubHeightC + height != padded_height)
            {
                return Err(crate::Error::UnsupportedImageSize {
                    #[cfg(feature = "backtrace")]
                    backtrace: Backtrace::capture(),
                });
            }

            Some([lpad, rpad, tpad, bpad])
        } else {
            None
        };

        // SPS
        let sps = Sps::new(
            profile_idc,
            pic_width_in_mbs_minus1,
            pic_height_in_map_units_minus1,
            frame_cropping,
            Some(Vui::new(true)),
        );
        let sps_nal_unit = NalUnit::new(
            NalRefIdc::Three,
            NalUnitType::SequenceParameterSet,
            sps.to_rbsp(),
        );

        // PPS
        let pps = Pps::new(0);
        let pps_nal_unit = NalUnit::new(
            NalRefIdc::Three,
            NalUnitType::PictureParameterSet,
            pps.to_rbsp(),
        );

        let mbs_width = (pic_width_in_mbs_minus1 + 1).try_into().unwrap();
        let mbs_height = (pic_height_in_map_units_minus1 + 1).try_into().unwrap();

        let mut self_ = Self {
            width,
            height,
            mbs_width,
            mbs_height,
            sps,
            pps,
        };

        let frame_nal_unit = self_.encode(y4m_frame)?;
        let nal_units = InitialNalUnits {
            sps: sps_nal_unit,
            pps: pps_nal_unit,
            frame: frame_nal_unit,
        };
        Ok((nal_units, self_))
    }

    /// Encode a frame, converting an input image [YCbCrImage] into [NalUnit].
    pub fn encode(&mut self, y4m_frame: &YCbCrImage) -> Result<NalUnit> {
        y4m_frame.check_sizes()?;

        debug_assert_eq!(self.width, y4m_frame.width);
        debug_assert_eq!(self.height, y4m_frame.height);

        let mut slice_data = SliceHeader::new().to_rbsp(&self.sps, &self.pps);

        let luma_only = self.sps.profile_idc.is_monochrome();

        let num_macroblocks = self.mbs_height * self.mbs_width;

        // reserve space for frame without requiring reallocation
        let row_sz = match y4m_frame.luma_bit_depth() {
            BitDepth::Depth8 => 16,
            BitDepth::Depth12 => 24,
        };
        let orig_len = slice_data.data.len();

        // space for macroblock data
        let mut reserve_size = if luma_only {
            // luma only in output
            num_macroblocks * row_sz * 16
        } else {
            // 4:2:0
            num_macroblocks * row_sz * 16 * 3 / 2
        };

        // space for header and final slice stop bit
        reserve_size +=
            (num_macroblocks - 1) * MacroblockType::I_PCM.as_encoded_macroblock_header().len() + 1;

        slice_data.data.reserve(reserve_size);

        for mbs_row in 0..self.mbs_height {
            for mbs_col in 0..self.mbs_width {
                // todo: look at mb_skip_flag and mb_skip_run for luma only images.
                macroblock(mbs_row, mbs_col, &mut slice_data, y4m_frame, luma_only);
            }
        }

        slice_data.data.push(0x80); // slice stop bit

        let final_len = slice_data.data.len();
        let should_have_reserved = final_len - orig_len;

        debug_assert_eq!(should_have_reserved, reserve_size);

        Ok(NalUnit::new(
            NalRefIdc::One,
            NalUnitType::CodedSliceOfAnIDRPicture,
            slice_data,
        ))
    }
}
