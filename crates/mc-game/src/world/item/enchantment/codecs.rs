//! Codec helpers shared by the enchantment types: registry-keyed `dispatch` type keys (`byNameCodec().dispatch`), `Vec3` /
//! `Vec3i` codecs and the holder codecs that consult the registry being loaded.

use std::cell::Cell;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::Vec3i;
use mc_dfu::{Codec, DataResult, DOUBLE, INT};
use mc_protocol::components::holder::{Holder, HolderSet};
use mc_protocol::registry_context as global;

use crate::registry::codec::RegistryDecodeContext;

thread_local! {
    /// The decode context of the registry entry being parsed (see [`with_decode_context`]).
    static SCOPE: Cell<*const RegistryDecodeContext<'static>> = const { Cell::new(std::ptr::null()) };
}

/// Runs `f` with `ctx` as the registry view of the holder codecs of this module (`RegistryOps`): references to entries of
/// the registry being loaded stay pending, missing entries/tags fail with vanilla's messages. Outside a scope the
/// process-wide [`registry_context`](mc_protocol::registry_context) answers.
pub fn with_decode_context<R>(ctx: &RegistryDecodeContext<'_>, f: impl FnOnce() -> R) -> R {
    struct Restore(*const RegistryDecodeContext<'static>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPE.with(|s| s.set(self.0));
        }
    }
    // SAFETY: the pointer is only dereferenced on this thread while `f` runs (the `Restore` guard clears it on every exit
    // path, including unwinding), during which `ctx` is borrowed by this function. The lifetime is erased only to fit the
    // thread-local's `'static` type.
    let erased = ctx as *const RegistryDecodeContext<'_> as *const RegistryDecodeContext<'static>;
    let _restore = Restore(SCOPE.with(|s| s.replace(erased)));
    f()
}

fn scoped<R>(f: impl FnOnce(&RegistryDecodeContext<'_>) -> R) -> Option<R> {
    let ptr = SCOPE.with(Cell::get);
    // SAFETY: see `with_decode_context`: a non-null pointer is live for the whole scope.
    (!ptr.is_null()).then(|| f(unsafe { &*ptr }))
}

/// `scoped(ctx)` when a decode scope is active, else `fallback()`.
pub fn with_scoped_ctx<R>(scoped_f: impl FnOnce(&RegistryDecodeContext<'_>) -> R, fallback: impl FnOnce() -> R) -> R {
    match scoped(scoped_f) {
        Some(r) => r,
        None => fallback(),
    }
}

/// Whether `id` may be referenced as an element of `registry`.
pub fn check_holder(registry: &str, id: &Identifier) -> Result<(), String> {
    scoped(|c| c.check_holder(registry, id)).unwrap_or_else(|| {
        if global::registry_contains(registry, id) {
            Ok(())
        } else {
            Err(format!("Failed to get element {id} from registry {registry}"))
        }
    })
}

fn check_tag(registry: &str, tag: &Identifier) -> Result<(), String> {
    scoped(|c| c.check_tag(registry, tag)).unwrap_or_else(|| {
        if global::get().tag_exists(registry, tag) { Ok(()) } else { Err(format!("Missing tag: '{tag}' in '{registry}'")) }
    })
}

/// `registry.byNameCodec()` of a built-in registry whose entries are modelled by name only (the effect/type registries
/// that `dispatch` the codecs of this package): the static entry name.
pub fn type_codec(registry: &'static str) -> Codec<&'static str> {
    mc_dfu::extra::IDENTIFIER.flat_xmap(
        move |id| {
            let table = mc_data::registry::by_name(registry).expect("built-in registry");
            match table.id_of(&id.to_string()).and_then(|i| table.name_of(i)) {
                Some(name) => DataResult::success(name),
                None => {
                    let m = format!("Unknown registry key in ResourceKey[minecraft:root / {registry}]: {id}");
                    DataResult::error(move || m.clone())
                }
            }
        },
        |name: &&'static str| DataResult::success(Identifier::parse(name).expect("registered name")),
    )
}

/// `RegistryFixedCodec.create(key)` for references into a data-driven registry, checked against the registry being loaded.
pub fn holder_fixed_codec<T: 'static>(registry: &'static str) -> Codec<Holder<T>> {
    Codec::from_fns(
        move |ops, input: &Holder<T>, prefix| match input {
            Holder::Reference(id) => mc_dfu::extra::IDENTIFIER.encode(ops, id, prefix),
            Holder::Direct(_) => DataResult::error(move || format!("Elements from registry {registry} can't be serialized to a value")),
        },
        move |ops, input| {
            mc_dfu::extra::IDENTIFIER.decode(ops, input).flat_map(|(id, rest)| match check_holder(registry, &id) {
                Ok(()) => DataResult::success((Holder::Reference(id), rest)),
                Err(m) => DataResult::error(move || m.clone()),
            })
        },
    )
}

/// `RegistryFileCodec.create(key, elementCodec, allowInline)`.
pub fn holder_file_codec<T: Clone + 'static>(registry: &'static str, element: &Codec<T>, allow_inline: bool) -> Codec<Holder<T>> {
    let (e, d) = (element.clone(), element.clone());
    Codec::from_fns(
        move |ops, input: &Holder<T>, prefix| match input {
            Holder::Reference(id) => mc_dfu::extra::IDENTIFIER.encode(ops, id, prefix),
            Holder::Direct(v) => e.encode(ops, v, prefix),
        },
        move |ops, input| match mc_dfu::extra::IDENTIFIER.decode(ops, input) {
            DataResult::Success((id, rest)) => match check_holder(registry, &id) {
                Ok(()) => DataResult::success((Holder::Reference(id), rest)),
                Err(m) => DataResult::error(move || m.clone()),
            },
            DataResult::Error { .. } => {
                if allow_inline {
                    d.decode(ops, input).map(|(v, r)| (Holder::Direct(v), r))
                } else {
                    DataResult::error_str("Inline definitions not allowed here")
                }
            }
        },
    )
}

/// `RegistryCodecs.holderSet(key)` (`HolderSetCodec.create(key, RegistryFixedCodec.create(key), false)`) over a data-driven
/// registry: `"#tag"`, a single id or a list of ids.
pub fn holder_set_codec<T: Clone + Send + Sync + 'static>(registry: &'static str) -> Codec<HolderSet<T>> {
    let element = holder_fixed_codec::<T>(registry);
    let direct = mc_dfu::extra::compact_list_codec_with(&element, &element.list_of());
    let tag = mc_dfu::STRING.comap_flat_map(
        |s| match s.strip_prefix('#') {
            None => DataResult::error(move || format!("Not a tag id: {s}")),
            Some(rest) => match Identifier::read(rest) {
                Ok(id) => DataResult::success(id),
                Err(e) => DataResult::error(move || e.clone()),
            },
        },
        |id: &Identifier| format!("#{id}"),
    );
    let either = tag.either(&direct);
    let (e1, e2) = (either.clone(), either);
    Codec::from_fns(
        move |ops, input: &HolderSet<T>, prefix| match input {
            HolderSet::Named(t) => e1.encode(ops, &mc_dfu::Either::Left(t.clone()), prefix),
            HolderSet::Direct(v) => e1.encode(ops, &mc_dfu::Either::Right(v.clone()), prefix),
        },
        move |ops, input| {
            e2.decode(ops, input).flat_map(|(either, rest)| match either {
                mc_dfu::Either::Left(t) => match check_tag(registry, &t) {
                    Ok(()) => DataResult::success((HolderSet::Named(t), rest)),
                    Err(m) => DataResult::error(move || m.clone()),
                },
                mc_dfu::Either::Right(v) => DataResult::success((HolderSet::Direct(v), rest)),
            })
        },
    )
}

/// `RegistryCodecs.homogeneousList(key)` for a built-in registry with typed ids, tag existence checked like
/// [`holder_set_codec`].
pub fn registry_set_codec<T: mc_data::registry::RegistryId>() -> Codec<mc_protocol::components::RegistrySet<T>> {
    mc_protocol::components::RegistrySet::<T>::codec()
}

/// `Vec3.CODEC`: a list of exactly three doubles.
pub fn vec3_codec() -> Codec<Vec3> {
    DOUBLE.list_of().comap_flat_map(
        |list| match mc_dfu::extra::fixed_size(list, 3) {
            DataResult::Success(l) => DataResult::success(Vec3::new(l[0], l[1], l[2])),
            DataResult::Error { message, .. } => DataResult::error_msg(message, None),
        },
        |v: &Vec3| vec![v.x, v.y, v.z],
    )
}

/// `Vec3i.CODEC`: a list of exactly three ints.
pub fn vec3i_codec() -> Codec<Vec3i> {
    INT.list_of().comap_flat_map(
        |list| match mc_dfu::extra::fixed_size(list, 3) {
            DataResult::Success(l) => DataResult::success(Vec3i::new(l[0], l[1], l[2])),
            DataResult::Error { message, .. } => DataResult::error_msg(message, None),
        },
        |v: &Vec3i| vec![v.x(), v.y(), v.z()],
    )
}
