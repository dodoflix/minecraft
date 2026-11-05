//! `net.minecraft.world.level.block.TurtleEggBlock`. The trample hooks (`stepOn`, `fallOn`) are the entity-side wiring
//! of `EntitiesCore2`: they call [`destroy_egg`] with the entity's `canDestroyEgg` verdict.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::hatch_spawns;
use crate::world::level::gameevent::GameEventContext;
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct TurtleEggBlock;

/// `TurtleEggBlock.isSand`.
pub fn is_sand(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    level.get_block_state(pos).is_tag("sand")
}

/// `TurtleEggBlock.onSand`.
pub fn on_sand(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    is_sand(level, pos.below())
}

/// `shouldUpdateHatchLevel`.
fn should_update_hatch_level(level: &mut ServerLevel, _pos: BlockPos) -> bool {
    let chance = level.environment_float("gameplay/turtle_egg_hatch_chance");
    chance > 0.0 && level.random().next_float() < chance
}

/// `decreaseEggs`.
pub fn decrease_eggs(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let pitch = 0.9f32 + level.random().next_float() * 0.2f32;
    level.play_sound(None, pos, SoundEvent::ENTITY_TURTLE_EGG_BREAK, SoundSource::Blocks, 0.7, pitch);
    let number_of_eggs = state.get(props::EGGS);
    if number_of_eggs <= 1 {
        level.destroy_block(pos, false, None, 512);
    } else {
        level.set_block_flags(pos, state.set(props::EGGS, number_of_eggs - 1), 2);
        level.game_event_at_block(GameEvent::BLOCK_DESTROY, pos, &GameEventContext::of_state(Some(state)));
        level.level_event(None, 2001, pos, state.id() as i32);
    }
}

/// `destroyEgg(level, state, pos, entity, randomness)` once `canDestroyEgg(level, pos, entity)` (entity-dependent,
/// evaluated by the caller) allowed it.
pub fn destroy_egg(level: &mut ServerLevel, state: BlockState, pos: BlockPos, can_destroy_egg: bool, randomness: i32) {
    if state.is_block(Block::TURTLE_EGG) && can_destroy_egg && level.random().next_int_bounded(randomness) == 0 {
        decrease_eggs(level, pos, state);
    }
}

impl BlockBehaviour for TurtleEggBlock {
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if should_update_hatch_level(level, pos) && on_sand(&*level, pos) {
            let hatch = state.get(props::HATCH);
            if hatch < 2 {
                let pitch = 0.9f32 + level.random().next_float() * 0.2f32;
                level.play_sound(None, pos, SoundEvent::ENTITY_TURTLE_EGG_CRACK, SoundSource::Blocks, 0.7, pitch);
                level.set_block_flags(pos, state.set(props::HATCH, hatch + 1), 2);
                level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(state)));
            } else {
                let pitch = 0.9f32 + level.random().next_float() * 0.2f32;
                level.play_sound(None, pos, SoundEvent::ENTITY_TURTLE_EGG_HATCH, SoundSource::Blocks, 0.7, pitch);
                level.remove_block(pos, false);
                level.game_event_at_block(GameEvent::BLOCK_DESTROY, pos, &GameEventContext::of_state(Some(state)));
                for i in 0..state.get(props::EGGS) {
                    level.level_event(None, 2001, pos, state.id() as i32);
                    let mut tag = hatch_spawns::snap_to_tag(f64::from(pos.x()) + 0.3 + f64::from(i) * 0.2, f64::from(pos.y()), f64::from(pos.z()) + 0.3, 0.0, 0.0);
                    tag.put_int("Age", hatch_spawns::BABY_AGE);
                    tag.put_int_array("home_pos", vec![pos.x(), pos.y(), pos.z()]);
                    hatch_spawns::spawn_breeding(level, EntityType::TURTLE, &tag);
                }
            }
        }
    }
    fn on_place(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        if on_sand(&*level, pos) {
            level.level_event(None, 2012, pos, 15);
        }
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let held = context.use_on.get_item_in_hand();
        if !context.is_secondary_use_active() && !held.is_empty() && Some(held.item()) == state.block().item() && state.get(props::EGGS) < 4 {
            true
        } else {
            state.can_be_replaced() && (held.is_empty() || Some(held.item()) != state.block().item())
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let state = level.get_block_state(context.get_clicked_pos());
        Some(if state.is_block(block) { state.set(props::EGGS, (state.get(props::EGGS) + 1).min(4)) } else { block.default_state() })
    }
}

register_block_class!("TurtleEggBlock", |_| Box::new(TurtleEggBlock));
