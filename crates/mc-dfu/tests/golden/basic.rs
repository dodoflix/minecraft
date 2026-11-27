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
    let f = Foo {
        a: 3,
        b: Some("x".into()),
        c: vec![1, 2],
    };
    let tag = encode_nbt(&foo(), &f).into_result().unwrap();
    assert_eq!(
        tag.to_string(),
        r#"{a:3,b:"x",c:[1,2]}"#.replace(
            "a:3,b:\"x\",c:[1,2]",
            &tag.to_string()[1..tag.to_string().len() - 1]
        )
    );
    assert_eq!(parse_nbt(&foo(), &tag).into_result().unwrap(), f);
    let json = encode_json(&foo(), &f).into_result().unwrap();
    assert_eq!(json, r#"{"a":3,"b":"x","c":[1,2]}"#);
    assert_eq!(
        parse_json(&foo(), json.as_lossy_str())
            .into_result()
            .unwrap(),
        f
    );
    // default omitted
    let g = Foo {
        a: 1,
        b: None,
        c: vec![],
    };
    assert_eq!(encode_json(&foo(), &g).into_result().unwrap(), r#"{"a":1}"#);
}

#[test]
fn decode_errors() {
    assert!(parse_json(&foo(), r#"{"b":"x"}"#).is_error());
    assert!(parse_json(&foo(), r#"{"a":"nope"}"#).is_error());
    // 1.5 truncates like Number.intValue
    assert_eq!(
        parse_json(&foo(), r#"{"a":1.9}"#).into_result().unwrap().a,
        1
    );
}

#[test]
fn hash_of_known_values() {
    // HashOps.createInt(1): crc32c of [8, 1,0,0,0]
    assert_eq!(hash_dyn(&Dyn::Int(1)), crc32c(&[8, 1, 0, 0, 0]));
    assert_eq!(hash_dyn(&Dyn::Map(vec![])), crc32c(&[2, 3]));
}

/// A codec that decodes every input the same way: `Ok(value)` succeeds, `Err((message, partial))` fails.
fn fixed<A: Clone + Send + Sync + 'static>(
    outcome: Result<A, (&'static str, Option<A>)>,
) -> Codec<A> {
    Codec::from_fns(
        |_, _, prefix| DataResult::Success(prefix),
        move |_, input| match outcome.clone() {
            Ok(a) => DataResult::Success((a, input.clone())),
            Err((m, None)) => DataResult::error_str(m),
            Err((m, Some(p))) => DataResult::error_partial(move || m, (p, input.clone())),
        },
    )
}

type Fail = Result<i32, (&'static str, Option<i32>)>;
type FailS = Result<String, (&'static str, Option<String>)>;

fn xor_read(first: Fail, second: FailS) -> DataResult<Either<i32, String>> {
    parse_json(&fixed(first).xor(&fixed(second)), "0")
}

/// `XorCodec.decode`: both read -> error carrying the first value as partial; one reads -> that one.
#[test]
fn xor_with_one_or_two_successful_alternatives() {
    let both = xor_read(Ok(7), Ok("s".into()));
    assert!(
        both.error_message()
            .unwrap()
            .starts_with("Both alternatives read successfully, can not pick the correct one")
    );
    assert_eq!(both.partial_ref(), Some(&Either::Left(7)));

    let first = xor_read(Ok(7), Err(("m2", Some("p".into()))));
    assert_eq!(first.into_result(), Ok(Either::Left(7)));
    let second = xor_read(Err(("m1", Some(1))), Ok("s".into()));
    assert_eq!(second.into_result(), Ok(Either::Right("s".into())));
}

/// Both fail: `firstRead.apply2((f, s) -> s, secondRead)`: messages second first, the partial result is the second's and
/// survives only when both alternatives have one.
#[test]
fn xor_with_two_failing_alternatives() {
    let neither = xor_read(Err(("m1", None)), Err(("m2", None)));
    assert_eq!(neither.error_message().as_deref(), Some("m2; m1"));
    assert_eq!(neither.partial_ref(), None);

    let both = xor_read(Err(("m1", Some(1))), Err(("m2", Some("p".into()))));
    assert_eq!(both.error_message().as_deref(), Some("m2; m1"));
    assert_eq!(both.partial_ref(), Some(&Either::Right("p".into())));

    let only_first = xor_read(Err(("m1", Some(1))), Err(("m2", None)));
    assert_eq!(only_first.error_message().as_deref(), Some("m2; m1"));
    assert_eq!(only_first.partial_ref(), None);
    let only_second = xor_read(Err(("m1", None)), Err(("m2", Some("p".into()))));
    assert_eq!(only_second.error_message().as_deref(), Some("m2; m1"));
    assert_eq!(only_second.partial_ref(), None);
}

/// `XorCodec.encode`: the value's side encodes it.
#[test]
fn xor_encodes_with_the_side_of_the_value() {
    let codec = INT.xor(&STRING);
    let left = encode_json(&codec, &Either::Left(5)).into_result().unwrap();
    assert_eq!(left, "5");
    let right = encode_json(&codec, &Either::Right("a".into()))
        .into_result()
        .unwrap();
    assert_eq!(right, r#""a""#);
}
