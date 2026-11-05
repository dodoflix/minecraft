//! `BrushableBlockEntity` (suspicious sand / gravel): brush progress, the archaeology loot (`LootTable` + seed or the
//! unpacked `item`) and the direction the brushing started from.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_dfu::extra::IDENTIFIER;
use mc_nbt::CompoundTag;
use crate::server::level::ServerLevel;
use crate::world::level::LevelAccessor;
use crate::world::item::ItemStack;
use mc_data::props;

pub const BRUSH_COOLDOWN_TICKS: i64 = 10;
pub const BRUSH_RESET_TICKS: i64 = 40;
pub const REQUIRED_BRUSHES_TO_BREAK: i32 = 10;

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
                level.set_block_and_update(pos, self.get_block_state().set(props::DUSTED, completion_state));
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

    fn load_additional(&mut self, tag: &CompoundTag) {
        // `tryLoadLootTable`
        self.loot_table = read_codec(tag, "LootTable", &IDENTIFIER);
        self.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        self.item = if self.loot_table.is_none() { read_codec(tag, "item", &ItemStack::codec()).unwrap_or_else(ItemStack::empty) } else { ItemStack::empty() };
        self.hit_direction = tag.get_byte("hit_direction").map(|b| Direction::from_3d_data_value(i32::from(b)));
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

    fn get_update_packet(&self) -> Option<mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket> {
        Some(mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BRUSHABLE: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::BRUSHABLE_BLOCK, create: |pos, state| Box::new(BrushableBlockEntity::new(pos, state)) };
