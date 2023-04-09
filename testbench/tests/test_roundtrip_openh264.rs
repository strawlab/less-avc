// Copyright 2022-2023 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use testbench::*;

#[test]
fn test_roundtrip_openh264() -> anyhow::Result<()> {
    let mut outputs = Vec::new();
    // openh264 does not seem to support chroma_format_idc=0 (monochrome)
    for pixfmt in ["rgb8"].iter() {
        for width in [16u32, 640, 30, 32].iter() {
            for height in [16u32, 480, 30, 32].iter() {
                outputs.push((pixfmt.to_string(), *width, *height));
            }
        }
    }

    for (pixfmt_str, width, height) in outputs.iter() {
        let output_name = format!("test-movie-{}-{}x{}.h264", pixfmt_str, width, height);
        println!("testing {}", output_name);

        let mut my_h264_writer = less_avc::H264Writer::new(vec![])?;

        let input_yuv = generate_image(pixfmt_str, *width, *height)?;
        let frame_view = input_yuv.view();
        my_h264_writer.write(&frame_view)?;

        // Get Vec<u8> buffer with h264 NAL units.
        let h264_raw_buf = my_h264_writer.into_inner();

        // Potentially save to disk for inspection.
        let do_save = match std::env::var_os("LESSAVC_SAVE_TEST_H264") {
            Some(v) => &v != "0",
            None => false,
        };

        if do_save {
            use std::io::Write;
            let mut fd = std::fs::File::create(&output_name)?;
            fd.write_all(&h264_raw_buf)?;
        }

        // decode a single frame
        let mut decoder = openh264::decoder::Decoder::new()?;
        let decoded_yuv = decoder.decode(&h264_raw_buf)?.unwrap();

        let (oys, ous, ovs) = decoded_yuv.strides_yuv();

        // luma test
        let input_valid_size = input_yuv.view_luma().stride * input_yuv.height as usize;
        for (input_y_row, decoded_y_row) in input_yuv.view_luma().data[..input_valid_size]
            .chunks_exact(input_yuv.view_luma().stride)
            .zip(decoded_yuv.y_with_stride().chunks_exact(oys))
        {
            assert_eq!(
                decoded_y_row[..*width as usize],
                input_y_row[..*width as usize]
            );
        }

        // compare chroma data
        let (cb_plane, cr_plane) = match input_yuv.planes {
            MyPlanes::Mono(_) => {
                let stride = next_multiple(width / 2, 8) as usize;
                let alloc_rows = next_multiple(height / 2, 8) as usize;
                let data = vec![128u8; stride * alloc_rows];
                let chroma_plane = MyImagePlane::new(data, stride)?;
                (chroma_plane.clone(), chroma_plane)
            }
            MyPlanes::YCbCr((_, cb_plane, cr_plane)) => (cb_plane, cr_plane),
        };

        let input_valid_size = cb_plane.stride * (input_yuv.height / 2) as usize;
        let chroma_width = (width / 2) as usize;

        // chroma U test
        for (input_u_row, decoded_u_row) in cb_plane.data[..input_valid_size]
            .chunks_exact(cb_plane.stride)
            .zip(decoded_yuv.u_with_stride().chunks_exact(ous))
        {
            assert_eq!(decoded_u_row[..chroma_width], input_u_row[..chroma_width]);
        }

        // chroma V test
        for (input_v_row, decoded_v_row) in cr_plane.data[..input_valid_size]
            .chunks_exact(cr_plane.stride)
            .zip(decoded_yuv.v_with_stride().chunks_exact(ovs))
        {
            assert_eq!(decoded_v_row[..chroma_width], input_v_row[..chroma_width]);
        }
    }

    Ok(())
}

fn generate_image(fmt: &str, width: u32, height: u32) -> anyhow::Result<MyYCbCrImage> {
    // luma
    let stride = next_multiple(width, 16) as usize;
    let alloc_rows = next_multiple(height, 16) as usize;

    let mut data = vec![0u8; stride * alloc_rows];

    let image_row_mono8: Vec<u8> = (0..width)
        .map(|idx| ((idx as f64) * 255.0 / (width - 1) as f64) as u8)
        .collect();

    for row in 0..height {
        let start_idx: usize = row as usize * stride;
        let dest_row = &mut data[start_idx..(start_idx + width as usize)];
        dest_row.copy_from_slice(&image_row_mono8);
    }

    let luma_plane = MyImagePlane::new(data, stride)?;
    match fmt {
        "mono8" => Ok(MyYCbCrImage::new(
            MyPlanes::Mono(luma_plane),
            width,
            height,
        )?),
        "rgb8" => {
            // chroma
            let stride = next_multiple(width / 2, 8) as usize;
            let alloc_rows = next_multiple(height / 2, 8) as usize;
            let data = vec![128u8; stride * alloc_rows];
            let chroma_plane = MyImagePlane::new(data, stride)?;

            Ok(MyYCbCrImage::new(
                MyPlanes::YCbCr((luma_plane, chroma_plane.clone(), chroma_plane)),
                width,
                height,
            )?)
        }
        _ => {
            panic!("unknown pix format");
        }
    }
}
