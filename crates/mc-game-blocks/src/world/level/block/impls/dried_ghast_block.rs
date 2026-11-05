//! `net.minecraft.world.level.block.DriedGhastBlock`.
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_game_core::world::level::block::{hatch_spawns, simple_waterlogged};
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_data::block::PathComputationType;
use mc_data::fluid::{Fluid, FluidState};
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct DriedGhastBlock;

const MAX_HYDRATION_LEVEL: i32 = 3;
const HYDRATION_TICK_DELAY: i32 = 5000;

/// `spawnGhastling`.
fn spawn_ghastling(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    level.remove_block(pos, false);
    let block_rotation = Direction::get_y_rot(state.get(props::HORIZONTAL_FACING));
    let mut tag = hatch_spawns::snap_to_tag(f64::from(pos.x()) + 0.5, f64::from(pos.y()), f64::from(pos.z()) + 0.5, block_rotation, 0.0);
    tag.put_int("Age", hatch_spawns::BABY_AGE);
    if let Some(ghastling) = hatch_spawns::spawn_breeding(level, EntityType::HAPPY_GHAST, &tag) {
        let seed = level.random().next_long();
        level.play_seeded_sound_from_entity(None, ghastling, SoundEvent::ENTITY_GHASTLING_SPAWN, SoundSource::Blocks, 1.0, 1.0, seed);
    }
}

impl BlockBehaviour for DriedGhastBlock {
    // SimpleWaterloggedBlock (placeLiquid is overridden below)
    fn is_liquid_block_container(&self) -> bool {
        true
    }
    fn can_place_liquid(&self, user: Option<&dyn mc_game_core::world::entity::Entity>, level: &dyn BlockGetter, pos: BlockPos, state: BlockState, fluid: Fluid) -> bool {
        simple_waterlogged::can_place_liquid(user, level, pos, state, fluid)
    }
    fn is_bucket_pickup(&self) -> bool {
        true
    }
    fn pickup_block(&self, user: Option<&dyn mc_game_core::world::entity::Entity>, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState) -> ItemStack {
        simple_waterlogged::pickup_block(user, level, pos, state)
    }
    fn get_pickup_sound(&self) -> Option<SoundEvent> {
        simple_waterlogged::get_pickup_sound()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let hydration_level = state.get(props::DRIED_GHAST_HYDRATION_LEVELS);
        if state.get(props::WATERLOGGED) {
            if hydration_level != MAX_HYDRATION_LEVEL {
                level.play_sound(None, pos, SoundEvent::BLOCK_DRIED_GHAST_TRANSITION, SoundSource::Blocks, 1.0, 1.0);
                level.set_block_flags(pos, state.set(props::DRIED_GHAST_HYDRATION_LEVELS, hydration_level + 1), 2);
                level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(state)));
            } else {
                spawn_ghastling(level, pos, state);
            }
        } else if hydration_level > 0 {
            level.set_block_flags(pos, state.set(props::DRIED_GHAST_HYDRATION_LEVELS, hydration_level - 1), 2);
            level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(state)));
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if (state.get(props::WATERLOGGED) || state.get(props::DRIED_GHAST_HYDRATION_LEVELS) > 0) && !level.has_scheduled_block_tick(pos, state.block()) {
            helpers::schedule_block_tick(level, pos, state.block(), HYDRATION_TICK_DELAY);
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
        let is_water_source = replaced_fluid_state.get_type() == Fluid::WATER;
        Some(block.default_state().set(props::WATERLOGGED, is_water_source).set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite()))
    }
    fn place_liquid(&self, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState, fluid_state: FluidState) -> bool {
        if !state.get(props::WATERLOGGED) && fluid_state.get_type() == Fluid::WATER {
            level.set_block_and_update(pos, state.set(props::WATERLOGGED, true));
            let delay = mc_game_core::world::level::material::fluid_behaviour(fluid_state.get_type()).get_tick_delay(&*level);
            level.schedule_fluid_tick(pos, fluid_state.get_type(), delay, mc_world::ticks::TickPriority::Normal);
            level.play_sound(None, pos, SoundEvent::BLOCK_DRIED_GHAST_PLACE_IN_WATER, SoundSource::Blocks, 1.0, 1.0);
            true
        } else {
            false
        }
    }
    fn set_placed_by(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _placer: Option<&mut ServerPlayer>, _stack: &ItemStack) {
        let sound = if state.get(props::WATERLOGGED) { SoundEvent::BLOCK_DRIED_GHAST_PLACE_IN_WATER } else { SoundEvent::BLOCK_DRIED_GHAST_PLACE };
        level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
}

register_block_class!("DriedGhastBlock", |_| Box::new(DriedGhastBlock));
