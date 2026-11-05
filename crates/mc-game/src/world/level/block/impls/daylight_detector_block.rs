//! `net.minecraft.world.level.block.DaylightDetectorBlock` (a `BaseEntityBlock`; the ticker is
//! `DaylightDetectorBlockEntity::server_tick`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity as _, ServerPlayer};
use crate::world::item::InteractionResult;
use crate::world::level::LightLayer;
use crate::world::level::gameevent::GameEventContext;
use mc_core::math::mth;
use mc_data::GameEvent;

pub struct DaylightDetectorBlock;

/// `DaylightDetectorBlock.updateSignalStrength`.
pub fn update_signal_strength(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let mut target = (level.get_brightness(LightLayer::Sky, pos) - level.get_sky_darken()).max(0);
    let mut sun_angle = level.environment_float("visual/sun_angle") * (std::f64::consts::PI / 180.0) as f32;
    if state.get(props::INVERTED) {
        target = 15 - target;
    } else if target > 0 {
        let offset = if sun_angle < std::f64::consts::PI as f32 { 0.0 } else { (std::f64::consts::PI * 2.0) as f32 };
        sun_angle += (offset - sun_angle) * 0.2;
        // `Math.round(float)`: floor(x + 0.5), exact in double
        target = (f64::from(target as f32 * mth::cos(f64::from(sun_angle))) + 0.5).floor() as i32;
    }
    target = mth::clamp_i32(target, 0, 15);
    if state.get(props::POWER) != target {
        level.set_block_and_update(pos, state.set(props::POWER, target));
    }
}

impl BlockBehaviour for DaylightDetectorBlock {
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        if !player.player.abilities.may_build {
            return InteractionResult::PASS;
        }
        let new_state = state.cycle(props::INVERTED);
        level.set_block_flags(pos, new_state, 2);
        level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of(Some(player.id()), Some(new_state)));
        update_signal_strength(new_state, level, pos);
        InteractionResult::SUCCESS
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        state.get(props::POWER)
    }
}

register_block_class!("DaylightDetectorBlock", |_| Box::new(DaylightDetectorBlock));
