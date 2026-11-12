//! `BrushItem`: brushing suspicious sand and gravel while the use animation runs.

use super::{InteractionResult, ItemBehaviour, ItemStack, UseOnContext, hurt_and_break};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::movement::is_render_shape_invisible;
use crate::world::entity::projectile::util::get_hit_result_on_view_vector;
use crate::world::entity::projectile::{HitResult, HitType};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::behaviour;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_data::block::block_flags;
use mc_data::{Attribute, SoundEvent};
use mc_protocol::components::ItemUseAnimation;
use mc_protocol::packets::play::SoundSource;

/// `ANIMATION_DURATION`: a stroke lands every 10 ticks.
const ANIMATION_DURATION: i32 = 10;
/// `USE_DURATION`.
const USE_DURATION: i32 = 200;

pub struct BrushItem;

/// `calculateHitResult(player)`: `ProjectileUtil.getHitResultOnViewVector(player, CAN_BE_PICKED, blockInteractionRange)`.
fn calculate_hit_result(level: &ServerLevel, player: &ServerPlayer) -> HitResult {
    let range = player
        .player
        .living
        .get_attribute_value(Attribute::BLOCK_INTERACTION_RANGE);
    // `EntitySelector.CAN_BE_PICKED` = `NO_SPECTATORS.and(Entity::isPickable)`
    get_hit_result_on_view_vector(
        level,
        player,
        &|e| !e.is_spectator() && e.is_pickable(),
        range,
    )
}

impl ItemBehaviour for BrushItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        if calculate_hit_result(level, player).get_type() == HitType::Block {
            player.start_using_item(level, context.hand);
        }
        InteractionResult::CONSUME
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Brush
    }

    fn get_use_duration(&self, _stack: &ItemStack) -> i32 {
        USE_DURATION
    }

    fn on_use_tick(
        &self,
        level: &mut ServerLevel,
        user: &mut dyn Entity,
        stack: &mut ItemStack,
        ticks_remaining: i32,
    ) {
        let Some(player) = user.as_any_mut().downcast_mut::<ServerPlayer>() else {
            crate::world::entity::living_entity::using_item::release_using_item(user, level);
            return;
        };
        let hit = calculate_hit_result(level, player);
        let HitResult::Block(hit) = hit else {
            player.release_using_item(level);
            return;
        };
        if ticks_remaining < 0 || hit.miss {
            player.release_using_item(level);
            return;
        }
        let time_elapsed = USE_DURATION - ticks_remaining + 1;
        if time_elapsed % ANIMATION_DURATION != 5 {
            return;
        }
        let pos = hit.block_pos;
        let state = level.get_block_state(pos);
        if state.block().data().flags & block_flags::SPAWN_TERRAIN_PARTICLES != 0
            && !is_render_shape_invisible(state)
        {
            // `spawnDustParticles`: the particles are client-side, the level random draws are kept
            // (the count, then the x and z velocity factor of every particle).
            let particles = level.random().next_int_between(7, 12);
            for _ in 0..particles * 2 {
                level.random().next_double();
            }
        }
        let sound = behaviour(state.block())
            .as_brushable()
            .map_or(SoundEvent::ITEM_BRUSH_BRUSHING_GENERIC, |b| b.brush_sound);
        level.play_sound(
            Some(player.base_id()),
            pos,
            sound,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        if crate::world::block_entity::brush_block(level, pos, player, hit.direction, stack) {
            let slot = player.get_used_item_hand().as_equipment_slot();
            hurt_and_break(stack, 1, level, player, slot);
        }
    }
}

register_item_class!("BrushItem", |_| Box::new(BrushItem));
