//! `net.minecraft.world.level.block.DetectorRailBlock` (extends `BaseRailBlock`).
use mc_core::phys::AABB;
use mc_data::EntityType;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::get_redstone_signal_from_container;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::vehicle::minecart::minecart_command_block::MinecartCommandBlock;
use mc_game_core::world::level::block::impls::base_rail_block::{self, rail_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::rail_state::RailState;

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

/// `EntitySelector.CONTAINER_ENTITY_SELECTOR`: `entity instanceof Container && entity.isAlive()`.
fn is_live_container_entity(entity: &dyn Entity) -> bool {
    entity.chest_vehicle().is_some() && entity.is_alive()
}

/// `DetectorRailBlock.getSearchBB`.
fn get_search_bb(pos: BlockPos) -> AABB {
    let (x, y, z) = (pos.x(), pos.y(), pos.z());
    AABB::new(
        x as f64 + 0.2,
        y as f64,
        z as f64 + 0.2,
        (x + 1) as f64 - 0.2,
        (y + 1) as f64 - 0.2,
        (z + 1) as f64 - 0.2,
    )
}

/// `DetectorRailBlock.getInteractingMinecartOfType`: the minecarts (matching `test`) in the search box, in entity
/// query order.
fn get_interacting_minecarts(
    level: &ServerLevel,
    pos: BlockPos,
    test: impl Fn(&dyn Entity) -> bool,
    selector: impl FnMut(&dyn Entity) -> bool,
) -> Vec<mc_game_core::world::entity::EntityId> {
    level.get_entities_of_class(&get_search_bb(pos), test, selector)
}

/// `DetectorRailBlock.checkPressed`.
fn check_pressed(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    if base_rail_block::can_survive(&*level, pos)
        && level.get_block_state(pos).block() == state.block()
    {
        let was_pressed = state.get(props::POWERED);
        let should_be_pressed =
            !get_interacting_minecarts(level, pos, is_abstract_minecart, |_| true).is_empty();
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
        level.neighbor_changed_with_state(
            connection_state,
            *connection_pos,
            connection_state.block(),
            None,
            false,
        );
    }
}

/// `DetectorRailBlock.onPlace`.
fn on_place(
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    old_state: BlockState,
    moved_by_piston: bool,
) {
    if old_state.block() != state.block() {
        let updated = base_rail_block::update_state_moved(state, level, pos, moved_by_piston);
        check_pressed(level, pos, updated);
    }
}

/// The first live container minecart in the search box of the rail at `pos` (query order).
fn first_container_minecart(
    level: &ServerLevel,
    pos: BlockPos,
) -> Option<mc_game_core::world::entity::EntityId> {
    get_interacting_minecarts(level, pos, is_abstract_minecart, is_live_container_entity)
        .first()
        .copied()
}

/// `getAnalogOutputSignal` of a powered rail: the success count of the first command block minecart on it, else the first
/// container minecart on it as a container.
fn minecart_signal(level: &ServerLevel, pos: BlockPos) -> Option<i32> {
    let command_blocks = get_interacting_minecarts(
        level,
        pos,
        |e| e.get_type() == EntityType::COMMAND_BLOCK_MINECART,
        |_| true,
    );
    if let Some(&id) = command_blocks.first() {
        let cart = level
            .get_entity(id)?
            .as_any()
            .downcast_ref::<MinecartCommandBlock>()?;
        return Some(cart.get_command_block().get_success_count());
    }
    let cart = level.get_entity(first_container_minecart(level, pos)?)?;
    Some(get_redstone_signal_from_container(cart.as_container()))
}

/// The lazy loot table unpack of `getRedstoneSignalFromContainer` for the container minecart a comparator reads through the
/// detector rail at `pos` (the analog output getters only get a shared level, so the comparator calls this first).
pub fn unpack_loot_for_signal(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    if !state.get(props::POWERED) {
        return;
    }
    let Some(id) = first_container_minecart(level, pos) else {
        return;
    };
    level.with_entity(id, |level, cart| {
        let position = cart.position();
        if let Some(chest) = cart.chest_vehicle_mut() {
            chest.unpack_for_access(level, position);
        }
    });
}

#[rustfmt::skip]
rail_behaviour!(DetectorRailBlock, |_state, _level, _pos, _block| {}, on_place,
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn get_analog_output_signal(&self, state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        if state.get(props::POWERED) {
            minecart_signal(level, pos).unwrap_or(0)
        } else {
            0
        }
    }
    fn entity_inside(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier, _is_precise: bool) {
        if !state.get(props::POWERED) {
            // the entity runs outside the level's storage: lent, the box query of the check finds it
            level.lend(entity, |level| check_pressed(level, pos, state));
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::POWERED) {
            check_pressed(level, pos, state);
        }
    }
    fn get_direct_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::POWERED) && direction == Direction::Up { 15 } else { 0 }
    }
);

register_block_class!("DetectorRailBlock", |_| Box::new(DetectorRailBlock));
