//! `net.minecraft.world.level.block.DetectorRailBlock` (extends `BaseRailBlock`).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::base_rail_block::{self, rail_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_core::phys::AABB;
use mc_game_core::world::level::block::rail_state::RailState;
use mc_data::EntityType;

pub struct DetectorRailBlock;

const PRESSED_CHECK_PERIOD: i32 = 20;

/// `entity instanceof AbstractMinecart`.
fn is_abstract_minecart(entity: &dyn Entity) -> bool {
    matches!(
        entity.get_type(),
        EntityType::MINECART
            | EntityType::CHEST_MINECART
            | EntityType::COMMAND_BLOCK_MINECART
            | EntityType::FURNACE_MINECART
            | EntityType::HOPPER_MINECART
            | EntityType::SPAWNER_MINECART
            | EntityType::TNT_MINECART
    )
}

/// `DetectorRailBlock.getSearchBB`.
fn get_search_bb(pos: BlockPos) -> AABB {
    let (x, y, z) = (pos.x(), pos.y(), pos.z());
    AABB::new(x as f64 + 0.2, y as f64, z as f64 + 0.2, (x + 1) as f64 - 0.2, (y + 1) as f64 - 0.2, (z + 1) as f64 - 0.2)
}

/// `DetectorRailBlock.getInteractingMinecartOfType`: the minecarts (matching `test`) in the search box, in entity
/// query order.
fn get_interacting_minecarts(level: &ServerLevel, pos: BlockPos, test: impl Fn(&dyn Entity) -> bool, selector: impl FnMut(&dyn Entity) -> bool) -> Vec<mc_game_core::world::entity::EntityId> {
    level.get_entities_of_class(&get_search_bb(pos), test, selector)
}

/// `DetectorRailBlock.checkPressed`. `ticking` is the entity whose `entityInside` triggered the check: it is not in the
/// level's entity storage while it ticks, vanilla's query finds it there.
fn check_pressed(level: &mut ServerLevel, pos: BlockPos, state: BlockState, ticking: Option<&dyn Entity>) {
    if base_rail_block::can_survive(&*level, pos) && level.get_block_state(pos).block() == state.block() {
        let was_pressed = state.get(props::POWERED);
        let should_be_pressed = !get_interacting_minecarts(level, pos, is_abstract_minecart, |_| true).is_empty()
            || ticking.is_some_and(|e| is_abstract_minecart(e) && get_search_bb(pos).intersects_aabb(&e.get_bounding_box()));
        let block = state.block();
        if should_be_pressed && !was_pressed {
            set_pressed(level, pos, state, true);
        }
        if !should_be_pressed && was_pressed {
            set_pressed(level, pos, state, false);
        }
        if should_be_pressed {
            level.schedule_tick_block(pos, block, PRESSED_CHECK_PERIOD);
        }
        level.update_neighbour_for_output_signal(pos, block);
    }
}

fn set_pressed(level: &mut ServerLevel, pos: BlockPos, state: BlockState, powered: bool) {
    let block = state.block();
    let new_state = state.set(props::POWERED, powered);
    level.set_block_and_update(pos, new_state);
    update_power_to_connected(level, pos, new_state);
    level.update_neighbors_at(pos, block);
    level.update_neighbors_at(pos.below(), block);
    // `setBlocksDirty` only matters to client levels
}

/// `DetectorRailBlock.updatePowerToConnected`.
fn update_power_to_connected(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let rail = RailState::new(pos, state);
    for connection_pos in rail.get_connections() {
        let connection_state = level.get_block_state(*connection_pos);
        level.neighbor_changed_with_state(connection_state, *connection_pos, connection_state.block(), None, false);
    }
}

/// `DetectorRailBlock.onPlace`.
fn on_place(state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, moved_by_piston: bool) {
    if old_state.block() != state.block() {
        let updated = base_rail_block::update_state_moved(state, level, pos, moved_by_piston);
        check_pressed(level, pos, updated, None);
    }
}

rail_behaviour!(DetectorRailBlock, |_state, _level, _pos, _block| {}, on_place,
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn entity_inside(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier, _is_precise: bool) {
        if !state.get(props::POWERED) {
            check_pressed(level, pos, state, Some(&*entity));
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::POWERED) {
            check_pressed(level, pos, state, None);
        }
    }
    fn get_direct_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::POWERED) && direction == Direction::Up { 15 } else { 0 }
    }
);

register_block_class!("DetectorRailBlock", |_| Box::new(DetectorRailBlock));
