//! `LootTableTrigger`: `player_generates_container_loot`.

use super::prelude::*;
use mc_core::ident::Identifier;

/// `LootTableTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct LootTableTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub loot_table: LootTableSet,
}

impl TriggerInstance for LootTableTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<LootTableTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(LootTableTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                loot_table: loot_table_set_codec().field_of("loot_tables"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for LootTableTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(LootTableTriggerInstance);

crate::register_triggers!(LootTableTriggerInstance {
    GENERATE_LOOT = "player_generates_container_loot",
});

impl LootTableTriggerInstance {
    /// `matches(lootTable)`: `lootTable.stream().anyMatch(table -> table.is(lootTable))`.
    pub fn matches(&self, registries: &crate::registry::RegistryAccess, loot_table: &Identifier) -> bool {
        match &self.loot_table {
            LootTableSet::Elements(ids) => ids.contains(loot_table),
            LootTableSet::Tag(tag) => registries.is_in_tag(crate::registry::keys::LOOT_TABLE, loot_table, tag),
        }
    }
}

impl Trigger<LootTableTriggerInstance> {
    /// `trigger(player, lootTable)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, loot_table: &Identifier) {
        let registries = level.registries.clone();
        self.trigger_with(level, player, |t| t.matches(&registries, loot_table));
    }
}

/// `HolderSet<LootTable>` of `LootTable.LIST_CODEC` as the trigger reads it: a `#tag` or loot table ids (inline tables
/// are accepted by the codec but can never equal a key).
#[derive(Clone)]
pub enum LootTableSet {
    Tag(Identifier),
    Elements(Vec<Identifier>),
}

pub fn loot_table_set_codec() -> Codec<LootTableSet> {
    use crate::registry::keys::LOOT_TABLE;
    use crate::world::level::storage::loot::holder::{HolderSet, holder_set_codec};
    use crate::world::level::storage::loot::table::loot_table_direct_codec;
    static C: std::sync::LazyLock<Codec<LootTableSet>> = std::sync::LazyLock::new(|| {
        holder_set_codec(LOOT_TABLE, loot_table_direct_codec()).xmap(
            |set| match set {
                HolderSet::Named { tag, .. } => LootTableSet::Tag(tag),
                HolderSet::Direct(holders) => LootTableSet::Elements(
                    holders
                        .into_iter()
                        .filter_map(|h| match h {
                            crate::world::level::storage::loot::holder::Holder::Reference { id, .. } => Some(id),
                            _ => None,
                        })
                        .collect(),
                ),
            },
            |s: &LootTableSet| match s {
                LootTableSet::Tag(tag) => HolderSet::Named { registry: LOOT_TABLE, tag: tag.clone() },
                LootTableSet::Elements(ids) => HolderSet::Direct(
                    ids.iter().map(|id| crate::world::level::storage::loot::holder::Holder::Reference { registry: LOOT_TABLE, id: id.clone() }).collect(),
                ),
            },
        )
    });
    C.clone()
}
