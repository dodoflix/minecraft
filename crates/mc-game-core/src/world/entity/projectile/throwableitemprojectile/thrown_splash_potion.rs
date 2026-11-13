//! `net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownSplashPotion`.

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{HitResult, Projectile, ProjectileData, util};
use super::abstract_thrown_potion::{self, contents_of};
use crate::server::level::ServerLevel;
use crate::world::effect::{EffectCause, MobEffectExt, potion_contents};
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::phys::BlockHitResult;
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;
use mc_protocol::components::POTION_DURATION_SCALE;

pub struct ThrownSplashPotion {
    base: EntityData,
    projectile: ProjectileData,
}

impl ThrownSplashPotion {
    /// `ThrownSplashPotion(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::SPLASH_POTION));
        });
        Self {
            base,
            projectile: ProjectileData::default(),
        }
    }

    /// `ThrownSplashPotion(level, owner, itemStack)`.
    pub fn with_owner(level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::SPLASH_POTION, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `ThrownSplashPotion(level, x, y, z, itemStack)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::SPLASH_POTION, level.next_entity_id());
        s.set_pos(x, y, z);
        throwable::set_item(&mut s.base, item);
        s
    }
}

/// `onHitAsPotion(level, potionItem, hitResult)`: every living entity within 4 blocks gets the effects scaled by its
/// distance to the potion's box at the hit location.
fn on_hit_as_potion(
    p: &mut dyn Projectile,
    level: &mut ServerLevel,
    potion_item: &ItemStack,
    hit: &HitResult,
) {
    let contents = contents_of(potion_item);
    let duration_scale = potion_item
        .get(&POTION_DURATION_SCALE)
        .copied()
        .unwrap_or(1.0);
    let effects = potion_contents::get_all_effects(&contents);
    let potion_aabb = p
        .get_bounding_box()
        .move_vec(hit.get_location().subtract(p.position()));
    let effect_aabb = potion_aabb.inflate(4.0, 2.0, 4.0);
    let entities = level.get_entities_of_class(&effect_aabb, |e| e.living().is_some(), |_| true);
    let margin = util::compute_margin(p.as_dyn());
    if entities.is_empty() {
        return;
    }
    let effect_source = p.get_effect_source(level);
    // Only instantaneous effects read the owner: it is lent out of the level storage for such an application so it
    // can be passed next to `&mut level`.
    let owner_id = p
        .get_owner(level)
        .map(|o| o.id())
        .filter(|_| effects.iter().any(|e| e.effect.is_instantaneous()));
    for id in entities {
        let Some(entity) = level.get_entity(id) else {
            continue;
        };
        if !entity.is_affected_by_potions() {
            continue;
        }
        let dist = potion_aabb
            .distance_to_sqr_aabb(&entity.get_bounding_box().inflate_scalar(f64::from(margin)));
        if dist >= 16.0 {
            continue;
        }
        let scale = 1.0 - dist.sqrt() / 4.0;
        let owner_is_victim = owner_id == Some(id);
        let lent_owner = owner_id
            .filter(|_| !owner_is_victim)
            .and_then(|o| level.take_entity(o).map(|e| (o, e)));
        level.with_entity(id, |level, entity| {
            let owner_cause = match (&lent_owner, owner_is_victim) {
                (Some((_, owner)), _) => Some(EffectCause::Entity(owner.as_ref())),
                (None, true) => Some(EffectCause::Mob),
                (None, false) => None,
            };
            for effect_instance in &effects {
                if effect_instance.effect.is_instantaneous() {
                    effect_instance.effect.apply_instantaneous_effect(
                        level,
                        Some(EffectCause::Entity(p.as_dyn())),
                        owner_cause,
                        entity,
                        effect_instance.amplifier,
                        scale,
                    );
                } else {
                    let duration = effect_instance.map_duration(|d| {
                        (scale * f64::from(d) * f64::from(duration_scale) + 0.5) as i32
                    });
                    let new_effect = MobEffectInstance::with_flags(
                        effect_instance.effect,
                        duration,
                        effect_instance.amplifier,
                        effect_instance.ambient,
                        effect_instance.visible,
                    );
                    if !new_effect.ends_within(20) {
                        entity.add_effect(level, new_effect, Some(effect_source));
                    }
                }
            }
        });
        if let Some((owner, entity)) = lent_owner {
            level.put_back_entity(owner, entity);
        }
    }
}

impl Projectile for ThrownSplashPotion {
    impl_throwable_projectile!();

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        abstract_thrown_potion::on_hit_block(self, level, hit);
    }

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        abstract_thrown_potion::on_hit(self, level, hit, on_hit_as_potion);
    }
}

impl Entity for ThrownSplashPotion {
    impl_throwable_entity!(0.05, away);

    fn thrown_potion_hurts_water_sensitive(&self) -> Option<bool> {
        Some(abstract_thrown_potion::hurts_water_sensitive(self))
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        throwable::read_additional_save_data(self, input, Item::SPLASH_POTION);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SPLASH_POTION_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SPLASH_POTION,
    create: |ty, id, _| Box::new(ThrownSplashPotion::new(ty, id)),
};
