//! `BrushableBlockEntity` (suspicious sand / gravel): brush progress, the archaeology loot (`LootTable` + seed or the
//! unpacked `item`) and the direction the brushing started from.

use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{BlockEntityType, EntityType, props};
use mc_dfu::extra::IDENTIFIER;
use mc_game_core::advancements::triggers::CriteriaTriggers;
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
    set_changed,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::behaviour;
use mc_game_core::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
use mc_nbt::CompoundTag;

const BRUSH_COOLDOWN_TICKS: i64 = 10;
const BRUSH_RESET_TICKS: i64 = 40;
const REQUIRED_BRUSHES_TO_BREAK: i32 = 10;

pub struct BrushableBlockEntity {
    base: BlockEntityData,
    pub brush_count: i32,
    pub brush_count_resets_at_tick: i64,
    pub cool_down_ends_at_tick: i64,
    pub item: ItemStack,
    pub hit_direction: Option<Direction>,
    pub loot_table: Option<Identifier>,
    pub loot_table_seed: i64,
}

impl BrushableBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::BRUSHABLE_BLOCK, pos, state),
            brush_count: 0,
            brush_count_resets_at_tick: 0,
            cool_down_ends_at_tick: 0,
            item: ItemStack::empty(),
            hit_direction: None,
            loot_table: None,
            loot_table_seed: 0,
        }
    }

    /// `setLootTable(lootTable, seed)`.
    pub fn set_loot_table(&mut self, loot_table: Identifier, seed: i64) {
        self.loot_table = Some(loot_table);
        self.loot_table_seed = seed;
    }

    /// `brush(gameTime, level, user, direction, brush)`: one brush stroke; `true` once the block is fully brushed (its loot
    /// dropped and the block turned into sand / gravel). The block entity must be out of its chunk (`with_block_entity`),
    /// see `mc_game_core::world::block_entity::brush_block`.
    pub fn brush(
        &mut self,
        game_time: i64,
        level: &mut ServerLevel,
        user: &mut ServerPlayer,
        direction: Direction,
        brush: &ItemStack,
    ) -> bool {
        if self.hit_direction.is_none() {
            self.hit_direction = Some(direction);
        }
        self.brush_count_resets_at_tick = game_time + BRUSH_RESET_TICKS;
        if game_time < self.cool_down_ends_at_tick {
            return false;
        }
        self.cool_down_ends_at_tick = game_time + BRUSH_COOLDOWN_TICKS;
        self.unpack_loot_table(level, user, brush);
        let previous_completion_state = self.get_completion_state();
        self.brush_count += 1;
        if self.brush_count >= REQUIRED_BRUSHES_TO_BREAK {
            self.brushing_completed(level, user, brush);
            return true;
        }
        let pos = self.get_block_pos();
        level.schedule_tick_block(pos, self.get_block_state().block(), 2);
        let completion_state = self.get_completion_state();
        if previous_completion_state != completion_state {
            level.set_block_and_update(
                pos,
                self.get_block_state().set(props::DUSTED, completion_state),
            );
        }
        false
    }

    /// `unpackLootTable(level, user, brush)`: rolls the archaeology table (`LootTableSeed`) into `item`.
    fn unpack_loot_table(
        &mut self,
        level: &mut ServerLevel,
        user: &mut ServerPlayer,
        brush: &ItemStack,
    ) {
        let Some(table_key) = self.loot_table.take() else {
            return;
        };
        CriteriaTriggers::GENERATE_LOOT.trigger(level, user, &table_key);
        let table = loading::get_loot_table(&level.registries, &table_key);
        let luck = user.living().map_or(0.0, |l| {
            l.get_attribute_value(mc_data::Attribute::LUCK) as f32
        });
        let params = LootParams::builder(level)
            .origin(Vec3::at_center_of(&self.get_block_pos()))
            .luck(luck)
            .this_entity(&*user)
            .tool(brush)
            .create(&LootContextParamSets::ARCHAEOLOGY);
        let loot = table.get_random_items_with_seed(&params, self.loot_table_seed);
        drop(params);
        if loot.len() > 1 {
            log::warn!(
                "Expected max 1 loot from loot table {table_key}, but got {}",
                loot.len()
            );
        }
        self.item = loot.into_iter().next().unwrap_or_else(ItemStack::empty);
        let (pos, state) = (self.get_block_pos(), self.get_block_state());
        set_changed(level, pos, state);
    }

    /// `brushingCompleted(level, user, brush)`.
    fn brushing_completed(
        &mut self,
        level: &mut ServerLevel,
        user: &mut ServerPlayer,
        brush: &ItemStack,
    ) {
        self.drop_content(level, user, brush);
        let (pos, state) = (self.get_block_pos(), self.get_block_state());
        LevelAccessor::level_event(level, None, 3008, pos, state.id() as i32);
        let turns_into = behaviour(state.block())
            .as_brushable()
            .map_or(Block::AIR, |b| b.turns_into);
        level.set_block_and_update(pos, turns_into.default_state());
    }

    /// `dropContent(level, user, brush)`: the loot (10..30 of it) appears at rest on the side the brushing started from.
    fn drop_content(
        &mut self,
        level: &mut ServerLevel,
        user: &mut ServerPlayer,
        brush: &ItemStack,
    ) {
        self.unpack_loot_table(level, user, brush);
        if self.item.is_empty() {
            return;
        }
        let item = EntityType::ITEM.data();
        let size = f64::from(item.width);
        let center_range = 1.0 - size;
        let half_size = size / 2.0;
        let drop_pos = self
            .get_block_pos()
            .relative(self.hit_direction.unwrap_or(Direction::Up));
        let xo = f64::from(drop_pos.x()) + 0.5 * center_range + half_size;
        let yo = f64::from(drop_pos.y()) + 0.5 + f64::from(item.height / 2.0_f32);
        let zo = f64::from(drop_pos.z()) + 0.5 * center_range + half_size;
        let amount = level.random().next_int_bounded(21) + 10;
        let mut entity = ItemEntity::with_stack(level, xo, yo, zo, self.item.split(amount));
        entity.base_mut().set_delta_movement(Vec3::ZERO);
        level.add_fresh_entity(Box::new(entity));
        self.item = ItemStack::empty();
    }

    /// `getCompletionState()`: the `dusted` value for the current brush count.
    pub fn get_completion_state(&self) -> i32 {
        if self.brush_count == 0 {
            0
        } else if self.brush_count < 3 {
            1
        } else if self.brush_count < 6 {
            2
        } else {
            3
        }
    }

    /// `checkReset(level)`: relaxes the brush count (two steps back every 4 ticks once the reset time passed) and keeps the
    /// block ticking while it is partially brushed.
    pub fn check_reset(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        if self.brush_count != 0 && level.get_game_time() >= self.brush_count_resets_at_tick {
            let previous_completion_state = self.get_completion_state();
            self.brush_count = (self.brush_count - 2).max(0);
            let completion_state = self.get_completion_state();
            if previous_completion_state != completion_state {
                level.set_block_and_update(
                    pos,
                    self.get_block_state().set(props::DUSTED, completion_state),
                );
            }
            self.brush_count_resets_at_tick = level.get_game_time() + 4;
        }
        if self.brush_count == 0 {
            self.hit_direction = None;
            self.brush_count_resets_at_tick = 0;
            self.cool_down_ends_at_tick = 0;
        } else {
            level.schedule_tick_block(pos, self.get_block_state().block(), 2);
        }
    }

    pub fn get_hit_direction(&self) -> Option<Direction> {
        self.hit_direction
    }

    pub fn get_item(&self) -> &ItemStack {
        &self.item
    }
}

impl BlockEntity for BrushableBlockEntity {
    block_entity_base!();

    fn try_brush(
        &mut self,
        game_time: i64,
        level: &mut ServerLevel,
        user: &mut ServerPlayer,
        direction: Direction,
        brush: &ItemStack,
    ) -> Option<bool> {
        Some(self.brush(game_time, level, user, direction, brush))
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        // `tryLoadLootTable`
        self.loot_table = read_codec(tag, "LootTable", &IDENTIFIER);
        self.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        self.item = if self.loot_table.is_none() {
            read_codec(tag, "item", &ItemStack::codec()).unwrap_or_else(ItemStack::empty)
        } else {
            ItemStack::empty()
        };
        self.hit_direction = tag
            .get_byte("hit_direction")
            .map(|b| Direction::from_3d_data_value(i32::from(b)));
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        // `trySaveLootTable` || item
        if let Some(loot_table) = &self.loot_table {
            if let Ok(encoded) = mc_dfu::encode_nbt(&IDENTIFIER, loot_table).into_result() {
                tag.put("LootTable", encoded);
            }
            if self.loot_table_seed != 0 {
                tag.put_long("LootTableSeed", self.loot_table_seed);
            }
        } else if !self.item.is_empty()
            && let Ok(encoded) = mc_dfu::encode_nbt(&ItemStack::codec(), &self.item).into_result()
        {
            tag.put("item", encoded);
        }
    }

    fn get_update_tag(&self) -> CompoundTag {
        // `super.getUpdateTag` = `saveCustomOnly`-less empty tag; `hit_direction` and `item` follow
        let mut tag = CompoundTag::new();
        if let Some(direction) = self.hit_direction {
            tag.put_byte("hit_direction", direction.get_3d_data_value() as i8);
        }
        if !self.item.is_empty()
            && let Ok(encoded) = mc_dfu::encode_nbt(&ItemStack::codec(), &self.item).into_result()
        {
            tag.put("item", encoded);
        }
        tag
    }

    fn get_update_packet(
        &self,
    ) -> Option<mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket> {
        Some(
            mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket {
                pos: self.get_block_pos(),
                r#type: self.get_type(),
                tag: self.get_update_tag(),
            },
        )
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BRUSHABLE: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::BRUSHABLE_BLOCK,
    create: |pos, state| Box::new(BrushableBlockEntity::new(pos, state)),
};
