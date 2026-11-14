//! `ShearsDispenseItemBehavior`: shears a honey-full beehive, a shearable entity or a leash in front.

use crate::world::level::block::impls::beehive_block::{
    MAX_HONEY_LEVELS, drop_honeycomb, release_bees_and_reset_honey_level,
};
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::{GameEvent, SoundEvent, props};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::BeeReleaseStatus;
use mc_game_core::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, play_optional_sound,
};
use mc_game_core::world::entity::mob::interact::shear_off_all_leash_connections;
use mc_game_core::world::item::{ItemStack, hurt_and_break_stack};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_protocol::packets::play::SoundSource;

struct ShearsDispense(OptionalState);

/// `tryShearBeehive(level, tool, pos)`.
fn try_shear_beehive(level: &mut ServerLevel, tool: &ItemStack, pos: BlockPos) -> bool {
    let state = level.get_block_state(pos);
    if state.is_tag("beehives")
        && state.has_property(props::LEVEL_HONEY)
        && state.get(props::LEVEL_HONEY) >= MAX_HONEY_LEVELS
    {
        level.play_sound(
            None,
            pos,
            SoundEvent::BLOCK_BEEHIVE_SHEAR,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        drop_honeycomb(level, tool, state, None, pos);
        release_bees_and_reset_honey_level(level, state, pos, None, BeeReleaseStatus::BeeReleased);
        level.game_event_at_block(GameEvent::SHEAR, pos, &GameEventContext::of_entity(None));
        return true;
    }
    false
}

/// `tryShearEntity(level, pos, tool)`: the first non-spectator entity whose leashes are cut or that is ready to shear.
fn try_shear_entity(level: &mut ServerLevel, pos: BlockPos, tool: &ItemStack) -> bool {
    let entities =
        level.get_entities_of_class(&AABB::from_block_pos(&pos), |_| true, |e| !e.is_spectator());
    for id in entities {
        let sheared = level.with_entity(id, |level, entity| {
            if shear_off_all_leash_connections(entity, level, None) {
                return true;
            }
            if entity.is_alive() && entity.is_shearable() && entity.ready_for_shearing() {
                entity.shear(level, SoundSource::Blocks, tool);
                level.game_event_at_block(
                    GameEvent::SHEAR,
                    pos,
                    &GameEventContext::of_entity(None),
                );
                return true;
            }
            false
        });
        if sheared == Some(true) {
            return true;
        }
    }
    false
}

impl DispenseTemplate for ShearsDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let pos = source.pos.relative(source.facing());
        let success = try_shear_beehive(source.level, &dispensed, pos)
            || try_shear_entity(source.level, pos, &dispensed);
        self.0.set_success(success);
        if success {
            hurt_and_break_stack(&mut dispensed, 1, source.level, None);
        }
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

mc_game_core::register_dispense_behavior!(&["minecraft:shears"], |_| Box::new(Templated(
    ShearsDispense(OptionalState::new())
)));
