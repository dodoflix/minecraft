//! `loot.providers.nbt`: `NbtProvider` (`storage`, `context`) and the `NbtPath` codec.

use std::any::Any;
use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DataResult, Either, STRING, record};
use mc_nbt::{CompoundTag, Tag};

use crate::commands::arguments::nbt_path_argument::NbtPath;
use crate::world::level::storage::loot::context::{ContextKey, EntityTarget, LootContext};
use crate::world::level::storage::loot::holder::{TypeTable, lift};
use crate::world::level::storage::loot::server_access;

/// `NbtPathArgument.NbtPath.CODEC`.
pub fn nbt_path_codec() -> Codec<Arc<NbtPath>> {
    STRING.comap_flat_map(
        |string| match NbtPath::of(&string) {
            Ok(p) => DataResult::success(Arc::new(p)),
            Err(e) => {
                let m = format!("Failed to parse path {string}: {}", e.get_message());
                DataResult::error(move || m.clone())
            }
        },
        |p: &Arc<NbtPath>| p.as_string().to_owned(),
    )
}

/// `NbtProvider`.
pub trait NbtProvider: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    fn get(&self, context: &LootContext) -> Option<Tag>;
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    fn validate(
        &self,
        context: &crate::world::level::storage::loot::validation::ValidationContext,
    ) {
        context.validate_context_usage(&self.referenced_params());
    }
}

/// `StorageNbtProvider`.
#[derive(Clone)]
pub struct StorageNbtProvider {
    pub id: Identifier,
}

impl NbtProvider for StorageNbtProvider {
    fn type_name(&self) -> &'static str {
        "storage"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, context: &LootContext) -> Option<Tag> {
        Some(Tag::Compound(server_access::command_storage(
            context.get_level(),
            &self.id,
        )))
    }
}

/// `ContextNbtProvider`'s source: an entity target or the block entity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NbtSource {
    Entity(EntityTarget),
    BlockEntity,
}

impl NbtSource {
    fn name(self) -> &'static str {
        match self {
            NbtSource::Entity(t) => t.name(),
            NbtSource::BlockEntity => "block_entity",
        }
    }

    fn codec() -> Codec<NbtSource> {
        static ALL: LazyLock<Vec<(NbtSource, &'static str)>> = LazyLock::new(|| {
            let mut all: Vec<(NbtSource, &'static str)> = EntityTarget::ALL
                .iter()
                .map(|t| (NbtSource::Entity(*t), t.name()))
                .collect();
            all.push((NbtSource::BlockEntity, "block_entity"));
            all
        });
        mc_dfu::string_enum_codec(LazyLock::force(&ALL).as_slice())
    }

    pub fn context_param(self) -> ContextKey {
        match self {
            NbtSource::Entity(t) => t.context_param(),
            NbtSource::BlockEntity => ContextKey::BlockEntity,
        }
    }
}

/// `ContextNbtProvider`.
#[derive(Clone)]
pub struct ContextNbtProvider {
    pub source: NbtSource,
}

impl NbtProvider for ContextNbtProvider {
    fn type_name(&self) -> &'static str {
        "context"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, context: &LootContext) -> Option<Tag> {
        match self.source {
            NbtSource::BlockEntity => context
                .params()
                .block_entity
                .map(|be| Tag::Compound(be.save_with_full_metadata())),
            NbtSource::Entity(target) => target.get(context).map(|e| {
                Tag::Compound(
                    crate::advancements::predicates::nbt_predicate::get_entity_tag_to_compare(
                        context.get_level(),
                        e,
                    ),
                )
            }),
        }
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![self.source.context_param()]
    }
}

static TYPES: LazyLock<Arc<TypeTable<dyn NbtProvider>>> = LazyLock::new(|| {
    TypeTable::new(
        "minecraft:loot_nbt_provider_type",
        vec![
            (
                "storage",
                lift(
                    record!(StorageNbtProvider {
                        id: IDENTIFIER.field_of("source")
                    }),
                    |v| Arc::new(v) as Arc<dyn NbtProvider>,
                    |t| t.as_any().downcast_ref::<StorageNbtProvider>(),
                ),
            ),
            (
                "context",
                lift(
                    record!(ContextNbtProvider {
                        source: NbtSource::codec().field_of("target")
                    }),
                    |v| Arc::new(v) as Arc<dyn NbtProvider>,
                    |t| t.as_any().downcast_ref::<ContextNbtProvider>(),
                ),
            ),
        ],
    )
});

/// `NbtProviders.CODEC`.
pub fn nbt_provider_codec() -> Codec<Arc<dyn NbtProvider>> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<Arc<dyn NbtProvider>>> = LazyLock::new(|| {
            let typed = TYPES.codec(|e| e.type_name());
            let inline = NbtSource::codec().xmap(
                |source| ContextNbtProvider { source },
                |c: &ContextNbtProvider| c.source,
            );
            inline.either(&typed).xmap(
                |e| match e {
                    Either::Left(c) => Arc::new(c) as Arc<dyn NbtProvider>,
                    Either::Right(p) => p,
                },
                |p: &Arc<dyn NbtProvider>| match p.as_any().downcast_ref::<ContextNbtProvider>() {
                    Some(c) => Either::Left(c.clone()),
                    None => Either::Right(p.clone()),
                },
            )
        });
        C.clone()
    })
}
