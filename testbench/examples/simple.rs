use testbench::*;

fn main() {
    let input_yuv = generate_image(&PixFmt::Rgb8, 1920, 1080).unwrap();
    let frame_view = input_yuv.view();
    let fd = std::fs::File::create("simple.h264").unwrap();
    let mut my_h264_writer = less_avc::H264Writer::new(fd).unwrap();
    for _ in 0..10 {
        my_h264_writer.write(&frame_view).unwrap();
    }
}
