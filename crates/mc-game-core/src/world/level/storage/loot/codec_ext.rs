//! Codec helpers that vanilla keeps in `ExtraCodecs` and that `mc_dfu` lacks.

use mc_dfu::{Codec, DataResult, Dyn, MapCodec};

/// `ExtraCodecs.dispatchOptionalValue(typeKey, valueKey, typeCodec, typeGetter, valueCodec)`: the `valueKey` object may be
/// absent (decoded as an empty map) and an empty encoded value is not written.
pub fn dispatch_optional_value<K: Clone + 'static, V: 'static>(
    type_key: &'static str,
    value_key: &'static str,
    type_codec: Codec<K>,
    type_getter: fn(&V) -> K,
    value_codec: fn(&K) -> DataResult<Codec<V>>,
) -> MapCodec<V> {
    let (tc_enc, tc_dec) = (type_codec.clone(), type_codec);
    MapCodec::from_fns(
        move |ops, input: &V, builder| {
            let key = type_getter(input);
            builder.add_result(type_key, tc_enc.encode_start(ops, &key));
            let parameters = value_codec(&key).flat_map(|c| c.encode_start(ops, input));
            let empty = ops.empty_map();
            match &parameters {
                DataResult::Success(v) if *v == empty => {}
                _ => {
                    builder.add_result(value_key, parameters);
                }
            }
        },
        move |ops, input| match input.get(type_key) {
            None => {
                let m = format!("Missing \"{type_key}\" in: {input}");
                DataResult::error(move || m.clone())
            }
            Some(type_name) => tc_dec.decode(ops, type_name).flat_map(|(key, _)| {
                let value: Dyn = input
                    .get(value_key)
                    .cloned()
                    .unwrap_or_else(|| ops.empty_map());
                value_codec(&key).flat_map(|c| c.decode(ops, &value).map(|(v, _)| v))
            }),
        },
        move || {
            vec![
                Dyn::Str(type_key.to_owned()),
                Dyn::Str(value_key.to_owned()),
            ]
        },
    )
}

/// `optionalFieldOf(name, default)` for values without `PartialEq` (holders): absent reads as `default`, always written.
pub trait OptionalDefaultWith<H> {
    fn optional_field_of_default_with(&self, name: &str, default: H) -> MapCodec<H>;
}

impl<H: Clone + Send + Sync + 'static> OptionalDefaultWith<H> for Codec<H> {
    fn optional_field_of_default_with(&self, name: &str, default: H) -> MapCodec<H> {
        self.optional_field_of(name).xmap(
            move |o| o.unwrap_or_else(|| default.clone()),
            |h: &H| Some(h.clone()),
        )
    }
}
