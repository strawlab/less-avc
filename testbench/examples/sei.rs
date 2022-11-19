use std::io::Write;

use chrono::TimeZone;

use less_avc::nal_unit::NalUnit;
use less_avc::sei::*;
use testbench::*;

/// Create precision time stamp as descripted in MISB Standard 0604
fn precision_time_stamp(timestamp: chrono::DateTime<chrono::Utc>) -> UserDataUnregistered {
    let precision_time_stamp = timestamp.timestamp_micros();
    let precision_time_stamp_bytes: [u8; 8] = precision_time_stamp.to_be_bytes();

    let mut payload: Vec<u8> = vec![0u8; 12];
    payload[0] = 0x0F;
    payload[1..3].copy_from_slice(&precision_time_stamp_bytes[0..2]);
    payload[3] = 0xff;
    payload[4..6].copy_from_slice(&precision_time_stamp_bytes[2..4]);
    payload[6] = 0xff;
    payload[7..9].copy_from_slice(&precision_time_stamp_bytes[4..6]);
    payload[9] = 0xff;
    payload[9..11].copy_from_slice(&precision_time_stamp_bytes[6..8]);

    UserDataUnregistered::new(*b"MISPmicrosectime", payload)
}

fn timestamp_to_nal_unit(timestamp: chrono::DateTime<chrono::Utc>) -> Vec<u8> {
    to_annex_b(precision_time_stamp(timestamp))
}

fn sei_comment(msg: Vec<u8>) -> Vec<u8> {
    // x264 says "random ID number generated according to ISO-11578", so we made up ours here.
    let uuid = b"\x05\xdeG\x06\x03u_T\xe9\x8e4P\x1d\x0erq";
    let udu = UserDataUnregistered::new(*uuid, msg);
    to_annex_b(udu)
}

fn to_annex_b(udu: UserDataUnregistered) -> Vec<u8> {
    let rbsp_data = SupplementalEnhancementInformation::UserDataUnregistered(udu).to_rbsp();
    NalUnit::new(
        less_avc::nal_unit::NalRefIdc::Zero,
        less_avc::nal_unit::NalUnitType::SupplementalEnhancementInformation,
        rbsp_data,
    )
    .to_annex_b_data()
}

fn main() -> anyhow::Result<()> {
    let input_yuv = generate_image(&PixFmt::Rgb8, 1920, 1080).unwrap();
    let frame_view = input_yuv.view();
    let mut fd = std::fs::File::create("sei.h264")?;

    let mut timestamp = chrono::Utc
        .with_ymd_and_hms(2022, 11, 19, 12, 34, 56)
        .unwrap();

    let (initial_nal_units, mut encoder) = less_avc::LessEncoder::new(&frame_view)?;

    fd.write_all(&sei_comment(b"hello from rust".to_vec()))?;

    fd.write_all(&initial_nal_units.sps.to_annex_b_data())?;
    fd.write_all(&initial_nal_units.pps.to_annex_b_data())?;
    fd.write_all(&timestamp_to_nal_unit(timestamp))?;
    fd.write_all(&initial_nal_units.frame.to_annex_b_data())?;

    for _ in 0..9 {
        timestamp += chrono::Duration::milliseconds(50);
        let nal_unit = encoder.encode(&frame_view).unwrap();
        fd.write_all(&timestamp_to_nal_unit(timestamp))?;
        fd.write_all(&nal_unit.to_annex_b_data())?;
    }
    Ok(())
}
