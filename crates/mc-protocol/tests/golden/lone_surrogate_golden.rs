//! Strings with lone UTF-16 surrogates and malformed UTF-8 on the wire against vanilla (`Utf8String`, Netty's UTF-8 codec,
//! the data component codecs): `fixtures/lone_surrogate.tsv`, recorded by `fixtures/lone_surrogate/LoneSurrogateGolden.java`.
//! Every recorded cell is replayed exactly, lone surrogates included (the network writes each as `?`, NBT as `ed a0 80`).

use mc_core::java_string::JavaString;
use mc_data::Item;
use mc_dfu::{Dyn, NBT};
use mc_nbt::nbt_io;
use mc_protocol::codec::{Decode, Encode, read_utf, write_java_utf};
use mc_protocol::components::{
    CUSTOM_NAME, Filterable, ItemStack, SignText, WritableBookContent, WrittenBookContent,
};
use mc_protocol::packets::play::ClientboundSystemChatPacket;
use mc_testkit::{hex, rows, unesc_units, unhex};
use mc_text::Component;
use mc_text::nbt_text::{Styling, TextComponentTagVisitor};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");
/// The component rows (`comp_json`, `comp_snbt`, `tag_pretty`) record the system chat packet in their last cell.
const TEXT: &str = include_str!("../../../mc-text/tests/fixtures/lone_surrogate.tsv");

/// A recorded string, exactly.
fn java(cell: &str) -> JavaString {
    JavaString::from_utf16_units(&unesc_units(cell))
}

fn read(payload: &[u8]) -> JavaString {
    let mut framed = vec![payload.len() as u8];
    framed.extend_from_slice(payload);
    read_utf(&mut &framed[..], 32767).unwrap().into()
}

fn nbt_hex(dyn_value: &Dyn) -> String {
    let mut out = Vec::new();
    nbt_io::write_any_tag(&dyn_value.to_tag().unwrap(), &mut out).unwrap();
    hex(&out)
}

fn wire(value: &impl Encode) -> String {
    let mut out = Vec::new();
    value.encode(&mut out).unwrap();
    hex(&out)
}

#[test]
fn network_strings_decode_like_java() {
    let mut n = 0;
    for op in ["net_read", "net_read_all"] {
        for c in rows(GOLDEN, op) {
            let want = String::from_utf16(&unesc_units(c[2]))
                .expect("the decoder yields no lone surrogate");
            assert_eq!(read(&unhex(c[1])), want, "{op} {}", c[1]);
            n += 1;
        }
    }
    assert!(n > 16_000, "{n} rows");
}

#[test]
fn network_strings_encode_like_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "net_write") {
        let s = java(c[1]);
        let mut out = Vec::new();
        write_java_utf(&mut out, &s, 32767).unwrap();
        assert_eq!(hex(&out), c[2], "net_write {}", c[1]);
        assert_eq!(hex(&s.to_java_utf8()), c[3], "getBytes of {}", c[1]);
        assert_eq!(read(&out[1..]), java(c[4]), "read back {}", c[1]);
        n += 1;
    }
    for c in rows(GOLDEN, "net_write_max2") {
        let s = java(c[1]);
        let mut out = Vec::new();
        let got = match write_java_utf(&mut out, &s, 2) {
            Ok(()) => hex(&out),
            Err(e) => format!("ERR:EncoderException:{}", e.0),
        };
        assert_eq!(got, c[2], "net_write_max2 {}", c[1]);
        n += 1;
    }
    assert_eq!(n, 40);
}

#[test]
fn book_text_matches_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "book_page") {
        let s = java(c[1]);
        let book = WritableBookContent {
            pages: vec![Filterable::pass_through(s.clone())],
        };
        let nbt = WritableBookContent::codec().encode_start(&NBT, &book);
        assert_eq!(
            nbt_hex(&nbt.into_result().unwrap()),
            c[2],
            "nbt of page {}",
            c[1]
        );
        let bytes = wire(&book);
        assert_eq!(bytes, c[3], "network bytes of page {}", c[1]);
        // the network turned each lone surrogate into `?`
        let decoded = WritableBookContent::decode(&mut &unhex(&bytes)[..]).unwrap();
        assert_eq!(decoded.pages[0].raw, java(c[4]), "page read back {}", c[1]);
        n += 1;
    }
    for c in rows(GOLDEN, "book_title") {
        let s = java(c[1]);
        let book = WrittenBookContent {
            title: Filterable::pass_through(s.clone()),
            author: s.clone(),
            generation: 0,
            pages: vec![Filterable::pass_through(Component::literal(s))],
            resolved: true,
        };
        assert_eq!(wire(&book), c[2], "network bytes of title {}", c[1]);
        let nbt = WrittenBookContent::codec().encode_start(&NBT, &book);
        assert_eq!(
            nbt_hex(&nbt.into_result().unwrap()),
            c[3],
            "nbt of title {}",
            c[1]
        );
        n += 1;
    }
    assert_eq!(n, 8);
}

#[test]
fn sign_text_matches_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "sign") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        let line = Component::from_json(&mc_core::gson::parse_lenient_json_parser(&input).unwrap())
            .unwrap();
        let mut messages = vec![Component::empty(); 4];
        messages[0] = line;
        let text = SignText {
            filtered_messages: messages.clone(),
            messages,
            color: mc_protocol::components::DyeColor::Black,
            has_glowing_text: false,
        };
        let nbt = SignText::codec().encode_start(&NBT, &text);
        assert_eq!(
            nbt_hex(&nbt.into_result().unwrap()),
            c[2],
            "nbt of sign {input}"
        );
        assert_eq!(wire(&text), c[3], "network bytes of sign {input}");
        n += 1;
    }
    assert_eq!(n, 4);
}

#[test]
fn item_names_match_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "item_name") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        let name = Component::from_nbt(&mc_nbt::snbt::parse(&input).unwrap()).unwrap();
        let mut stack = ItemStack::new(Item::STICK);
        stack.set(CUSTOM_NAME, name.clone());
        let nbt = ItemStack::codec().encode_start(&NBT, &stack);
        assert_eq!(nbt_hex(&nbt.into_result().unwrap()), c[2], "nbt of {input}");
        assert_eq!(wire(&stack), c[3], "network bytes of {input}");
        // `getHoverName` of an item with a custom name is that name
        assert_eq!(name.get_java_string(), java(c[4]), "hover name of {input}");
        n += 1;
    }
    assert_eq!(n, 4);
}

/// The last cell of a component row of the text fixture is the `system_chat` packet carrying the component.
#[test]
fn system_chat_packets_match_vanilla() {
    let mut n = 0;
    for op in ["comp_json", "comp_snbt", "tag_pretty"] {
        for c in rows(TEXT, op) {
            if c[2].starts_with("ERR:") {
                continue;
            }
            let input = String::from_utf16(&unesc_units(c[1])).unwrap();
            let component = match op {
                "comp_json" => {
                    Component::from_json(&mc_core::gson::parse_lenient_json_parser(&input).unwrap())
                }
                "comp_snbt" => Component::from_nbt(&mc_nbt::snbt::parse(&input).unwrap()),
                _ => Ok(
                    TextComponentTagVisitor::with_options("  ", Styling::Plain, true)
                        .visit(&mc_nbt::snbt::parse(&input).unwrap()),
                ),
            }
            .unwrap();
            let packet = ClientboundSystemChatPacket {
                content: component,
                overlay: false,
            };
            assert_eq!(wire(&packet), c[5], "{op} packet of {input}");
            n += 1;
        }
    }
    assert_eq!(n, 13 - 1 + 10 + 17);
}
