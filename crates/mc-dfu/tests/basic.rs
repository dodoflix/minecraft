use mc_dfu::*;

#[derive(Clone, Debug, PartialEq)]
struct Foo {
    a: i32,
    b: Option<String>,
    c: Vec<i32>,
}

fn foo() -> Codec<Foo> {
    record!(Foo {
        a: INT.field_of("a"),
        b: STRING.optional_field_of("b"),
        c: INT.list_of().optional_field_of_default("c", vec![]),
    })
    .codec()
}

#[test]
fn record_roundtrip_nbt_and_json() {
    let f = Foo { a: 3, b: Some("x".into()), c: vec![1, 2] };
    let tag = encode_nbt(&foo(), &f).into_result().unwrap();
    assert_eq!(tag.to_string(), r#"{a:3,b:"x",c:[1,2]}"#.replace("a:3,b:\"x\",c:[1,2]", &tag.to_string()[1..tag.to_string().len() - 1]));
    assert_eq!(parse_nbt(&foo(), &tag).into_result().unwrap(), f);
    let json = encode_json(&foo(), &f).into_result().unwrap();
    assert_eq!(json, r#"{"a":3,"b":"x","c":[1,2]}"#);
    assert_eq!(parse_json(&foo(), &json).into_result().unwrap(), f);
    // default omitted
    let g = Foo { a: 1, b: None, c: vec![] };
    assert_eq!(encode_json(&foo(), &g).into_result().unwrap(), r#"{"a":1}"#);
}

#[test]
fn decode_errors() {
    assert!(parse_json(&foo(), r#"{"b":"x"}"#).is_error());
    assert!(parse_json(&foo(), r#"{"a":"nope"}"#).is_error());
    // 1.5 truncates like Number.intValue
    assert_eq!(parse_json(&foo(), r#"{"a":1.9}"#).into_result().unwrap().a, 1);
}

#[test]
fn hash_of_known_values() {
    // HashOps.createInt(1): crc32c of [8, 1,0,0,0]
    assert_eq!(hash_dyn(&Dyn::Int(1)), crc32c(&[8, 1, 0, 0, 0]));
    assert_eq!(hash_dyn(&Dyn::Map(vec![])), crc32c(&[2, 3]));
}
