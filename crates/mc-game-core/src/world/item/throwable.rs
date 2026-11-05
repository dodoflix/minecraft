//! Item side of the thrown potions and the experience bottle: `ThrowablePotionItem.use`, `SplashPotionItem.use`,
//! `LingeringPotionItem.use`, `ExperienceBottleItem.use`.
//!
//! Only `ThrownExperienceBottle` is spawned (`Projectile.spawnProjectileFromRotation`); the splash and lingering potion
//! projectile entities do not exist, so those items are still used up (sound, stat, consumption) without an entity.
//! `ProjectileItem.asProjectile` (dispenser) is the other entry point of the same entities.

use super::{DispenseConfig, InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt, ProjectileItem};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::projectile::spawn::spawn_projectile_from_rotation;
use crate::world::entity::projectile::throwableitemprojectile::thrown_experience_bottle::ThrownExperienceBottle;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::LevelAccessor;
use mc_data::SoundEvent;
use mc_protocol::packets::play::Stat;
use mc_protocol::packets::play::SoundSource;

/// The throwable items handled here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThrowKind {
    SplashPotion,
    LingeringPotion,
    ExperienceBottle,
}

impl ThrowKind {
    /// `ThrowablePotionItem.PROJECTILE_SHOOT_POWER` (0.5) / `ExperienceBottleItem` (0.7).
    fn power(self) -> f32 {
        if self == Self::ExperienceBottle { 0.7 } else { 0.5 }
    }
}

/// `use(level, player, hand)` of `SplashPotionItem` / `LingeringPotionItem` (throw sound, then
/// `ThrowablePotionItem.use`) and `ExperienceBottleItem` (throw sound, spawn, stat, consume).
pub fn throw_item(kind: ThrowKind, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
    let mut stack = player.get_item_in_hand(hand);
    let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
    let (sound, source) = match kind {
        ThrowKind::SplashPotion => (SoundEvent::ENTITY_SPLASH_POTION_THROW, SoundSource::Players),
        ThrowKind::LingeringPotion => (SoundEvent::ENTITY_LINGERING_POTION_THROW, SoundSource::Neutral),
        ThrowKind::ExperienceBottle => (SoundEvent::ENTITY_EXPERIENCE_BOTTLE_THROW, SoundSource::Neutral),
    };
    let pitch = 0.4 / (level.random().next_float() * 0.4 + 0.8);
    level.play_sound_at(None, x, y, z, sound, source, 0.5, pitch);
    if kind == ThrowKind::ExperienceBottle {
        spawn_projectile_from_rotation(level, |level, source, item| Box::new(ThrownExperienceBottle::with_owner(level, source, item)), &stack, player, -20.0, kind.power(), 1.0);
    }
    player.award_stat_once(level, Stat::Used(stack.item()));
    let infinite = player.player.abilities.instabuild;
    stack.consume(1, infinite);
    player.set_item_in_hand_slot(level, hand, stack);
    InteractionResult::SUCCESS
}

/// `ExperienceBottleItem`.
struct ExperienceBottleItem;

impl ItemBehaviour for ExperienceBottleItem {
    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        throw_item(ThrowKind::ExperienceBottle, level, player, hand)
    }

    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

impl ProjectileItem for ExperienceBottleItem {
    fn as_projectile(&self, level: &mut ServerLevel, position: Vec3, item_stack: &ItemStack, _direction: Direction) -> Box<dyn Entity> {
        Box::new(ThrownExperienceBottle::at(level, position.x, position.y, position.z, item_stack))
    }

    /// `uncertainty * 0.5`, `power * 1.25` of the defaults.
    fn create_dispense_config(&self) -> DispenseConfig {
        DispenseConfig { uncertainty: DispenseConfig::DEFAULT.uncertainty * 0.5, power: DispenseConfig::DEFAULT.power * 1.25, ..DispenseConfig::DEFAULT }
    }
}

register_item_class!("ExperienceBottleItem", |_| Box::new(ExperienceBottleItem));
