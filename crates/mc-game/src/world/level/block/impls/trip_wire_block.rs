//! `net.minecraft.world.level.block.TripWireBlock`.
use super::prelude::*;
use super::trip_wire_hook_block::calculate_state;
use crate::server::level::ServerLevel;

pub struct TripWireBlock;

/// `TripWireBlock.shouldConnectTo` (`this` is the tripwire, its hook is `Blocks.TRIPWIRE_HOOK`).
fn should_connect_to(this: Block, block_state: BlockState, direction: Direction) -> bool {
    if block_state.is_block(Block::TRIPWIRE_HOOK) {
        block_state.get(props::HORIZONTAL_FACING) == direction.get_opposite()
    } else {
        block_state.is_block(this)
    }
}

/// `TripWireBlock.updateSource`.
fn update_source(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    for direction in [Direction::South, Direction::West] {
        for i in 1..42 {
            let test_pos = pos.relative_n(direction, i);
            let block = level.get_block_state(test_pos);
            if block.is_block(Block::TRIPWIRE_HOOK) {
                if block.get(props::HORIZONTAL_FACING) == direction.get_opposite() {
                    calculate_state(level, test_pos, block, false, true, i, Some(state));
                }
                break;
            }
            if !block.is_block(state.block()) {
                break;
            }
        }
    }
}

/// `TripWireBlock.checkPressed(level, pos, entities)`: `any_triggering` = some entity of the list does not ignore block
/// triggers.
fn check_pressed_with(level: &mut ServerLevel, pos: BlockPos, any_triggering: bool) {
    let state = level.get_block_state(pos);
    let was_pressed = state.get(props::POWERED);
    let should_be_pressed = any_triggering;
    let mut state = state;
    if should_be_pressed != was_pressed {
        state = state.set(props::POWERED, should_be_pressed);
        level.set_block_and_update(pos, state);
        update_source(level, pos, state);
    }
    if should_be_pressed {
        level.schedule_tick_block(pos, state.block(), 10);
    } else if was_pressed {
        level.schedule_tick_block(pos, state.block(), 1);
    }
}

/// `TripWireBlock.checkPressed(level, pos)`: the entities (`NO_SPECTATORS`) inside the wire's shape.
fn check_pressed(level: &mut ServerLevel, pos: BlockPos) {
    let state = level.get_block_state(pos);
    let bounds = state.get_shape(&*level, pos).bounds().move_(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
    let triggering = !level.get_entities(None, &bounds, |e| !e.is_spectator() && !e.is_ignoring_block_triggers()).is_empty();
    check_pressed_with(level, pos, triggering);
}

impl BlockBehaviour for TripWireBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_axis().is_horizontal() {
            state.set(helpers::pipe_property(direction), should_connect_to(state.block(), neighbour_state, direction))
        } else {
            state
        }
    });
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let mut state = block.default_state();
        for direction in [Direction::North, Direction::East, Direction::South, Direction::West] {
            let neighbour = level.get_block_state(pos.relative(direction));
            state = state.set(helpers::pipe_property(direction), should_connect_to(block, neighbour, direction));
        }
        Some(state)
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !old_state.is_block(state.block()) {
            update_source(level, pos, state);
        }
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
        if !moved_by_piston {
            update_source(level, pos, state.set(props::POWERED, true));
        }
    }
    fn get_entity_inside_collision_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, _entity: &dyn crate::world::entity::Entity) -> mc_core::phys::shapes::VoxelShape {
        state.get_shape(level, pos)
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if !state.get(props::POWERED) && !level.has_scheduled_block_tick(pos, state.block()) {
            check_pressed_with(level, pos, !entity.is_ignoring_block_triggers());
        }
    }
    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer) -> BlockState {
        use crate::world::entity::Entity as _;
        let held = player.get_item_in_hand(crate::world::item::InteractionHand::MainHand);
        if !held.is_empty() && held.is(mc_data::Item::SHEARS) {
            level.set_block_flags(pos, state.set(props::DISARMED, true), 260);
            level.game_event_at_block(mc_data::GameEvent::SHEAR, pos, &crate::world::level::gameevent::GameEventContext::of_entity(Some(player.id())));
        }
        crate::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    fn tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.get_block_state(pos).get(props::POWERED) {
            check_pressed(level, pos);
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(state, rotation, [props::NORTH, props::EAST, props::SOUTH, props::WEST])
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(state, mirror, [props::NORTH, props::EAST, props::SOUTH, props::WEST])
    }
}

register_block_class!("TripWireBlock", |_| Box::new(TripWireBlock));
