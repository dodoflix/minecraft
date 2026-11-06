//! LZ4Block framing vs lz4-java golden fixtures (`fixtures/region/Lz4Tool.java`).

use crate::region_common::*;
use mc_world::region::lz4;
use mc_world::region::region_file_version::VERSION_LZ4;
use std::io::Read;

const NAMES: [&str; 6] = ["empty", "one", "short", "repeat", "random", "mixed"];

fn decode(frame: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    lz4::Lz4BlockReader::new(frame).read_to_end(&mut out)?;
    Ok(out)
}

/// `(token, compressed_len, decompressed_len, checksum)` per block.
fn block_headers(frame: &[u8]) -> Vec<(u8, i32, i32, i32)> {
    let mut i = 0;
    let mut v = Vec::new();
    while i < frame.len() {
        assert_eq!(&frame[i..i + 8], b"LZ4Block");
        let int = |o: usize| i32::from_le_bytes(frame[i + o..i + o + 4].try_into().unwrap());
        let h = (frame[i + 8], int(9), int(13), int(17));
        i += 21 + h.1 as usize;
        v.push(h);
    }
    v
}

#[test]
fn decodes_java_frames() {
    let dir = fixtures().join("lz4");
    for n in NAMES {
        let bin = std::fs::read(dir.join(format!("{n}.bin"))).unwrap();
        let frame = std::fs::read(dir.join(format!("{n}.lz4"))).unwrap();
        assert!(decode(frame).unwrap() == bin, "{n}");
    }
}

#[test]
fn our_frames_round_trip_and_match_java_framing() {
    let dir = fixtures().join("lz4");
    for n in NAMES {
        let bin = std::fs::read(dir.join(format!("{n}.bin"))).unwrap();
        let java = std::fs::read(dir.join(format!("{n}.lz4"))).unwrap();
        let ours = VERSION_LZ4.compress(&bin).unwrap();
        assert!(decode(ours.clone()).unwrap() == bin, "{n} round trip");
        let (jh, oh) = (block_headers(&java), block_headers(&ours));
        assert_eq!(jh.len(), oh.len(), "{n} block count");
        for (j, o) in jh.iter().zip(&oh) {
            // decompressed length and checksum are fixed by the data; the compressed form may differ
            assert_eq!((j.2, j.3), (o.2, o.3), "{n}");
            assert_eq!(j.0 & 0x0F, o.0 & 0x0F, "{n} level");
        }
        if ["empty", "one", "short", "random"].contains(&n) {
            assert!(ours == java, "{n}: raw-block frames must be byte-identical");
        }
    }
}

#[test]
fn rejects_corrupt_frames() {
    let ours = VERSION_LZ4.compress(b"hello lz4 blk").unwrap();
    // truncated: missing the terminating empty block / partial block
    assert!(decode(ours[..ours.len() - 21].to_vec()).is_err());
    assert!(decode(ours[..30].to_vec()).is_err());
    // bad magic, bad checksum
    let mut bad = ours.clone();
    bad[0] = b'X';
    assert!(decode(bad).is_err());
    let mut bad = ours.clone();
    bad[17] ^= 1;
    assert!(decode(bad).is_err());
    // compressed block with an out-of-range match offset
    let data = std::fs::read(fixtures().join("lz4/repeat.lz4")).unwrap();
    let mut bad = data.clone();
    bad[21 + 3] = 0xFF;
    bad[21 + 4] = 0xFF;
    assert!(decode(bad).is_err());
}

#[test]
fn rejects_a_compressed_length_beyond_the_compressor_bound() {
    let mut frame = b"LZ4Block".to_vec();
    frame.push(0x20 | 0x0F); // LZ4 block, level 15
    frame.extend_from_slice(&i32::MAX.to_le_bytes()); // compressed length
    frame.extend_from_slice(&16i32.to_le_bytes()); // decompressed length
    frame.extend_from_slice(&0i32.to_le_bytes()); // checksum
    let err = decode(frame).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}
