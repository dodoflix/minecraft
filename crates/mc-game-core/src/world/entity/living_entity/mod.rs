//! Behaviour of `net.minecraft.world.entity.LivingEntity`, generic over every entity whose `living()` is `Some`
//! (players and mobs). Free functions take `&mut dyn Entity`; the overridable vanilla methods are hooks on
//! `trait Entity` whose defaults call into this module, overrides call these functions as their `super` call.

pub mod ai_step;
pub mod base_tick;
pub mod blocking;
pub mod combat;
pub mod damage;
pub mod death;
pub mod effects;
pub mod equipment;
pub mod equipment_changes;
pub mod fall;
pub mod fall_flying;
pub mod jump;
pub mod push;
pub mod sleeping;
pub mod swing;
pub mod teleport;
pub mod tick;
pub mod totem;
pub mod travel;
pub mod using_item;
pub mod util;

use crate::world::entity::Entity;
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use crate::world::entity::entity::FLAG_SPRINTING;
use mc_core::ident::Identifier;
use mc_data::Attribute;
use std::sync::LazyLock;

/// `SPRINTING_MODIFIER_ID`.
static SPRINTING_MODIFIER_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::new("minecraft", "sprinting").expect("valid id"));

/// `LivingEntity.setSprinting(isSprinting)`: the flag plus the `SPEED_MODIFIER_SPRINTING` modifier on `MOVEMENT_SPEED`.
pub fn set_sprinting(entity: &mut dyn Entity, sprinting: bool) {
    entity.base_mut().set_shared_flag(FLAG_SPRINTING, sprinting);
    let Some(living) = entity.living_mut() else {
        return;
    };
    if let Some(speed) = living.attributes.get_instance(Attribute::MOVEMENT_SPEED) {
        speed.remove_modifier(&SPRINTING_MODIFIER_ID);
        if sprinting {
            speed.add_transient_modifier(AttributeModifier {
                id: SPRINTING_MODIFIER_ID.clone(),
                amount: f64::from(0.3f32),
                operation: AttributeModifierOperation::AddMultipliedTotal,
            });
        }
    }
}
