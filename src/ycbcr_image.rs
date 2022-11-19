// Copyright 2022 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Data representations for YCbCr image data

use super::*;

/// An image in YCbCr format
///
/// This references data stored elsewhere and provides only minimal metadata to
/// describe the actual image data.
///
/// The luma stride must be evenly divisible by 16 and the luma data size must
/// have an integer multiple of 16 rows. For chroma, this number is 8.
pub struct YCbCrImage<'a> {
    /// The data planes for the image
    pub planes: Planes<'a>,
    /// The width of the image, in pixels
    pub width: u32,
    /// The height of the image, in pixels
    pub height: u32,
}

impl<'a> YCbCrImage<'a> {
    pub(crate) fn luma_bit_depth(&self) -> BitDepth {
        match &self.planes {
            Planes::Mono(y) => y.bit_depth,
            Planes::YCbCr((y, _, _)) => y.bit_depth,
        }
    }
}

/// The data plane(s) within an [YCbCrImage].
pub enum Planes<'a> {
    //// Luminance only (monochrome) data.
    Mono(DataPlane<'a>),
    //// Luminance and chrominance data.
    YCbCr((DataPlane<'a>, DataPlane<'a>, DataPlane<'a>)),
}

/// Data for a single plane (luminance or chrominance) of an image.
///
/// The actual data are stored elsewhere and this provides metadata.
pub struct DataPlane<'a> {
    /// The image data
    pub data: &'a [u8],
    /// The row stride of the image data
    pub stride: usize,
    /// The bit depth of the image data
    pub bit_depth: BitDepth,
}

impl<'a> YCbCrImage<'a> {
    pub(crate) fn check_sizes(&self) -> Result<()> {
        match &self.planes {
            Planes::Mono(y_plane) | Planes::YCbCr((y_plane, _, _)) => {
                y_plane.check_sizes(self.width, self.height, 16)?;
            }
        }

        match &self.planes {
            Planes::Mono(_) => {}
            Planes::YCbCr((_, cb_plane, cr_plane)) => {
                for chroma_plane in [cb_plane, cr_plane] {
                    chroma_plane.check_sizes(self.width / 2, self.height / 2, 8)?;
                }
            }
        }
        Ok(())
    }
}

impl<'a> DataPlane<'a> {
    pub(crate) fn check_sizes(&self, width: u32, height: u32, mb_sz: u32) -> Result<()> {
        let (width_factor_num, width_factor_denom) = match self.bit_depth {
            BitDepth::Depth8 => (1, 1),
            BitDepth::Depth12 => (3, 2),
        };
        // Check width
        if self.stride
            < next_multiple(width, mb_sz) as usize * width_factor_num / width_factor_denom
        {
            return Err(Error::DataShapeProblem {
                msg: "stride too small",
                #[cfg(feature = "backtrace")]
                backtrace: Backtrace::capture(),
            });
        }
        // check height
        let num_rows = div_ceil(
            self.data.len().try_into().unwrap(),
            self.stride.try_into().unwrap(),
        );
        if num_rows < next_multiple(height, mb_sz) {
            return Err(Error::DataShapeProblem {
                msg: "number of rows too small",
                #[cfg(feature = "backtrace")]
                backtrace: Backtrace::capture(),
            });
        }

        Ok(())
    }
}
