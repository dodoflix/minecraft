//! Write-side limits vs vanilla's encoders (fixture `tests/fixtures/wire_limits.golden`, generator
//! `tests/fixtures/WireLimitsGolden.java` via `tests/fixtures/wire_limits.sh`): what is encoded exactly at a limit, and the
//! `EncoderException` raised one past it (message text included). Packet cases also check the `Failed to encode packet`
//! wrapper (`PacketEncodeError`) and `Packet.isSkippable()`.
use std::collections::BTreeMap;

use mc_core::{ident::Identifier, pos::block_pos::BlockPos};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::codec::{
    BitSet, EncodeBounded, EncodeError, EncodeResult, FixedBitSet, Instant, LongArray, encode_tag,
    write_count, write_utf,
};
use mc_protocol::components::{DyeColor, SignText};
use mc_protocol::packet::{Packet, encode_packet};
use mc_protocol::packets::common::{
    ChatVisibility, ClientInformation, HumanoidArm, ParticleStatus,
};
use mc_protocol::packets::configuration::{
    ClientboundCustomReportDetailsPacket, ClientboundStoreCookiePacket, KnownPack,
    ServerboundClientInformationPacket, ServerboundSelectKnownPacks,
};
use mc_protocol::packets::play::*;
use mc_protocol::{Encode, codec::to_bytes};
use mc_text::Component;

/// Vanilla's verdict for one label.
#[derive(Debug, PartialEq)]
enum Verdict {
    Ok(String),
    /// `Throwable.toString()`, the cause's (if wrapped), `Packet.isSkippable()`.
    Err {
        text: String,
        cause: Option<String>,
        skippable: bool,
    },
}

fn golden() -> BTreeMap<String, Verdict> {
    include_str!("../fixtures/wire_limits.golden")
        .lines()
        .map(|l| {
            let mut f = l.split('\t');
            let (label, kind) = (f.next().unwrap(), f.next().unwrap());
            let v = if kind == "OK" {
                Verdict::Ok(f.next().unwrap().to_owned())
            } else {
                let text = f.next().unwrap().to_owned();
                let (mut cause, mut skippable) = (None, false);
                for extra in f {
                    match extra.strip_prefix("cause=") {
                        Some(c) => cause = Some(c.to_owned()),
                        None => skippable = extra == "skippable",
                    }
                }
                Verdict::Err {
                    text,
                    cause,
                    skippable,
                }
            };
            (label.to_owned(), v)
        })
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

const ENCODER: &str = "io.netty.handler.codec.EncoderException: ";

/// Our result in vanilla's terms: a bare codec failure is an `EncoderException` with the message of the `EncodeError`.
fn codec_verdict(r: EncodeResult<Vec<u8>>) -> Verdict {
    match r {
        Ok(b) => Verdict::Ok(hex(&b)),
        Err(EncodeError(m)) => {
            // `new EncoderException(cause)` (an `IOException` of the NBT writer) carries the cause; the codec's own
            // `new EncoderException(message)` does not.
            let cause = m.starts_with("java.").then(|| m.clone());
            Verdict::Err {
                text: format!("{ENCODER}{m}"),
                cause,
                skippable: false,
            }
        }
    }
}

fn packet_verdict<P: Packet>(p: &P) -> Verdict {
    match encode_packet(p) {
        Ok(b) => Verdict::Ok(hex(&b)),
        Err(e) => Verdict::Err {
            text: e.exception_text(),
            cause: Some(format!("{ENCODER}{}", e.cause.0)),
            skippable: e.skippable,
        },
    }
}

fn run(f: impl FnOnce(&mut Vec<u8>) -> EncodeResult<()>) -> EncodeResult<Vec<u8>> {
    let mut out = Vec::new();
    f(&mut out)?;
    Ok(out)
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

fn compare(label: &str, ours: &Verdict, vanilla: &Verdict) {
    assert_eq!(ours, vanilla, "{label}");
}

#[test]
fn strings_arrays_lists_maps_bitsets() {
    let g = golden();
    let smile = "\u{1F600}";
    let mut ours: BTreeMap<String, Verdict> = BTreeMap::new();
    let mut put = |label: &str, v: Verdict| {
        ours.insert(label.to_owned(), v);
    };

    for max in [
        0usize, 1, 4, 16, 20, 32, 40, 64, 128, 256, 1024, 4096, 32500, 32767,
    ] {
        put(
            &format!("str/{max}/at_limit"),
            codec_verdict(run(|o| write_utf(o, &"a".repeat(max), max))),
        );
        put(
            &format!("str/{max}/over"),
            codec_verdict(run(|o| write_utf(o, &"a".repeat(max + 1), max))),
        );
        put(
            &format!("str/{max}/over_by_many"),
            codec_verdict(run(|o| write_utf(o, &"a".repeat(max + 40000), max))),
        );
    }
    put(
        "str/4/smile2_ok",
        codec_verdict(run(|o| write_utf(o, &smile.repeat(2), 4))),
    );
    put(
        "str/4/smile2_a_over",
        codec_verdict(run(|o| write_utf(o, &format!("{}a", smile.repeat(2)), 4))),
    );
    put(
        "str/3/smile2_over",
        codec_verdict(run(|o| write_utf(o, &smile.repeat(2), 3))),
    );
    put(
        "str/16/e_acute_16",
        codec_verdict(run(|o| write_utf(o, &"\u{e9}".repeat(16), 16))),
    );
    put(
        "str/16/e_acute_17",
        codec_verdict(run(|o| write_utf(o, &"\u{e9}".repeat(17), 16))),
    );
    put(
        "str/32767/euro_max",
        codec_verdict(run(|o| write_utf(o, &"\u{20ac}".repeat(32767), 32767))),
    );
    put(
        "str/32767/euro_over",
        codec_verdict(run(|o| write_utf(o, &"\u{20ac}".repeat(32768), 32767))),
    );
    put(
        "str/32767/smile_max_units",
        codec_verdict(run(|o| {
            write_utf(o, &format!("{}a", smile.repeat(16383)), 32767)
        })),
    );
    put(
        "str/32767/smile_over_units",
        codec_verdict(run(|o| write_utf(o, &smile.repeat(16384), 32767))),
    );
    put("str/default/empty", codec_verdict(to_bytes("")));
    put(
        "friendly/writeUtf_default_over",
        codec_verdict(to_bytes(&"a".repeat(32768))),
    );
    put(
        "friendly/writeUtf_16_over",
        codec_verdict(run(|o| write_utf(o, &"a".repeat(17), 16))),
    );
    put(
        "friendly/writeUtf_16_ok",
        codec_verdict(run(|o| write_utf(o, &"a".repeat(16), 16))),
    );

    for max in [0usize, 1, 512, 2048, 5120, 2097152] {
        put(
            &format!("bytes/{max}/at_limit"),
            codec_verdict(run(|o| vec![0u8; max].encode_bounded(o, max))),
        );
        put(
            &format!("bytes/{max}/over"),
            codec_verdict(run(|o| vec![0u8; max + 1].encode_bounded(o, max))),
        );
    }
    put(
        "friendly/writeByteArray_unlimited",
        codec_verdict(to_bytes(&vec![0u8; 100000])),
    );

    for max in [0usize, 1, 20, 32, 64, 100, 128, 256] {
        put(
            &format!("list/{max}/at_limit"),
            codec_verdict(run(|o| vec![7i32; max].encode_bounded(o, max))),
        );
        put(
            &format!("list/{max}/over"),
            codec_verdict(run(|o| vec![7i32; max + 1].encode_bounded(o, max))),
        );
    }
    put(
        "list/unlimited/1000",
        codec_verdict(to_bytes(&vec![1i32; 1000])),
    );
    // `fixedSizeList(4)` is a derive-level contract of the packets that use it (`[T; 4]` has no length to get wrong);
    // the Rust form of the failing case is a `Vec` of the wrong size handed to a fixed-size encoder, see `sign_lines` below.
    for max in [1usize, 32, 128, 256] {
        let entries = |n: usize| (0..n as i32).map(|i| (i, i * 3)).collect::<Vec<_>>();
        put(
            &format!("map/{max}/at_limit"),
            codec_verdict(run(|o| {
                write_count(o, max, max)?;
                <(i32, i32)>::encode_slice(&entries(max), o)
            })),
        );
        put(
            &format!("map/{max}/over"),
            codec_verdict(run(|o| entries(max + 1).encode_bounded(o, max))),
        );
    }

    let bits = |idx: &[usize]| {
        let mut b = BitSet::new();
        for &i in idx {
            b.set(i);
        }
        b
    };
    put(
        "fixedbitset/8/ok",
        codec_verdict(to_bytes(&FixedBitSet::<8>(bits(&[0, 7])))),
    );
    put(
        "fixedbitset/8/over",
        codec_verdict(to_bytes(&FixedBitSet::<8>(bits(&[0, 8])))),
    );
    put(
        "fixedbitset/20/over",
        codec_verdict(to_bytes(&FixedBitSet::<20>(bits(&[19, 20])))),
    );
    put(
        "fixedbitset/20/ok",
        codec_verdict(to_bytes(&FixedBitSet::<20>(bits(&[19])))),
    );
    let _ = LongArray::default();

    for (label, v) in &ours {
        let vanilla = g
            .get(label)
            .unwrap_or_else(|| panic!("no golden for {label}"));
        compare(label, v, vanilla);
    }
    // every golden label of these groups was exercised
    for label in g.keys().filter(|l| {
        [
            "str/",
            "friendly/",
            "bytes/",
            "list/",
            "map/",
            "fixedbitset/",
        ]
        .iter()
        .any(|p| l.starts_with(p))
    }) {
        assert!(ours.contains_key(label), "unchecked golden {label}");
    }
}

#[test]
fn fixed_size_list_mismatch_fails_like_vanilla() {
    // `ByteBufCodecs.fixedSizeList(4)` (the four sign lines): "Invalid list size, expected 4, but got N".
    let g = golden();
    for (label, n) in [("fixedlist/4/short", 3usize), ("fixedlist/4/long", 5)] {
        let sign = SignText {
            messages: vec![Component::empty(); n],
            filtered_messages: vec![Component::empty(); n],
            color: DyeColor::Black,
            has_glowing_text: false,
        };
        let ours = codec_verdict(to_bytes(&sign));
        assert_eq!(ours, g[label], "{label}");
    }
}

fn nbt_cases() -> Vec<(&'static str, Tag)> {
    let s = |n: usize, c: &str| Tag::String(c.repeat(n));
    let smile = "\u{1F600}";
    let mut v = vec![
        ("nbt/tag/end", Tag::End),
        ("nbt/tag/empty_compound", Tag::Compound(CompoundTag::new())),
        ("nbt/tag/string_65535", s(65535, "a")),
        ("nbt/tag/string_65536", s(65536, "a")),
        ("nbt/tag/string_65537", s(65537, "a")),
        ("nbt/tag/string_e_acute_32767", s(32767, "\u{e9}")),
        ("nbt/tag/string_e_acute_32768", s(32768, "\u{e9}")),
        ("nbt/tag/string_euro_21845", s(21845, "\u{20ac}")),
        ("nbt/tag/string_euro_21846", s(21846, "\u{20ac}")),
        ("nbt/tag/string_nul_32767", s(32767, "\0")),
        ("nbt/tag/string_nul_32768", s(32768, "\0")),
        ("nbt/tag/string_smile_16383", s(16383, smile)),
        ("nbt/tag/string_smile_16384", s(16384, smile)),
    ];
    let mut c = CompoundTag::new();
    c.put_string("k", &"b".repeat(70000));
    v.push(("nbt/compound/long_value", Tag::Compound(c)));
    let mut c = CompoundTag::new();
    c.put_int(&"k".repeat(65536), 1);
    v.push(("nbt/compound/long_key", Tag::Compound(c)));
    let mut c = CompoundTag::new();
    c.put_int(&"k".repeat(65535), 1);
    v.push(("nbt/compound/key_65535", Tag::Compound(c)));
    let mut list = ListTag::new();
    list.push(Tag::String("c".repeat(65536)));
    let mut c = CompoundTag::new();
    c.put("l", Tag::List(list));
    v.push(("nbt/compound/long_string_in_list", Tag::Compound(c)));
    v
}

#[test]
fn nbt() {
    let g = golden();
    for (label, tag) in nbt_cases() {
        let ours = codec_verdict(to_bytes(&tag));
        compare(label, &ours, &g[label]);
    }
    // no write-side quota and no write-side depth limit (the read side has both)
    let mut big = CompoundTag::new();
    for i in 0..40 {
        big.put_string(&format!("s{i}"), &"z".repeat(60000));
    }
    let bytes = to_bytes(&Tag::Compound(big)).unwrap();
    assert!(bytes.len() > 2_000_000);
    assert!(matches!(g["nbt/compound/over_read_quota"], Verdict::Ok(_)));
    let mut deep = CompoundTag::new();
    for _ in 0..600 {
        let mut outer = CompoundTag::new();
        outer.put("a", Tag::Compound(deep));
        deep = outer;
    }
    let bytes = to_bytes(&Tag::Compound(deep)).unwrap();
    let Verdict::Ok(n) = &g["nbt/compound/depth_600"] else {
        panic!("vanilla refused a deep tag")
    };
    assert_eq!(
        *n,
        hex(&(bytes.len() as u32).to_be_bytes()),
        "encoded size of the 600-deep compound"
    );
    // `FriendlyByteBuf.writeNbt(null)` = the single byte 0 (`encode_tag` of `End`)
    let mut out = Vec::new();
    encode_tag(&Tag::End, &mut out).unwrap();
    assert_eq!(Verdict::Ok(hex(&out)), g["nbt/net/write_null"]);
}

#[test]
fn components() {
    let g = golden();
    let cases: Vec<(&str, Component)> = vec![
        ("component/literal_empty", Component::empty()),
        ("component/literal_hi", Component::literal("hi")),
        (
            "component/literal_65535",
            Component::literal(&"a".repeat(65535)),
        ),
        (
            "component/literal_65536",
            Component::literal(&"a".repeat(65536)),
        ),
        (
            "component/translatable_key_65536",
            Component::translatable("t".repeat(65536), vec![]),
        ),
        (
            "component/nested_long_string",
            Component::literal("x").append(Component::literal(&"a".repeat(70000))),
        ),
        (
            "component/stream_string_100000",
            Component::literal(&"\u{20ac}".repeat(30000)),
        ),
    ];
    for (label, c) in cases {
        compare(label, &codec_verdict(to_bytes(&c)), &g[label]);
    }
}

fn client_information(language: &str) -> ClientInformation {
    ClientInformation {
        language: language.to_owned(),
        view_distance: 12,
        chat_visibility: ChatVisibility::Full,
        chat_colors: true,
        model_customisation: 0x7f,
        main_hand: HumanoidArm::Right,
        text_filtering_enabled: false,
        allows_listing: true,
        particle_status: ParticleStatus::All,
    }
}

#[test]
fn packets() {
    let g = golden();
    let mut ours: Vec<(&str, Verdict)> = Vec::new();
    let mut details: Vec<(String, String)> =
        (0..32).map(|i| (format!("k{i}"), "v".to_owned())).collect();
    ours.push((
        "packet/report_details_32",
        packet_verdict(&configuration_details(details.clone())),
    ));
    details.push(("k32".into(), "v".into()));
    ours.push((
        "packet/report_details_33",
        packet_verdict(&configuration_details(details)),
    ));
    ours.push((
        "packet/report_details_long_key",
        packet_verdict(&configuration_details(vec![("k".repeat(129), "v".into())])),
    ));
    ours.push((
        "packet/report_details_long_value",
        packet_verdict(&configuration_details(vec![("k".into(), "v".repeat(4097))])),
    ));
    ours.push((
        "packet/report_details_max_value",
        packet_verdict(&configuration_details(vec![(
            "k".repeat(128),
            "v".repeat(4096),
        )])),
    ));
    ours.push((
        "packet/store_cookie_5120",
        packet_verdict(&ClientboundStoreCookiePacket {
            key: id("mod:c"),
            payload: vec![0; 5120],
        }),
    ));
    ours.push((
        "packet/store_cookie_5121",
        packet_verdict(&ClientboundStoreCookiePacket {
            key: id("mod:c"),
            payload: vec![0; 5121],
        }),
    ));
    let pack = |i: usize| KnownPack {
        namespace: "minecraft".into(),
        id: format!("p{i}"),
        version: "1".into(),
    };
    ours.push((
        "packet/known_packs_64",
        packet_verdict(&ServerboundSelectKnownPacks {
            known_packs: (0..64).map(pack).collect(),
        }),
    ));
    ours.push((
        "packet/known_packs_65",
        packet_verdict(&ServerboundSelectKnownPacks {
            known_packs: (0..65).map(pack).collect(),
        }),
    ));
    // `ClientInformation` writes the language with the default limit although it reads at most 16 chars
    ours.push((
        "packet/client_information_lang_16",
        packet_verdict(&ServerboundClientInformationPacket {
            information: client_information(&"l".repeat(16)),
        }),
    ));
    ours.push((
        "packet/client_information_lang_17",
        packet_verdict(&ServerboundClientInformationPacket {
            information: client_information(&"l".repeat(17)),
        }),
    ));

    let chat = |m: String| ServerboundChatPacket {
        message: m,
        time_stamp: Instant(0),
        salt: 0,
        signature: None,
        last_seen_messages: LastSeenMessagesUpdate {
            offset: 0,
            acknowledged: FixedBitSet(BitSet::new()),
            checksum: 0,
        },
    };
    ours.push(("packet/chat_256", packet_verdict(&chat("a".repeat(256)))));
    ours.push(("packet/chat_257", packet_verdict(&chat("a".repeat(257)))));
    let book = |pages: Vec<String>, title: Option<String>| ServerboundEditBookPacket {
        slot: 0,
        pages,
        title,
    };
    ours.push((
        "packet/edit_book_100_pages",
        packet_verdict(&book(vec!["p".into(); 100], None)),
    ));
    ours.push((
        "packet/edit_book_101_pages",
        packet_verdict(&book(vec!["p".into(); 101], None)),
    ));
    ours.push((
        "packet/edit_book_page_1024",
        packet_verdict(&book(vec!["p".repeat(1024)], None)),
    ));
    ours.push((
        "packet/edit_book_page_1025",
        packet_verdict(&book(vec!["p".repeat(1025)], None)),
    ));
    ours.push((
        "packet/edit_book_title_32",
        packet_verdict(&book(vec![], Some("t".repeat(32)))),
    ));
    ours.push((
        "packet/edit_book_title_33",
        packet_verdict(&book(vec![], Some("t".repeat(33)))),
    ));
    let sign = |lines: [String; 4]| ServerboundSignUpdatePacket {
        pos: BlockPos::new(1, 2, 3),
        lines,
        slot: SignTextSlot::Front,
    };
    let blank = || String::new();
    ours.push((
        "packet/sign_384",
        packet_verdict(&sign(["s".repeat(384), blank(), blank(), blank()])),
    ));
    ours.push((
        "packet/sign_385",
        packet_verdict(&sign([blank(), "s".repeat(385), blank(), blank()])),
    ));

    // skippable clientbound packets
    ours.push((
        "packet/system_chat_ok",
        packet_verdict(&ClientboundSystemChatPacket {
            content: Component::literal("hi"),
            overlay: false,
        }),
    ));
    ours.push((
        "packet/system_chat_long",
        packet_verdict(&ClientboundSystemChatPacket {
            content: Component::literal(&"a".repeat(70000)),
            overlay: false,
        }),
    ));
    let mut tq = CompoundTag::new();
    tq.put_string("k", &"q".repeat(70000));
    ours.push((
        "packet/tag_query_long",
        packet_verdict(&ClientboundTagQueryPacket {
            transaction_id: 1,
            tag: Some(tq),
        }),
    ));

    for (label, v) in &ours {
        compare_packet(label, v, &g[*label]);
    }
    for label in g.keys().filter(|l| l.starts_with("packet/")) {
        assert!(
            ours.iter().any(|(l, _)| l == label),
            "unchecked golden {label}"
        );
    }
}

fn configuration_details(v: Vec<(String, String)>) -> ClientboundCustomReportDetailsPacket {
    ClientboundCustomReportDetailsPacket {
        details: mc_protocol::packets::common::ReportDetails(v),
    }
}

/// Like [`compare`], but the (huge) NBT string excerpts inside the cause are cut at the first `(`.
fn compare_packet(label: &str, ours: &Verdict, vanilla: &Verdict) {
    let cut = |v: &Verdict| match v {
        Verdict::Err {
            text,
            cause,
            skippable,
        } => Verdict::Err {
            text: text.clone(),
            cause: cause
                .as_ref()
                .map(|c| c.split(" (").next().unwrap().to_owned()),
            skippable: *skippable,
        },
        ok => Verdict::Ok(match ok {
            Verdict::Ok(h) => h.clone(),
            _ => unreachable!(),
        }),
    };
    assert_eq!(cut(ours), cut(vanilla), "{label}");
}
