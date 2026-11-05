//! `Mob`'s equipment handling: slots, pickup, drop chances, default/enchanted gear by difficulty, drops on death,
//! `EquipmentUser.equip(table)`.

use super::{MobData, MAX_ENCHANTED_ARMOR_CHANCE, MAX_ENCHANTED_WEAPON_CHANCE};
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::equipment::{EquipmentSlot, EquipmentSlotType};
use crate::world::entity::equipment_table::EquipmentTable;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::living_entity::util::{l, lm};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment::enchantment_effect_components::EffectComponent;
use crate::world::item::enchantment_helper;
use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_data::{Attribute, Item};
use mc_protocol::components::{ATTRIBUTE_MODIFIERS, ENCHANTMENTS, EQUIPPABLE};
use mc_protocol::packets::play::ClientboundTakeItemEntityPacket;
use std::sync::LazyLock;

/// `EQUIPMENT_POPULATION_ORDER`.
const EQUIPMENT_POPULATION_ORDER: [EquipmentSlot; 4] = [EquipmentSlot::Head, EquipmentSlot::Chest, EquipmentSlot::Legs, EquipmentSlot::Feet];

/// `VanillaEnchantmentProviders.MOB_SPAWN_EQUIPMENT`.
static MOB_SPAWN_EQUIPMENT: LazyLock<Identifier> = LazyLock::new(|| Identifier::new("minecraft", "mob_spawn_equipment").expect("valid id"));

fn mm(e: &mut dyn Entity) -> &mut MobData {
    e.mob_mut().expect("mob")
}

fn item_is_in_tag(stack: &ItemStack, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `LivingEntity.getEquipmentSlotForItem(stack)`.
pub fn get_equipment_slot_for_item(e: &dyn Entity, stack: &ItemStack) -> EquipmentSlot {
    if let Some(eq) = stack.get(&EQUIPPABLE) {
        let slot = EquipmentSlot::from_protocol(eq.slot);
        if e.can_use_slot(slot) {
            return slot;
        }
    }
    EquipmentSlot::MainHand
}

/// `LivingEntity.isEquippableInSlot(stack, slot)`.
pub fn is_equippable_in_slot(e: &dyn Entity, stack: &ItemStack, slot: EquipmentSlot) -> bool {
    match stack.get(&EQUIPPABLE) {
        None => slot == EquipmentSlot::MainHand && e.can_use_slot(EquipmentSlot::MainHand),
        Some(eq) => {
            let eq_slot = EquipmentSlot::from_protocol(eq.slot);
            slot == eq_slot && e.can_use_slot(eq_slot) && eq.allowed_entities.as_ref().is_none_or(|set| super::ext::entity_set_contains(set, e.get_type()))
        }
    }
}

/// `setItemSlot(slot, stack)`: stores the stack and runs `onEquipItem`.
pub fn set_item_slot(e: &mut dyn Entity, level: &mut ServerLevel, slot: EquipmentSlot, stack: ItemStack) {
    let old = lm(e).equipment.set(slot, stack.clone());
    e.on_equip_item(level, slot, &old, &stack);
}

/// `setItemSlot(slot, stack)` for a mob that has not ticked yet (spawn-time equipment: `onEquipItem` does nothing
/// during `firstTick`, so no level is needed).
pub fn set_item_slot_spawn(e: &mut dyn Entity, slot: EquipmentSlot, stack: ItemStack) {
    debug_assert!(e.base().first_tick, "set_item_slot_spawn is for mobs that did not tick yet");
    lm(e).equipment.set(slot, stack);
}

/// `setGuaranteedDrop(slot)`.
pub fn set_guaranteed_drop(e: &mut dyn Entity, slot: EquipmentSlot) {
    let m = mm(e);
    m.drop_chances = m.drop_chances.with_guaranteed_drop(slot);
}

/// `setDropChance(slot, percent)`.
pub fn set_drop_chance(e: &mut dyn Entity, slot: EquipmentSlot, percent: f32) {
    let m = mm(e);
    m.drop_chances = m.drop_chances.with_equipment_chance(slot, percent);
}

/// `setItemSlotAndDropWhenKilled(slot, stack)`.
pub fn set_item_slot_and_drop_when_killed(e: &mut dyn Entity, level: &mut ServerLevel, slot: EquipmentSlot, stack: ItemStack) {
    set_item_slot(e, level, slot, stack);
    set_guaranteed_drop(e, slot);
}

/// `getApproximateAttributeWith(itemStack, attribute, slot)`.
fn get_approximate_attribute_with(e: &dyn Entity, stack: &ItemStack, attribute: Attribute, slot: EquipmentSlot) -> f64 {
    use crate::world::entity::attributes::AttributeModifierOperation as Op;
    use crate::world::entity::living_entity::equipment_changes::equipment_slot_group_test;
    let living = e.living().expect("living");
    let base_value = if living.attributes.has_attribute(attribute) { living.attributes.get_base_value(attribute) } else { 0.0 };
    let mut value = base_value;
    if let Some(mods) = stack.get(&ATTRIBUTE_MODIFIERS) {
        for entry in &mods.modifiers {
            if equipment_slot_group_test(entry.slot, slot) && entry.attribute == attribute {
                let amount = entry.modifier.amount;
                value += match entry.modifier.operation {
                    Op::AddValue => amount,
                    Op::AddMultipliedBase => amount * base_value,
                    Op::AddMultipliedTotal => amount * value,
                };
            }
        }
    }
    value
}

/// `canReplaceEqualItem(newItemStack, currentItemStack)`.
pub fn can_replace_equal_item(new: &ItemStack, current: &ItemStack) -> bool {
    let count = |s: &ItemStack| s.get(&ENCHANTMENTS).map_or(0, |e| e.len());
    let (current_n, new_n) = (count(current), count(new));
    if new_n != current_n {
        return new_n > current_n;
    }
    let new_damage = new.get_damage_value();
    let current_damage = current.get_damage_value();
    if new_damage != current_damage {
        new_damage < current_damage
    } else {
        new.get(&mc_protocol::components::CUSTOM_NAME).is_some() && current.get(&mc_protocol::components::CUSTOM_NAME).is_none()
    }
}

fn compare_armor(e: &dyn Entity, level: &ServerLevel, new: &ItemStack, current: &ItemStack, slot: EquipmentSlot) -> bool {
    if enchantment_helper::has(&level.registries, current, EffectComponent::prevent_armor_change) {
        return false;
    }
    let new_defense = get_approximate_attribute_with(e, new, Attribute::ARMOR, slot);
    let old_defense = get_approximate_attribute_with(e, current, Attribute::ARMOR, slot);
    let new_toughness = get_approximate_attribute_with(e, new, Attribute::ARMOR_TOUGHNESS, slot);
    let old_toughness = get_approximate_attribute_with(e, current, Attribute::ARMOR_TOUGHNESS, slot);
    if new_defense != old_defense {
        new_defense > old_defense
    } else if new_toughness != old_toughness {
        new_toughness > old_toughness
    } else {
        e.can_replace_equal_item(new, current)
    }
}

fn compare_weapons(e: &dyn Entity, new: &ItemStack, current: &ItemStack, slot: EquipmentSlot) -> bool {
    if let Some(preferred) = e.get_preferred_weapon_type() {
        let cur_pref = item_is_in_tag(current, preferred);
        let new_pref = item_is_in_tag(new, preferred);
        if cur_pref && !new_pref {
            return false;
        }
        if !cur_pref && new_pref {
            return true;
        }
    }
    let new_damage = get_approximate_attribute_with(e, new, Attribute::ATTACK_DAMAGE, slot);
    let old_damage = get_approximate_attribute_with(e, current, Attribute::ATTACK_DAMAGE, slot);
    if new_damage != old_damage { new_damage > old_damage } else { e.can_replace_equal_item(new, current) }
}

/// `canReplaceCurrentItem(newItemStack, currentItemStack, slot)`.
pub fn can_replace_current_item(e: &dyn Entity, level: &ServerLevel, new: &ItemStack, current: &ItemStack, slot: EquipmentSlot) -> bool {
    if current.is_empty() {
        true
    } else if slot.is_armor() {
        compare_armor(e, level, new, current, slot)
    } else if slot == EquipmentSlot::MainHand {
        compare_weapons(e, new, current, slot)
    } else {
        false
    }
}

/// `equipItemIfPossible(level, itemStack)`: the equipped stack (empty when it was not equipped).
pub fn equip_item_if_possible(e: &mut dyn Entity, level: &mut ServerLevel, mut item_stack: ItemStack) -> ItemStack {
    let mut slot = e.get_equipment_slot_for_item(&item_stack);
    if !e.is_equippable_in_slot(&item_stack, slot) {
        return ItemStack::empty();
    }
    let mut current = e.get_item_by_slot(slot);
    let mut can_replace = can_replace_current_item(e, level, &item_stack, &current, slot);
    if slot.is_armor() && !can_replace {
        slot = EquipmentSlot::MainHand;
        current = e.get_item_by_slot(slot);
        can_replace = current.is_empty();
    }
    if can_replace && e.can_hold_item(&item_stack) {
        let drop_chance = f64::from(e.mob().expect("mob").drop_chances.by_equipment(slot));
        if !current.is_empty() {
            let roll = e.base_mut().random.next_float();
            if f64::from((roll - 0.1f32).max(0.0)) < drop_chance {
                crate::world::entity::item::spawn_at_location(level, e, current, mc_core::phys::Vec3::ZERO);
            }
        }
        let to_equip = slot.limit(&mut item_stack);
        set_item_slot_and_drop_when_killed(e, level, slot, to_equip.clone());
        mm(e).persistence_required = true;
        to_equip
    } else {
        ItemStack::empty()
    }
}

/// `pickUpItem(level, entity)`.
pub fn pick_up_item(e: &mut dyn Entity, level: &mut ServerLevel, item_id: EntityId) {
    let Some(item_stack) = level.get_entity(item_id).and_then(|x| x.as_any().downcast_ref::<ItemEntity>()).map(|x| x.get_item()) else { return };
    let equipped = equip_item_if_possible(e, level, item_stack.copy());
    if !equipped.is_empty() {
        // onItemPickup(entity) is empty for mobs; take(entity, count)
        if !level.get_entity(item_id).is_some_and(|x| x.is_removed()) {
            let packet = ClientboundTakeItemEntityPacket { item_id, player_id: e.id(), amount: equipped.count() };
            level.entity_tracker.send_to_tracking_players(item_id, &encode_payload(&packet));
        }
        level.with_entity(item_id, |_, item| {
            if let Some(ie) = item.as_any_mut().downcast_mut::<ItemEntity>() {
                let mut stack = ie.get_item();
                stack.shrink(equipped.count());
                let empty = stack.is_empty();
                ie.set_item_in_place(stack);
                if empty {
                    ie.discard();
                }
            }
        });
    }
}

/// `populateDefaultEquipmentSlots(random, difficulty)`.
pub fn populate_default_equipment_slots(e: &mut dyn Entity, random: &mut dyn RandomSource, difficulty: &DifficultyInstance) {
    if random.next_float() < 0.15f32 * difficulty.get_special_multiplier() {
        let mut armor_type = random.next_int_bounded(3);
        let mut i = 1;
        while (i as f32) <= 3.0 {
            if random.next_float() < 0.1087f32 {
                armor_type += 1;
            }
            i += 1;
        }
        let partial_chance = if difficulty.get_difficulty() == mc_world::level_storage::Difficulty::Hard { 0.1f32 } else { 0.25f32 };
        let mut first = true;
        for slot in EQUIPMENT_POPULATION_ORDER {
            let item_stack = e.get_item_by_slot(slot);
            if !first && random.next_float() < partial_chance {
                break;
            }
            first = false;
            if item_stack.is_empty() {
                if let Some(equip) = get_equipment_for_slot(slot, armor_type) {
                    set_item_slot_spawn(e, slot, ItemStack::new(equip));
                }
            }
        }
    }
}

/// `getEquipmentForSlot(slot, type)`.
pub fn get_equipment_for_slot(slot: EquipmentSlot, ty: i32) -> Option<Item> {
    // the Java switch falls through between slots, but every `type` that matches a slot returns there and the others
    // reach `default -> null`
    let row = match slot {
        EquipmentSlot::Head => [Item::LEATHER_HELMET, Item::COPPER_HELMET, Item::GOLDEN_HELMET, Item::CHAINMAIL_HELMET, Item::IRON_HELMET, Item::DIAMOND_HELMET],
        EquipmentSlot::Chest => [Item::LEATHER_CHESTPLATE, Item::COPPER_CHESTPLATE, Item::GOLDEN_CHESTPLATE, Item::CHAINMAIL_CHESTPLATE, Item::IRON_CHESTPLATE, Item::DIAMOND_CHESTPLATE],
        EquipmentSlot::Legs => [Item::LEATHER_LEGGINGS, Item::COPPER_LEGGINGS, Item::GOLDEN_LEGGINGS, Item::CHAINMAIL_LEGGINGS, Item::IRON_LEGGINGS, Item::DIAMOND_LEGGINGS],
        EquipmentSlot::Feet => [Item::LEATHER_BOOTS, Item::COPPER_BOOTS, Item::GOLDEN_BOOTS, Item::CHAINMAIL_BOOTS, Item::IRON_BOOTS, Item::DIAMOND_BOOTS],
        _ => return None,
    };
    usize::try_from(ty).ok().and_then(|i| row.get(i).copied())
}

/// `populateDefaultEquipmentEnchantments(level, random, localDifficulty)`.
pub fn populate_default_equipment_enchantments(e: &mut dyn Entity, registries: &RegistryAccess, random: &mut dyn RandomSource, difficulty: &DifficultyInstance) {
    e.enchant_spawned_weapon(registries, random, difficulty);
    for slot in EquipmentSlot::VALUES {
        if slot.get_type() == EquipmentSlotType::HumanoidArmor {
            e.enchant_spawned_armor(registries, random, slot, difficulty);
        }
    }
}

/// `enchantSpawnedWeapon(level, random, difficulty)`.
pub fn enchant_spawned_weapon(e: &mut dyn Entity, registries: &RegistryAccess, random: &mut dyn RandomSource, difficulty: &DifficultyInstance) {
    enchant_spawned_equipment(e, registries, EquipmentSlot::MainHand, random, MAX_ENCHANTED_WEAPON_CHANCE, difficulty);
}

/// `enchantSpawnedArmor(level, random, slot, difficulty)`.
pub fn enchant_spawned_armor(e: &mut dyn Entity, registries: &RegistryAccess, random: &mut dyn RandomSource, slot: EquipmentSlot, difficulty: &DifficultyInstance) {
    enchant_spawned_equipment(e, registries, slot, random, MAX_ENCHANTED_ARMOR_CHANCE, difficulty);
}

/// `enchantSpawnedEquipment(level, slot, random, chance, difficulty)`.
pub fn enchant_spawned_equipment(e: &mut dyn Entity, registries: &RegistryAccess, slot: EquipmentSlot, random: &mut dyn RandomSource, chance: f32, difficulty: &DifficultyInstance) {
    let mut item_stack = e.get_item_by_slot(slot);
    if !item_stack.is_empty() && random.next_float() < chance * difficulty.get_special_multiplier() {
        enchantment_helper::enchant_item_from_provider(&mut item_stack, registries, &MOB_SPAWN_EQUIPMENT, difficulty, random);
        // the stack is mutated in place in vanilla; `setItemSlot(slot, itemStack)` stores the same object
        set_item_slot_spawn(e, slot, item_stack);
    }
}

/// `Mob.dropCustomDeathLoot(level, source, killedByPlayer)` (the equipment part; `dropFromLootTable` already ran).
pub fn drop_custom_death_loot(e: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, killed_by_player: bool) {
    // `Mob.dropFromLootTable` clears the `DeathLootTable` override after the table dropped.
    mm(e).loot_table = None;
    for slot in EquipmentSlot::VALUES {
        let mut item_stack = e.get_item_by_slot(slot);
        let mut drop_chance = e.mob().expect("mob").drop_chances.by_equipment(slot);
        if drop_chance != 0.0 {
            let preserve = e.mob().expect("mob").drop_chances.is_preserved(slot);
            let living_source = source.get_entity().filter(|id| level.get_entity(*id).is_some_and(|s| s.living().is_some()));
            if let Some(killer) = living_source {
                let mut causing = source.causing_snapshot(level);
                let mut actors = Actors::of(&mut *e);
                if let Some(c) = causing.as_mut() {
                    actors.push(&mut ***c);
                }
                drop_chance = enchantment_helper::process_equipment_drop_chance(level, &mut actors, killer, source, drop_chance);
            }
            let prevents = enchantment_helper::has(&level.registries, &item_stack, EffectComponent::prevent_equipment_drop);
            if !item_stack.is_empty() && !prevents && (killed_by_player || preserve) && e.base_mut().random.next_float() < drop_chance {
                if !preserve && item_stack.is_damageable_item() {
                    let max = item_stack.get_max_damage();
                    let inner = e.base_mut().random.next_int_bounded((max - 3).max(1));
                    let outer = e.base_mut().random.next_int_bounded(1 + inner);
                    item_stack.set_damage_value(max - outer);
                }
                crate::world::entity::item::spawn_at_location(level, e, item_stack, mc_core::phys::Vec3::ZERO);
                set_item_slot(e, level, slot, ItemStack::empty());
            }
        }
    }
}

/// `dropPreservedEquipment(level, shouldDrop)`: the slots prevented from dropping.
pub fn drop_preserved_equipment_where(e: &mut dyn Entity, level: &mut ServerLevel, mut should_drop: impl FnMut(&ItemStack) -> bool) -> Vec<EquipmentSlot> {
    let mut prevented = Vec::new();
    for slot in EquipmentSlot::VALUES {
        let item_stack = e.get_item_by_slot(slot);
        if !item_stack.is_empty() {
            if !should_drop(&item_stack) {
                prevented.push(slot);
            } else if e.mob().expect("mob").drop_chances.is_preserved(slot) {
                set_item_slot(e, level, slot, ItemStack::empty());
                crate::world::entity::item::spawn_at_location(level, e, item_stack, mc_core::phys::Vec3::ZERO);
            }
        }
    }
    prevented
}

/// `EquipmentUser.resolveSlot(toEquip, alreadyInsertedIntoSlots)`.
fn resolve_slot(to_equip: &ItemStack, already: &[EquipmentSlot]) -> Option<EquipmentSlot> {
    if to_equip.is_empty() {
        return None;
    }
    match to_equip.get(&EQUIPPABLE) {
        Some(eq) => {
            let slot = EquipmentSlot::from_protocol(eq.slot);
            (!already.contains(&slot)).then_some(slot)
        }
        None => (!already.contains(&EquipmentSlot::MainHand)).then_some(EquipmentSlot::MainHand),
    }
}

/// `Mob.equip(EquipmentTable)` (`createEquipmentParams`, seed 0).
pub fn equip(e: &mut dyn Entity, level: &mut ServerLevel, table: &EquipmentTable) {
    use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
    let loot_table = loading::find_loot_table(&level.registries, &table.loot_table);
    let Some(loot_table) = loot_table else { return };
    let possible = {
        let this: &dyn Entity = &*e;
        let params = LootParams::builder(level).origin(this.position()).this_entity(this).create(&LootContextParamSets::EQUIPMENT);
        loot_table.get_random_items_with_seed(&params, 0)
    };
    let mut inserted: Vec<EquipmentSlot> = Vec::new();
    for mut to_equip in possible {
        if let Some(slot) = resolve_slot(&to_equip, &inserted) {
            let equipped = slot.limit(&mut to_equip);
            if e.base().first_tick {
                set_item_slot_spawn(e, slot, equipped);
            } else {
                set_item_slot(e, level, slot, equipped);
            }
            if let Some(chance) = table.drop_chance(slot) {
                set_drop_chance(e, slot, chance);
            }
            inserted.push(slot);
        }
    }
}

#[allow(dead_code)]
fn _unused(_: &mut dyn Entity) {
    let _ = l;
}

/// `getBaseExperienceReward(level)`: `Animal`: `1 + nextInt(3)`; `Mob`: `xpReward` plus 1-3 per worn equipment.
pub fn get_base_experience_reward(e: &mut dyn Entity) -> i32 {
    let Some(m) = e.mob() else { return 0 };
    if m.animal.is_some() {
        return super::animal::get_base_experience_reward(e);
    }
    let xp_reward = m.xp_reward;
    if xp_reward > 0 {
        let mut result = xp_reward;
        for slot in EquipmentSlot::VALUES {
            if slot.can_increase_experience() {
                let item = e.get_item_by_slot(slot);
                if !item.is_empty() && e.mob().expect("mob").drop_chances.by_equipment(slot) <= 1.0 {
                    result += 1 + e.base_mut().random.next_int_bounded(3);
                }
            }
        }
        result
    } else {
        xp_reward
    }
}
