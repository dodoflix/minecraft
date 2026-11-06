//! `DropChances`.

use super::super::equipment::EquipmentSlot;
use mc_nbt::{CompoundTag, Tag};

pub const DEFAULT_EQUIPMENT_DROP_CHANCE: f32 = 0.085;
pub const PRESERVE_ITEM_DROP_CHANCE_THRESHOLD: f32 = 1.0;
pub const PRESERVE_ITEM_DROP_CHANCE: i32 = 2;

/// `DropChances` (an `EnumMap<EquipmentSlot, Float>`, indexed by `EquipmentSlot` ordinal).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DropChances {
    by_equipment: [f32; 8],
}

impl DropChances {
    pub const DEFAULT: DropChances = DropChances {
        by_equipment: [DEFAULT_EQUIPMENT_DROP_CHANCE; 8],
    };

    /// `byEquipment(slot)`.
    pub fn by_equipment(&self, slot: EquipmentSlot) -> f32 {
        self.by_equipment[slot as usize]
    }

    /// `withGuaranteedDrop(slot)`.
    pub fn with_guaranteed_drop(self, slot: EquipmentSlot) -> Self {
        self.with_equipment_chance(slot, 2.0)
    }

    /// `withEquipmentChance(slot, chance)`: panics on a negative chance like vanilla's `IllegalArgumentException`.
    pub fn with_equipment_chance(mut self, slot: EquipmentSlot, chance: f32) -> Self {
        assert!(
            chance >= 0.0,
            "Tried to set invalid equipment chance {chance} for {slot:?}"
        );
        self.by_equipment[slot as usize] = chance;
        self
    }

    /// `isPreserved(slot)`.
    pub fn is_preserved(&self, slot: EquipmentSlot) -> bool {
        self.by_equipment(slot) > 1.0
    }

    /// `CODEC` encode: only the non-default entries (`filterDefaultValues`).
    pub fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        for slot in EquipmentSlot::VALUES {
            let chance = self.by_equipment(slot);
            if chance != DEFAULT_EQUIPMENT_DROP_CHANCE {
                tag.put_float(slot.get_name(), chance);
            }
        }
        tag
    }

    /// `CODEC` decode (`unboundedMap(EquipmentSlot.CODEC, NON_NEGATIVE_FLOAT)`); `None` when it does not decode.
    pub fn load(tag: &Tag) -> Option<Self> {
        let compound = tag.as_compound()?;
        let mut chances = Self::DEFAULT;
        for (key, value) in compound.iter() {
            let slot = EquipmentSlot::by_name(key)?;
            let chance = value.as_number()?.float_value();
            if chance < 0.0 || chance.is_nan() {
                return None;
            }
            chances.by_equipment[slot as usize] = chance;
        }
        Some(chances)
    }
}

impl Default for DropChances {
    fn default() -> Self {
        Self::DEFAULT
    }
}
