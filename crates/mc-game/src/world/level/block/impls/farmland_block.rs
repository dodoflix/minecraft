//! `net.minecraft.world.level.block.FarmlandBlock`.
use super::prelude::*;

pub struct FarmlandBlock;

impl BlockBehaviour for FarmlandBlock {
    fn fall_on(&self, level: &mut crate::server::level::ServerLevel, state: BlockState, pos: BlockPos, entity: &mut dyn crate::world::entity::Entity, fall_distance: f64) {
        use mc_core::random::RandomSource;
        if f64::from(LevelAccessor::random(level).next_float()) < fall_distance - 0.5 && entity.living().is_some() {
            let can_trample = match entity.as_any().downcast_ref::<crate::world::entity::ServerPlayer>() {
                Some(player) => !level.is_under_spawn_protection(player, pos),
                None => level.mob_griefing(),
            };
            let (width, height) = (entity.base().get_bb_width(), entity.base().get_bb_height());
            if can_trample && width * width * height > 0.512 {
                // `turnToBaseBlock`: the base block of every farmland is dirt
                let new_state = helpers::push_entities_up(level, state, Block::DIRT.default_state(), pos);
                level.set_block(pos, new_state, 3, 512);
                level.game_event_at_block(mc_data::GameEvent::BLOCK_CHANGE, pos, &crate::world::level::gameevent::GameEventContext::of(Some(entity.id()), Some(new_state)));
            }
        }
        // `super.fallOn`
        let reduced_fall_distance = fall_distance * f64::from(1.0f32 - state.block().data().fall_distance_reduction);
        let source = level.damage_sources().fall();
        entity.cause_fall_damage(level, reduced_fall_distance, 1.0, &source);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = level.get_block_state(pos.above());
        !above_state.is_solid() || level.get_block_state(pos.above()).is_tag("maintains_farmland")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
}

register_block_class!("FarmlandBlock", |_| Box::new(FarmlandBlock));
