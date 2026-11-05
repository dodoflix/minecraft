//! The `Enchantment` methods that evaluate its effects: the `modify*` value pipelines, the entity/location effect runners and the
//! loot contexts (`damageContext`, `itemContext`, `locationContext`, `entityContext`, `blockHitContext`) the requirements run in.

use mc_core::phys::Vec3;
use mc_data::block::BlockState;

use super::actors::Actors;
use super::conditional_effect::{ConditionalEffect, EnchantmentTarget, TargetedConditionalEffect};
use super::effects::apply;
use super::effects::{EnchantmentEntityEffect, EnchantmentLocationBasedEffect, EnchantmentValueEffect};
use super::enchanted_item_in_use::EnchantedItemInUse;
use super::enchantment::Enchantment;
use super::requirements::{Requirement, requirement_test};
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::EntityId;
use crate::world::item::ItemStack;

/// Which `LootContextParamSet` a requirement is evaluated in, with the parameters of that set.
pub enum ContextKind<'a> {
    /// `damageContext` (`ENCHANTED_DAMAGE`): this = victim, damage source, attacker, direct attacker, origin = victim position.
    Damage { victim: EntityId, source: &'a DamageSource },
    /// `itemContext` (`ENCHANTED_ITEM`): the tool.
    Item { stack: &'a ItemStack },
    /// `locationContext` (`ENCHANTED_LOCATION`): this = entity, origin = its position, `enchantment_active`.
    Location { entity: EntityId, active: bool },
    /// `entityContext` (`ENCHANTED_ENTITY`): this = entity, origin = `position`.
    Entity { entity: EntityId, position: Vec3 },
    /// `blockHitContext` (`HIT_BLOCK`): this = entity, origin = `position`, the hit block state.
    HitBlock { entity: EntityId, position: Vec3, state: BlockState },
}

/// `effect.matches(context)` for `Optional<Holder<LootItemCondition>> requirements`.
pub fn requirements_match(level: &mut ServerLevel, actors: &Actors<'_>, requirements: &Option<Requirement>, enchantment_level: i32, kind: &ContextKind<'_>) -> bool {
    match requirements {
        None => true,
        Some(requirement) => requirement_test(level, actors, requirement, enchantment_level, kind),
    }
}

/// Which random source a value effect draws from.
#[derive(Clone, Copy)]
pub enum RandomFrom {
    /// `serverLevel.getRandom()`.
    Level,
    /// `entity.getRandom()`.
    Entity(EntityId),
}

/// `applyEffects(effects, context, value, action)` for value effects: every matching effect folds into `value`.
pub fn apply_value_effects(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    effects: &[ConditionalEffect<EnchantmentValueEffect>],
    kind: &ContextKind<'_>,
    enchantment_level: i32,
    random: RandomFrom,
    value: &mut f32,
) {
    for effect in effects {
        if requirements_match(level, actors, &effect.requirements, enchantment_level, kind) {
            *value = process_with(level, actors, random, &effect.effect, enchantment_level, *value);
        }
    }
}

fn process_with(level: &mut ServerLevel, actors: &mut Actors<'_>, random: RandomFrom, effect: &EnchantmentValueEffect, enchantment_level: i32, value: f32) -> f32 {
    match random {
        RandomFrom::Entity(id) => actors.with_random(level, id, |r| effect.process(enchantment_level, r, value)),
        RandomFrom::Level => {
            use crate::world::level::LevelAccessor;
            effect.process(enchantment_level, level.random(), value)
        }
    }
}

/// `applyEffects(effects, context, action)` for entity effects.
fn apply_entity_effects(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    effects: &[ConditionalEffect<EnchantmentEntityEffect>],
    kind: &ContextKind<'_>,
    enchantment_level: i32,
    item: &mut EnchantedItemInUse,
    target: EntityId,
    position: Vec3,
) {
    for effect in effects {
        if requirements_match(level, actors, &effect.requirements, enchantment_level, kind) {
            apply::apply(&effect.effect, level, actors, enchantment_level, item, target, position);
        }
    }
}

fn position_of(level: &ServerLevel, actors: &Actors<'_>, id: EntityId) -> Vec3 {
    actors.get(level, id).map_or(Vec3::ZERO, |e| e.position())
}

impl Enchantment {
    /// `isImmuneToDamage(level, enchantmentLevel, victim, source)`.
    pub fn is_immune_to_damage(&self, level: &mut ServerLevel, actors: &Actors<'_>, enchantment_level: i32, victim: EntityId, source: &DamageSource) -> bool {
        let Some(effects) = &self.effects.damage_immunity else { return false };
        let kind = ContextKind::Damage { victim, source };
        effects.iter().any(|e| requirements_match(level, actors, &e.requirements, enchantment_level, &kind))
    }

    /// `modifyDamageProtection(...)`.
    pub fn modify_damage_protection(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, victim: EntityId, source: &DamageSource, protection: &mut f32) {
        if let Some(effects) = &self.effects.damage_protection {
            let kind = ContextKind::Damage { victim, source };
            apply_value_effects(level, actors, effects, &kind, enchantment_level, RandomFrom::Entity(victim), protection);
        }
    }

    fn modify_item_filtered_count(
        &self,
        effects: &Option<Vec<ConditionalEffect<EnchantmentValueEffect>>>,
        level: &mut ServerLevel,
        enchantment_level: i32,
        stack: &ItemStack,
        value: &mut f32,
    ) {
        if let Some(effects) = effects {
            let kind = ContextKind::Item { stack };
            apply_value_effects(level, &mut Actors::none(), effects, &kind, enchantment_level, RandomFrom::Level, value);
        }
    }

    fn modify_entity_filtered_value(
        &self,
        effects: &Option<Vec<ConditionalEffect<EnchantmentValueEffect>>>,
        level: &mut ServerLevel,
        actors: &mut Actors<'_>,
        enchantment_level: i32,
        entity: EntityId,
        value: &mut f32,
    ) {
        if let Some(effects) = effects {
            let position = position_of(level, actors, entity);
            let kind = ContextKind::Entity { entity, position };
            apply_value_effects(level, actors, effects, &kind, enchantment_level, RandomFrom::Entity(entity), value);
        }
    }

    fn modify_damage_filtered_value(
        &self,
        effects: &Option<Vec<ConditionalEffect<EnchantmentValueEffect>>>,
        level: &mut ServerLevel,
        actors: &mut Actors<'_>,
        enchantment_level: i32,
        victim: EntityId,
        source: &DamageSource,
        value: &mut f32,
    ) {
        if let Some(effects) = effects {
            let kind = ContextKind::Damage { victim, source };
            apply_value_effects(level, actors, effects, &kind, enchantment_level, RandomFrom::Entity(victim), value);
        }
    }

    /// `modifyDurabilityChange`.
    pub fn modify_durability_change(&self, level: &mut ServerLevel, enchantment_level: i32, stack: &ItemStack, change: &mut f32) {
        self.modify_item_filtered_count(&self.effects.item_damage, level, enchantment_level, stack, change);
    }

    /// `modifyAmmoCount`.
    pub fn modify_ammo_count(&self, level: &mut ServerLevel, enchantment_level: i32, ammo: &ItemStack, change: &mut f32) {
        self.modify_item_filtered_count(&self.effects.ammo_use, level, enchantment_level, ammo, change);
    }

    /// `modifyPiercingCount`.
    pub fn modify_piercing_count(&self, level: &mut ServerLevel, enchantment_level: i32, ammo: &ItemStack, count: &mut f32) {
        self.modify_item_filtered_count(&self.effects.projectile_piercing, level, enchantment_level, ammo, count);
    }

    /// `modifyBlockExperience`.
    pub fn modify_block_experience(&self, level: &mut ServerLevel, enchantment_level: i32, stack: &ItemStack, count: &mut f32) {
        self.modify_item_filtered_count(&self.effects.block_experience, level, enchantment_level, stack, count);
    }

    /// `modifyDurabilityToRepairFromXp`.
    pub fn modify_durability_to_repair_from_xp(&self, level: &mut ServerLevel, enchantment_level: i32, stack: &ItemStack, change: &mut f32) {
        self.modify_item_filtered_count(&self.effects.repair_with_xp, level, enchantment_level, stack, change);
    }

    /// `modifyMobExperience(.., killer, ..)` (vanilla passes the *killed* entity here).
    pub fn modify_mob_experience(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, killed: EntityId, experience: &mut f32) {
        self.modify_entity_filtered_value(&self.effects.mob_experience, level, actors, enchantment_level, killed, experience);
    }

    /// `modifyTridentReturnToOwnerAcceleration`.
    pub fn modify_trident_return_to_owner_acceleration(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, trident: EntityId, count: &mut f32) {
        self.modify_entity_filtered_value(&self.effects.trident_return_acceleration, level, actors, enchantment_level, trident, count);
    }

    /// `modifyFishingTimeReduction`.
    pub fn modify_fishing_time_reduction(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, fisher: EntityId, time_reduction: &mut f32) {
        self.modify_entity_filtered_value(&self.effects.fishing_time_reduction, level, actors, enchantment_level, fisher, time_reduction);
    }

    /// `modifyFishingLuckBonus`.
    pub fn modify_fishing_luck_bonus(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, fisher: EntityId, luck: &mut f32) {
        self.modify_entity_filtered_value(&self.effects.fishing_luck_bonus, level, actors, enchantment_level, fisher, luck);
    }

    /// `modifyProjectileCount`.
    pub fn modify_projectile_count(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, shooter: EntityId, count: &mut f32) {
        self.modify_entity_filtered_value(&self.effects.projectile_count, level, actors, enchantment_level, shooter, count);
    }

    /// `modifyProjectileSpread`.
    pub fn modify_projectile_spread(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, shooter: EntityId, angle: &mut f32) {
        self.modify_entity_filtered_value(&self.effects.projectile_spread, level, actors, enchantment_level, shooter, angle);
    }

    /// `modifyDamage`.
    pub fn modify_damage(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, victim: EntityId, source: &DamageSource, amount: &mut f32) {
        self.modify_damage_filtered_value(&self.effects.damage, level, actors, enchantment_level, victim, source, amount);
    }

    /// `modifyFallBasedDamage`.
    pub fn modify_fall_based_damage(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, victim: EntityId, source: &DamageSource, amount: &mut f32) {
        self.modify_damage_filtered_value(&self.effects.smash_damage_per_fallen_block, level, actors, enchantment_level, victim, source, amount);
    }

    /// `modifyKnockback`.
    pub fn modify_knockback(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, victim: EntityId, source: &DamageSource, amount: &mut f32) {
        self.modify_damage_filtered_value(&self.effects.knockback, level, actors, enchantment_level, victim, source, amount);
    }

    /// `modifyArmorEffectivness`.
    pub fn modify_armor_effectiveness(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, victim: EntityId, source: &DamageSource, amount: &mut f32) {
        self.modify_damage_filtered_value(&self.effects.armor_effectiveness, level, actors, enchantment_level, victim, source, amount);
    }

    /// `modifyCrossbowChargeTime` (`modifyUnfilteredValue`).
    pub fn modify_crossbow_charge_time(&self, random: &mut dyn mc_core::random::RandomSource, enchantment_level: i32, time: &mut f32) {
        if let Some(effect) = &self.effects.crossbow_charge_time {
            *time = effect.process(enchantment_level, random, *time);
        }
    }

    /// `modifyTridentSpinAttackStrength` (`modifyUnfilteredValue`).
    pub fn modify_trident_spin_attack_strength(&self, random: &mut dyn mc_core::random::RandomSource, enchantment_level: i32, strength: &mut f32) {
        if let Some(effect) = &self.effects.trident_spin_attack_strength {
            *strength = effect.process(enchantment_level, random, *strength);
        }
    }

    /// `doPostAttack(level, enchantmentLevel, item, forTarget, victim, source)`.
    pub fn do_post_attack(
        &self,
        level: &mut ServerLevel,
        actors: &mut Actors<'_>,
        enchantment_level: i32,
        item: &mut EnchantedItemInUse,
        for_target: EnchantmentTarget,
        victim: EntityId,
        source: &DamageSource,
    ) {
        if let Some(effects) = &self.effects.post_attack {
            for effect in effects {
                if for_target == effect.enchanted {
                    do_post_attack(effect, level, actors, enchantment_level, item, victim, source);
                }
            }
        }
    }

    /// `doPostPiercingAttack`.
    pub fn do_post_piercing_attack(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, item: &mut EnchantedItemInUse, user: EntityId) {
        if let Some(effects) = &self.effects.post_piercing_attack {
            let position = position_of(level, actors, user);
            apply_entity_effects(level, actors, effects, &ContextKind::Entity { entity: user, position }, enchantment_level, item, user, position);
        }
    }

    /// `tick`.
    pub fn tick(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, item: &mut EnchantedItemInUse, entity: EntityId) {
        if let Some(effects) = &self.effects.tick {
            let position = position_of(level, actors, entity);
            apply_entity_effects(level, actors, effects, &ContextKind::Entity { entity, position }, enchantment_level, item, entity, position);
        }
    }

    /// `onProjectileSpawned`.
    pub fn on_projectile_spawned(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, weapon: &mut EnchantedItemInUse, projectile: EntityId) {
        if let Some(effects) = &self.effects.projectile_spawned {
            let position = position_of(level, actors, projectile);
            apply_entity_effects(level, actors, effects, &ContextKind::Entity { entity: projectile, position }, enchantment_level, weapon, projectile, position);
        }
    }

    /// `onHitBlock`.
    pub fn on_hit_block(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, weapon: &mut EnchantedItemInUse, projectile: EntityId, position: Vec3, hit_block: BlockState) {
        if let Some(effects) = &self.effects.hit_block {
            let kind = ContextKind::HitBlock { entity: projectile, position, state: hit_block };
            apply_entity_effects(level, actors, effects, &kind, enchantment_level, weapon, projectile, position);
        }
    }

    /// `runLocationChangedEffects(level, enchantmentLevel, item, entity)`; `active` is the entity's active-effect state
    /// for this enchantment in `item.in_slot` (indices into the `location_changed` list), updated in place.
    pub fn run_location_changed_effects(
        &self,
        level: &mut ServerLevel,
        actors: &mut Actors<'_>,
        enchantment_level: i32,
        item: &mut EnchantedItemInUse,
        entity: EntityId,
        active: &mut Vec<u16>,
    ) {
        let Some(effects) = &self.effects.location_changed else { return };
        for (index, filtered) in effects.iter().enumerate() {
            let index = index as u16;
            let was_active = active.contains(&index);
            let position = position_of(level, actors, entity);
            let kind = ContextKind::Location { entity, active: was_active };
            if requirements_match(level, actors, &filtered.requirements, enchantment_level, &kind) {
                if !was_active {
                    active.push(index);
                }
                let position = position_of(level, actors, entity);
                apply::on_changed_block(&filtered.effect, level, actors, enchantment_level, item, entity, position, !was_active);
            } else if let Some(at) = active.iter().position(|i| *i == index) {
                active.remove(at);
                apply::on_deactivated(&filtered.effect, level, actors, item, entity, position, enchantment_level);
            }
        }
    }

    /// `stopLocationBasedEffects(enchantmentLevel, item, entity)` for the effects (by `location_changed` index) that were active.
    pub fn stop_location_based_effects(&self, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, item: &EnchantedItemInUse, entity: EntityId, active: &[u16]) {
        let Some(effects) = &self.effects.location_changed else { return };
        let position = position_of(level, actors, entity);
        for index in active {
            if let Some(filtered) = effects.get(*index as usize) {
                apply::on_deactivated(&filtered.effect, level, actors, item, entity, position, enchantment_level);
            }
        }
    }
}

/// `Enchantment.doPostAttack(effect, ...)` (static).
pub fn do_post_attack(
    effect: &TargetedConditionalEffect<EnchantmentEntityEffect>,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    enchantment_level: i32,
    item: &mut EnchantedItemInUse,
    victim: EntityId,
    source: &DamageSource,
) {
    if requirements_match(level, actors, &effect.requirements, enchantment_level, &ContextKind::Damage { victim, source }) {
        let target = match effect.affected {
            EnchantmentTarget::Attacker => source.get_entity(),
            EnchantmentTarget::DamagingEntity => source.get_direct_entity(),
            EnchantmentTarget::Victim => Some(victim),
        };
        if let Some(target) = target {
            let position = position_of(level, actors, target);
            apply::apply(&effect.effect, level, actors, enchantment_level, item, target, position);
        }
    }
}

/// The location-based effect kinds a `location_changed` entry resolves to (for the apply module).
pub type LocationEffect = EnchantmentLocationBasedEffect;
