//! `EnchantmentHelper`: every pipeline that reads enchantments off stacks and entities and runs their effects.
//!
//! Conventions (see `world::item::enchantment::actors`):
//! * the running entity is taken out of the level, so entities are named by [`EntityId`] and the ones that are not in the
//!   level's storage are lent through [`Actors`];
//! * enchantments are named by their registry key ([`Identifier`]); definitions come from `level.registries`;
//! * `MutableFloat` accumulators are plain `f32` locals, `intValue()` is an `as i32` cast (Java narrowing).

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_protocol::components::{ENCHANTABLE, ENCHANTMENTS, EnchantmentHolder, EquipmentSlotGroup, HolderSet, ItemEnchantments, NoDirect, STORED_ENCHANTMENTS};

use crate::registry::RegistryAccess;
use crate::registry::keys::ENCHANTMENT as ENCHANTMENT_REGISTRY;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment::conditional_effect::EnchantmentTarget;
use crate::world::item::enchantment::enchanted_item_in_use::{EnchantedItemInUse, OnBreak};
use crate::world::item::enchantment::enchantment::Enchantment;
use crate::world::item::enchantment::enchantment_effect_components::{ChargingSounds, EffectComponent, EnchantmentEffects};
use crate::world::item::enchantment::enchantment_instance::EnchantmentInstance;
use crate::world::item::enchantment::evaluate::{ContextKind, requirements_match};
use crate::world::item::enchantment::item_enchantments::{ItemEnchantmentsMutable, component_key, holder};
use mc_protocol::packets::play::AttributeModifier;

/// The definition of `enchantment` (`holder.value()`).
pub fn definition<'r>(registries: &'r RegistryAccess, enchantment: &Identifier) -> Option<&'r Enchantment> {
    registries.get::<Enchantment>(ENCHANTMENT_REGISTRY, enchantment)
}

/// The registry key of an `ItemEnchantments` entry (they always are references).
fn key_of(holder: &EnchantmentHolder) -> Option<&Identifier> {
    holder.name()
}

/// The `(enchantment, level)` entries of `stack`'s `enchantments` component.
fn enchantments_of(stack: &ItemStack) -> Vec<(Identifier, i32)> {
    stack
        .get(&ENCHANTMENTS)
        .map(|e| e.entries().iter().filter_map(|(h, l)| key_of(h).map(|k| (k.clone(), *l))).collect())
        .unwrap_or_default()
}

// ---- stack accessors ----------------------------------------------------------------------------------------------------------

/// `getItemEnchantmentLevel(enchantment, piece)`.
pub fn get_item_enchantment_level(enchantment: &Identifier, piece: &ItemStack) -> i32 {
    piece.get(&ENCHANTMENTS).map_or(0, |e| e.get_level(&holder(enchantment)))
}

/// `updateEnchantments(itemStack, consumer)`: `EMPTY` when the stack cannot store enchantments.
pub fn update_enchantments(stack: &mut ItemStack, consumer: impl FnOnce(&mut ItemEnchantmentsMutable)) -> ItemEnchantments {
    let key = component_key(stack);
    let Some(old) = stack.get(&key).cloned() else { return ItemEnchantments::default() };
    let mut mutable = ItemEnchantmentsMutable::new(&old);
    consumer(&mut mutable);
    let new = mutable.to_immutable();
    stack.set(key, new.clone());
    new
}

/// `canStoreEnchantments(itemStack)`.
pub fn can_store_enchantments(stack: &ItemStack) -> bool {
    stack.has(if stack.is(mc_data::Item::ENCHANTED_BOOK) { mc_data::DataComponentType::STORED_ENCHANTMENTS } else { mc_data::DataComponentType::ENCHANTMENTS })
}

/// `setEnchantments(itemStack, enchantments)`.
pub fn set_enchantments(stack: &mut ItemStack, enchantments: ItemEnchantments) {
    stack.set(component_key(stack), enchantments);
}

/// `getEnchantmentsForCrafting(itemStack)`.
pub fn get_enchantments_for_crafting(stack: &ItemStack) -> ItemEnchantments {
    stack.get(&component_key(stack)).cloned().unwrap_or_default()
}

/// `hasAnyEnchantments(itemStack)`.
pub fn has_any_enchantments(stack: &ItemStack) -> bool {
    stack.get(&ENCHANTMENTS).is_some_and(|e| !e.is_empty()) || stack.get(&STORED_ENCHANTMENTS).is_some_and(|e| !e.is_empty())
}

/// `createBook(enchant)`.
pub fn create_book(enchant: &EnchantmentInstance) -> ItemStack {
    let mut stack = ItemStack::new(mc_data::Item::ENCHANTED_BOOK);
    crate::world::item::enchantment::item_enchantments::enchant(&mut stack, &enchant.enchantment, enchant.level);
    stack
}

/// `hasTag(item, tag)`: any enchantment of `enchantments` is in `tag`.
pub fn has_tag(registries: &RegistryAccess, item: &ItemStack, tag: &Identifier) -> bool {
    enchantments_of(item).iter().any(|(e, _)| registries.is_in_tag(ENCHANTMENT_REGISTRY, e, tag))
}

/// `has(item, effectType)`: any enchantment on the stack has the effect component.
pub fn has(registries: &RegistryAccess, item: &ItemStack, effect: EffectComponent) -> bool {
    enchantments_of(item).iter().any(|(e, _)| definition(registries, e).is_some_and(|d| d.effects.has(effect)))
}

/// `getHighestLevel(item, effectType)`: the effect value of the highest-level enchantment that has the component.
pub fn get_highest_level<'r, T>(registries: &'r RegistryAccess, item: &ItemStack, component: impl Fn(&'r EnchantmentEffects) -> Option<&'r T>) -> Option<(&'r T, i32)> {
    let mut found: Option<(&'r T, i32)> = None;
    for (e, level) in enchantments_of(item) {
        if found.as_ref().is_none_or(|(_, l)| *l < level) {
            if let Some(effect) = definition(registries, &e).and_then(|d| component(&d.effects)) {
                found = Some((effect, level));
            }
        }
    }
    found
}

/// `pickHighestLevel(itemStack, componentType)` for list components: `list.get(min(level, size) - 1)`.
pub fn pick_highest_level<'r, T>(registries: &'r RegistryAccess, item: &ItemStack, component: impl Fn(&'r EnchantmentEffects) -> Option<&'r Vec<T>>) -> Option<&'r T> {
    let (list, level) = get_highest_level(registries, item, component)?;
    let at = (level.min(list.len() as i32) - 1).max(0) as usize;
    list.get(at)
}

/// `getCrossbowChargingSounds`' source: `pickHighestLevel(stack, CROSSBOW_CHARGING_SOUNDS)`.
pub fn pick_crossbow_charging_sounds<'r>(registries: &'r RegistryAccess, crossbow: &ItemStack) -> Option<&'r ChargingSounds> {
    pick_highest_level(registries, crossbow, |e| e.crossbow_charging_sounds.as_ref())
}

// ---- iteration -----------------------------------------------------------------------------------------------------------------

/// `runIterationOnItem(piece, method)` without a slot: the definitions of the stack's enchantments with their levels.
fn run_iteration_on_item(registries: &RegistryAccess, piece: &ItemStack, mut method: impl FnMut(&Enchantment, i32)) {
    for (key, level) in enchantments_of(piece) {
        if let Some(e) = definition(registries, &key) {
            method(e, level);
        }
    }
}

type SlotVisitor<'v> = dyn FnMut(&mut ServerLevel, &mut Actors<'_>, &Identifier, &Enchantment, i32, &mut EnchantedItemInUse) + 'v;

/// `runIterationOnItem(piece, slot, owner, method)`: runs `method` for each enchantment of `piece` that applies to `slot`, with
/// one shared [`EnchantedItemInUse`]; durability changes made by effects are written back to the owner's slot.
fn run_iteration_on_item_in_slot(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    registries: &RegistryAccess,
    piece: &ItemStack,
    slot: EquipmentSlot,
    owner: EntityId,
    method: &mut SlotVisitor<'_>,
) {
    if piece.is_empty() {
        return;
    }
    let enchantments = enchantments_of(piece);
    if enchantments.is_empty() {
        return;
    }
    let mut item = EnchantedItemInUse::new(piece.clone(), Some(slot), owner);
    for (key, enchantment_level) in enchantments {
        if let Some(e) = definition(registries, &key) {
            if e.matching_slot(slot) {
                method(level, actors, &key, e, enchantment_level, &mut item);
            }
        }
    }
    write_back(level, actors, &item);
}

/// Stores the working stack of `item` back into its slot when an effect changed it.
fn write_back(level: &mut ServerLevel, actors: &mut Actors<'_>, item: &EnchantedItemInUse) {
    if let (true, Some(slot), Some(owner)) = (item.modified, item.in_slot, item.owner) {
        let stack = item.item_stack.clone();
        actors.with(level, owner, |_, e| {
            if let Some(live) = e.item_by_slot_mut(slot) {
                *live = stack;
            }
        });
    }
}

/// `runIterationOnEquipment(owner, method)`.
fn run_iteration_on_equipment(level: &mut ServerLevel, actors: &mut Actors<'_>, registries: &RegistryAccess, owner: EntityId, method: &mut SlotVisitor<'_>) {
    for slot in EquipmentSlot::VALUES {
        let Some(piece) = actors.get(level, owner).map(|e| e.get_item_by_slot(slot)) else { return };
        run_iteration_on_item_in_slot(level, actors, registries, &piece, slot, owner, method);
    }
}

fn is_living(level: &ServerLevel, actors: &Actors<'_>, id: EntityId) -> bool {
    actors.get(level, id).is_some_and(|e| e.living().is_some())
}

// ---- value pipelines on items ---------------------------------------------------------------------------------------------------

/// `processDurabilityChange(level, itemStack, amount)`.
pub fn process_durability_change(level: &mut ServerLevel, stack: &ItemStack, amount: i32) -> i32 {
    let registries = level.registries.clone();
    let mut modified = amount as f32;
    run_iteration_on_item(&registries, stack, |e, lvl| e.modify_durability_change(level, lvl, stack, &mut modified));
    modified as i32
}

/// `processAmmoUse(level, weapon, ammo, amount)`.
pub fn process_ammo_use(level: &mut ServerLevel, weapon: &ItemStack, ammo: &ItemStack, amount: i32) -> i32 {
    let registries = level.registries.clone();
    let mut modified = amount as f32;
    run_iteration_on_item(&registries, weapon, |e, lvl| e.modify_ammo_count(level, lvl, ammo, &mut modified));
    modified as i32
}

/// `processBlockExperience(level, itemStack, amount)`.
pub fn process_block_experience(level: &mut ServerLevel, stack: &ItemStack, amount: i32) -> i32 {
    let registries = level.registries.clone();
    let mut modified = amount as f32;
    run_iteration_on_item(&registries, stack, |e, lvl| e.modify_block_experience(level, lvl, stack, &mut modified));
    modified as i32
}

/// `getPiercingCount(level, weapon, ammo)`.
pub fn get_piercing_count(level: &mut ServerLevel, weapon: &ItemStack, ammo: &ItemStack) -> i32 {
    let registries = level.registries.clone();
    let mut modified = 0.0f32;
    run_iteration_on_item(&registries, weapon, |e, lvl| e.modify_piercing_count(level, lvl, ammo, &mut modified));
    (modified as i32).max(0)
}

/// `modifyDurabilityToRepairFromXp(level, item, durability)`.
pub fn modify_durability_to_repair_from_xp(level: &mut ServerLevel, item: &ItemStack, durability: i32) -> i32 {
    let registries = level.registries.clone();
    let mut modified = durability as f32;
    run_iteration_on_item(&registries, item, |e, lvl| e.modify_durability_to_repair_from_xp(level, lvl, item, &mut modified));
    (modified as i32).max(0)
}

// ---- value pipelines on entities --------------------------------------------------------------------------------------------------

/// `processMobExperience(level, killer, killed, amount)`.
pub fn process_mob_experience(level: &mut ServerLevel, actors: &mut Actors<'_>, killer: Option<EntityId>, killed: EntityId, amount: i32) -> i32 {
    let Some(killer) = killer.filter(|k| is_living(level, actors, *k)) else { return amount };
    let registries = level.registries.clone();
    let mut modified = amount as f32;
    run_iteration_on_equipment(level, actors, &registries, killer, &mut |level, actors, _, e, lvl, _item| e.modify_mob_experience(level, actors, lvl, killed, &mut modified));
    modified as i32
}

/// `isImmuneToDamage(level, victim, source)`.
pub fn is_immune_to_damage(level: &mut ServerLevel, actors: &mut Actors<'_>, victim: EntityId, source: &DamageSource) -> bool {
    let registries = level.registries.clone();
    let mut result = false;
    run_iteration_on_equipment(level, actors, &registries, victim, &mut |level, actors, _, e, lvl, _item| {
        result = result || e.is_immune_to_damage(level, actors, lvl, victim, source);
    });
    result
}

/// `getDamageProtection(level, victim, source)`.
pub fn get_damage_protection(level: &mut ServerLevel, actors: &mut Actors<'_>, victim: EntityId, source: &DamageSource) -> f32 {
    let registries = level.registries.clone();
    let mut result = 0.0f32;
    run_iteration_on_equipment(level, actors, &registries, victim, &mut |level, actors, _, e, lvl, _item| {
        e.modify_damage_protection(level, actors, lvl, victim, source, &mut result)
    });
    result
}

/// `modifyDamage(level, itemStack, victim, damageSource, damage)`.
pub fn modify_damage(level: &mut ServerLevel, actors: &mut Actors<'_>, item: &ItemStack, victim: EntityId, source: &DamageSource, damage: f32) -> f32 {
    let registries = level.registries.clone();
    let mut result = damage;
    run_iteration_on_item(&registries, item, |e, lvl| e.modify_damage(level, actors, lvl, victim, source, &mut result));
    result
}

/// `modifyFallBasedDamage(level, itemStack, victim, damageSource, damage)`.
pub fn modify_fall_based_damage(level: &mut ServerLevel, actors: &mut Actors<'_>, item: &ItemStack, victim: EntityId, source: &DamageSource, damage: f32) -> f32 {
    let registries = level.registries.clone();
    let mut result = damage;
    run_iteration_on_item(&registries, item, |e, lvl| e.modify_fall_based_damage(level, actors, lvl, victim, source, &mut result));
    result
}

/// `modifyArmorEffectiveness(level, itemStack, victim, damageSource, armorFraction)`.
pub fn modify_armor_effectiveness(level: &mut ServerLevel, actors: &mut Actors<'_>, item: &ItemStack, victim: EntityId, source: &DamageSource, armor_fraction: f32) -> f32 {
    let registries = level.registries.clone();
    let mut result = armor_fraction;
    run_iteration_on_item(&registries, item, |e, lvl| e.modify_armor_effectiveness(level, actors, lvl, victim, source, &mut result));
    result
}

/// `modifyKnockback(level, itemStack, victim, damageSource, knockback)`.
pub fn modify_knockback(level: &mut ServerLevel, actors: &mut Actors<'_>, item: &ItemStack, victim: EntityId, source: &DamageSource, knockback: f32) -> f32 {
    let registries = level.registries.clone();
    let mut result = knockback;
    run_iteration_on_item(&registries, item, |e, lvl| e.modify_knockback(level, actors, lvl, victim, source, &mut result));
    result
}

/// `processProjectileCount(level, weapon, shooter, count)`.
pub fn process_projectile_count(level: &mut ServerLevel, actors: &mut Actors<'_>, weapon: &ItemStack, shooter: EntityId, count: i32) -> i32 {
    let registries = level.registries.clone();
    let mut modified = count as f32;
    run_iteration_on_item(&registries, weapon, |e, lvl| e.modify_projectile_count(level, actors, lvl, shooter, &mut modified));
    (modified as i32).max(0)
}

/// `processProjectileSpread(level, weapon, shooter, angle)`.
pub fn process_projectile_spread(level: &mut ServerLevel, actors: &mut Actors<'_>, weapon: &ItemStack, shooter: EntityId, angle: f32) -> f32 {
    let registries = level.registries.clone();
    let mut modified = angle;
    run_iteration_on_item(&registries, weapon, |e, lvl| e.modify_projectile_spread(level, actors, lvl, shooter, &mut modified));
    modified.max(0.0)
}

/// `getFishingLuckBonus(level, rod, fisher)`.
pub fn get_fishing_luck_bonus(level: &mut ServerLevel, actors: &mut Actors<'_>, rod: &ItemStack, fisher: EntityId) -> i32 {
    let registries = level.registries.clone();
    let mut modified = 0.0f32;
    run_iteration_on_item(&registries, rod, |e, lvl| e.modify_fishing_luck_bonus(level, actors, lvl, fisher, &mut modified));
    (modified as i32).max(0)
}

/// `getFishingTimeReduction(level, rod, fisher)`.
pub fn get_fishing_time_reduction(level: &mut ServerLevel, actors: &mut Actors<'_>, rod: &ItemStack, fisher: EntityId) -> f32 {
    let registries = level.registries.clone();
    let mut modified = 0.0f32;
    run_iteration_on_item(&registries, rod, |e, lvl| e.modify_fishing_time_reduction(level, actors, lvl, fisher, &mut modified));
    modified.max(0.0)
}

/// `getTridentReturnToOwnerAcceleration(level, weapon, trident)`.
pub fn get_trident_return_to_owner_acceleration(level: &mut ServerLevel, actors: &mut Actors<'_>, weapon: &ItemStack, trident: EntityId) -> i32 {
    let registries = level.registries.clone();
    let mut modified = 0.0f32;
    run_iteration_on_item(&registries, weapon, |e, lvl| e.modify_trident_return_to_owner_acceleration(level, actors, lvl, trident, &mut modified));
    (modified as i32).max(0)
}

/// `modifyCrossbowChargingTime(crossbow, holder, time)`: draws from the holder's random.
pub fn modify_crossbow_charging_time(registries: &RegistryAccess, crossbow: &ItemStack, holder: &mut dyn Entity, time: f32) -> f32 {
    let mut modified = time;
    run_iteration_on_item(registries, crossbow, |e, lvl| e.modify_crossbow_charge_time(&mut holder.base_mut().random, lvl, &mut modified));
    modified.max(0.0)
}

/// `getTridentSpinAttackStrength(trident, holder)`: draws from the holder's random.
pub fn get_trident_spin_attack_strength(registries: &RegistryAccess, trident: &ItemStack, holder: &mut dyn Entity) -> f32 {
    let mut strength = 0.0f32;
    run_iteration_on_item(registries, trident, |e, lvl| e.modify_trident_spin_attack_strength(&mut holder.base_mut().random, lvl, &mut strength));
    strength
}

/// `getEnchantmentLevel(enchantment, entity)`: the best level over the slots the enchantment applies to.
pub fn get_enchantment_level(registries: &RegistryAccess, enchantment: &Identifier, entity: &dyn Entity) -> i32 {
    let Some(e) = definition(registries, enchantment) else { return 0 };
    let mut best = 0;
    for slot in EquipmentSlot::VALUES {
        if e.matching_slot(slot) {
            let piece = entity.get_item_by_slot(slot);
            if !piece.is_empty() {
                best = best.max(get_item_enchantment_level(enchantment, &piece));
            }
        }
    }
    best
}

/// `processEquipmentDropChance(level, entity, killingBlow, chance)`.
pub fn process_equipment_drop_chance(level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId, killing_blow: &DamageSource, chance: f32) -> f32 {
    let registries = level.registries.clone();
    let cell = std::cell::Cell::new(chance);
    let run = |level: &mut ServerLevel, actors: &mut Actors<'_>, e: &Enchantment, lvl: i32, enchanted: EnchantmentTarget| {
        if let Some(effects) = &e.effects.equipment_drops {
            for filtered in effects {
                if filtered.enchanted == enchanted
                    && filtered.affected == EnchantmentTarget::Victim
                    && requirements_match(level, actors, &filtered.requirements, lvl, &ContextKind::Damage { victim: entity, source: killing_blow })
                {
                    let value = cell.get();
                    cell.set(actors.with_random(level, entity, |r| filtered.effect.process(lvl, r, value)));
                }
            }
        }
    };
    run_iteration_on_equipment(level, actors, &registries, entity, &mut |level, actors, _, e, lvl, _| run(level, actors, e, lvl, EnchantmentTarget::Victim));
    if let Some(attacker) = killing_blow.get_entity().filter(|a| is_living(level, actors, *a)) {
        run_iteration_on_equipment(level, actors, &registries, attacker, &mut |level, actors, _, e, lvl, _| run(level, actors, e, lvl, EnchantmentTarget::Attacker));
    }
    cell.get()
}

// ---- attribute modifiers ----------------------------------------------------------------------------------------------------------

/// `forEachModifier(itemStack, slot, consumer)` for an equipment slot.
pub fn for_each_modifier(registries: &RegistryAccess, item: &ItemStack, slot: EquipmentSlot, mut consumer: impl FnMut(mc_data::Attribute, AttributeModifier)) {
    run_iteration_on_item(registries, item, |e, lvl| {
        if let Some(attributes) = &e.effects.attributes {
            for effect in attributes {
                if e.matching_slot(slot) {
                    consumer(effect.attribute, effect.get_modifier(lvl, slot.get_name()));
                }
            }
        }
    });
}

/// `forEachModifier(itemStack, slotGroup, consumer)` for an equipment slot group (item tooltips).
pub fn for_each_modifier_in_group(registries: &RegistryAccess, item: &ItemStack, group: EquipmentSlotGroup, mut consumer: impl FnMut(mc_data::Attribute, AttributeModifier)) {
    run_iteration_on_item(registries, item, |e, lvl| {
        if let Some(attributes) = &e.effects.attributes {
            for effect in attributes {
                if e.definition.slots.contains(&group) {
                    consumer(effect.attribute, effect.get_modifier(lvl, group.name()));
                }
            }
        }
    });
}

// ---- entity effects ----------------------------------------------------------------------------------------------------------------

/// `doPostAttackEffects(level, victim, damageSource)`.
pub fn do_post_attack_effects(level: &mut ServerLevel, actors: &mut Actors<'_>, victim: EntityId, damage_source: &DamageSource) {
    let weapon = damage_source
        .get_entity()
        .filter(|a| is_living(level, actors, *a))
        .and_then(|a| actors.get(level, a).and_then(|e| e.get_weapon_item()));
    do_post_attack_effects_with_item_source(level, actors, victim, damage_source, weapon.as_ref());
}

/// `doPostPiercingAttackEffects(level, user)`.
pub fn do_post_piercing_attack_effects(level: &mut ServerLevel, actors: &mut Actors<'_>, user: EntityId) {
    let registries = level.registries.clone();
    let Some(weapon) = actors.get(level, user).and_then(|e| e.get_weapon_item()) else { return };
    run_iteration_on_item_in_slot(level, actors, &registries, &weapon, EquipmentSlot::MainHand, user, &mut |level, actors, _, e, lvl, item| {
        e.do_post_piercing_attack(level, actors, lvl, item, user)
    });
}

/// `doPostAttackEffectsWithItemSource(level, victim, damageSource, source)`.
pub fn do_post_attack_effects_with_item_source(level: &mut ServerLevel, actors: &mut Actors<'_>, victim: EntityId, damage_source: &DamageSource, source: Option<&ItemStack>) {
    do_post_attack_effects_with_item_source_on_break(level, actors, victim, damage_source, source, None);
}

/// `doPostAttackEffectsWithItemSourceOnBreak(level, victim, damageSource, source, attackerlessOnBreak)`.
pub fn do_post_attack_effects_with_item_source_on_break(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    victim: EntityId,
    damage_source: &DamageSource,
    source: Option<&ItemStack>,
    attackerless_on_break: Option<Box<dyn FnMut(&mut ServerLevel, &ItemStack) + Send>>,
) {
    let registries = level.registries.clone();
    if is_living(level, actors, victim) {
        run_iteration_on_equipment(level, actors, &registries, victim, &mut |level, actors, _, e, lvl, item| {
            e.do_post_attack(level, actors, lvl, item, EnchantmentTarget::Victim, victim, damage_source)
        });
    }
    if let Some(source) = source {
        let attacker = damage_source.get_entity().filter(|a| is_living(level, actors, *a));
        if let Some(attacker) = attacker {
            run_iteration_on_item_in_slot(level, actors, &registries, source, EquipmentSlot::MainHand, attacker, &mut |level, actors, _, e, lvl, item| {
                e.do_post_attack(level, actors, lvl, item, EnchantmentTarget::Attacker, victim, damage_source)
            });
        } else if let Some(on_break) = attackerless_on_break {
            let mut item = EnchantedItemInUse::with_on_break(source.clone(), None, None, OnBreak::Callback(on_break));
            for (key, lvl) in enchantments_of(source) {
                if let Some(e) = definition(&registries, &key) {
                    e.do_post_attack(level, actors, lvl, &mut item, EnchantmentTarget::Attacker, victim, damage_source);
                }
            }
        }
    }
}

/// `tickEffects(level, entity)`.
pub fn tick_effects(level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId) {
    let registries = level.registries.clone();
    run_iteration_on_equipment(level, actors, &registries, entity, &mut |level, actors, _, e, lvl, item| e.tick(level, actors, lvl, item, entity));
}

/// `onProjectileSpawned(level, weapon, projectile, onBreak)`: `owner` is the projectile's owner when it is a living entity.
pub fn on_projectile_spawned(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    weapon: &mut ItemStack,
    projectile: EntityId,
    owner: Option<EntityId>,
    on_break: Box<dyn FnMut(&mut ServerLevel, &ItemStack) + Send>,
) {
    let registries = level.registries.clone();
    let mut item = EnchantedItemInUse::with_on_break(weapon.clone(), None, owner, OnBreak::Callback(on_break));
    for (key, lvl) in enchantments_of(weapon) {
        if let Some(e) = definition(&registries, &key) {
            e.on_projectile_spawned(level, actors, lvl, &mut item, projectile);
        }
    }
    if item.modified {
        *weapon = item.item_stack;
    }
}

/// `onHitBlock(level, weapon, owner, entity, slot, hitLocation, hitBlock, onBreak)`.
pub fn on_hit_block(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    weapon: &mut ItemStack,
    owner: Option<EntityId>,
    entity: EntityId,
    slot: Option<EquipmentSlot>,
    hit_location: Vec3,
    hit_block: BlockState,
    on_break: Box<dyn FnMut(&mut ServerLevel, &ItemStack) + Send>,
) {
    let registries = level.registries.clone();
    let mut item = EnchantedItemInUse::with_on_break(weapon.clone(), slot, owner, OnBreak::Callback(on_break));
    for (key, lvl) in enchantments_of(weapon) {
        if let Some(e) = definition(&registries, &key) {
            e.on_hit_block(level, actors, lvl, &mut item, entity, hit_location, hit_block);
        }
    }
    if item.modified {
        *weapon = item.item_stack;
    }
}

// ---- location based effects ---------------------------------------------------------------------------------------------------------

fn slot_index(slot: EquipmentSlot) -> usize {
    EquipmentSlot::VALUES.iter().position(|s| *s == slot).expect("slot in VALUES")
}

/// The active effects of `enchantment` in `slot` of the entity (`activeLocationDependentEnchantments(slot).get(enchantment)`).
fn take_active(level: &ServerLevel, actors: &Actors<'_>, entity: EntityId, slot: EquipmentSlot, enchantment: &Identifier) -> Vec<u16> {
    actors
        .get(level, entity)
        .and_then(|e| e.living())
        .and_then(|l| l.active_location_dependent_enchantments[slot_index(slot)].iter().find(|(k, _)| k == enchantment).map(|(_, v)| v.clone()))
        .unwrap_or_default()
}

fn put_active(level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId, slot: EquipmentSlot, enchantment: &Identifier, active: Vec<u16>) {
    actors.with(level, entity, |_, e| {
        if let Some(l) = e.living_mut() {
            let list = &mut l.active_location_dependent_enchantments[slot_index(slot)];
            list.retain(|(k, _)| k != enchantment);
            if !active.is_empty() {
                list.push((enchantment.clone(), active));
            }
        }
    });
}

/// `Enchantment.runLocationChangedEffects(level, enchantmentLevel, item, entity)` for one enchantment of the stack in `slot`.
fn run_location_changed_for(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    key: &Identifier,
    e: &Enchantment,
    enchantment_level: i32,
    item: &mut EnchantedItemInUse,
    entity: EntityId,
) {
    let Some(slot) = item.in_slot else { return };
    let mut active = take_active(level, actors, entity, slot, key);
    if !e.matching_slot(slot) {
        // deactivate whatever was active (`remove(this)` + `onDeactivated`)
        if !active.is_empty() {
            e.stop_location_based_effects(level, actors, enchantment_level, item, entity, &active);
            put_active(level, actors, entity, slot, key, Vec::new());
        }
    } else {
        e.run_location_changed_effects(level, actors, enchantment_level, item, entity, &mut active);
        put_active(level, actors, entity, slot, key, active);
    }
}

/// `runLocationChangedEffects(level, entity)`.
pub fn run_location_changed_effects(level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId) {
    let registries = level.registries.clone();
    run_iteration_on_equipment(level, actors, &registries, entity, &mut |level, actors, key, e, lvl, item| run_location_changed_for(level, actors, key, e, lvl, item, entity));
}

/// `runLocationChangedEffects(level, stack, entity, slot)`.
pub fn run_location_changed_effects_for_slot(level: &mut ServerLevel, actors: &mut Actors<'_>, stack: &ItemStack, entity: EntityId, slot: EquipmentSlot) {
    let registries = level.registries.clone();
    // vanilla iterates the *given* stack (it is the stack of `slot` before/after a change), not necessarily the slot's current one
    if stack.is_empty() {
        return;
    }
    let mut item = EnchantedItemInUse::new(stack.clone(), Some(slot), entity);
    for (key, lvl) in enchantments_of(stack) {
        if let Some(e) = definition(&registries, &key) {
            // `matchingSlot` is checked inside `Enchantment.runLocationChangedEffects` for the location variant
            run_location_changed_for(level, actors, &key, e, lvl, &mut item, entity);
        }
    }
    write_back(level, actors, &item);
}

/// `stopLocationBasedEffects(entity)`.
pub fn stop_location_based_effects(level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId) {
    let registries = level.registries.clone();
    run_iteration_on_equipment(level, actors, &registries, entity, &mut |level, actors, key, e, lvl, item| stop_for(level, actors, key, e, lvl, item, entity));
}

/// `stopLocationBasedEffects(stack, entity, slot)`.
pub fn stop_location_based_effects_for_slot(level: &mut ServerLevel, actors: &mut Actors<'_>, stack: &ItemStack, entity: EntityId, slot: EquipmentSlot) {
    let registries = level.registries.clone();
    if stack.is_empty() {
        return;
    }
    let mut item = EnchantedItemInUse::new(stack.clone(), Some(slot), entity);
    for (key, lvl) in enchantments_of(stack) {
        if let Some(e) = definition(&registries, &key) {
            if e.matching_slot(slot) {
                stop_for(level, actors, &key, e, lvl, &mut item, entity);
            }
        }
    }
}

fn stop_for(level: &mut ServerLevel, actors: &mut Actors<'_>, key: &Identifier, e: &Enchantment, enchantment_level: i32, item: &mut EnchantedItemInUse, entity: EntityId) {
    let Some(slot) = item.in_slot else { return };
    let active = take_active(level, actors, entity, slot, key);
    if !active.is_empty() {
        put_active(level, actors, entity, slot, key, Vec::new());
        e.stop_location_based_effects(level, actors, enchantment_level, item, entity, &active);
    }
}

// ---- random item picks -----------------------------------------------------------------------------------------------------------------

/// `getRandomItemWith(componentType, source, predicate)`: draws `Util.getRandomSafe` from the source's random.
pub fn get_random_item_with(
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    component: EffectComponent,
    source: EntityId,
    predicate: impl Fn(&ItemStack) -> bool,
) -> Option<EnchantedItemInUse> {
    let registries = level.registries.clone();
    let mut items: Vec<EnchantedItemInUse> = Vec::new();
    for slot in EquipmentSlot::VALUES {
        let Some(item) = actors.get(level, source).map(|e| e.get_item_by_slot(slot)) else { return None };
        if predicate(&item) {
            for (key, _) in enchantments_of(&item) {
                if definition(&registries, &key).is_some_and(|e| e.effects.has(component) && e.matching_slot(slot)) {
                    items.push(EnchantedItemInUse::new(item.clone(), Some(slot), source));
                }
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    let at = actors.with_random(level, source, |r| r.next_int_bounded(items.len() as i32)) as usize;
    Some(items.swap_remove(at))
}

// ---- enchanting table / anvil --------------------------------------------------------------------------------------------------------

/// `getEnchantmentCost(random, slot, bookcases, itemStack)`.
pub fn get_enchantment_cost(random: &mut dyn RandomSource, slot: i32, mut bookcases: i32, item: &ItemStack) -> i32 {
    if item.get(&ENCHANTABLE).is_none() {
        return 0;
    }
    if bookcases > 15 {
        bookcases = 15;
    }
    let selected = random.next_int_bounded(8) + 1 + (bookcases >> 1) + random.next_int_bounded(bookcases + 1);
    if slot == 0 {
        (selected / 3).max(1)
    } else if slot == 1 {
        selected * 2 / 3 + 1
    } else {
        selected.max(bookcases * 2)
    }
}

/// `enchantItem(random, itemStack, enchantmentCost, registryAccess, set)`: `set` limits the candidates (`None`: every enchantment
/// of the registry, in registry order).
pub fn enchant_item(random: &mut dyn RandomSource, item: ItemStack, enchantment_cost: i32, registries: &RegistryAccess, set: Option<&HolderSet<NoDirect>>) -> ItemStack {
    let candidates = candidates_of(registries, set);
    enchant_item_from(random, item, enchantment_cost, registries, &candidates)
}

/// The enchantments of a holder set (`HolderSet.stream()`) or of the whole registry.
pub fn candidates_of(registries: &RegistryAccess, set: Option<&HolderSet<NoDirect>>) -> Vec<Identifier> {
    let Some(registry) = registries.registry(ENCHANTMENT_REGISTRY) else { return Vec::new() };
    match set {
        None => registry.names(),
        Some(HolderSet::Named(tag)) => registry.tag(tag).map(|ids| ids.iter().filter_map(|i| registry.name_of(*i)).collect()).unwrap_or_default(),
        Some(HolderSet::Direct(list)) => list.iter().filter_map(|h| h.name().cloned()).collect(),
    }
}

/// `enchantItem(random, itemStack, enchantmentCost, source)`.
pub fn enchant_item_from(random: &mut dyn RandomSource, mut item: ItemStack, enchantment_cost: i32, registries: &RegistryAccess, source: &[Identifier]) -> ItemStack {
    let enchants = select_enchantment(random, &item, enchantment_cost, registries, source);
    if item.is(mc_data::Item::BOOK) {
        item = ItemStack::new(mc_data::Item::ENCHANTED_BOOK);
    }
    for enchant in &enchants {
        crate::world::item::enchantment::item_enchantments::enchant(&mut item, &enchant.enchantment, enchant.level);
    }
    item
}

/// `selectEnchantment(random, itemStack, enchantmentCost, source)`.
pub fn select_enchantment(random: &mut dyn RandomSource, item: &ItemStack, mut enchantment_cost: i32, registries: &RegistryAccess, source: &[Identifier]) -> Vec<EnchantmentInstance> {
    let mut results: Vec<EnchantmentInstance> = Vec::new();
    let Some(enchantable) = item.get(&ENCHANTABLE) else { return results };
    let value = enchantable.value;
    enchantment_cost = enchantment_cost
        .wrapping_add(1)
        .wrapping_add(random.next_int_bounded(value / 4 + 1))
        .wrapping_add(random.next_int_bounded(value / 4 + 1));
    let random_span = (random.next_float() + random.next_float() - 1.0) * 0.15;
    enchantment_cost = mc_core::math::java::round_f32(enchantment_cost as f32 + enchantment_cost as f32 * random_span).max(1);
    let mut enchantments = get_available_enchantment_results(enchantment_cost, item, registries, source);
    if !enchantments.is_empty() {
        if let Some(picked) = weighted_random_item(random, &enchantments, registries) {
            results.push(picked);
        }
        while random.next_int_bounded(50) <= enchantment_cost {
            if let Some(last) = results.last().cloned() {
                filter_compatible_enchantments(registries, &mut enchantments, &last);
            }
            if enchantments.is_empty() {
                break;
            }
            if let Some(picked) = weighted_random_item(random, &enchantments, registries) {
                results.push(picked);
            }
            enchantment_cost /= 2;
        }
    }
    results
}

/// `WeightedRandom.getRandomItem(random, enchantments, EnchantmentInstance::weight)`.
fn weighted_random_item(random: &mut dyn RandomSource, items: &[EnchantmentInstance], registries: &RegistryAccess) -> Option<EnchantmentInstance> {
    let weight = |i: &EnchantmentInstance| definition(registries, &i.enchantment).map_or(0, |e| e.get_weight());
    let total: i64 = items.iter().map(|i| weight(i) as i64).sum();
    assert!(total <= i32::MAX as i64, "Sum of weights must be <= 2147483647");
    let total = total as i32;
    if total == 0 {
        return None;
    }
    let mut selection = random.next_int_bounded(total);
    for item in items {
        selection -= weight(item);
        if selection < 0 {
            return Some(item.clone());
        }
    }
    None
}

/// `filterCompatibleEnchantments(enchants, target)`.
pub fn filter_compatible_enchantments(registries: &RegistryAccess, enchants: &mut Vec<EnchantmentInstance>, target: &EnchantmentInstance) {
    enchants.retain(|e| Enchantment::are_compatible(registries, &target.enchantment, &e.enchantment));
}

/// `isEnchantmentCompatible(enchants, target)`.
pub fn is_enchantment_compatible(registries: &RegistryAccess, enchants: &[Identifier], target: &Identifier) -> bool {
    enchants.iter().all(|existing| Enchantment::are_compatible(registries, existing, target))
}

/// `getAvailableEnchantmentResults(value, itemStack, source)`.
pub fn get_available_enchantment_results(value: i32, item: &ItemStack, registries: &RegistryAccess, source: &[Identifier]) -> Vec<EnchantmentInstance> {
    let mut results = Vec::new();
    let is_book = item.is(mc_data::Item::BOOK);
    for key in source {
        let Some(e) = definition(registries, key) else { continue };
        if !(e.is_primary_item(registries, item) || is_book) {
            continue;
        }
        let mut level = e.get_max_level();
        while level >= e.get_min_level() {
            if value >= e.get_min_cost(level) && value <= e.get_max_cost(level) {
                results.push(EnchantmentInstance { enchantment: key.clone(), level });
                break;
            }
            level -= 1;
        }
    }
    results
}

/// `enchantItemFromProvider(itemStack, registryAccess, providerKey, difficulty, random)`.
pub fn enchant_item_from_provider(item: &mut ItemStack, registries: &RegistryAccess, provider_key: &Identifier, difficulty: &crate::world::difficulty_instance::DifficultyInstance, random: &mut dyn RandomSource) {
    let Some(provider) = registries.get::<crate::world::item::enchantment::providers::EnchantmentProvider>(crate::registry::keys::ENCHANTMENT_PROVIDER, provider_key) else { return };
    let snapshot = item.clone();
    update_enchantments(item, |enchantments| provider.enchant(registries, &snapshot, enchantments, random, difficulty));
}
