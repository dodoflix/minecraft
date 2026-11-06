//! Behavioural tests of the tag model and SNBT entry points that don't need the oracle.

use mc_nbt::{CompoundTag, ListTag, NbtAccounter, NbtError, Tag, compound, nbt_io, snbt};

#[test]
fn compound_getters_follow_numeric_tag_semantics() {
    let c = compound! { "b" => 1i8, "f" => 2.7f32, "d" => -2.5f64, "s" => "str", "l" => 5i64 };
    assert_eq!(c.get_int("f"), Some(2));
    assert_eq!(c.get_int("d"), Some(-3), "Mth.floor");
    assert_eq!(c.get_byte("l"), Some(5));
    assert_eq!(c.get_int("s"), None);
    assert_eq!(c.get_int_or("missing", 7), 7);
    assert_eq!(c.get_string("s"), Some("str"));
    assert_eq!(c.get_string("b"), None);
    assert_eq!(c.get_string_or("b", "dflt"), "dflt");
    assert_eq!(c.get_boolean("b"), Some(true));
    assert!(c.get_boolean_or("zz", true));
    assert!(c.get_compound_or_empty("zz").is_empty());
    assert!(c.get_list_or_empty("zz").is_empty());
}

#[test]
fn compound_keeps_insertion_order_but_writes_in_java_order() {
    let mut c = CompoundTag::new();
    for k in ["zeta", "alpha", "mid"] {
        c.put_int(k, 1);
    }
    assert_eq!(
        c.keys().map(String::as_str).collect::<Vec<_>>(),
        ["zeta", "alpha", "mid"]
    );
    c.remove("alpha");
    assert_eq!(
        c.keys().map(String::as_str).collect::<Vec<_>>(),
        ["zeta", "mid"]
    );
    // In a 16-slot Java HashMap "zeta" lands in bucket 0 and "mid" in bucket 9.
    let java: Vec<&str> = c
        .iter_java_order()
        .into_iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(java, ["zeta", "mid"]);
}

#[test]
fn equality_follows_java_records() {
    assert_eq!(Tag::Double(f64::NAN), Tag::Double(f64::NAN));
    assert_ne!(Tag::Double(0.0), Tag::Double(-0.0));
    assert_eq!(
        Tag::double(-0.0),
        Tag::Double(0.0),
        "DoubleTag.valueOf collapses -0.0"
    );
    assert_ne!(Tag::Int(1), Tag::Long(1));
    let a = compound! { "x" => 1, "y" => 2 };
    let b = compound! { "y" => 2, "x" => 1 };
    assert_eq!(a, b);
}

#[test]
fn list_unwraps_wrapper_compounds() {
    let mut l = ListTag::new();
    l.add_and_unwrap(Tag::Compound(compound! { "" => 5 }));
    l.add_and_unwrap(Tag::Compound(compound! { "" => 5, "x" => 1 }));
    assert_eq!(l[0], Tag::Int(5));
    assert!(matches!(l[1], Tag::Compound(_)));
    assert_eq!(
        ListTag::from(vec![Tag::Int(1), Tag::String("a".into())]).identify_raw_element_type(),
        10
    );
}

#[test]
fn accounter_limits() {
    let mut acc = NbtAccounter::create(100);
    acc.account_bytes(60).unwrap();
    assert_eq!(
        acc.account_bytes(41),
        Err(NbtError::Accounter("Tried to read NBT tag that was too big; tried to allocate: 60 + 41 bytes where max allowed: 100".into()))
    );
    assert!(matches!(
        acc.account_bytes(-1),
        Err(NbtError::IllegalArgument(_))
    ));
    let mut deep = NbtAccounter::default_quota();
    for _ in 0..512 {
        deep.push_depth().unwrap();
    }
    assert_eq!(
        deep.push_depth(),
        Err(NbtError::Accounter(
            "Tried to read NBT tag with too high complexity, depth > 512".into()
        ))
    );
}

#[test]
fn snbt_argument_parsing_uses_byte_offsets() {
    let input = "say é {a:1} rest";
    let start = input.find('{').unwrap();
    let (tag, end) = snbt::parse_as_argument(input, start).unwrap();
    assert_eq!(tag.to_string(), "{a:1}");
    assert_eq!(&input[end..], " rest");
    let (c, _) = snbt::parse_compound_as_argument(input, start).unwrap();
    assert_eq!(c.get_int("a"), Some(1));
    let err = snbt::parse_compound_as_argument("[1]", 0).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Expected compound tag at position 3: [1]<--[HERE]"
    );
    let err = snbt::parse("{\"é\":}").unwrap_err();
    assert_eq!(err.cursor(), 5);
    assert_eq!(err.byte_cursor(), 6);
}

#[test]
fn pretty_printer_orders_structure_keys() {
    let t = snbt::parse("{z:1,DataVersion:5,size:[1,2],palette:[{Name:\"x\"}]}").unwrap();
    assert_eq!(
        snbt::print_pretty(&t),
        "{\n    DataVersion: 5,\n    size: [1, 2],\n    palette: [\n        {Name: \"x\"}\n    ],\n    z: 1\n}"
    );
}

#[test]
fn network_roundtrip_of_any_tag() {
    for t in [
        Tag::End,
        Tag::Int(7),
        Tag::String("hé".into()),
        snbt::parse("{a:[1,{b:2}]}").unwrap(),
    ] {
        let mut out = Vec::new();
        nbt_io::write_any_tag(&t, &mut out).unwrap();
        assert_eq!(
            nbt_io::read_any_tag(&mut &out[..], &mut NbtAccounter::default_quota()).unwrap(),
            t
        );
    }
}

#[test]
fn deep_nesting_is_bounded_not_a_stack_overflow() {
    let ok = format!("{}{}", "[".repeat(600), "]".repeat(600));
    assert!(snbt::parse(&ok).is_ok());
    let deep = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
    assert_eq!(
        snbt::parse(&deep).unwrap_err().message(),
        "Failed to parse: nesting too deep"
    );
}

#[test]
fn file_roundtrip() {
    let dir = std::env::temp_dir().join(format!("mc-nbt-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let c = compound! { "a" => 1, "b" => "x" };
    let plain = dir.join("plain.nbt");
    assert_eq!(nbt_io::read_file(&plain).unwrap(), None);
    nbt_io::write_file(&c, &plain).unwrap();
    assert_eq!(nbt_io::read_file(&plain).unwrap(), Some(c.clone()));
    let gz = dir.join("level.dat");
    nbt_io::write_compressed_file(&c, &gz).unwrap();
    assert_eq!(
        nbt_io::read_compressed_file(&gz, &mut NbtAccounter::default_quota()).unwrap(),
        c
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
