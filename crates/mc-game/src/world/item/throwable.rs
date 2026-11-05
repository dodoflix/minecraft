//! Item side of the thrown potions and the experience bottle: `ThrowablePotionItem.use`, `SplashPotionItem.use`,
//! `LingeringPotionItem.use`, `ExperienceBottleItem.use`.
//!
//! The projectile entities (`ThrownSplashPotion`, `ThrownLingeringPotion`, `ThrownExperienceBottle`) belong to the
//! throwables slice. It installs one [`ThrownSpawner`] per [`ThrowKind`] with [`set_thrown_spawner`]; the spawner is the
//! vanilla `Projectile.spawnProjectileFromRotation(Thrown*::new, level, stack, owner, yOffset, power, uncertainty)` call
//! (create the entity for `owner` and `stack`, `shootFromRotation`, `addFreshEntity`, `applyOnProjectileSpawned`).
//! Without a spawner the item is still used up (sound, stat, consumption), exactly the part of vanilla that does not
//! need the entity. `ProjectileItem.asProjectile` (dispenser) is the other entry point of the same entities.

use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::LevelAccessor;
use mc_data::SoundEvent;
use mc_protocol::packets::play::Stat;
use mc_protocol::packets::play::SoundSource;
use std::sync::OnceLock;

/// The throwable items handled here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThrowKind {
    SplashPotion,
    LingeringPotion,
    ExperienceBottle,
}

impl ThrowKind {
    fn index(self) -> usize {
        self as usize
    }

    /// `ThrowablePotionItem.PROJECTILE_SHOOT_POWER` (0.5) / `ExperienceBottleItem` (0.7).
    fn power(self) -> f32 {
        if self == Self::ExperienceBottle { 0.7 } else { 0.5 }
    }
}

/// `Projectile.spawnProjectileFromRotation(Thrown*::new, level, stack, owner, yOffset, pow, uncertainty)`.
pub type ThrownSpawner = fn(level: &mut ServerLevel, owner: &mut ServerPlayer, stack: &ItemStack, y_offset: f32, power: f32, uncertainty: f32);

static SPAWNERS: [OnceLock<ThrownSpawner>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];

/// Installed by the throwable entities' module (first caller wins).
pub fn set_thrown_spawner(kind: ThrowKind, spawner: ThrownSpawner) {
    let _ = SPAWNERS[kind.index()].set(spawner);
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
    if let Some(spawn) = SPAWNERS[kind.index()].get() {
        spawn(level, player, &stack, -20.0, kind.power(), 1.0);
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
}

register_item_class!("ExperienceBottleItem", |_| Box::new(ExperienceBottleItem));
