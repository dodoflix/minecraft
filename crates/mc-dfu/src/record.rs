//! The `RecordCodecBuilder` equivalent.

/// Build a `MapCodec<Struct>` from one `MapCodec<FieldType>` per struct field (the field codecs are the
/// `.fieldOf("x").forGetter(Struct::x)` of vanilla; the getter is the struct field of the same name).
///
/// ```ignore
/// record!(Foo { a: INT.field_of("a"), b: STRING.optional_field_of("b") })
/// ```
/// Decoding evaluates every field, joins all error messages with `"; "` (last field first, like the nested `ap` of the
/// Java applicative), and yields a partial result when every field has a result or partial value.
#[macro_export]
macro_rules! record {
    ($ty:ident $(<$($g:ty),+>)? { $($field:ident : $codec:expr),+ $(,)? }) => {{
        $( let $field = $codec; )+
        let enc = { $( let $field = $field.clone(); )+
            move |ops: &$crate::Ops, input: &$ty $(<$($g),+>)?, b: &mut $crate::RecordBuilder| { $( $field.encode(ops, &input.$field, b); )+ } };
        let dec = { $( let $field = $field.clone(); )+
            move |ops: &$crate::Ops, m: &$crate::MapLike<'_>| -> $crate::DataResult<$ty $(<$($g),+>)?> {
                $( let $field = $field.decode(ops, m); )+
                let mut errs: Vec<$crate::Msg> = Vec::new();
                let mut all_partial = true;
                $( if let $crate::DataResult::Error { message, partial } = &$field {
                    errs.push(message.clone());
                    all_partial &= partial.is_some();
                } )+
                if errs.is_empty() {
                    $crate::DataResult::Success($ty { $( $field: $field.result().unwrap() ),+ })
                } else {
                    let message = $crate::Msg::new(move || errs.iter().rev().map(|m| m.get()).collect::<Vec<_>>().join("; "));
                    let partial = if all_partial { Some($ty { $( $field: $field.result_or_partial().unwrap() ),+ }) } else { None };
                    $crate::DataResult::Error { message, partial }
                }
            } };
        let keys = { $( let $field = $field.clone(); )+
            move || { let mut k: Vec<$crate::Dyn> = Vec::new(); $( k.extend($field.keys(&$crate::NBT)); )+ k } };
        $crate::MapCodec::from_fns(enc, dec, keys)
    }};
}
