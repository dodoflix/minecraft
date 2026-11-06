//! `net.minecraft.world.level.block.SnifferEggBlock`.
use mc_core::math::mth;
use mc_data::block::PathComputationType;
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::hatch_spawns;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_protocol::packets::play::level::SoundSource;

pub struct SnifferEggBlock;

const REGULAR_HATCH_TIME_TICKS: i32 = 24000;
const BOOSTED_HATCH_TIME_TICKS: i32 = 12000;
const RANDOM_HATCH_OFFSET_TICKS: i32 = 300;

/// `SnifferEggBlock.hatchBoost`.
pub fn hatch_boost(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("sniffer_egg_hatch_boost")
}

impl BlockBehaviour for SnifferEggBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let hatch_level = state.get(props::HATCH);
        if hatch_level != 2 {
            let pitch = 0.9f32 + level.random().next_float() * 0.2f32;
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_SNIFFER_EGG_CRACK,
                SoundSource::Blocks,
                0.7,
                pitch,
            );
            level.set_block_flags(pos, state.set(props::HATCH, hatch_level + 1), 2);
        } else {
            let pitch = 0.9f32 + level.random().next_float() * 0.2f32;
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_SNIFFER_EGG_HATCH,
                SoundSource::Blocks,
                0.7,
                pitch,
            );
            level.destroy_block(pos, false, None, 512);
            let yaw = mth::wrap_degrees_f32(level.random().next_float() * 360.0f32);
            let mut tag = hatch_spawns::snap_to_tag(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 0.5,
                f64::from(pos.z()) + 0.5,
                yaw,
                0.0,
            );
            tag.put_int("Age", hatch_spawns::BABY_AGE);
            hatch_spawns::spawn_breeding(level, EntityType::SNIFFER, &tag);
        }
    }
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        let boosted = hatch_boost(&*level, pos);
        if boosted {
            level.level_event(None, 3009, pos, 0);
        }
        let hatch_time = if boosted {
            BOOSTED_HATCH_TIME_TICKS
        } else {
            REGULAR_HATCH_TIME_TICKS
        };
        let progression_tick_delay = hatch_time / 3;
        level.game_event_at_block(
            GameEvent::BLOCK_PLACE,
            pos,
            &GameEventContext::of_state(Some(state)),
        );
        let delay =
            progression_tick_delay + level.random().next_int_bounded(RANDOM_HATCH_OFFSET_TICKS);
        helpers::schedule_block_tick(level, pos, state.block(), delay);
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
}

register_block_class!("SnifferEggBlock", |_| Box::new(SnifferEggBlock));
