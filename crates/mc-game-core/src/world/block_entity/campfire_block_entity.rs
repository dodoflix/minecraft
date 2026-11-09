//! `CampfireBlockEntity`: four cooking slots (the campfire cooking recipes), the cooking progress of each slot,
//! `cookTick`/`cooldownTick` (the server ticker chosen by `CampfireBlock.getTicker`) and `placeFood`.
//! The particle ticker is client only.

use super::ComponentReader;
use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::container::helper;
use crate::world::containers::{drop_contents_list, drop_item_stack};
use crate::world::entity::EntityId;
use crate::world::item::crafting::{RecipeCheck, SingleRecipeInput};
use crate::world::item::{ItemStack, ItemStackExt, is_item_enabled};
use crate::world::level::LevelReader;
use crate::world::level::gameevent::GameEventContext;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent, RecipeType, props};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::{CONTAINER, ItemContainerContents, map::DataComponentMapBuilder};
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

const BURN_COOL_SPEED: i32 = 2;
const NUM_SLOTS: usize = 4;

pub struct CampfireBlockEntity {
    base: BlockEntityData,
    items: [ItemStack; NUM_SLOTS],
    cooking_progress: [i32; NUM_SLOTS],
    cooking_time: [i32; NUM_SLOTS],
    quick_check: RecipeCheck,
}

impl CampfireBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::CAMPFIRE, pos, state),
            items: std::array::from_fn(|_| ItemStack::empty()),
            cooking_progress: [0; NUM_SLOTS],
            cooking_time: [0; NUM_SLOTS],
            quick_check: RecipeCheck::new(RecipeType::CAMPFIRE_COOKING),
        }
    }

    /// `cookTick(level, pos, state, entity, recipeCache)`.
    fn cook_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.base.pos;
        let state = self.base.block_state;
        let mut changed = false;
        for slot in 0..NUM_SLOTS {
            if self.items[slot].is_empty() {
                continue;
            }
            changed = true;
            self.cooking_progress[slot] += 1;
            if self.cooking_progress[slot] >= self.cooking_time[slot] {
                let item_stack = self.items[slot].clone();
                let input = SingleRecipeInput(item_stack.clone());
                let result = self
                    .quick_check
                    .get_recipe_for(&input, level)
                    .and_then(|r| r.value.as_cooking().map(|c| c.result().create()))
                    .unwrap_or(item_stack);
                if is_item_enabled(&result, level.enabled_features()) {
                    drop_item_stack(
                        level,
                        f64::from(pos.x()),
                        f64::from(pos.y()),
                        f64::from(pos.z()),
                        result,
                    );
                    self.items[slot] = ItemStack::empty();
                    level.send_block_updated(pos, state, state, 3);
                    level.game_event_at_block(
                        GameEvent::BLOCK_CHANGE,
                        pos,
                        &GameEventContext::of_state(Some(state)),
                    );
                }
            }
        }
        if changed {
            super::set_changed(level, pos, state);
        }
    }

    /// `cooldownTick(level, pos, state, entity)`.
    fn cooldown_tick(&mut self, level: &mut ServerLevel) {
        let mut changed = false;
        for slot in 0..NUM_SLOTS {
            if self.cooking_progress[slot] > 0 {
                changed = true;
                self.cooking_progress[slot] = mth::clamp_i32(
                    self.cooking_progress[slot] - BURN_COOL_SPEED,
                    0,
                    self.cooking_time[slot],
                );
            }
        }
        if changed {
            super::set_changed(level, self.base.pos, self.base.block_state);
        }
    }

    /// `placeFood(serverLevel, sourceEntity, placeItem)`: `source` is the placing entity, `infinite_materials` its
    /// `hasInfiniteMaterials()` (what `consumeAndReturn(1, sourceEntity)` reads).
    pub fn place_food(
        &mut self,
        level: &mut ServerLevel,
        source: Option<EntityId>,
        infinite_materials: bool,
        place_item: &mut ItemStack,
    ) -> bool {
        for slot in 0..NUM_SLOTS {
            if !self.items[slot].is_empty() {
                continue;
            }
            let input = SingleRecipeInput(place_item.clone());
            let Some(recipe) = level.recipe_access().get_recipe_for(
                RecipeType::CAMPFIRE_COOKING,
                &input,
                level,
                None,
            ) else {
                return false;
            };
            self.cooking_time[slot] = recipe.value.as_cooking().map_or(0, |c| c.cooking_time());
            self.cooking_progress[slot] = 0;
            self.items[slot] = place_item.consume_and_return(1, infinite_materials);
            level.game_event_at_block(
                GameEvent::BLOCK_CHANGE,
                self.base.pos,
                &GameEventContext::of(source, Some(self.base.block_state)),
            );
            self.mark_updated(level);
            return true;
        }
        false
    }

    /// `markUpdated()`.
    fn mark_updated(&mut self, level: &mut ServerLevel) {
        let (pos, state) = (self.base.pos, self.base.block_state);
        super::set_changed(level, pos, state);
        level.send_block_updated(pos, state, state, 3);
    }
}

impl BlockEntity for CampfireBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.items = std::array::from_fn(|_| ItemStack::empty());
        helper::load_all_items(tag, &mut self.items);
        load_times(tag, "CookingTimes", &mut self.cooking_progress);
        load_times(tag, "CookingTotalTimes", &mut self.cooking_time);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(items) = helper::save_all_items(&self.items, true) {
            tag.put("Items", Tag::List(items));
        }
        tag.put_int_array("CookingTimes", self.cooking_progress.to_vec());
        tag.put_int_array("CookingTotalTimes", self.cooking_time.to_vec());
    }

    /// `getUpdateTag`: the items only.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        if let Some(items) = helper::save_all_items(&self.items, true) {
            tag.put("Items", Tag::List(items));
        }
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.base.pos,
            r#type: self.base.ty,
            tag: self.get_update_tag(),
        })
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("Items");
    }

    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        match components.get(&CONTAINER) {
            Some(contents) => contents.copy_into(&mut self.items),
            None => ItemContainerContents::default().copy_into(&mut self.items),
        }
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = std::mem::take(builder).set(
            CONTAINER,
            ItemContainerContents::from_item_stacks(&self.items),
        );
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `CampfireBlock.getTicker` (server side): `cookTick` while lit, else `cooldownTick`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        if self.base.block_state.get(props::LIT) {
            self.cook_tick(level);
        } else {
            self.cooldown_tick(level);
        }
    }

    /// `preRemoveSideEffects`: `Containers.dropContents(level, pos, items)`.
    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
    ) {
        drop_contents_list(level, pos, &self.items);
    }
}

/// `System.arraycopy(times, 0, dst, 0, min(dst.length, times.length))` or zeros when the key is missing.
fn load_times(tag: &CompoundTag, key: &str, dst: &mut [i32; NUM_SLOTS]) {
    match tag.get_int_array(key) {
        Some(times) => {
            for (d, s) in dst.iter_mut().zip(times) {
                *d = *s;
            }
        }
        None => *dst = [0; NUM_SLOTS],
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CAMPFIRE: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::CAMPFIRE,
    create: |pos, state| Box::new(CampfireBlockEntity::new(pos, state)),
};
