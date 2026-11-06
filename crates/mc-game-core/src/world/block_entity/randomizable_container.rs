//! `net.minecraft.world.RandomizableContainer` / the loot part of `RandomizableContainerBlockEntity`.
//!
//! Vanilla's block entities hold their `Level` and unpack the loot table lazily inside `getItem`/`isEmpty`/`removeItem`/
//! `setItem`. Rust containers have no level at hand there, so the unpacking happens at the entry points that do have it
//! ([`BlockEntity::unpack_loot_table`]: hopper/dropper container resolution, `createMenu`, ...). Observable behavior is
//! the same: every vanilla access path unpacks before it reads, and the fill uses its own seeded random.
//! Not implemented: the `GENERATE_LOOT` criterion trigger of a player's unpack is not fired.

use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::Entity;
use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_nbt::CompoundTag;
use mc_protocol::components::{CONTAINER_LOOT, DataComponentMapBuilder, SeededContainerLoot};

/// `LootTable`/`LootTableSeed` of a `RandomizableContainerBlockEntity` (`lootTable` is `null` once unpacked).
#[derive(Clone, Debug, Default)]
pub struct RandomizableContainerData {
    pub loot_table: Option<Identifier>,
    pub loot_table_seed: i64,
}

impl RandomizableContainerData {
    /// `RandomizableContainer.tryLoadLootTable(input)`: whether a loot table was present (an invalid key reads as none).
    pub fn try_load_loot_table(&mut self, tag: &CompoundTag) -> bool {
        self.loot_table = tag.get_string("LootTable").and_then(Identifier::try_parse);
        self.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        self.loot_table.is_some()
    }

    /// `RandomizableContainer.trySaveLootTable(output)`: whether a loot table was written (items must not be then).
    pub fn try_save_loot_table(&self, tag: &mut CompoundTag) -> bool {
        let Some(table) = &self.loot_table else {
            return false;
        };
        tag.put_string("LootTable", table.to_string());
        if self.loot_table_seed != 0 {
            tag.put_long("LootTableSeed", self.loot_table_seed);
        }
        true
    }

    /// `setLootTable(table, seed)`.
    pub fn set_loot_table(&mut self, table: Identifier, seed: i64) {
        self.loot_table = Some(table);
        self.loot_table_seed = seed;
    }

    /// `RandomizableContainerBlockEntity.applyImplicitComponents`: the `container_loot` component.
    pub fn apply_implicit_components(&mut self, stack: &crate::world::item::ItemStack) {
        if let Some(loot) = stack.get(&CONTAINER_LOOT) {
            self.loot_table = Some(loot.loot_table.clone());
            self.loot_table_seed = loot.seed;
        }
    }

    /// `RandomizableContainerBlockEntity.collectImplicitComponents`.
    pub fn collect_implicit_components(
        &self,
        components: DataComponentMapBuilder,
    ) -> DataComponentMapBuilder {
        match &self.loot_table {
            Some(table) => components.set(
                CONTAINER_LOOT,
                SeededContainerLoot {
                    loot_table: table.clone(),
                    seed: self.loot_table_seed,
                },
            ),
            None => components,
        }
    }
}

/// `RandomizableContainer.unpackLootTable(player)` for a block entity that is out of its chunk: rolls the table into
/// the empty slots (`LootTable.fill` with the `chest` parameter set: origin = center of the block, the player's luck
/// and `this_entity`).
pub fn unpack_loot_table(
    level: &ServerLevel,
    pos: BlockPos,
    data: &mut RandomizableContainerData,
    container: &mut dyn Container,
    player: Option<&dyn Entity>,
) {
    let Some(table_key) = data.loot_table.take() else {
        return;
    };
    let table = loading::get_loot_table(&level.registries, &table_key);
    let mut builder = LootParams::builder(level).origin(Vec3::at_center_of(&pos));
    if let Some(player) = player {
        let luck = player.living().map_or(0.0, |l| {
            l.get_attribute_value(mc_data::Attribute::LUCK) as f32
        });
        builder = builder.luck(luck).this_entity(player);
    }
    let params = builder.create(&LootContextParamSets::CHEST);
    table.fill(container, &params, data.loot_table_seed);
}
