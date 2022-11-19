#![feature(test)]
extern crate test;

#[cfg(test)]
mod bench {

    use test::Bencher;
    use testbench::*;

    fn bench_write(b: &mut Bencher, pixfmt: &PixFmt, width: u32, height: u32) {
        let input_yuv = generate_image(pixfmt, width, height).unwrap();
        let frame_view = input_yuv.view();
        let out_buf = std::io::Cursor::new(vec![]);
        let mut my_h264_writer = less_avc::H264Writer::new(out_buf).unwrap();
        b.iter(|| {
            my_h264_writer.write(&frame_view).unwrap();
        });
    }

    #[bench]
    fn encapsulate_raw(b: &mut Bencher) {
        let one_megabyte = less_avc::nal_unit::NalUnit::new(
            less_avc::nal_unit::NalRefIdc::Zero,
            less_avc::nal_unit::NalUnitType::CodedSliceOfAnIDRPicture,
            less_avc::RbspData {
                data: vec![0u8; 1024 * 1024],
            },
        );
        b.iter(|| {
            one_megabyte.to_annex_b_data();
        });
    }

    #[bench]
    fn mono12_1920x1080_write(b: &mut Bencher) {
        bench_write(b, &PixFmt::Mono12, 1920, 1080)
    }

    #[bench]
    fn mono8_1920x1080_write(b: &mut Bencher) {
        bench_write(b, &PixFmt::Mono8, 1920, 1080)
    }

    #[bench]
    fn mono8_1920x1080_encode_raw(b: &mut Bencher) {
        let input_yuv = generate_image(&PixFmt::Mono8, 1920, 1080).unwrap();
        let frame_view = input_yuv.view();
        let (_initial, mut encoder) = less_avc::LessEncoder::new(&frame_view).unwrap();
        b.iter(|| {
            encoder.encode(&frame_view).unwrap();
        });
    }

    #[bench]
    fn rgb12_1920x1080_write(b: &mut Bencher) {
        bench_write(b, &PixFmt::Rgb12, 1920, 1080)
    }

    #[bench]
    fn rgb8_1920x1080_write(b: &mut Bencher) {
        bench_write(b, &PixFmt::Rgb8, 1920, 1080)
    }
}
