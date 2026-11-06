//! `net.minecraft.world.level.block.FarmlandBlock`.
use mc_data::GameEvent;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};

pub struct FarmlandBlock;

/// `FarmlandBlock.shouldMaintainFarmland`.
fn should_maintain_farmland(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    level
        .get_block_state(pos.above())
        .is_tag("maintains_farmland")
}

/// `FarmlandBlock.isNearWater`: `findBlocksIn(pos + (-4, 0, -4), pos + (4, 1, 4))` has a block with water.
fn is_near_water(level: &dyn LevelReader, pos: BlockPos) -> bool {
    for x in pos.x() - 4..=pos.x() + 4 {
        for y in pos.y()..=pos.y() + 1 {
            for z in pos.z() - 4..=pos.z() + 4 {
                if level
                    .get_block_state(BlockPos::new(x, y, z))
                    .get_fluid_state()
                    .is_tag("water")
                {
                    return true;
                }
            }
        }
    }
    false
}

/// `FarmlandBlock.turnToBaseBlock(sourceEntity, state, level, pos)` (the base block of every farmland is dirt).
pub fn turn_to_base_block(
    source_entity: Option<EntityId>,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
) {
    let new_state = helpers::push_entities_up(level, state, Block::DIRT.default_state(), pos);
    level.set_block_and_update(pos, new_state);
    level.game_event_at_block(
        GameEvent::BLOCK_CHANGE,
        pos,
        &GameEventContext::of(source_entity, Some(new_state)),
    );
}

impl BlockBehaviour for FarmlandBlock {
    fn fall_on(
        &self,
        level: &mut mc_game_core::server::level::ServerLevel,
        state: BlockState,
        pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        fall_distance: f64,
    ) {
        use mc_core::random::RandomSource;
        if f64::from(LevelAccessor::random(level).next_float()) < fall_distance - 0.5
            && entity.living().is_some()
        {
            let can_trample = match entity
                .as_any()
                .downcast_ref::<mc_game_core::world::entity::ServerPlayer>()
            {
                Some(player) => !level.is_under_spawn_protection(player, pos),
                None => level.mob_griefing(),
            };
            let (width, height) = (entity.base().get_bb_width(), entity.base().get_bb_height());
            if can_trample && width * width * height > 0.512 {
                // `turnToBaseBlock`: the base block of every farmland is dirt
                let new_state =
                    helpers::push_entities_up(level, state, Block::DIRT.default_state(), pos);
                level.set_block(pos, new_state, 3, 512);
                level.game_event_at_block(
                    mc_data::GameEvent::BLOCK_CHANGE,
                    pos,
                    &mc_game_core::world::level::gameevent::GameEventContext::of(
                        Some(entity.id()),
                        Some(new_state),
                    ),
                );
            }
        }
        // `super.fallOn`
        let reduced_fall_distance =
            fall_distance * f64::from(1.0f32 - state.block().data().fall_distance_reduction);
        let source = level.damage_sources().fall();
        entity.cause_fall_damage(level, reduced_fall_distance, 1.0, &source);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = level.get_block_state(pos.above());
        !above_state.is_solid() || should_maintain_farmland(level, pos)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        if !block
            .default_state()
            .can_survive(&*level, context.get_clicked_pos())
        {
            Some(Block::DIRT.default_state())
        } else {
            Some(block.default_state())
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Up && !state.can_survive(&*level, pos) {
                helpers::schedule_block_tick(level, pos, state.block(), 1);
            }
            state
        }
    );
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            turn_to_base_block(None, state, level, pos);
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let moisture = state.get(props::MOISTURE);
        if !is_near_water(&*level, pos) && !level.is_raining_at(pos.above()) {
            if moisture > 0 {
                level.set_block_flags(pos, state.set(props::MOISTURE, moisture - 1), 2);
            } else if !should_maintain_farmland(&*level, pos) {
                turn_to_base_block(None, state, level, pos);
            }
        } else if moisture < 7 {
            level.set_block_flags(pos, state.set(props::MOISTURE, 7), 2);
        }
    }
}

register_block_class!("FarmlandBlock", |_| Box::new(FarmlandBlock));
