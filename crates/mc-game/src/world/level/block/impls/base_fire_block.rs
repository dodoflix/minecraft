//! `net.minecraft.world.level.block.BaseFireBlock` (the static helpers; `FireBlock`/`SoulFireBlock` extend it).
use super::prelude::*;

/// `BaseFireBlock.getState(level, pos)`: soul fire on soul-fire base blocks, else `FireBlock.getStateForPlacement`.
pub fn get_state(level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    if super::soul_fire_block::can_survive_on_block(level.get_block_state(pos.below())) {
        Block::SOUL_FIRE.default_state()
    } else {
        super::fire_block::get_state_for_placement(level, pos)
    }
}

/// `BaseFireBlock.entityInside` (`fire_damage`: 1.0 for fire, 2.0 for soul fire).
pub fn entity_inside(effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier, fire_damage: f32) {
    use crate::world::entity::inside_block_effects::InsideBlockEffectType;
    effect_applier.apply(InsideBlockEffectType::ClearFreeze);
    effect_applier.apply(InsideBlockEffectType::FireIgnite);
    effect_applier.run_after(
        InsideBlockEffectType::FireIgnite,
        Box::new(move |entity, level| {
            let source = level.damage_sources().in_fire();
            entity.hurt_server(level, &source, fire_damage);
        }),
    );
}

/// `BaseFireBlock.playerWillDestroy`: the fire-extinguish sound event, then `Block.playerWillDestroy`.
pub fn player_will_destroy(state: BlockState, level: &mut crate::server::level::ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer) -> BlockState {
    level.level_event(None, 1009, pos, 0);
    crate::server::level::block_break::player_will_destroy(level, pos, state, player)
}
