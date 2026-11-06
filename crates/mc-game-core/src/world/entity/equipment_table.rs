//! `net.minecraft.world.entity.EquipmentTable`: the loot table that equips a mob (`SpawnData.equipment`, trial
//! spawner configs, `Mob.equip`) and the per-slot drop chances.

use super::equipment::EquipmentSlot;
use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, Either, FLOAT, record, string_enum_codec};
use std::sync::LazyLock;

/// `EquipmentTable(lootTable, slotDropChances)`: `slot_drop_chances` keeps the decoded entry order.
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentTable {
    pub loot_table: Identifier,
    pub slot_drop_chances: Vec<(EquipmentSlot, f32)>,
}

impl EquipmentTable {
    /// `EquipmentTable(lootTable, dropChance)`: the same chance for every slot.
    pub fn new(loot_table: Identifier, drop_chance: f32) -> Self {
        Self {
            loot_table,
            slot_drop_chances: create_for_all_slots(drop_chance),
        }
    }

    /// `slotDropChances().get(slot)`.
    pub fn drop_chance(&self, slot: EquipmentSlot) -> Option<f32> {
        self.slot_drop_chances
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, c)| *c)
    }
}

fn create_for_all_slots(drop_chance: f32) -> Vec<(EquipmentSlot, f32)> {
    EquipmentSlot::VALUES
        .iter()
        .map(|s| (*s, drop_chance))
        .collect()
}

const SLOT_NAMES: [(EquipmentSlot, &str); 8] = [
    (EquipmentSlot::MainHand, "mainhand"),
    (EquipmentSlot::OffHand, "offhand"),
    (EquipmentSlot::Feet, "feet"),
    (EquipmentSlot::Legs, "legs"),
    (EquipmentSlot::Chest, "chest"),
    (EquipmentSlot::Head, "head"),
    (EquipmentSlot::Body, "body"),
    (EquipmentSlot::Saddle, "saddle"),
];

/// `EquipmentSlot.CODEC`.
fn slot_codec() -> Codec<EquipmentSlot> {
    string_enum_codec(&SLOT_NAMES)
}

/// `EquipmentTable.DROP_CHANCES_CODEC`: a single float (all slots) or a slot -> float map.
fn drop_chances_codec() -> Codec<Vec<(EquipmentSlot, f32)>> {
    FLOAT.either(&slot_codec().unbounded_map(&FLOAT)).xmap(
        |either| either.map(create_for_all_slots, |map| map),
        |provider: &Vec<(EquipmentSlot, f32)>| {
            let first = provider.first().map(|(_, v)| v.to_bits());
            let all_same = provider.iter().all(|(_, v)| Some(v.to_bits()) == first);
            let all_present = EquipmentSlot::VALUES
                .iter()
                .all(|s| provider.iter().any(|(k, _)| k == s));
            if all_same && all_present {
                Either::Left(provider.first().map(|(_, v)| *v).unwrap_or(0.0))
            } else {
                Either::Right(provider.clone())
            }
        },
    )
}

/// `EquipmentTable.CODEC`.
pub static CODEC: LazyLock<Codec<EquipmentTable>> = LazyLock::new(|| {
    record!(EquipmentTable {
        loot_table: IDENTIFIER.field_of("loot_table"),
        slot_drop_chances: drop_chances_codec()
            .optional_field_of_default("slot_drop_chances", Vec::new()),
    })
    .codec()
});
