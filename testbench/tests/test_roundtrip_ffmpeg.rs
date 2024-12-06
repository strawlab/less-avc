// Copyright 2022-2023 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use anyhow::Result;
use tiff::decoder::DecodingResult;

use testbench::*;

const ENV_VAR_NAME: &str = "LESSAVC_SAVE_TEST_H264";

fn do_save_output() -> bool {
    // Potentially do not delete temporary directory

    match std::env::var_os(ENV_VAR_NAME) {
        Some(v) => &v != "0",
        None => false,
    }
}

const WIDTHS: &[u32] = &[14, 16, 638, 640];
const HEIGHTS: &[u32] = &[14, 16, 478, 480];

#[test]
fn test_roundtrip_ffmpeg_mono8_even_widths() -> Result<()> {
    check_roundtrip_ffmpeg(PixFmt::Mono8, WIDTHS, HEIGHTS)?;
    Ok(())
}

#[test]
fn test_roundtrip_ffmpeg_mono12_even_widths() -> Result<()> {
    check_roundtrip_ffmpeg(PixFmt::Mono12, WIDTHS, HEIGHTS)?;
    Ok(())
}

#[test]
fn test_roundtrip_ffmpeg_rgb8_even_widths() -> Result<()> {
    check_roundtrip_ffmpeg(PixFmt::Rgb8, WIDTHS, HEIGHTS)?;
    Ok(())
}

#[test]
fn test_roundtrip_ffmpeg_rgb12_div4_widths() -> Result<()> {
    // Width must be divisible by 4 in this case.
    check_roundtrip_ffmpeg(PixFmt::Rgb12, &[12, 16, 636, 640], HEIGHTS)?;
    Ok(())
}

#[test]
fn test_roundtrip_ffmpeg_rgb12_even_widths() -> Result<()> {
    // Width must be divisible by 4 in this case.
    for width in WIDTHS.iter() {
        for height in HEIGHTS.iter() {
            let result = check_roundtrip_ffmpeg(PixFmt::Rgb12, &[*width], &[*height]);
            if width % 4 == 0 {
                assert!(result.is_ok());
            } else {
                match result {
                    Err(anyhow_err) => {
                        if let Some(e) = anyhow_err
                            .chain()
                            .next()
                            .unwrap()
                            .downcast_ref::<less_avc::Error>()
                        {
                            match e {
                                less_avc::Error::DataShapeProblem {
                                    msg: _,
                                    #[cfg(feature = "backtrace")]
                                        backtrace: _,
                                } => {}
                                other => {
                                    panic!("unexpected error {other}");
                                }
                            }
                        } else {
                            panic!("should return DataShapeProblem");
                        }
                    }
                    _other => {
                        panic!("should return DataShapeProblem");
                    }
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_roundtrip_ffmpeg_mono8_odd_widths() -> Result<()> {
    let pixfmt = PixFmt::Mono8;
    let widths = [15];
    let heights = [14];
    check_roundtrip_ffmpeg(pixfmt, &widths[..], &heights[..])?;
    Ok(())
}

fn check_roundtrip_ffmpeg(pixfmt: PixFmt, widths: &[u32], heights: &[u32]) -> Result<()> {
    let tmpdir = tempfile::tempdir()?;
    let base_path = tmpdir.path().to_path_buf();

    println!("temporary directory with files: {}", base_path.display());

    // Potentially do not delete temporary directory
    if do_save_output() {
        std::mem::forget(tmpdir); // do not drop it, so do not delete it
    } else {
        println!(
            "  This temporary directory will be deleted. (Set environment \
            variable \"{ENV_VAR_NAME}\" to keep it.)"
        );
    }

    let mut outputs = Vec::new();
    for width in widths.iter() {
        for height in heights.iter() {
            outputs.push((pixfmt.clone(), *width, *height));
        }
    }

    for (pixfmt, width, height) in outputs.iter() {
        let pixfmt_str = pixfmt.as_str();
        let output_name = format!("test_less-avc_{}_{}x{}.h264", pixfmt_str, width, height);
        let full_output_name = base_path.join(&output_name);

        println!("** {output_name}: h264 output from less-avc");

        let input_yuv = {
            let out_fd = std::fs::File::create(&full_output_name)?;
            let mut my_h264_writer = less_avc::H264Writer::new(out_fd)?;

            let input_yuv = generate_image(pixfmt, *width, *height)?;
            let frame_view = input_yuv.view();
            my_h264_writer.write(&frame_view)?;
            input_yuv
        };

        let tif_pix_fmt = match &pixfmt {
            PixFmt::Mono12 => "gray12",
            PixFmt::Rgb12 => "rgb48",
            PixFmt::Mono8 => "gray8",
            PixFmt::Rgb8 => "rgb24",
        };

        let mut input_image_decoder = input_yuv.to_image(&base_path)?;
        let mut decoder = ffmpeg_to_frame(&base_path, &output_name, tif_pix_fmt)?;

        let (decoded_width, decoded_height) = decoder.dimensions()?;

        assert_eq!(decoded_width, *width);
        assert_eq!(decoded_height, *height);

        match &pixfmt {
            PixFmt::Mono12 | PixFmt::Rgb12 => {
                // TODO: assert colorspace etc.
                let input_image = input_image_decoder.read_image()?;
                let colortype = input_image_decoder.colortype()?;
                let vals_12bit = if let DecodingResult::U16(vals) = input_image {
                    vals
                } else {
                    panic!()
                };
                let ffmpeg_image = decoder.read_image()?;
                let from_ffmpeg_16bit = if let DecodingResult::U16(vals) = ffmpeg_image {
                    vals
                } else {
                    panic!()
                };
                if pixfmt_str == "mono12" {
                    assert_eq!(colortype, tiff::ColorType::Gray(16));
                } else {
                    assert_eq!(pixfmt_str, "rgb12");
                    assert_eq!(colortype, tiff::ColorType::RGB(16));
                }
                println!("left: (raw) -> y4m --(ffmpeg)--> tiff");
                println!("right: (raw) -> less-avc --(ffmpeg)--> tiff");
                assert_eq!(vals_12bit, from_ffmpeg_16bit);
            }
            PixFmt::Mono8 | PixFmt::Rgb8 => {
                let input_image = input_image_decoder.read_image()?;
                let input_vals = if let DecodingResult::U8(vals) = input_image {
                    vals
                } else {
                    panic!()
                };
                let ffmpeg_image = decoder.read_image()?;
                let output_vals = if let DecodingResult::U8(vals) = ffmpeg_image {
                    vals
                } else {
                    panic!()
                };
                assert_eq!(input_image_decoder.colortype()?, decoder.colortype()?);
                assert_eq!(input_vals.len(), output_vals.len());
                println!("left: (raw) -> y4m -> tiff");
                println!("right: (raw) -> h264 -> tiff");
                assert_eq!(input_vals, output_vals);
            }
        }
    }

    Ok(())
}
