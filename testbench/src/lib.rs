use anyhow::{Context, Result};
use less_avc::{
    ycbcr_image::{DataPlane, Planes, YCbCrImage},
    BitDepth,
};

use tiff::decoder::Decoder as TiffDecoder;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PixFmt {
    Mono8,
    Mono12,
    Rgb8,
    Rgb12,
}

impl PixFmt {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mono8 => "mono8",
            Self::Mono12 => "mono12",
            Self::Rgb8 => "rgb8",
            Self::Rgb12 => "rgb12",
        }
    }
}

#[derive(Clone)]
pub struct MyImagePlane {
    pub data: Vec<u8>,
    pub stride: usize,
    pub bit_depth: BitDepth,
}

impl MyImagePlane {
    pub fn new(data: Vec<u8>, stride: usize) -> Result<Self> {
        Ok(Self {
            data,
            stride,
            bit_depth: BitDepth::Depth8,
        })
    }
    fn new_bit_depth(data: Vec<u8>, stride: usize, bit_depth: BitDepth) -> Result<Self> {
        Ok(Self {
            data,
            stride,
            bit_depth,
        })
    }
    fn to_trimmed_y4m(&self, width: u32, height: u32) -> Result<Vec<u8>> {
        // src is often bigger then dest, so trim it.
        match self.bit_depth {
            BitDepth::Depth8 => {
                let width: usize = width.try_into().unwrap();
                let height: usize = height.try_into().unwrap();
                let mut result = vec![0u8; width * height];
                for (src_row, dest_row) in self
                    .data
                    .chunks_exact(self.stride)
                    .zip(result.chunks_exact_mut(width))
                {
                    dest_row.copy_from_slice(&src_row[..width]);
                }
                Ok(result)
            }
            BitDepth::Depth12 => {
                let width: usize = width.try_into().unwrap();
                if width % 2 != 0 {
                    anyhow::bail!("luma or chroma plane width must be multiple of 2");
                }
                let height: usize = height.try_into().unwrap();
                let dest_stride = width * 2;
                let mut result = vec![0u8; dest_stride * height];
                for (src_row, dest_row) in self
                    .data
                    .chunks_exact(self.stride)
                    .zip(result.chunks_exact_mut(dest_stride))
                {
                    let unpacked = unpack12be_to_16le(&src_row[..(width * 3 / 2)]);
                    // dest_row.copy_from_slice(&src_row[..stride]);
                    dest_row.copy_from_slice(&unpacked[..]);
                }
                Ok(result)
            }
        }
    }
}

#[inline]
fn pack_to_12be(vals: &[u16]) -> [u8; 3] {
    debug_assert_eq!(vals.len(), 2);
    let v0 = vals[0];
    let v1 = vals[1];
    debug_assert_eq!(v0 & 0xF000, 0);
    debug_assert_eq!(v1 & 0xF000, 0);
    [
        ((v0 & 0x0FF0) >> 4) as u8,
        ((v0 & 0x000F) << 4) as u8 | ((v1 & 0x0F00) >> 8) as u8,
        (v1 & 0x00FF) as u8,
    ]
}

#[inline]
fn unpack_12be(bytes: &[u8]) -> [u16; 2] {
    debug_assert_eq!(bytes.len(), 3);
    let b0 = bytes[0];
    let b1 = bytes[1];
    let b2 = bytes[2];
    [
        ((b0 as u16) << 4) | (((b1 & 0xF0) as u16) >> 4),
        (((b1 & 0x0F) as u16) << 8) | (b2 as u16),
    ]
}

#[test]
fn test_pack_12be() {
    fn check_pack_12be(orig: [u16; 2]) {
        let packed = pack_to_12be(&orig);
        let unpacked = unpack_12be(&packed);
        assert_eq!(orig, unpacked);
    }
    check_pack_12be([0x0123, 0x0FFF]);
    check_pack_12be([0x0FFF, 0x0123]);
    check_pack_12be([0x0000, 0x0FFF]);
    check_pack_12be([0x0FFF, 0x0000]);
}

fn unpack12be_to_16le(packed: &[u8]) -> Vec<u8> {
    let dest_size = packed.len() / 3 * 4;
    let mut dest = Vec::with_capacity(dest_size);
    let src_iter = packed.chunks_exact(3);
    // debug_assert_eq!(src_iter.remainder().len(), 0);
    for packed in src_iter {
        let vals = unpack_12be(&packed);
        let unpacked0 = vals[0].to_le_bytes();
        let unpacked1 = vals[1].to_le_bytes();
        dest.extend(unpacked0);
        dest.extend(unpacked1);
    }
    dest
}

#[derive(Clone)]
pub enum MyPlanes {
    Mono(MyImagePlane),
    YCbCr((MyImagePlane, MyImagePlane, MyImagePlane)),
}

impl MyPlanes {
    fn iter(&self) -> std::vec::IntoIter<&MyImagePlane> {
        match self {
            Self::Mono(yplane) => vec![yplane].into_iter(),
            Self::YCbCr((y, u, v)) => vec![y, u, v].into_iter(),
        }
    }
}

#[derive(Clone)]
pub struct MyYCbCrImage {
    pub planes: MyPlanes,
    pub width: u32,
    pub height: u32,
    pub bit_depth: BitDepth,
}

impl MyYCbCrImage {
    pub fn new(planes: MyPlanes, width: u32, height: u32) -> Result<Self> {
        Ok(Self {
            planes,
            width,
            height,
            bit_depth: BitDepth::Depth8,
        })
    }

    fn new_bit_depth(
        planes: MyPlanes,
        width: u32,
        height: u32,
        bit_depth: BitDepth,
    ) -> Result<Self> {
        for plane in planes.iter() {
            assert_eq!(plane.bit_depth, bit_depth);
        }
        Ok(Self {
            planes,
            width,
            height,
            bit_depth,
        })
    }

    pub fn to_image(&self, base_path: &std::path::Path) -> Result<TiffDecoder<std::fs::File>> {
        // This is pretty roundabout... first save to .y4m file then use ffmpeg
        // to convert to .tiff then load the .tiff.
        let (cname, colorspace, vec_planes, tif_pix_fmt) = match &self.planes {
            MyPlanes::Mono(y_plane) => {
                let (cname, colorspace, tif_pix_fmt) = match self.bit_depth {
                    BitDepth::Depth8 => ("mono", y4m::Colorspace::Cmono, "gray8"),
                    BitDepth::Depth12 => ("mono12", y4m::Colorspace::Cmono12, "gray16"),
                };
                (
                    cname,
                    colorspace,
                    [
                        y_plane.to_trimmed_y4m(self.width, self.height)?,
                        vec![],
                        vec![],
                    ],
                    tif_pix_fmt,
                )
            }
            MyPlanes::YCbCr((y_plane, cb_plane, cr_plane)) => {
                let (cname, colorspace, tif_pix_fmt) = match self.bit_depth {
                    BitDepth::Depth8 => ("yuv420", y4m::Colorspace::C420, "rgb24"),
                    BitDepth::Depth12 => ("yuv420p12", y4m::Colorspace::C420p12, "rgb48"),
                };
                (
                    cname,
                    colorspace,
                    [
                        y_plane.to_trimmed_y4m(self.width, self.height)?,
                        cb_plane.to_trimmed_y4m(self.width / 2, self.height / 2)?,
                        cr_plane.to_trimmed_y4m(self.width / 2, self.height / 2)?,
                    ],
                    tif_pix_fmt,
                )
            }
        };

        let raw_params = None;
        let planes = [
            vec_planes[0].as_slice(),
            vec_planes[1].as_slice(),
            vec_planes[2].as_slice(),
        ];
        let frame = y4m::Frame::new(planes, raw_params);

        let output_name = format!("test_{}_{}x{}.y4m", cname, self.width, self.height);

        {
            let full_output_name = base_path.join(&output_name);
            let out_fd = std::fs::File::create(&full_output_name)?;

            let enc_builder = y4m::encode(
                self.width.try_into()?,
                self.height.try_into()?,
                y4m::Ratio::new(25, 1),
            )
            .with_colorspace(colorspace)
            .append_vendor_extension("COLORRANGE=FULL".to_string());
            let mut enc = enc_builder.write_header(out_fd)?;

            enc.write_frame(&frame)?;
        }
        println!("** {output_name}: (raw) -> y4m");

        ffmpeg_to_frame(&base_path, &output_name, tif_pix_fmt)
    }
    pub fn view_luma<'a>(&'a self) -> DataPlane<'a> {
        let (data, stride) = match &self.planes {
            &MyPlanes::Mono(ref y_plane) | &MyPlanes::YCbCr((ref y_plane, _, _)) => {
                (&y_plane.data, y_plane.stride)
            }
        };
        DataPlane {
            data,
            stride,
            bit_depth: less_avc::BitDepth::Depth8,
        }
    }
    pub fn view<'a>(&'a self) -> YCbCrImage<'a> {
        let planes = match &self.planes {
            MyPlanes::Mono(y_plane) => Planes::Mono(DataPlane {
                data: &y_plane.data,
                stride: y_plane.stride,
                bit_depth: y_plane.bit_depth,
            }),
            MyPlanes::YCbCr((y_plane, cb_plane, cr_plane)) => {
                let y_plane = DataPlane {
                    data: &y_plane.data,
                    stride: y_plane.stride,
                    bit_depth: y_plane.bit_depth,
                };

                let cb_plane = DataPlane {
                    data: &cb_plane.data,
                    stride: cb_plane.stride,
                    bit_depth: y_plane.bit_depth,
                };

                let cr_plane = DataPlane {
                    data: &cr_plane.data,
                    stride: cr_plane.stride,
                    bit_depth: y_plane.bit_depth,
                };
                Planes::YCbCr((y_plane, cb_plane, cr_plane))
            }
        };
        YCbCrImage {
            planes,
            width: self.width,
            height: self.height,
        }
    }
}

pub fn div_ceil(a: u32, b: u32) -> u32 {
    // See https://stackoverflow.com/a/72442854
    (a + b - 1) / b
}

pub fn next_multiple(a: u32, b: u32) -> u32 {
    div_ceil(a, b) * b
}

pub fn generate_image(fmt: &PixFmt, width: u32, height: u32) -> Result<MyYCbCrImage> {
    if (fmt == &PixFmt::Mono12) || (fmt == &PixFmt::Rgb12) {
        // luma
        let stride_pixels = next_multiple(width, 16) as usize;
        let stride = stride_pixels * 3 / 2; // space for 12 bits per pixel

        let alloc_rows = next_multiple(height, 16) as usize;

        let mut data = vec![0u8; stride * alloc_rows];

        // calculate maximum value for 12 bit encoding
        let max_value = (1 << 12) as f64 - 1.0;
        let values_mono12: Vec<u16> = (0..width)
            .map(|idx| ((idx as f64) * max_value / (width - 1) as f64) as u16)
            .collect();

        let image_row_mono12: Vec<u8> = values_mono12
            .chunks_exact(2)
            .map(pack_to_12be)
            .flatten()
            .collect();

        let valid_width = (width * 3 / 2) as usize;
        debug_assert_eq!(valid_width, image_row_mono12.len());

        for row in 0..height {
            let start_idx: usize = row as usize * stride;
            let dest_row = &mut data[start_idx..(start_idx + valid_width)];
            dest_row.copy_from_slice(&image_row_mono12);
        }

        let luma_plane = MyImagePlane::new_bit_depth(data, stride, BitDepth::Depth12)?;
        match fmt {
            &PixFmt::Mono12 => {
                return MyYCbCrImage::new_bit_depth(
                    MyPlanes::Mono(luma_plane),
                    width,
                    height,
                    BitDepth::Depth12,
                );
            }
            &PixFmt::Rgb12 => {
                // chroma
                let stride_pixels = next_multiple(width / 2, 8) as usize;
                assert_eq!(stride_pixels % 2, 0);
                let stride = stride_pixels * 3 / 2; // space for 12 bits per pixel

                let alloc_rows = next_multiple(height / 2, 8) as usize;
                let mut data = vec![0u8; stride * alloc_rows];

                let valid_width_bytes = ((width / 2) * 3 / 2) as usize;

                let neutral_chroma12: Vec<u16> = vec![0x0800; width as usize / 2];

                let image_row_chroma12: Vec<u8> = neutral_chroma12
                    .chunks_exact(2)
                    .map(pack_to_12be)
                    .flatten()
                    .collect();

                for row in 0..alloc_rows {
                    let start_idx: usize = row as usize * stride;
                    let dest_row = &mut data[start_idx..(start_idx + valid_width_bytes)];
                    // dest_row.copy_from_slice(&image_row_chroma12);

                    // If full image width is cleanly divisible 4, the chroma
                    // width in 4:2:0 is divisible by 2 and thus the packing
                    // with 12 bits per pixel for the chroma samples will fit
                    // into an integer number of bytes. However, if this is not
                    // the case, we want this to still succeed because we want
                    // to test that less-avc returns an
                    // `Error::DataShapeProblem`.
                    (&mut dest_row[..image_row_chroma12.len()])
                        .copy_from_slice(&image_row_chroma12);
                }

                let chroma_plane = MyImagePlane::new_bit_depth(data, stride, BitDepth::Depth12)?;

                return MyYCbCrImage::new_bit_depth(
                    MyPlanes::YCbCr((luma_plane, chroma_plane.clone(), chroma_plane)),
                    width,
                    height,
                    BitDepth::Depth12,
                );
            }
            fmt => {
                panic!("unexpected format '{fmt:?}'");
            }
        }
    }

    // luma (8 bit)
    let stride = next_multiple(width, 16) as usize;
    let alloc_rows = next_multiple(height, 16) as usize;

    let mut data = vec![0u8; stride * alloc_rows];

    let max_value = (1 << 8) as f64 - 1.0; // 255.0 for 8 bit encoding
    let image_row_mono8: Vec<u8> = (0..width)
        .map(|idx| ((idx as f64) * max_value / (width - 1) as f64) as u8)
        .collect();

    for row in 0..height {
        let start_idx: usize = row as usize * stride;
        let dest_row = &mut data[start_idx..(start_idx + width as usize)];
        dest_row.copy_from_slice(&image_row_mono8);
    }

    let luma_plane = MyImagePlane::new(data, stride)?;
    match fmt {
        PixFmt::Mono8 => MyYCbCrImage::new(MyPlanes::Mono(luma_plane), width, height),
        PixFmt::Rgb8 => {
            // chroma
            let stride = next_multiple(width / 2, 8) as usize;
            let alloc_rows = next_multiple(height / 2, 8) as usize;
            let data = vec![128u8; stride * alloc_rows];
            let chroma_plane = MyImagePlane::new(data, stride)?;

            MyYCbCrImage::new(
                MyPlanes::YCbCr((luma_plane, chroma_plane.clone(), chroma_plane)),
                width,
                height,
            )
        }
        _ => {
            panic!("unknown pix format '{fmt:?}'");
        }
    }
}

pub fn ffmpeg_to_frame(
    base_path: &std::path::Path,
    fname: &str,
    tif_pix_fmt: &str,
) -> Result<TiffDecoder<std::fs::File>> {
    let base = fname;
    let tiff_fname = format!("{base}.tiff");
    let full_tiff_fname = base_path.join(&tiff_fname);

    let input_fname = fname;
    let args = vec!["-i", &input_fname, "-pix_fmt", tif_pix_fmt, &tiff_fname];

    println!(
        "** {tiff_fname}: generated with ffmpeg from {fname}. Full args: \n    ffmpeg {}",
        args.join(" ")
    );

    let output = std::process::Command::new("ffmpeg")
        .args(&args)
        .current_dir(base_path)
        .output()
        .with_context(|| format!("When running: ffmpeg {:?}", args))?;

    if !output.status.success() {
        anyhow::bail!(
            "'ffmpeg {}' failed. stdout: {}, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if tif_pix_fmt == "gray16" {
        // Ensure ffmpeg is recent enough.
        let ffmpeg_stderr = String::from_utf8_lossy(&output.stderr);
        let mut ffmpeg_stderr_iter = ffmpeg_stderr.split_ascii_whitespace();
        assert_eq!(ffmpeg_stderr_iter.next(), Some("ffmpeg"));
        assert_eq!(ffmpeg_stderr_iter.next(), Some("version"));

        if let Some(version_str) = ffmpeg_stderr_iter.next() {
            let version = semver::Version::parse(version_str)?;
            let req = semver::VersionReq::parse(">=5.1.1")?;
            if !req.matches(&version) {
                anyhow::bail!(
                    "You have ffmpeg {version} but requirement is \
                {req} for tiff pix_fmt {tif_pix_fmt}."
                );
            }
        } else {
            panic!("no ffmpeg version could be read");
        }
    }

    let rdr = std::fs::File::open(&full_tiff_fname)?;
    let decoder = tiff::decoder::Decoder::new(rdr)?;
    Ok(decoder)
}
