//! `Holder` / `HolderSet` of the reloadable loot registries (`loot_table`, `predicate`, `item_modifier`,
//! `context_*_provider`, `slot_source`), their codecs (`RegistryFileCodec`, `RegistryCodecs.holderSet`) and the typed
//! `type`-dispatch codecs over code-defined type registries (`loot_condition_type`, ...).
//!
//! The values of those registries are stored as `Arc<T>` in the registry entry (`Registry::get::<Arc<T>>`), where `T` is
//! the (possibly unsized) element type, e.g. `dyn LootItemCondition`.

use std::cell::Cell;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_dfu::extra::{IDENTIFIER, compact_list_codec_with};
use mc_dfu::{Codec, DataResult, Either, MapCodec, STRING};

use crate::registry::RegistryAccess;
use crate::registry::codec::RegistryDecodeContext;

// ---- decode context ----------------------------------------------------------------------------------------------------

thread_local! {
    static DECODE_CONTEXT: Cell<*const RegistryDecodeContext<'static>> = const { Cell::new(std::ptr::null()) };
}

/// Runs `f` with `ctx` as the registry context of the loot codecs (`RegistryOps`): references are checked against it.
pub fn with_decode_context<R>(ctx: &RegistryDecodeContext, f: impl FnOnce() -> R) -> R {
    struct Restore(*const RegistryDecodeContext<'static>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DECODE_CONTEXT.with(|c| c.set(self.0));
        }
    }
    // SAFETY: the pointer is only dereferenced on this thread while `f` runs (the guard restores the previous value even
    // on unwind), during which `ctx` is borrowed; the erased lifetime never escapes.
    let erased = ctx as *const RegistryDecodeContext<'_> as *const RegistryDecodeContext<'static>;
    let _restore = Restore(DECODE_CONTEXT.with(|c| c.replace(erased)));
    f()
}

fn with_current<R>(f: impl FnOnce(&RegistryDecodeContext) -> R) -> Option<R> {
    let p = DECODE_CONTEXT.with(Cell::get);
    // SAFETY: see `with_decode_context`.
    (!p.is_null()).then(|| f(unsafe { &*p }))
}

/// `RegistryFileCodec` reference check against the current context (accepted when there is none).
fn check_holder(registry: &str, id: &Identifier) -> Result<(), String> {
    with_current(|ctx| ctx.check_holder(registry, id)).unwrap_or(Ok(()))
}

/// `HolderSetCodec.lookupTag`.
fn check_tag(registry: &str, tag: &Identifier) -> Result<(), String> {
    with_current(|ctx| ctx.check_tag(registry, tag)).unwrap_or(Ok(()))
}

// ---- Holder -------------------------------------------------------------------------------------------------------------

/// `Holder<T>`: a reference to a registry element by key, or an inline (direct) value.
pub enum Holder<T: ?Sized + 'static> {
    Reference { registry: &'static str, id: Identifier },
    Direct(Arc<T>),
}

impl<T: ?Sized + 'static> Clone for Holder<T> {
    fn clone(&self) -> Self {
        match self {
            Holder::Reference { registry, id } => Holder::Reference { registry, id: id.clone() },
            Holder::Direct(v) => Holder::Direct(v.clone()),
        }
    }
}

impl<T: ?Sized + 'static> Holder<T> {
    pub fn direct(value: Arc<T>) -> Holder<T> {
        Holder::Direct(value)
    }
}

impl<T: ?Sized + Send + Sync + 'static> Holder<T> {
    /// `value()` where the reference may be unbound (`None`).
    pub fn try_value(&self, resolver: &RegistryAccess) -> Option<Arc<T>> {
        match self {
            Holder::Direct(v) => Some(v.clone()),
            Holder::Reference { registry, id } => resolver.get::<Arc<T>>(registry, id).cloned(),
        }
    }

    /// `value()`: unbound references cannot exist after registry loading succeeded.
    pub fn value(&self, resolver: &RegistryAccess) -> Arc<T> {
        self.try_value(resolver).unwrap_or_else(|| match self {
            Holder::Reference { registry, id } => panic!("Trying to access unbound value 'ResourceKey[{registry} / {id}]'"),
            Holder::Direct(_) => unreachable!(),
        })
    }
}

impl<T: ?Sized + 'static> std::fmt::Debug for Holder<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Holder::Reference { id, .. } => write!(f, "Reference({id})"),
            Holder::Direct(_) => write!(f, "Direct(..)"),
        }
    }
}

/// `HolderSet<T>`: a tag or a direct list of holders.
pub enum HolderSet<T: ?Sized + 'static> {
    Named { registry: &'static str, tag: Identifier },
    Direct(Vec<Holder<T>>),
}

impl<T: ?Sized + 'static> Clone for HolderSet<T> {
    fn clone(&self) -> Self {
        match self {
            HolderSet::Named { registry, tag } => HolderSet::Named { registry, tag: tag.clone() },
            HolderSet::Direct(v) => HolderSet::Direct(v.clone()),
        }
    }
}

impl<T: ?Sized + Send + Sync + 'static> HolderSet<T> {
    /// `isBound()`: a tag is bound once its registry has it.
    pub fn is_bound(&self, resolver: &RegistryAccess) -> bool {
        match self {
            HolderSet::Named { registry, tag } => resolver.tag(registry, tag).is_some(),
            HolderSet::Direct(_) => true,
        }
    }

    /// The elements in order (`iterator()`); an unbound tag is empty.
    pub fn holders(&self, resolver: &RegistryAccess) -> Vec<Holder<T>> {
        match self {
            HolderSet::Direct(v) => v.clone(),
            HolderSet::Named { registry, tag } => {
                let Some(reg) = resolver.registry(registry) else { return Vec::new() };
                reg.tag(tag)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|id| reg.name_of(*id))
                    .map(|id| Holder::Reference { registry, id })
                    .collect()
            }
        }
    }

    /// The values in order.
    pub fn values(&self, resolver: &RegistryAccess) -> Vec<Arc<T>> {
        self.holders(resolver).iter().map(|h| h.value(resolver)).collect()
    }

    pub fn size(&self, resolver: &RegistryAccess) -> usize {
        match self {
            HolderSet::Direct(v) => v.len(),
            HolderSet::Named { registry, tag } => resolver.tag(registry, tag).map_or(0, <[u32]>::len),
        }
    }
}

// ---- codecs -------------------------------------------------------------------------------------------------------------

/// `RegistryFileCodec.create(registry, direct)`: a registered element's id, or an inline definition.
pub fn holder_codec<T: ?Sized + Send + Sync + 'static>(registry: &'static str, direct: Codec<Arc<T>>) -> Codec<Holder<T>> {
    let (enc, dec) = (direct.clone(), direct);
    Codec::from_fns(
        move |ops, input: &Holder<T>, prefix| match input {
            Holder::Reference { id, .. } => IDENTIFIER.encode(ops, id, prefix),
            Holder::Direct(v) => enc.encode(ops, v, prefix),
        },
        move |ops, input| match IDENTIFIER.decode(ops, input) {
            DataResult::Success((id, rest)) => match check_holder(registry, &id) {
                Ok(()) => DataResult::success((Holder::Reference { registry, id }, rest)),
                Err(m) => DataResult::error(move || m.clone()),
            },
            DataResult::Error { .. } => dec.decode(ops, input).map(|(v, rest)| (Holder::Direct(v), rest)),
        },
    )
}

/// `TagKey.hashedCodec(registry)`.
fn hashed_tag_codec() -> Codec<Identifier> {
    STRING.comap_flat_map(
        |s| match s.strip_prefix('#') {
            None => DataResult::error(move || format!("Not a tag id: {s}")),
            Some(rest) => match Identifier::read(rest) {
                Ok(id) => DataResult::success(id),
                Err(e) => DataResult::error(move || e.clone()),
            },
        },
        |id: &Identifier| format!("#{id}"),
    )
}

/// `RegistryCodecs.holderSet(registry, elementCodec)` (`HolderSetCodec.create(key, RegistryFileCodec(key, element), false)`).
pub fn holder_set_codec<T: ?Sized + Send + Sync + 'static>(registry: &'static str, element: Codec<Arc<T>>) -> Codec<HolderSet<T>> {
    let element = holder_codec(registry, element);
    let list = element.list_of();
    let direct = compact_list_codec_with(&element, &list);
    let either = hashed_tag_codec().either(&direct);
    let (e1, e2) = (either.clone(), either);
    Codec::from_fns(
        move |ops, input: &HolderSet<T>, prefix| match input {
            HolderSet::Named { tag, .. } => e1.encode(ops, &Either::Left(tag.clone()), prefix),
            HolderSet::Direct(v) => e1.encode(ops, &Either::Right(v.clone()), prefix),
        },
        move |ops, input| {
            e2.decode(ops, input).flat_map(|(either, rest)| match either {
                Either::Left(tag) => match check_tag(registry, &tag) {
                    Ok(()) => DataResult::success((HolderSet::Named { registry, tag }, rest)),
                    Err(m) => DataResult::error(move || m.clone()),
                },
                Either::Right(values) => DataResult::success((HolderSet::Direct(values), rest)),
            })
        },
    )
}

/// A `type`-keyed set of `MapCodec`s over a code-defined registry (`BuiltInRegistries.LOOT_CONDITION_TYPE`, ...):
/// `type_registry.byNameCodec().dispatch(codec, c -> c)`.
pub struct TypeTable<T: ?Sized + 'static> {
    registry: &'static str,
    entries: Vec<(&'static str, MapCodec<Arc<T>>)>,
}

impl<T: ?Sized + 'static> TypeTable<T> {
    pub fn new(registry: &'static str, entries: Vec<(&'static str, MapCodec<Arc<T>>)>) -> Arc<TypeTable<T>> {
        Arc::new(TypeTable { registry, entries })
    }

    /// The dispatch codec; `type_name` is the element's registered type name.
    pub fn codec(self: &Arc<Self>, type_name: fn(&T) -> &'static str) -> Codec<Arc<T>> {
        let (t1, t2) = (self.clone(), self.clone());
        let key_codec: Codec<&'static str> = IDENTIFIER.flat_xmap(
            move |id: Identifier| {
                if id.namespace() == "minecraft" {
                    if let Some((name, _)) = t1.entries.iter().find(|(n, _)| *n == id.path()) {
                        return DataResult::success(*name);
                    }
                }
                let registry = t1.registry;
                DataResult::error(move || format!("Unknown registry key in ResourceKey[minecraft:root / {registry}]: {id}"))
            },
            |name: &&'static str| DataResult::success(Identifier::new("minecraft", name).expect("valid type name")),
        );
        key_codec.dispatch(
            "type",
            move |e: &Arc<T>| type_name(&**e),
            move |name: &&'static str| t2.entries.iter().find(|(n, _)| n == name).expect("registered type").1.clone(),
        )
    }
}

/// A map codec of a concrete element `S` lifted to its trait object `Arc<T>`.
pub fn lift<T: ?Sized + 'static, S: Clone + 'static>(
    codec: MapCodec<S>,
    up: impl Fn(S) -> Arc<T> + Send + Sync + 'static,
    down: fn(&T) -> Option<&S>,
) -> MapCodec<Arc<T>> {
    codec.xmap(up, move |t: &Arc<T>| down(&**t).expect("type table entry matches the element").clone())
}

/// `RegistryFixedCodec.create(registry)` of an already loaded registry (`Enchantment.CODEC`, `WorldClock.CODEC`, ...): a
/// reference by id that must exist. The id is kept; resolving it happens where it is used.
pub fn registry_ref_codec(registry: &'static str) -> Codec<Identifier> {
    IDENTIFIER.flat_xmap(
        move |id: Identifier| {
            let checked = match with_current(|ctx| ctx.check_holder(registry, &id)) {
                Some(r) => r,
                None if mc_protocol::registry_context::registry_contains(registry, &id) => Ok(()),
                None => Err(format!("Failed to get element {id} from registry {registry}")),
            };
            match checked {
                Ok(()) => DataResult::success(id),
                Err(m) => DataResult::error(move || m.clone()),
            }
        },
        |id: &Identifier| DataResult::success(id.clone()),
    )
}
