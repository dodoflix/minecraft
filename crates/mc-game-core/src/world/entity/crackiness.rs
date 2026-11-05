//! `net.minecraft.world.entity.Crackiness`: how cracked a damageable armor / golem looks by remaining durability.

use crate::world::item::ItemStack;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CrackinessLevel {
    None,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy)]
pub struct Crackiness {
    fraction_low: f32,
    fraction_medium: f32,
    fraction_high: f32,
}

impl Crackiness {
    pub const GOLEM: Crackiness = Crackiness { fraction_low: 0.75, fraction_medium: 0.5, fraction_high: 0.25 };
    pub const WOLF_ARMOR: Crackiness = Crackiness { fraction_low: 0.95, fraction_medium: 0.69, fraction_high: 0.32 };

    /// `byFraction(fraction)`.
    pub fn by_fraction(&self, fraction: f32) -> CrackinessLevel {
        if fraction < self.fraction_high {
            CrackinessLevel::High
        } else if fraction < self.fraction_medium {
            CrackinessLevel::Medium
        } else if fraction < self.fraction_low {
            CrackinessLevel::Low
        } else {
            CrackinessLevel::None
        }
    }

    /// `byDamage(item)`.
    pub fn by_damage_item(&self, item: &ItemStack) -> CrackinessLevel {
        if !item.is_damageable_item() { CrackinessLevel::None } else { self.by_damage(item.get_damage_value(), item.get_max_damage()) }
    }

    /// `byDamage(damage, maxDamage)`.
    pub fn by_damage(&self, damage: i32, max_damage: i32) -> CrackinessLevel {
        self.by_fraction((max_damage - damage) as f32 / max_damage as f32)
    }
}
