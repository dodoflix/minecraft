//! `Mob.addAdditionalSaveData` / `readAdditionalSaveData` and the layered NBT of `AgeableMob`, `Animal`,
//! `TamableAnimal` (each layer's keys are written/read when the mob has that layer). Concrete classes call these first
//! ("super") and then write their own keys.

use super::drop_chances::DropChances;
use super::{ageable, animal, leashable, tamable};
use crate::world::entity::Entity;
use crate::world::entity::living;
use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};

/// `Mob.addAdditionalSaveData` plus the generic subclasses' keys (also `LivingEntity.addAdditionalSaveData`).
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    let l = e.living().expect("living");
    living::add_additional_save_data(l, e.get_absorption_amount(), &l.equipment, out);
    mob_add_additional_save_data(e, out);
    if e.mob().is_some_and(|m| m.ageable.is_some()) {
        ageable::add_additional_save_data(e, out);
    }
    if e.mob().is_some_and(|m| m.animal.is_some()) {
        animal::add_additional_save_data(e, out);
    }
    if e.mob().is_some_and(|m| m.tamable.is_some()) {
        tamable::add_additional_save_data(e, out);
    }
}

/// `Mob.addAdditionalSaveData` alone (after `LivingEntity`'s).
pub fn mob_add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    let m = e.mob().expect("mob");
    out.put_boolean("CanPickUpLoot", e.can_pick_up_loot());
    out.put_boolean("PersistenceRequired", m.persistence_required);
    if m.drop_chances != DropChances::DEFAULT {
        out.put("drop_chances", Tag::Compound(m.drop_chances.save()));
    }
    leashable::write_leash_data(e, out);
    if m.has_home() {
        out.put_int("home_radius", m.home_radius);
        out.put_int_array(
            "home_pos",
            vec![
                m.home_position.x(),
                m.home_position.y(),
                m.home_position.z(),
            ],
        );
    }
    out.put_boolean("LeftHanded", m.is_left_handed());
    if let Some(table) = &m.loot_table {
        out.put_string("DeathLootTable", table.to_string());
    }
    if m.loot_table_seed != 0 {
        out.put_long("DeathLootTableSeed", m.loot_table_seed);
    }
    if m.is_no_ai() {
        out.put_boolean("NoAI", true);
    }
}

/// `Mob.readAdditionalSaveData` plus the generic subclasses' keys (also `LivingEntity.readAdditionalSaveData`).
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    {
        let l = e.living_mut().expect("living");
        let (absorption, equipment) = living::read_additional_save_data(l, input);
        l.absorption_amount = absorption;
        l.equipment = equipment;
    }
    // `this.brain = this.makeBrain(packed)` (the Brain step of `LivingEntity.readAdditionalSaveData`)
    crate::world::entity::ai::brain::read_brain(e, input);
    mob_read_additional_save_data(e, input);
    if e.mob().is_some_and(|m| m.ageable.is_some()) {
        ageable::read_additional_save_data(e, input);
    }
    if e.mob().is_some_and(|m| m.animal.is_some()) {
        animal::read_additional_save_data(e, input);
    }
    if e.mob().is_some_and(|m| m.tamable.is_some()) {
        tamable::read_additional_save_data(e, input);
    }
}

/// `Mob.readAdditionalSaveData` alone (after `LivingEntity`'s).
pub fn mob_read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    e.set_can_pick_up_loot(input.get_boolean_or("CanPickUpLoot", false));
    leashable::read_leash_data(e, None, input);
    let m = e.mob_mut().expect("mob");
    m.persistence_required = input.get_boolean_or("PersistenceRequired", false);
    m.drop_chances = input
        .get("drop_chances")
        .and_then(DropChances::load)
        .unwrap_or(DropChances::DEFAULT);
    m.home_radius = input.get_int_or("home_radius", -1);
    if m.home_radius >= 0 {
        m.home_position =
            living::read_block_pos(input.get("home_pos")).unwrap_or(mc_core::pos::BlockPos::ZERO);
    }
    m.set_left_handed(input.get_boolean_or("LeftHanded", false));
    m.loot_table = input
        .get_string("DeathLootTable")
        .and_then(Identifier::try_parse);
    m.loot_table_seed = input.get_long_or("DeathLootTableSeed", 0);
    m.set_no_ai(input.get_boolean_or("NoAI", false));
}
