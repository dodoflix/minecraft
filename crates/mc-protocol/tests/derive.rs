//! Behaviour of `#[derive(Encode, Decode)]` and the decode limits vanilla enforces.
use mc_protocol::codec::EncodeResult;
use mc_protocol::codec::*;
use mc_protocol::{Decode, Encode};

#[derive(Debug, PartialEq, Encode, Decode)]
struct Mixed {
    #[mc(var)]
    a: i32,
    #[mc(var)]
    b: i64,
    #[mc(max = 3)]
    s: String,
    #[mc(max = 2)]
    v: Vec<u16>,
    #[mc(var)]
    ids: Vec<i32>,
    o: Option<String>,
    t: (u8, bool),
}

#[test]
fn struct_fields() {
    let m = Mixed { a: 300, b: -1, s: "abc".into(), v: vec![1, 2], ids: vec![1, 200], o: Some("x".into()), t: (7, true) };
    let bytes = to_bytes(&m).unwrap();
    assert_eq!(
        bytes,
        [0xac, 0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 3, b'a', b'b', b'c', 2, 0, 1, 0, 2, 2, 1, 0xc8, 0x01, 1, 1, b'x', 7, 1]
    );
    assert_eq!(from_bytes::<Mixed>(&bytes).unwrap(), m);
    // string longer than max = 3 chars
    let long = Mixed { s: "abcd".into(), ..m };
    assert_eq!(
        from_bytes::<Mixed>(&to_bytes(&long).unwrap()),
        Err(DecodeError::msg("The received string length is longer than maximum allowed (4 > 3)"))
    );
}

#[derive(Debug, PartialEq, Encode, Decode)]
#[mc(repr = "u8", error = "Bad thing: {}")]
enum Tagged {
    Unit,
    Tuple(i32, String),
    #[mc(id = 10)]
    Named { x: u16, y: Option<u8> },
    Next,
}

#[test]
fn enum_ids_and_fields() {
    assert_eq!(to_bytes(&Tagged::Unit).unwrap(), [0]);
    assert_eq!(to_bytes(&Tagged::Tuple(1, "a".into())).unwrap(), [1, 0, 0, 0, 1, 1, b'a']);
    assert_eq!(to_bytes(&Tagged::Named { x: 0x102, y: None }).unwrap(), [10, 1, 2, 0]);
    assert_eq!(to_bytes(&Tagged::Next).unwrap(), [11]);
    for t in [Tagged::Unit, Tagged::Tuple(-5, "é".into()), Tagged::Named { x: 9, y: Some(1) }, Tagged::Next] {
        assert_eq!(from_bytes::<Tagged>(&to_bytes(&t).unwrap()).unwrap(), t);
    }
    assert_eq!(from_bytes::<Tagged>(&[5]), Err(DecodeError::msg("Bad thing: 5")));
}

#[derive(Debug, PartialEq, Encode, Decode)]
enum VarTag {
    A,
    #[mc(id = 200)]
    B,
}

#[test]
fn varint_repr_is_default() {
    assert_eq!(to_bytes(&VarTag::B).unwrap(), [0xc8, 0x01]);
    assert_eq!(from_bytes::<VarTag>(&[0xc8, 0x01]).unwrap(), VarTag::B);
    assert_eq!(from_bytes::<VarTag>(&[3]), Err(DecodeError::msg("Unknown VarTag id: 3")));
}

mod custom {
    use super::*;
    pub fn encode(v: &u32, out: &mut Vec<u8>) -> EncodeResult<()> {
        out.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(take_array(input)?))
    }
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct With(#[mc(with = custom)] u32, bool);

#[test]
fn with_module() {
    assert_eq!(to_bytes(&With(1, true)).unwrap(), [1, 0, 0, 0, 1]);
    assert_eq!(from_bytes::<With>(&[1, 0, 0, 0, 1]).unwrap(), With(1, true));
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct Generic<T>(T);

#[test]
fn generics() {
    assert_eq!(from_bytes::<Generic<Vec<u8>>>(&[2, 9, 8]).unwrap(), Generic(vec![9, 8]));
}

#[test]
fn trailing_and_truncated_input() {
    assert!(from_bytes::<Generic<u8>>(&[1, 2]).is_err());
    assert!(matches!(from_bytes::<Generic<u32>>(&[1, 2]), Err(DecodeError::Eof { needed: 4, available: 2 })));
}

#[test]
fn vanilla_packet_limits() {
    use mc_protocol::packets::{common::*, configuration::*, handshake::*, login::*};
    // host name > 255 chars
    let mut b = Vec::new();
    write_varint(&mut b, 777);
    let host = "h".repeat(256);
    host.encode(&mut b).unwrap();
    25565u16.encode(&mut b).unwrap();
    write_varint(&mut b, 2);
    assert_eq!(
        ClientIntentionPacket::decode(&mut &b[..]),
        Err(DecodeError::msg("The received string length is longer than maximum allowed (256 > 255)"))
    );
    // unknown intention
    let mut b = Vec::new();
    write_varint(&mut b, 777);
    "x".encode(&mut b).unwrap();
    1u16.encode(&mut b).unwrap();
    write_varint(&mut b, 4);
    assert_eq!(ClientIntentionPacket::decode(&mut &b[..]), Err(DecodeError::msg("Unknown connection intent: 4")));
    // login name > 16
    let hello = ServerboundHelloPacket { name: "a".repeat(17), profile_id: uuid::Uuid::nil() };
    assert!(ServerboundHelloPacket::decode(&mut &to_bytes(&hello).unwrap()[..]).is_err());
    // known packs > 64
    let packs = ServerboundSelectKnownPacks {
        known_packs: (0..65).map(|_| KnownPack { namespace: "a".into(), id: "b".into(), version: "c".into() }).collect(),
    };
    assert_eq!(
        ServerboundSelectKnownPacks::decode(&mut &to_bytes(&packs).unwrap()[..]),
        Err(DecodeError::msg("65 elements exceeded max size of: 64"))
    );
    // client information: out-of-range enum ids use vanilla's WRAP / ZERO strategies
    let mut b = to_bytes(&ClientInformation::default()).unwrap();
    // layout: "en_us"(6) view(1) chat_visibility(1) ...
    b[7] = 5; // 5 mod 3 = 2 -> Hidden
    let info = ClientInformation::decode(&mut &b[..]).unwrap();
    assert_eq!(info.chat_visibility, ChatVisibility::Hidden);
    let mut b = to_bytes(&ClientInformation::default()).unwrap();
    b[10] = 9; // main hand out of bounds -> Left (0)
    assert_eq!(ClientInformation::decode(&mut &b[..]).unwrap().main_hand, HumanoidArm::Left);
    // custom payload over the serverbound limit
    let big = ServerboundCustomPayloadPacket { payload: CustomPayload { id: "a:b".parse_id(), data: vec![0; 32768] } };
    assert_eq!(
        ServerboundCustomPayloadPacket::decode(&mut &to_bytes(&big).unwrap()[..]),
        Err(DecodeError::msg("Payload may not be larger than 32767 bytes"))
    );
    // click payload prefix larger than allowed
    let mut b = to_bytes(&mc_core::ident::Identifier::parse("a:b").unwrap()).unwrap();
    write_varint(&mut b, 65537);
    assert_eq!(
        ServerboundCustomClickActionPacket::decode(&mut &b[..]),
        Err(DecodeError::msg("Buffer size 65537 is larger than allowed limit of 65536"))
    );
}

trait ParseId {
    fn parse_id(&self) -> mc_core::ident::Identifier;
}
impl ParseId for str {
    fn parse_id(&self) -> mc_core::ident::Identifier {
        mc_core::ident::Identifier::parse(self).unwrap()
    }
}
