//! `net.minecraft.world.level.block.TntBlock`.
use super::prelude::*;
use crate::world::level::signal_getter::SignalGetter;
use crate::server::level::ServerLevel;
use crate::world::entity::item::primed_tnt::PrimedTnt;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use crate::world::level::explosion::Explosion;
use crate::world::level::gameevent::GameEventContext;
use mc_data::enums::GameType;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;
use uuid::Uuid;

pub struct TntBlock;

/// `TntBlock.prime(level, pos)`.
pub fn prime(level: &mut ServerLevel, pos: BlockPos) -> bool {
    prime_by(level, pos, None, false)
}

/// `TntBlock.prime(level, pos, source, itemStack)` with the source reduced to what it needs: the living entity's id and
/// uuid, and whether the adventure-mode `can_break` check fails (`source instanceof Player` in adventure mode whose item
/// may not break this block).
fn prime_by(level: &mut ServerLevel, pos: BlockPos, source: Option<(EntityId, Uuid)>, blocked_in_adventure: bool) -> bool {
    if !level.tnt_explodes() || blocked_in_adventure {
        return false;
    }
    let tnt = PrimedTnt::at_owner_uuid(level, f64::from(pos.x()) + 0.5, f64::from(pos.y()), f64::from(pos.z()) + 0.5, source.map(|(_, uuid)| uuid));
    let (x, y, z) = (tnt.base().x(), tnt.base().y(), tnt.base().z());
    level.add_fresh_entity(Box::new(tnt));
    level.play_sound_at(None, x, y, z, SoundEvent::ENTITY_TNT_PRIMED, SoundSource::Blocks, 1.0, 1.0);
    level.game_event_at_block(GameEvent::PRIME_FUSE, pos, &GameEventContext::of_entity(source.map(|(id, _)| id)));
    true
}

impl BlockBehaviour for TntBlock {
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !old_state.is_block(state.block()) && level.has_neighbor_signal(pos) && prime(level, pos) {
            level.remove_block(pos, false);
        }
    }
    fn neighbor_changed(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<crate::world::level::redstone::Orientation>, _moved_by_piston: bool) {
        if level.has_neighbor_signal(pos) && prime(level, pos) {
            level.remove_block(pos, false);
        }
    }
    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer) -> BlockState {
        if !player.player.abilities.instabuild && state.get(props::UNSTABLE) {
            prime(level, pos);
        }
        crate::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    fn was_exploded(&self, level: &mut ServerLevel, pos: BlockPos, explosion: &dyn Explosion) {
        if level.tnt_explodes() {
            let owner = explosion.get_indirect_source_entity().and_then(|id| level.get_entity(id).map(|e| e.uuid()));
            let mut primed = PrimedTnt::at_owner_uuid(level, f64::from(pos.x()) + 0.5, f64::from(pos.y()), f64::from(pos.z()) + 0.5, owner);
            let fuse = PrimedTnt::get_random_short_fuse(primed.get_fuse(), LevelAccessor::random(level));
            primed.set_fuse(fuse);
            level.add_fresh_entity(Box::new(primed));
        }
    }
    fn drop_from_explosion(&self, _explosion: &dyn Explosion) -> bool {
        false
    }
    fn use_item_on(
        &self,
        stack: &ItemStack,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if !stack.is(Item::FLINT_AND_STEEL) && !stack.is(Item::FIRE_CHARGE) {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        }
        let blocked = player.game_mode.game_mode_for_player == GameType::Adventure && !crate::world::item::can_break_block_in_adventure_mode(stack, level, pos);
        if prime_by(level, pos, Some((player.base_id(), player.uuid())), blocked) {
            level.set_block_flags(pos, air(), 11);
            let mut used = stack.clone();
            if used.is(Item::FLINT_AND_STEEL) {
                crate::world::item::hurt_and_break(&mut used, 1, level, player, hand.as_equipment_slot());
            } else {
                used.consume(1, player.has_infinite_materials());
            }
            let item = stack.item();
            player.set_item_in_hand(hand, used);
            player.award_stat_once(level, crate::stats::Stat::Used(item));
        } else if !level.tnt_explodes() {
            player.send_overlay_message(mc_text::Component::translatable("block.minecraft.tnt.disabled", Vec::new()));
            return InteractionResult::PASS;
        }
        InteractionResult::SUCCESS
    }
    fn on_projectile_hit(&self, level: &mut ServerLevel, _state: BlockState, hit: &mc_core::phys::BlockHitResult, projectile: &mut dyn crate::world::entity::projectile::Projectile) {
        let pos = hit.block_pos;
        if !projectile.is_on_fire() || !projectile.may_interact(level, pos) {
            return;
        }
        // `owner instanceof LivingEntity` (a non-living owner is no source); `AbstractArrow.getPickupItemStackOrigin()` is
        // only read for the adventure-mode check of a player source, and arrows are not ported yet (see STATUS.md).
        let source = projectile.get_owner(level).filter(|o| o.living().is_some()).map(|o| (o.id(), o.uuid()));
        let blocked = source.is_some_and(|(id, _)| {
            level.get_entity(id).and_then(|o| o.as_any().downcast_ref::<ServerPlayer>()).is_some_and(|p| p.game_mode.game_mode_for_player == GameType::Adventure)
        }) && !crate::world::item::can_break_block_in_adventure_mode(&ItemStack::empty(), level, pos);
        if prime_by(level, pos, source, blocked) {
            level.remove_block(pos, false);
        }
    }
}

register_block_class!("TntBlock", |_| Box::new(TntBlock));
