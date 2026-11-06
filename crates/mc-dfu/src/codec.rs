//! `com.mojang.serialization.Codec` and `MapCodec`: object-safe, `Arc`-shared, built with the same fluent
//! combinators as vanilla (`xmap`, `flat_xmap`, `list_of`, `field_of`, `optional_field_of`, `dispatch`, ...).

use crate::dynv::Dyn;
use crate::ops::{MapLike, Ops, RecordBuilder};
use crate::result::DataResult;
use std::sync::{Arc, OnceLock};

/// `com.mojang.datafixers.util.Either`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub fn map<T>(self, l: impl FnOnce(L) -> T, r: impl FnOnce(R) -> T) -> T {
        match self {
            Either::Left(a) => l(a),
            Either::Right(b) => r(b),
        }
    }
}

impl<T> Either<T, T> {
    /// `Either.unwrap`.
    pub fn unwrap(self) -> T {
        match self {
            Either::Left(a) | Either::Right(a) => a,
        }
    }
}

/// `Dynamic<?>`: a value with the ops it was built by (`Codec.PASSTHROUGH`).
#[derive(Clone, Debug)]
pub struct Dynamic {
    pub ops: Ops,
    pub value: Dyn,
}

impl Dynamic {
    pub fn new(ops: Ops, value: Dyn) -> Self {
        Dynamic { ops, value }
    }
    /// `Dynamic.convert(outOps)` as the raw value (the `Dynamic`-returning form is `convert` in `dynamic_like.rs`).
    pub fn convert_value(&self, out: &Ops) -> Dyn {
        if self.ops == *out {
            self.value.clone()
        } else {
            self.ops.convert_to(out, &self.value)
        }
    }
}

// ---- Codec -------------------------------------------------------------------------------------------------

pub trait CodecImpl<A>: Send + Sync + 'static {
    fn encode(&self, ops: &Ops, input: &A, prefix: Dyn) -> DataResult<Dyn>;
    fn decode(&self, ops: &Ops, input: &Dyn) -> DataResult<(A, Dyn)>;
}

pub struct Codec<A: 'static>(Arc<dyn CodecImpl<A>>);

impl<A: 'static> Clone for Codec<A> {
    fn clone(&self) -> Self {
        Codec(self.0.clone())
    }
}

struct FnCodec<E, D> {
    enc: E,
    dec: D,
}

impl<A: 'static, E, D> CodecImpl<A> for FnCodec<E, D>
where
    E: Fn(&Ops, &A, Dyn) -> DataResult<Dyn> + Send + Sync + 'static,
    D: Fn(&Ops, &Dyn) -> DataResult<(A, Dyn)> + Send + Sync + 'static,
{
    fn encode(&self, ops: &Ops, input: &A, prefix: Dyn) -> DataResult<Dyn> {
        (self.enc)(ops, input, prefix)
    }
    fn decode(&self, ops: &Ops, input: &Dyn) -> DataResult<(A, Dyn)> {
        (self.dec)(ops, input)
    }
}

impl<A: 'static> Codec<A> {
    pub fn new(imp: impl CodecImpl<A>) -> Self {
        Codec(Arc::new(imp))
    }

    /// `Codec.of(encoder, decoder)` from closures (`encode(input, ops, prefix)`, `decode(ops, input)`).
    pub fn from_fns(
        enc: impl Fn(&Ops, &A, Dyn) -> DataResult<Dyn> + Send + Sync + 'static,
        dec: impl Fn(&Ops, &Dyn) -> DataResult<(A, Dyn)> + Send + Sync + 'static,
    ) -> Self {
        Codec::new(FnCodec { enc, dec })
    }

    /// `PrimitiveCodec`: `read` parses the value, `write` creates it; `mergeToPrimitive` on encode.
    pub fn primitive(
        read: impl Fn(&Ops, &Dyn) -> DataResult<A> + Send + Sync + 'static,
        write: impl Fn(&Ops, &A) -> Dyn + Send + Sync + 'static,
    ) -> Self {
        Codec::from_fns(
            move |ops, a, prefix| ops.merge_to_primitive(&prefix, write(ops, a)),
            move |ops, input| read(ops, input).map(|r| (r, ops.empty())),
        )
    }

    pub fn encode(&self, ops: &Ops, input: &A, prefix: Dyn) -> DataResult<Dyn> {
        self.0.encode(ops, input, prefix)
    }
    pub fn decode(&self, ops: &Ops, input: &Dyn) -> DataResult<(A, Dyn)> {
        self.0.decode(ops, input)
    }
    /// `Encoder.encodeStart`.
    pub fn encode_start(&self, ops: &Ops, input: &A) -> DataResult<Dyn> {
        self.0.encode(ops, input, ops.empty())
    }
    /// `Decoder.parse`.
    pub fn parse(&self, ops: &Ops, input: &Dyn) -> DataResult<A> {
        self.0.decode(ops, input).map(|(a, _)| a)
    }

    /// `Codec.recursive`: `build` receives the (lazily resolved) codec itself.
    pub fn recursive(build: impl FnOnce(Codec<A>) -> Codec<A>) -> Codec<A> {
        struct Rec<A: 'static> {
            cell: OnceLock<Codec<A>>,
        }
        impl<A: 'static> CodecImpl<A> for Rec<A> {
            fn encode(&self, ops: &Ops, input: &A, prefix: Dyn) -> DataResult<Dyn> {
                self.cell
                    .get()
                    .expect("recursive codec used before it was built")
                    .encode(ops, input, prefix)
            }
            fn decode(&self, ops: &Ops, input: &Dyn) -> DataResult<(A, Dyn)> {
                self.cell
                    .get()
                    .expect("recursive codec used before it was built")
                    .decode(ops, input)
            }
        }
        let rec = Arc::new(Rec {
            cell: OnceLock::new(),
        });
        struct Shared<A: 'static>(Arc<Rec<A>>);
        impl<A: 'static> CodecImpl<A> for Shared<A> {
            fn encode(&self, ops: &Ops, input: &A, prefix: Dyn) -> DataResult<Dyn> {
                self.0.encode(ops, input, prefix)
            }
            fn decode(&self, ops: &Ops, input: &Dyn) -> DataResult<(A, Dyn)> {
                self.0.decode(ops, input)
            }
        }
        let handle = Codec::new(Shared(rec.clone()));
        let built = build(handle.clone());
        let _ = rec.cell.set(built);
        handle
    }

    /// `Codec.lazyInitialized`: the delegate is built on first use (for forward references between statics).
    pub fn lazy(delegate: fn() -> Codec<A>) -> Codec<A> {
        struct Lazy<A: 'static> {
            f: fn() -> Codec<A>,
            cell: OnceLock<Codec<A>>,
        }
        impl<A: 'static> Lazy<A> {
            fn get(&self) -> &Codec<A> {
                self.cell.get_or_init(self.f)
            }
        }
        impl<A: 'static> CodecImpl<A> for Lazy<A> {
            fn encode(&self, ops: &Ops, input: &A, prefix: Dyn) -> DataResult<Dyn> {
                self.get().encode(ops, input, prefix)
            }
            fn decode(&self, ops: &Ops, input: &Dyn) -> DataResult<(A, Dyn)> {
                self.get().decode(ops, input)
            }
        }
        Codec::new(Lazy {
            f: delegate,
            cell: OnceLock::new(),
        })
    }

    // ---- mapping ---------------------------------------------------------------------------------------------

    /// `xmap(to, from)`.
    pub fn xmap<S: 'static>(
        &self,
        to: impl Fn(A) -> S + Send + Sync + 'static,
        from: impl Fn(&S) -> A + Send + Sync + 'static,
    ) -> Codec<S> {
        let (e, d) = (self.clone(), self.clone());
        Codec::from_fns(
            move |ops, s, prefix| e.encode(ops, &from(s), prefix),
            move |ops, input| d.decode(ops, input).map(|(a, rest)| (to(a), rest)),
        )
    }

    /// `flatXmap(to, from)`.
    pub fn flat_xmap<S: 'static>(
        &self,
        to: impl Fn(A) -> DataResult<S> + Send + Sync + 'static,
        from: impl Fn(&S) -> DataResult<A> + Send + Sync + 'static,
    ) -> Codec<S> {
        let (e, d) = (self.clone(), self.clone());
        Codec::from_fns(
            move |ops, s, prefix| from(s).flat_map(|a| e.encode(ops, &a, prefix)),
            move |ops, input| {
                d.decode(ops, input)
                    .flat_map(|(a, rest)| to(a).map(|s| (s, rest)))
            },
        )
    }

    /// `comapFlatMap(to, from)`: fallible decode, infallible encode.
    pub fn comap_flat_map<S: 'static>(
        &self,
        to: impl Fn(A) -> DataResult<S> + Send + Sync + 'static,
        from: impl Fn(&S) -> A + Send + Sync + 'static,
    ) -> Codec<S> {
        self.flat_xmap(to, move |s| DataResult::success(from(s)))
    }

    /// `flatComapMap(to, from)`: infallible decode, fallible encode.
    pub fn flat_comap_map<S: 'static>(
        &self,
        to: impl Fn(A) -> S + Send + Sync + 'static,
        from: impl Fn(&S) -> DataResult<A> + Send + Sync + 'static,
    ) -> Codec<S> {
        self.flat_xmap(move |a| DataResult::success(to(a)), from)
    }

    /// `validate(checker)` (`flatXmap(checker, checker)`).
    pub fn validate(
        &self,
        checker: impl Fn(A) -> DataResult<A> + Send + Sync + Clone + 'static,
    ) -> Codec<A>
    where
        A: Clone,
    {
        let c2 = checker.clone();
        self.flat_xmap(checker, move |a| c2(a.clone()))
    }

    /// `mapResult(ResultFunction)` building block used by `orElse`: post-process decode results.
    pub fn map_decode_result(
        &self,
        f: impl Fn(&Ops, &Dyn, DataResult<(A, Dyn)>) -> DataResult<(A, Dyn)> + Send + Sync + 'static,
    ) -> Codec<A> {
        let (e, d) = (self.clone(), self.clone());
        Codec::from_fns(
            move |ops, a, prefix| e.encode(ops, a, prefix),
            move |ops, input| f(ops, input, d.decode(ops, input)),
        )
    }

    /// `orElse(value)`: any decode failure yields `value` (consuming no input).
    pub fn or_else(&self, value: A) -> Codec<A>
    where
        A: Clone + Send + Sync,
    {
        self.map_decode_result(move |_, input, r| match r {
            DataResult::Success(p) => DataResult::success(p),
            DataResult::Error { .. } => DataResult::success((value.clone(), input.clone())),
        })
    }

    /// `orElseGet(supplier)`.
    pub fn or_else_get(&self, supplier: impl Fn() -> A + Send + Sync + 'static) -> Codec<A> {
        self.map_decode_result(move |_, input, r| match r {
            DataResult::Success(p) => DataResult::success(p),
            DataResult::Error { .. } => DataResult::success((supplier(), input.clone())),
        })
    }

    /// `orElse(onError, value)`: like [`or_else`] but reports the message.
    pub fn or_else_report(
        &self,
        on_error: impl Fn(&str) + Send + Sync + 'static,
        value: A,
    ) -> Codec<A>
    where
        A: Clone + Send + Sync,
    {
        self.map_decode_result(move |_, input, r| match r {
            DataResult::Success(p) => DataResult::success(p),
            DataResult::Error { message, .. } => {
                on_error(&message.get());
                DataResult::success((value.clone(), input.clone()))
            }
        })
    }

    /// `promotePartial(onError)` on decode.
    pub fn promote_partial(&self, on_error: impl Fn(&str) + Send + Sync + 'static) -> Codec<A> {
        self.map_decode_result(move |_, _, r| r.promote_partial(|m| on_error(m)))
    }

    // ---- lists / maps ----------------------------------------------------------------------------------------

    /// `listOf()`.
    pub fn list_of(&self) -> Codec<Vec<A>> {
        self.list_of_range(0, i32::MAX as usize)
    }

    /// `sizeLimitedListOf(max)`.
    pub fn size_limited_list_of(&self, max: usize) -> Codec<Vec<A>> {
        self.list_of_range(0, max)
    }

    /// `listOf(minSize, maxSize)` (`ListCodec`).
    pub fn list_of_range(&self, min: usize, max: usize) -> Codec<Vec<A>> {
        let (e, d) = (self.clone(), self.clone());
        Codec::from_fns(
            move |ops, input: &Vec<A>, prefix| {
                if input.len() < min {
                    let size = input.len();
                    return DataResult::error(move || {
                        format!("List is too short: {size}, expected range [{min}-{max}]")
                    });
                }
                if input.len() > max {
                    let size = input.len();
                    return DataResult::error(move || {
                        format!("List is too long: {size}, expected range [{min}-{max}]")
                    });
                }
                let mut b = ops.list_builder();
                for el in input {
                    b.add_result(e.encode_start(ops, el));
                }
                b.build(&prefix)
            },
            move |ops, input| {
                let list = match ops.get_list(input) {
                    DataResult::Success(l) => l,
                    DataResult::Error { message, .. } => {
                        return DataResult::error_msg(message, None);
                    }
                };
                let mut elements: Vec<A> = Vec::new();
                let mut failed: Vec<Dyn> = Vec::new();
                let mut result: DataResult<()> = DataResult::success(());
                let mut total = 0usize;
                for value in list.iter() {
                    total += 1;
                    if elements.len() >= max {
                        failed.push(value.clone());
                        continue;
                    }
                    let (step, el) = match d.decode(ops, value) {
                        DataResult::Success((a, _)) => (DataResult::success(()), Some(a)),
                        DataResult::Error { message, partial } => {
                            failed.push(value.clone());
                            (DataResult::error_msg(message, None), partial.map(|p| p.0))
                        }
                    };
                    if let Some(a) = el {
                        elements.push(a);
                    }
                    result = result.apply2(step, |_, _| ());
                }
                if elements.len() < min {
                    let size = elements.len();
                    return DataResult::error(move || {
                        format!("List is too short: {size}, expected range [{min}-{max}]")
                    });
                }
                let errors = ops.create_list(failed);
                if total > max {
                    result = DataResult::error(move || {
                        format!("List is too long: {total}, expected range [{min}-{max}]")
                    });
                }
                match result {
                    DataResult::Success(()) => DataResult::success((elements, errors)),
                    DataResult::Error { message, .. } => {
                        DataResult::error_msg(message, Some((elements, errors)))
                    }
                }
            },
        )
    }

    /// `Codec.unboundedMap(this, value)`: key/value pairs in input order; duplicate keys are an error.
    pub fn unbounded_map<V: 'static>(&self, value: &Codec<V>) -> Codec<Vec<(A, V)>>
    where
        A: PartialEq,
    {
        let (kc, vc) = (self.clone(), value.clone());
        let (kc2, vc2) = (self.clone(), value.clone());
        MapCodec::from_fns(
            move |ops, input: &Vec<(A, V)>, b| {
                for (k, v) in input {
                    b.add_results(kc.encode_start(ops, k), vc.encode_start(ops, v));
                }
            },
            move |ops, input| {
                // BaseMapCodec.decode
                let mut read: Vec<(A, V)> = Vec::new();
                let mut failed: Vec<(Dyn, Dyn)> = Vec::new();
                let mut result: DataResult<()> = DataResult::success(());
                for (k, v) in input.entries() {
                    let entry_result = kc2.parse(ops, k).apply2(vc2.parse(ops, v), |k, v| (k, v));
                    let step: DataResult<()> = match entry_result.error_msg_ref() {
                        Some(m) => DataResult::error_msg(m.clone(), None),
                        None => DataResult::success(()),
                    };
                    let is_error = entry_result.is_error();
                    if let Some((key, val)) = entry_result.result_or_partial() {
                        if read.iter().any(|(rk, _)| *rk == key) {
                            failed.push((k.clone(), v.clone()));
                            let key_text = k.clone();
                            result = result.apply2(
                                DataResult::error(move || {
                                    format!("Duplicate entry for key: '{key_text}'")
                                }),
                                |_, _: ()| (),
                            );
                            continue;
                        }
                        read.push((key, val));
                    }
                    if is_error {
                        failed.push((k.clone(), v.clone()));
                    }
                    result = result.apply2(step, |_, _| ());
                }
                let errors = ops.create_map(failed);
                match result {
                    DataResult::Success(()) => DataResult::success(read),
                    DataResult::Error { message, .. } => DataResult::error_msg(
                        crate::result::Msg::new(move || {
                            format!("{} missed input: {}", message.get(), errors)
                        }),
                        Some(read),
                    ),
                }
            },
            Vec::new,
        )
        .codec()
    }

    /// `Codec.pair(this, other)`.
    pub fn pair<B: 'static>(&self, second: &Codec<B>) -> Codec<(A, B)> {
        let (f1, s1, f2, s2) = (self.clone(), second.clone(), self.clone(), second.clone());
        Codec::from_fns(
            move |ops, (a, b): &(A, B), rest| {
                s1.encode(ops, b, rest).flat_map(|r| f1.encode(ops, a, r))
            },
            move |ops, input| {
                f2.decode(ops, input)
                    .flat_map(|(a, rest)| s2.decode(ops, &rest).map(|(b, rest2)| ((a, b), rest2)))
            },
        )
    }

    /// `Codec.either(this, other)`.
    pub fn either<B: 'static>(&self, second: &Codec<B>) -> Codec<Either<A, B>> {
        let (f1, s1, f2, s2) = (self.clone(), second.clone(), self.clone(), second.clone());
        Codec::from_fns(
            move |ops, input: &Either<A, B>, prefix| match input {
                Either::Left(a) => f1.encode(ops, a, prefix),
                Either::Right(b) => s1.encode(ops, b, prefix),
            },
            move |ops, input| {
                let first = f2.decode(ops, input).map(|(a, r)| (Either::Left(a), r));
                if first.is_success() {
                    return first;
                }
                let second = s2.decode(ops, input).map(|(b, r)| (Either::Right(b), r));
                if second.is_success() {
                    return second;
                }
                if first.has_result_or_partial() {
                    return first;
                }
                if second.has_result_or_partial() {
                    return second;
                }
                let (m1, m2) = (
                    first.error_msg_ref().cloned().unwrap(),
                    second.error_msg_ref().cloned().unwrap(),
                );
                DataResult::error(move || {
                    format!(
                        "Failed to parse either. First: {}; Second: {}",
                        m1.get(),
                        m2.get()
                    )
                })
            },
        )
    }

    /// `withAlternative(alternative)`: tries `self`, then `alternative` (same value type); encodes with `self`.
    pub fn with_alternative(&self, alternative: &Codec<A>) -> Codec<A> {
        let (e, d, alt) = (self.clone(), self.clone(), alternative.clone());
        Codec::from_fns(
            move |ops, a, prefix| e.encode(ops, a, prefix),
            move |ops, input| {
                // either(this, alt).xmap(unwrap, left)
                let first = d.decode(ops, input);
                if first.is_success() {
                    return first;
                }
                let second = alt.decode(ops, input);
                if second.is_success() {
                    return second;
                }
                if first.has_result_or_partial() {
                    return first;
                }
                if second.has_result_or_partial() {
                    return second;
                }
                let (m1, m2) = (
                    first.error_msg_ref().cloned().unwrap(),
                    second.error_msg_ref().cloned().unwrap(),
                );
                DataResult::error(move || {
                    format!(
                        "Failed to parse either. First: {}; Second: {}",
                        m1.get(),
                        m2.get()
                    )
                })
            },
        )
    }

    /// `withAlternative(alternative, converter)`: the alternative's value is converted; encodes with `self`.
    pub fn with_alternative_conv<U: 'static>(
        &self,
        alternative: &Codec<U>,
        converter: impl Fn(U) -> A + Send + Sync + 'static,
    ) -> Codec<A> {
        let (e, d, alt) = (self.clone(), self.clone(), alternative.clone());
        Codec::from_fns(
            move |ops, a, prefix| e.encode(ops, a, prefix),
            move |ops, input| {
                let first = d.decode(ops, input);
                if first.is_success() {
                    return first;
                }
                let second = alt.decode(ops, input).map(|(u, r)| (converter(u), r));
                if second.is_success() {
                    return second;
                }
                if first.has_result_or_partial() {
                    return first;
                }
                if second.has_result_or_partial() {
                    return second;
                }
                let (m1, m2) = (
                    first.error_msg_ref().cloned().unwrap(),
                    second.error_msg_ref().cloned().unwrap(),
                );
                DataResult::error(move || {
                    format!(
                        "Failed to parse either. First: {}; Second: {}",
                        m1.get(),
                        m2.get()
                    )
                })
            },
        )
    }

    /// Replace the encoder side with `enc`'s (used where vanilla's `xmap` back-conversion is trivial).
    pub fn with_encoder(&self, enc: &Codec<A>) -> Codec<A> {
        let (e, d) = (enc.clone(), self.clone());
        Codec::from_fns(
            move |ops, a, prefix| e.encode(ops, a, prefix),
            move |ops, input| d.decode(ops, input),
        )
    }

    /// `fieldOf(name)`.
    pub fn field_of(&self, name: &str) -> MapCodec<A> {
        let (e, d) = (self.clone(), self.clone());
        let (n1, n2, n3) = (name.to_string(), name.to_string(), name.to_string());
        MapCodec::from_fns(
            move |ops, input: &A, b| {
                b.add_result(&n1, e.encode_start(ops, input));
            },
            move |ops, input| match input.get(&n2) {
                None => {
                    let (n, i) = (n2.clone(), input.to_string());
                    DataResult::error(move || format!("No key {n} in {i}"))
                }
                Some(v) => d.parse(ops, v),
            },
            move || vec![Dyn::Str(n3.clone())],
        )
    }

    /// `optionalFieldOf(name)`.
    pub fn optional_field_of(&self, name: &str) -> MapCodec<Option<A>> {
        self.optional_field(name, false)
    }
    /// `lenientOptionalFieldOf(name)`: an invalid value reads as absent.
    pub fn lenient_optional_field_of(&self, name: &str) -> MapCodec<Option<A>> {
        self.optional_field(name, true)
    }

    fn optional_field(&self, name: &str, lenient: bool) -> MapCodec<Option<A>> {
        let (e, d) = (self.clone(), self.clone());
        let (n1, n2, n3) = (name.to_string(), name.to_string(), name.to_string());
        MapCodec::from_fns(
            move |ops, input: &Option<A>, b| {
                if let Some(a) = input {
                    b.add_result(&n1, e.encode_start(ops, a));
                }
            },
            move |ops, input| match input.get(&n2) {
                None => DataResult::success(None),
                Some(v) => {
                    let parsed = d.parse(ops, v);
                    if parsed.is_error() && lenient {
                        return DataResult::success(None);
                    }
                    match parsed {
                        DataResult::Success(a) => DataResult::success(Some(a)),
                        DataResult::Error { message, partial } => {
                            DataResult::error_msg(message, Some(partial.map(Some).unwrap_or(None)))
                        }
                    }
                }
            },
            move || vec![Dyn::Str(n3.clone())],
        )
    }

    /// `optionalFieldOf(name, default)`: absent reads as `default`; a value equal to `default` is not written.
    pub fn optional_field_of_default(&self, name: &str, default: A) -> MapCodec<A>
    where
        A: Clone + PartialEq + Send + Sync,
    {
        let d2 = default.clone();
        self.optional_field(name, false).xmap(
            move |o| o.unwrap_or_else(|| default.clone()),
            move |a: &A| if *a == d2 { None } else { Some(a.clone()) },
        )
    }
    /// `lenientOptionalFieldOf(name, default)`.
    pub fn lenient_optional_field_of_default(&self, name: &str, default: A) -> MapCodec<A>
    where
        A: Clone + PartialEq + Send + Sync,
    {
        let d2 = default.clone();
        self.optional_field(name, true).xmap(
            move |o| o.unwrap_or_else(|| default.clone()),
            move |a: &A| if *a == d2 { None } else { Some(a.clone()) },
        )
    }

    /// `dispatch(typeKey, type, codec)` with an infallible type function.
    pub fn dispatch<E: 'static>(
        &self,
        type_key: &str,
        type_of: impl Fn(&E) -> A + Send + Sync + 'static,
        codec_of: impl Fn(&A) -> MapCodec<E> + Send + Sync + 'static,
    ) -> Codec<E>
    where
        A: 'static,
    {
        self.dispatch_map(type_key, type_of, codec_of).codec()
    }

    /// `dispatchMap(typeKey, type, codec)`.
    pub fn dispatch_map<E: 'static>(
        &self,
        type_key: &str,
        type_of: impl Fn(&E) -> A + Send + Sync + 'static,
        codec_of: impl Fn(&A) -> MapCodec<E> + Send + Sync + 'static,
    ) -> MapCodec<E> {
        self.partial_dispatch_map(
            type_key,
            move |e| DataResult::success(type_of(e)),
            move |k| DataResult::success(codec_of(k)),
        )
    }

    /// `partialDispatch(typeKey, type, codec)`.
    pub fn partial_dispatch<E: 'static>(
        &self,
        type_key: &str,
        type_of: impl Fn(&E) -> DataResult<A> + Send + Sync + 'static,
        codec_of: impl Fn(&A) -> DataResult<MapCodec<E>> + Send + Sync + 'static,
    ) -> Codec<E> {
        self.partial_dispatch_map(type_key, type_of, codec_of)
            .codec()
    }

    /// `partialDispatchMap`-style `KeyDispatchCodec`.
    pub fn partial_dispatch_map<E: 'static>(
        &self,
        type_key: &str,
        type_of: impl Fn(&E) -> DataResult<A> + Send + Sync + 'static,
        codec_of: impl Fn(&A) -> DataResult<MapCodec<E>> + Send + Sync + 'static,
    ) -> MapCodec<E> {
        let key_codec = self.field_of(type_key);
        let (kc1, kc2, kc3) = (key_codec.clone(), key_codec.clone(), key_codec);
        let codec_of = Arc::new(codec_of);
        let (co1, co2) = (codec_of.clone(), codec_of);
        let type_of = Arc::new(type_of);
        MapCodec::from_fns(
            move |ops, input: &E, b| {
                let codec = type_of(input).flat_map(|k| co1(&k).map(|c| (k, c)));
                let errs = match &codec {
                    DataResult::Success(_) => None,
                    DataResult::Error { message, .. } => Some(message.clone()),
                };
                if let Some(m) = errs {
                    b.with_errors_from::<()>(&DataResult::error_msg(m, None));
                    return;
                }
                let (k, c) = codec.result().unwrap();
                c.encode(ops, input, b);
                kc1.encode(ops, &k, b);
            },
            move |ops, input| {
                kc2.decode(ops, input)
                    .flat_map(|k| co2(&k).flat_map(|c| c.decode(ops, input)))
            },
            move || {
                let mut k = kc3.keys(&crate::NBT);
                k.push(Dyn::Str("value".into()));
                k
            },
        )
    }
}

impl<A: 'static> Codec<A> {
    /// `Codec.unit(value)`: reads/writes nothing (still requires a map on decode).
    pub fn unit(value: A) -> Codec<A>
    where
        A: Clone + Send + Sync,
    {
        MapCodec::unit(value).codec()
    }
}

// ---- MapCodec ----------------------------------------------------------------------------------------------

pub trait MapCodecImpl<A>: Send + Sync + 'static {
    fn encode(&self, ops: &Ops, input: &A, builder: &mut RecordBuilder);
    fn decode(&self, ops: &Ops, input: &MapLike<'_>) -> DataResult<A>;
    /// `keys(ops)`: the string keys this codec reads/writes.
    fn keys(&self) -> Vec<Dyn>;
}

pub struct MapCodec<A: 'static>(Arc<dyn MapCodecImpl<A>>);

impl<A: 'static> Clone for MapCodec<A> {
    fn clone(&self) -> Self {
        MapCodec(self.0.clone())
    }
}

struct FnMapCodec<E, D, K> {
    enc: E,
    dec: D,
    keys: K,
}

impl<A: 'static, E, D, K> MapCodecImpl<A> for FnMapCodec<E, D, K>
where
    E: Fn(&Ops, &A, &mut RecordBuilder) + Send + Sync + 'static,
    D: Fn(&Ops, &MapLike<'_>) -> DataResult<A> + Send + Sync + 'static,
    K: Fn() -> Vec<Dyn> + Send + Sync + 'static,
{
    fn encode(&self, ops: &Ops, input: &A, builder: &mut RecordBuilder) {
        (self.enc)(ops, input, builder)
    }
    fn decode(&self, ops: &Ops, input: &MapLike<'_>) -> DataResult<A> {
        (self.dec)(ops, input)
    }
    fn keys(&self) -> Vec<Dyn> {
        (self.keys)()
    }
}

impl<A: 'static> MapCodec<A> {
    pub fn new(imp: impl MapCodecImpl<A>) -> Self {
        MapCodec(Arc::new(imp))
    }

    pub fn from_fns(
        enc: impl Fn(&Ops, &A, &mut RecordBuilder) + Send + Sync + 'static,
        dec: impl Fn(&Ops, &MapLike<'_>) -> DataResult<A> + Send + Sync + 'static,
        keys: impl Fn() -> Vec<Dyn> + Send + Sync + 'static,
    ) -> Self {
        MapCodec::new(FnMapCodec { enc, dec, keys })
    }

    pub fn encode(&self, ops: &Ops, input: &A, builder: &mut RecordBuilder) {
        self.0.encode(ops, input, builder)
    }
    pub fn decode(&self, ops: &Ops, input: &MapLike<'_>) -> DataResult<A> {
        self.0.decode(ops, input)
    }
    pub fn keys(&self, _ops: &Ops) -> Vec<Dyn> {
        self.0.keys()
    }

    /// `MapCodec.unit(value)`.
    pub fn unit(value: A) -> MapCodec<A>
    where
        A: Clone + Send + Sync,
    {
        MapCodec::from_fns(
            |_, _, _| {},
            move |_, _| DataResult::success(value.clone()),
            Vec::new,
        )
    }

    /// `MapCodec.codec()`.
    pub fn codec(&self) -> Codec<A> {
        let (e, d) = (self.clone(), self.clone());
        Codec::from_fns(
            move |ops, input, prefix| {
                let mut b = ops.map_builder();
                e.encode(ops, input, &mut b);
                b.build(&prefix)
            },
            move |ops, input| match ops.get_map(input) {
                DataResult::Success(m) => d.decode(ops, &m).map(|a| (a, input.clone())),
                DataResult::Error { message, .. } => DataResult::error_msg(message, None),
            },
        )
    }

    pub fn xmap<S: 'static>(
        &self,
        to: impl Fn(A) -> S + Send + Sync + 'static,
        from: impl Fn(&S) -> A + Send + Sync + 'static,
    ) -> MapCodec<S> {
        let (e, d, k) = (self.clone(), self.clone(), self.clone());
        MapCodec::from_fns(
            move |ops, s, b| e.encode(ops, &from(s), b),
            move |ops, m| d.decode(ops, m).map(&to),
            move || k.keys(&crate::NBT),
        )
    }

    pub fn flat_xmap<S: 'static>(
        &self,
        to: impl Fn(A) -> DataResult<S> + Send + Sync + 'static,
        from: impl Fn(&S) -> DataResult<A> + Send + Sync + 'static,
    ) -> MapCodec<S> {
        let (e, d, k) = (self.clone(), self.clone(), self.clone());
        MapCodec::from_fns(
            move |ops, s, b| {
                let r = from(s);
                match r {
                    DataResult::Success(a) => e.encode(ops, &a, b),
                    DataResult::Error { message, .. } => {
                        b.with_errors_from::<()>(&DataResult::error_msg(message, None));
                    }
                }
            },
            move |ops, m| d.decode(ops, m).flat_map(&to),
            move || k.keys(&crate::NBT),
        )
    }

    pub fn comap_flat_map<S: 'static>(
        &self,
        to: impl Fn(A) -> DataResult<S> + Send + Sync + 'static,
        from: impl Fn(&S) -> A + Send + Sync + 'static,
    ) -> MapCodec<S> {
        self.flat_xmap(to, move |s| DataResult::success(from(s)))
    }

    pub fn flat_comap_map<S: 'static>(
        &self,
        to: impl Fn(A) -> S + Send + Sync + 'static,
        from: impl Fn(&S) -> DataResult<A> + Send + Sync + 'static,
    ) -> MapCodec<S> {
        self.flat_xmap(move |a| DataResult::success(to(a)), from)
    }

    /// `MapCodec.validate`.
    pub fn validate(
        &self,
        checker: impl Fn(A) -> DataResult<A> + Send + Sync + Clone + 'static,
    ) -> MapCodec<A>
    where
        A: Clone,
    {
        let c2 = checker.clone();
        self.flat_xmap(checker, move |a| c2(a.clone()))
    }

    /// `MapCodec.orElse(value)`-style recovery: any decode failure yields `value()`.
    pub fn or_else_get(&self, value: impl Fn() -> A + Send + Sync + 'static) -> MapCodec<A> {
        let (e, d, k) = (self.clone(), self.clone(), self.clone());
        MapCodec::from_fns(
            move |ops, a, b| e.encode(ops, a, b),
            move |ops, m| match d.decode(ops, m) {
                DataResult::Success(a) => DataResult::success(a),
                DataResult::Error { .. } => DataResult::success(value()),
            },
            move || k.keys(&crate::NBT),
        )
    }

    /// `Codec.mapEither(this, other)`.
    pub fn either<B: 'static>(&self, second: &MapCodec<B>) -> MapCodec<Either<A, B>> {
        let (f1, s1, f2, s2, k1, k2) = (
            self.clone(),
            second.clone(),
            self.clone(),
            second.clone(),
            self.clone(),
            second.clone(),
        );
        MapCodec::from_fns(
            move |ops, input: &Either<A, B>, b| match input {
                Either::Left(a) => f1.encode(ops, a, b),
                Either::Right(c) => s1.encode(ops, c, b),
            },
            move |ops, m| {
                let first = f2.decode(ops, m).map(Either::Left);
                if first.is_success() {
                    return first;
                }
                let second = s2.decode(ops, m).map(Either::Right);
                if second.is_success() {
                    return second;
                }
                first.apply2(second, |_f, s| s)
            },
            move || {
                let mut k = k1.keys(&crate::NBT);
                k.extend(k2.keys(&crate::NBT));
                k
            },
        )
    }

    /// `Codec.mapPair(this, other)`.
    pub fn pair<B: 'static>(&self, second: &MapCodec<B>) -> MapCodec<(A, B)> {
        let (f1, s1, f2, s2, k1, k2) = (
            self.clone(),
            second.clone(),
            self.clone(),
            second.clone(),
            self.clone(),
            second.clone(),
        );
        MapCodec::from_fns(
            move |ops, (a, b): &(A, B), builder| {
                f1.encode(ops, a, builder);
                s1.encode(ops, b, builder);
            },
            move |ops, m| {
                f2.decode(ops, m)
                    .flat_map(|a| s2.decode(ops, m).map(|b| (a, b)))
            },
            move || {
                let mut k = k1.keys(&crate::NBT);
                k.extend(k2.keys(&crate::NBT));
                k
            },
        )
    }
}
