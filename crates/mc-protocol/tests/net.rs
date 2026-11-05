use cfb8::cipher::KeyIvInit;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use mc_protocol::codec::write_varint;
use mc_protocol::net::{Connection, FrameDecoder, FrameEncoder, NetError, PacketReader};
use std::io::{Read, Write};

const FIXTURE: &str = include_str!("fixtures/net_framing.txt");

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// Mirror of `payload(n)` in `fixtures/NetFraming.java`.
fn payload(n: usize) -> Vec<u8> {
    let mut b = vec![0x2b];
    b.extend((1..n).map(|i| ((i * 7 + i / 13) % 251) as u8));
    b
}

struct Case {
    scenario: String,
    threshold: i32,
    secret: Option<[u8; 16]>,
    payload: Vec<u8>,
    wire: Vec<u8>,
}

fn cases() -> Vec<Case> {
    FIXTURE
        .lines()
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            Case {
                scenario: f[0].into(),
                threshold: f[1].parse().unwrap(),
                secret: (f[2] != "-").then(|| unhex(f[2]).try_into().unwrap()),
                payload: payload(f[3].parse().unwrap()),
                wire: unhex(f[4]),
            }
        })
        .collect()
}

fn encoder_for(c: &Case) -> FrameEncoder {
    let mut e = FrameEncoder::new();
    e.set_compression(c.threshold);
    if let Some(s) = c.secret {
        e.enable_encryption(s);
    }
    e
}

fn decoder_for(c: &Case) -> FrameDecoder {
    let mut d = FrameDecoder::new();
    d.set_compression(c.threshold, true);
    if let Some(s) = c.secret {
        d.enable_encryption(s);
    }
    d
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, body.len() as i32);
    out.extend_from_slice(body);
    out
}

fn varint(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, v);
    out
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn inflate(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    ZlibDecoder::new(data).read_to_end(&mut out).unwrap();
    out
}

fn compressed_body(declared: i32, data: &[u8]) -> Vec<u8> {
    let mut b = varint(declared);
    b.extend(zlib(data));
    b
}

fn decode_err(d: &mut FrameDecoder, wire: &[u8]) -> String {
    d.push(wire);
    d.next_frame().expect_err("expected error").to_string()
}

/// Splits `[length][body]` into `body` (panics if not exactly one frame).
fn unframe(wire: &[u8]) -> &[u8] {
    let mut d = wire;
    let len = mc_protocol::codec::read_varint(&mut d).unwrap() as usize;
    assert_eq!(d.len(), len);
    d
}

#[test]
fn golden_encoder_matches_vanilla() {
    type RefDec = cfb8::Decryptor<aes::Aes128>;
    let mut cur: Option<(String, FrameEncoder, Option<RefDec>)> = None;
    for c in cases() {
        if cur.as_ref().is_none_or(|(s, ..)| *s != c.scenario) {
            let ref_dec = c.secret.map(|s| RefDec::new(&s.into(), &s.into()));
            cur = Some((c.scenario.clone(), encoder_for(&c), ref_dec));
        }
        let (_, enc, ref_dec) = cur.as_mut().unwrap();
        let mut ours = Vec::new();
        enc.encode(&c.payload, &mut ours).unwrap();

        if c.threshold < 0 {
            // No compression: byte-identical to vanilla (cipher state continues across lines).
            assert_eq!(ours, c.wire, "{} len {}", c.scenario, c.payload.len());
            continue;
        }
        // zlib output may differ between implementations: compare the decoded structure instead.
        if let Some(d) = ref_dec {
            d.decrypt(&mut ours); // independent CFB8 implementation-stream for our bytes
        }
        assert_inflates_to(unframe(&ours), &c.payload, c.threshold);
        if c.secret.is_none() {
            assert_inflates_to(unframe(&c.wire), &c.payload, c.threshold);
            if c.payload.len() < c.threshold as usize {
                assert_eq!(ours, c.wire);
            }
        }
    }
}

fn assert_inflates_to(mut body: &[u8], payload: &[u8], threshold: i32) {
    let declared = mc_protocol::codec::read_varint(&mut body).unwrap();
    if payload.len() < threshold as usize {
        assert_eq!(declared, 0);
        assert_eq!(body, payload);
    } else {
        assert_eq!(declared as usize, payload.len());
        assert_eq!(inflate(body), payload);
    }
}

#[test]
fn golden_decoder_accepts_vanilla_bytes() {
    let mut cur: Option<(String, FrameDecoder)> = None;
    for c in cases() {
        if cur.as_ref().is_none_or(|(s, _)| *s != c.scenario) {
            cur = Some((c.scenario.clone(), decoder_for(&c)));
        }
        let d = &mut cur.as_mut().unwrap().1;
        d.push(&c.wire);
        assert_eq!(d.next_frame().unwrap().as_deref(), Some(&c.payload[..]), "{}", c.scenario);
        assert_eq!(d.next_frame().unwrap(), None);
    }
}

#[test]
fn golden_decoder_byte_by_byte() {
    let mut cur: Option<(String, FrameDecoder)> = None;
    for c in cases() {
        if cur.as_ref().is_none_or(|(s, _)| *s != c.scenario) {
            cur = Some((c.scenario.clone(), decoder_for(&c)));
        }
        let d = &mut cur.as_mut().unwrap().1;
        let mut got = None;
        for (i, b) in c.wire.iter().enumerate() {
            assert!(got.is_none(), "frame completed early");
            d.push(std::slice::from_ref(b));
            got = d.next_frame().unwrap();
            assert!(got.is_some() || i + 1 < c.wire.len());
        }
        assert_eq!(got.as_deref(), Some(&c.payload[..]), "{} len {}", c.scenario, c.payload.len());
    }
}

#[test]
fn several_frames_in_one_push() {
    let mut enc = FrameEncoder::new();
    enc.set_compression(256);
    enc.enable_encryption([9; 16]);
    let payloads = [payload(1), payload(300), payload(40), payload(5000)];
    let mut wire = Vec::new();
    for p in &payloads {
        enc.encode(p, &mut wire).unwrap();
    }
    let mut dec = FrameDecoder::new();
    dec.set_compression(256, true);
    dec.enable_encryption([9; 16]);
    dec.push(&wire);
    for p in &payloads {
        assert_eq!(dec.next_frame().unwrap().as_deref(), Some(&p[..]));
    }
    assert_eq!(dec.next_frame().unwrap(), None);
}

#[test]
fn frame_length_wider_than_21_bit() {
    let mut d = FrameDecoder::new();
    assert_eq!(decode_err(&mut d, &[0x80, 0x80, 0x80, 0x01]), "length wider than 21-bit");
    // Two continuation bytes alone are just incomplete.
    let mut d = FrameDecoder::new();
    d.push(&[0x80, 0x80]);
    assert!(d.next_frame().unwrap().is_none());
}

#[test]
fn frame_length_zero() {
    let mut d = FrameDecoder::new();
    assert_eq!(decode_err(&mut d, &[0]), "Frame length cannot be zero");
}

#[test]
fn max_frame_length_is_accepted() {
    let mut d = FrameDecoder::new();
    d.push(&[0xFF, 0xFF, 0x7F]);
    assert!(d.next_frame().unwrap().is_none());
    d.push(&vec![0x2b; 2_097_151]);
    assert_eq!(d.next_frame().unwrap().unwrap().len(), 2_097_151);
}

#[test]
fn below_threshold() {
    let mut d = FrameDecoder::new();
    d.set_compression(256, true);
    let wire = frame(&compressed_body(10, &[7; 10]));
    assert_eq!(
        decode_err(&mut d, &wire),
        "Badly compressed packet - size of 10 is below server threshold of 256"
    );
}

#[test]
fn below_threshold_not_checked_without_validate() {
    let mut d = FrameDecoder::new();
    d.set_compression(256, false);
    d.push(&frame(&compressed_body(10, &[7; 10])));
    assert_eq!(d.next_frame().unwrap().unwrap(), [7; 10]);
}

#[test]
fn over_protocol_maximum() {
    for validate in [true, false] {
        let mut d = FrameDecoder::new();
        d.set_compression(256, validate);
        // The declared size is rejected before anything is allocated or inflated.
        let wire = frame(&compressed_body(8_388_609, &[0; 4]));
        assert_eq!(
            decode_err(&mut d, &wire),
            "Badly compressed packet - size of 8388609 is larger than protocol maximum of 8388608",
            "validate={validate}"
        );
    }
}

#[test]
fn max_uncompressed_length_roundtrips() {
    let big = vec![0x2b; 8_388_608];
    let mut enc = FrameEncoder::new();
    enc.set_compression(256);
    let mut wire = Vec::new();
    enc.encode(&big, &mut wire).unwrap();
    let mut d = FrameDecoder::new();
    d.set_compression(256, true);
    d.push(&wire);
    assert_eq!(d.next_frame().unwrap().unwrap(), big);
}

#[test]
fn declared_size_mismatch() {
    let mut d = FrameDecoder::new();
    d.set_compression(10, true);
    let wire = frame(&compressed_body(200, &[3; 100]));
    assert_eq!(
        decode_err(&mut d, &wire),
        "Badly compressed packet - actual length of uncompressed payload 100 is does not match declared size 200"
    );
}

#[test]
fn truncated_zlib_stream_is_size_mismatch() {
    let mut zl = zlib(&payload(400));
    zl.truncate(zl.len() / 2);
    let mut body = varint(400);
    body.extend(zl);
    let mut d = FrameDecoder::new();
    d.set_compression(256, true);
    let msg = decode_err(&mut d, &frame(&body));
    assert!(
        msg.starts_with("Badly compressed packet - actual length of uncompressed payload ")
            && msg.ends_with(" is does not match declared size 400"),
        "{msg}"
    );
}

#[test]
fn corrupt_zlib_stream() {
    let mut d = FrameDecoder::new();
    d.set_compression(0, true);
    let mut body = varint(300);
    body.extend([0xFF; 20]);
    d.push(&frame(&body));
    assert!(matches!(d.next_frame(), Err(NetError::Decoder(_))));
}

#[test]
fn negative_declared_size_without_validate() {
    let mut d = FrameDecoder::new();
    d.set_compression(0, false);
    let wire = frame(&compressed_body(-1, &[1, 2, 3]));
    assert_eq!(decode_err(&mut d, &wire), "capacity: -1 (expected: 0+)");
}

#[test]
fn truncated_varint_in_compressed_frame() {
    let mut d = FrameDecoder::new();
    d.set_compression(0, true);
    // Frame body is a single continuation byte: vanilla's VarInt.read runs off the frame.
    d.push(&frame(&[0x80]));
    assert!(matches!(d.next_frame(), Err(NetError::Decode(_))));
}

#[test]
fn encoder_packet_too_big() {
    let mut enc = FrameEncoder::new();
    enc.set_compression(256);
    let mut out = vec![1, 2, 3];
    let err = enc.encode(&vec![0; 8_388_609], &mut out).unwrap_err();
    assert_eq!(err.to_string(), "java.lang.IllegalArgumentException: Packet too big (is 8388609, should be less than 8388608)");
    assert_eq!(out, [1, 2, 3]);
}

#[test]
fn encoder_frame_too_large() {
    let mut enc = FrameEncoder::new();
    let mut out = Vec::new();
    assert!(enc.encode(&vec![0; 2_097_151], &mut out).is_ok());
    let err = enc.encode(&vec![0; 2_097_152], &mut Vec::new()).unwrap_err();
    assert_eq!(err.to_string(), "Packet too large: size 2097152 is over 8");
}

#[test]
fn encoder_threshold_boundary() {
    let mut enc = FrameEncoder::new();
    enc.set_compression(256);
    let mut out = Vec::new();
    enc.encode(&payload(255), &mut out).unwrap();
    assert_eq!(&out[..4], [0x80, 0x02, 0x00, 0x2b]); // length 256, uncompressed marker 0
    out.clear();
    enc.encode(&payload(256), &mut out).unwrap();
    assert_eq!(unframe(&out)[..2], [0x80, 0x02]); // declared 256
    // Negative threshold disables again.
    enc.set_compression(-1);
    out.clear();
    enc.encode(&[0x2b], &mut out).unwrap();
    assert_eq!(out, [1, 0x2b]);
}

#[tokio::test]
async fn async_roundtrip_enabling_features_mid_stream() {
    let (a, b) = tokio::io::duplex(1 << 16);
    let mut client = Connection::new(a);
    let mut server = Connection::new(b);
    let secret = [0x5a; 16];

    // Strict ping-pong so no bytes of the next phase are buffered before the switch.
    let client_task = async move {
        let mut seen = Vec::new();
        for phase in 0..3 {
            for n in [1usize, 300, 20_000] {
                client.write_payload(&payload(n)).await.unwrap();
                client.flush().await.unwrap();
                seen.push(client.read_payload().await.unwrap());
            }
            match phase {
                0 => client.set_compression(256, true),
                1 => client.enable_encryption(secret),
                _ => {}
            }
        }
        seen
    };
    let server_task = async move {
        for phase in 0..3 {
            for _ in 0..3 {
                let p = server.read_payload().await.unwrap();
                let mut reply = p.clone();
                reply[0] = 0x2c;
                server.write_payload(&reply).await.unwrap();
                server.flush().await.unwrap();
            }
            match phase {
                0 => server.set_compression(256, true),
                1 => server.enable_encryption(secret),
                _ => {}
            }
        }
        // client dropped => EOF
        assert!(matches!(server.read_payload().await, Err(NetError::Eof)));
    };
    let (seen, ()) = tokio::join!(client_task, server_task);
    assert_eq!(seen.len(), 9);
    for (i, got) in seen.iter().enumerate() {
        let mut want = payload([1usize, 300, 20_000][i % 3]);
        want[0] = 0x2c;
        assert_eq!(*got, want, "packet {i}");
    }
}

#[tokio::test]
async fn async_wire_matches_encoder_and_split_halves_work() {
    let (a, b) = tokio::io::duplex(1 << 16);
    let (_r, mut w) = Connection::new(a).into_split();
    w.set_compression(256);
    w.enable_encryption([1; 16]);
    w.write_payload(&payload(1000)).await.unwrap();
    w.flush().await.unwrap();
    drop(w);
    drop(_r);

    let mut expected = Vec::new();
    let mut enc = FrameEncoder::new();
    enc.set_compression(256);
    enc.enable_encryption([1; 16]);
    enc.encode(&payload(1000), &mut expected).unwrap();

    let mut got = Vec::new();
    let mut b = b;
    tokio::io::AsyncReadExt::read_to_end(&mut b, &mut got).await.unwrap();
    assert_eq!(got, expected);
}

#[tokio::test]
async fn async_eof_mid_frame_and_clean() {
    let mut r = PacketReader::new(&[5u8, 1, 2][..]);
    assert!(matches!(r.read_payload().await, Err(NetError::Eof)));
    let mut r = PacketReader::new(&[][..]);
    assert!(matches!(r.read_payload().await, Err(NetError::Eof)));
    let mut r = PacketReader::new(&[2u8, 7, 8, 0][..]);
    assert_eq!(r.read_payload().await.unwrap(), [7, 8]);
    assert!(matches!(r.read_payload().await, Err(NetError::CorruptedFrame("Frame length cannot be zero"))));
}
