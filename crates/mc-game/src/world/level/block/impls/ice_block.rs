//! `net.minecraft.world.level.block.IceBlock` and `FrostedIceBlock`.
//!
//! `IceBlock.playerDestroy` (silk touch / `prevents_ice_melting` check, then melt) is the inherent
//! [`after_player_destroy`]; `FrostedIceBlock.getCloneItemStack` is empty. Frost Walker (`ReplaceDisk`) only places the
//! block and schedules its tick like vanilla, nothing of it lives here.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::world::level::LightLayer;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::redstone::Orientation;
use mc_core::math::mth;
use mc_data::GameEvent;

pub struct IceBlock;
pub struct FrostedIceBlock;

/// `IceBlock.meltsInto`.
fn melts_into() -> BlockState {
    Block::WATER.default_state()
}

/// `environmentAttributes().getValue(WATER_EVAPORATES, pos)`.
fn water_evaporates(level: &ServerLevel, pos: BlockPos) -> bool {
    level.attribute_bool(crate::world::attribute::attributes::WATER_EVAPORATES, pos)
}

/// `IceBlock.melt(state, level, pos)`.
pub fn melt(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if water_evaporates(level, pos) {
        level.remove_block(pos, false);
    } else {
        level.set_block_and_update(pos, melts_into());
        level.neighbor_changed(pos, melts_into().block(), None);
        level.game_event_at_block(GameEvent::BLOCK_DESTROY, pos, &GameEventContext::of_state(Some(state)));
    }
}

/// The part of `IceBlock.playerDestroy` after `super.playerDestroy`; `prevents_ice_melting` is whether the tool has
/// the `#minecraft:prevents_ice_melting` enchantment tag (silk touch).
pub fn after_player_destroy(level: &mut ServerLevel, pos: BlockPos, prevents_ice_melting: bool) {
    if prevents_ice_melting {
        return;
    }
    if water_evaporates(level, pos) {
        level.remove_block(pos, false);
        return;
    }
    let below_state = level.get_block_state(pos.below());
    if below_state.is_tag("ice_melts_when_destroyed_above") || below_state.liquid() {
        level.set_block_and_update(pos, melts_into());
    }
}

impl BlockBehaviour for IceBlock {
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        ice_random_tick(state, level, pos);
    }
}

fn ice_random_tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if level.get_brightness(LightLayer::Block, pos) > 11 - i32::from(state.get_light_block()) {
        melt(state, level, pos);
    }
}

/// `FrostedIceBlock.fewerNeigboursThan`.
fn fewer_neighbours_than(level: &dyn BlockGetter, this: Block, pos: BlockPos, limit: i32) -> bool {
    let mut result = 0;
    for direction in Direction::ALL {
        if level.get_block_state(pos.relative(direction)).is_block(this) {
            result += 1;
            if result >= limit {
                return false;
            }
        }
    }
    true
}

/// `FrostedIceBlock.slightlyMelt`: returns whether the block melted away.
fn slightly_melt(state: BlockState, level: &mut ServerLevel, pos: BlockPos) -> bool {
    let age = state.get(props::AGE_3);
    if age < 3 {
        level.set_block(pos, state.set(props::AGE_3, age + 1), 2, 512);
        false
    } else {
        melt(state, level, pos);
        true
    }
}

impl BlockBehaviour for FrostedIceBlock {
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        ice_random_tick(state, level, pos);
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        let delay = mth::next_int(level.random(), 60, 120);
        level.schedule_tick_block(pos, state.block(), delay);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let this = state.block();
        if level.random().next_int_bounded(3) == 0 || fewer_neighbours_than(&*level, this, pos, 4) {
            let brightness = if level.dimension == mc_core::ident::Identifier::with_default_namespace("the_end").expect("valid id") {
                level.get_brightness(LightLayer::Block, pos)
            } else {
                level.get_max_local_raw_brightness(pos)
            };
            if brightness > 11 - state.get(props::AGE_3) - i32::from(state.get_light_block()) && slightly_melt(state, level, pos) {
                for direction in Direction::ALL {
                    let neighbour_pos = pos.relative(direction);
                    let neighbour = level.get_block_state(neighbour_pos);
                    if neighbour.is_block(this) && !slightly_melt(neighbour, level, neighbour_pos) {
                        let delay = mth::next_int(level.random(), 20, 40);
                        level.schedule_tick_block(neighbour_pos, this, delay);
                    }
                }
                return;
            }
        }
        let delay = mth::next_int(level.random(), 20, 40);
        level.schedule_tick_block(pos, this, delay);
    }
    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        if block == state.block() && fewer_neighbours_than(&*level, state.block(), pos, 2) {
            melt(state, level, pos);
        }
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::empty()
    }
}

register_block_class!("IceBlock", |_| Box::new(IceBlock));
register_block_class!("FrostedIceBlock", |_| Box::new(FrostedIceBlock));
