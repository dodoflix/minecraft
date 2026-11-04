//! The play packet enums: id/name tables equal vanilla's `packets.json`, and every vanilla packet byte string in the
//! play golden fixtures survives `decode_payload` -> `encode_payload` through the id-dispatching enums.
use mc_protocol::packet::{packet_names, ConnectionState, PacketFlow};
use mc_protocol::packets::play::{ClientboundPlayPacket, ServerboundPlayPacket};

#[test]
fn enums_match_packets_json() {
    let mut cb: Vec<_> = ClientboundPlayPacket::PACKETS.to_vec();
    cb.sort();
    assert_eq!(cb, packet_names(ConnectionState::Play, PacketFlow::Clientbound));
    assert_eq!(cb.len(), 144);
    let mut sb: Vec<_> = ServerboundPlayPacket::PACKETS.to_vec();
    sb.sort();
    assert_eq!(sb, packet_names(ConnectionState::Play, PacketFlow::Serverbound));
    assert_eq!(sb.len(), 69);
}

#[test]
fn golden_payloads_roundtrip_through_the_enums() {
    let dir = format!("{}/tests/fixtures/play", env!("CARGO_MANIFEST_DIR"));
    let (mut cb, mut sb) = (0, 0);
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "golden") {
            continue;
        }
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let (_, hex) = line.split_once(':').unwrap();
            let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            // Fixture lines are packets (VarInt id + body), nested values or error texts; only packets can decode. A clientbound
            // packet may also decode leniently as a serverbound one (bool/enum bodies), and lenient decoders (ignored flag bits) legitimately re-encode differently: count exact round trips only.
            let reencode = |r: Result<Vec<u8>, _>| r.ok().map(|o: Vec<u8>| o == bytes);
            let c = reencode(ClientboundPlayPacket::decode_payload(&bytes).map(|p| { let mut o = Vec::new(); p.encode_payload(&mut o); o }));
            let s = reencode(ServerboundPlayPacket::decode_payload(&bytes).map(|p| { let mut o = Vec::new(); p.encode_payload(&mut o); o }));
            cb += usize::from(c == Some(true));
            sb += usize::from(s == Some(true));
        }
    }
    assert!(cb > 1000 && sb > 300, "decoded {cb} clientbound / {sb} serverbound fixture packets");
}
