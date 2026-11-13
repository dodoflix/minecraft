//! `EggItem`, `SnowballItem` and `EnderpearlItem` (+ the shared throw sound of the throwable items).

use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt, ProjectileItem,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::projectile::spawn::spawn_projectile_from_rotation;
use crate::world::entity::projectile::throwableitemprojectile::snowball::Snowball;
use crate::world::entity::projectile::throwableitemprojectile::thrown_egg::ThrownEgg;
use crate::world::entity::projectile::throwableitemprojectile::thrown_enderpearl::ThrownEnderpearl;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::LevelAccessor;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_data::SoundEvent;
use mc_protocol::packets::play::{SoundSource, Stat};

/// `level.playSound(null, player.getX(), player.getY(), player.getZ(), sound, source, 0.5F,
/// 0.4F / (level.getRandom().nextFloat() * 0.4F + 0.8F))`.
pub fn play_throw_sound(
    level: &mut ServerLevel,
    player: &dyn Entity,
    sound: SoundEvent,
    source: SoundSource,
) {
    let pitch = 0.4 / (level.random().next_float() * 0.4 + 0.8);
    let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
    level.play_sound_at(None, x, y, z, sound, source, 0.5, pitch);
}

/// The tail every throwable item `use` shares: `player.awardStat(ITEM_USED)`, `itemStack.consume(1, player)`.
pub fn finish_throw(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    mut stack: ItemStack,
) -> InteractionResult {
    player.award_stat_once(level, Stat::Used(stack.item()));
    let infinite = player.player.abilities.instabuild;
    stack.consume(1, infinite);
    player.set_item_in_hand(hand, stack);
    InteractionResult::SUCCESS
}

struct EggItem;

impl ItemBehaviour for EggItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        play_throw_sound(
            level,
            player,
            SoundEvent::ENTITY_EGG_THROW,
            SoundSource::Players,
        );
        spawn_projectile_from_rotation(
            level,
            |level, source, item| Box::new(ThrownEgg::with_owner(level, source, item)),
            &stack,
            player,
            0.0,
            1.5,
            1.0,
        );
        finish_throw(level, player, hand, stack)
    }

    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

impl ProjectileItem for EggItem {
    fn as_projectile(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
        item_stack: &ItemStack,
        _direction: Direction,
    ) -> Box<dyn Entity> {
        Box::new(ThrownEgg::at(
            level, position.x, position.y, position.z, item_stack,
        ))
    }
}

register_item_class!("EggItem", |_| Box::new(EggItem));

struct SnowballItem;

impl ItemBehaviour for SnowballItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        play_throw_sound(
            level,
            player,
            SoundEvent::ENTITY_SNOWBALL_THROW,
            SoundSource::Neutral,
        );
        spawn_projectile_from_rotation(
            level,
            |level, source, item| Box::new(Snowball::with_owner(level, source, item)),
            &stack,
            player,
            0.0,
            1.5,
            1.0,
        );
        finish_throw(level, player, hand, stack)
    }

    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

impl ProjectileItem for SnowballItem {
    fn as_projectile(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
        item_stack: &ItemStack,
        _direction: Direction,
    ) -> Box<dyn Entity> {
        Box::new(Snowball::at(
            level, position.x, position.y, position.z, item_stack,
        ))
    }
}

register_item_class!("SnowballItem", |_| Box::new(SnowballItem));

struct EnderpearlItem;

impl ItemBehaviour for EnderpearlItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        play_throw_sound(
            level,
            player,
            SoundEvent::ENTITY_ENDER_PEARL_THROW,
            SoundSource::Neutral,
        );
        if let Some(pearl) = spawn_projectile_from_rotation(
            level,
            |level, source, item| Box::new(ThrownEnderpearl::with_owner(level, source, item)),
            &stack,
            player,
            0.0,
            1.5,
            1.0,
        ) {
            player.register_ender_pearl(pearl);
        }
        finish_throw(level, player, hand, stack)
    }
}

register_item_class!("EnderpearlItem", |_| Box::new(EnderpearlItem));
