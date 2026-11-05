//! `EnchantmentAttributeEffect`: an attribute modifier whose amount depends on the enchantment level.

use mc_core::ident::Identifier;
use mc_data::Attribute;
use mc_dfu::{MapCodec, record, string_enum_codec};
use mc_protocol::components::holder::registry_by_name_codec;
use mc_protocol::packets::play::{AttributeModifier, AttributeModifierOperation};

use crate::world::item::enchantment::level_based_value::LevelBasedValue;

#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentAttributeEffect {
    pub id: Identifier,
    pub attribute: Attribute,
    pub amount: LevelBasedValue,
    pub operation: AttributeModifierOperation,
}

static OPERATIONS: [(AttributeModifierOperation, &str); 3] = [
    (AttributeModifierOperation::AddValue, "add_value"),
    (AttributeModifierOperation::AddMultipliedBase, "add_multiplied_base"),
    (AttributeModifierOperation::AddMultipliedTotal, "add_multiplied_total"),
];

impl EnchantmentAttributeEffect {
    /// `MAP_CODEC`.
    pub fn map_codec() -> MapCodec<EnchantmentAttributeEffect> {
        record!(EnchantmentAttributeEffect {
            id: mc_dfu::extra::IDENTIFIER.field_of("id"),
            attribute: registry_by_name_codec::<Attribute>().field_of("attribute"),
            amount: LevelBasedValue::codec().field_of("amount"),
            operation: string_enum_codec(&OPERATIONS).field_of("operation"),
        })
    }

    /// `getModifier(level, slot)`: the modifier id is `<id>/<slot name>`.
    pub fn get_modifier(&self, level: i32, slot_name: &str) -> AttributeModifier {
        AttributeModifier {
            id: self.id.with_suffix(&format!("/{slot_name}")).expect("valid attribute effect id"),
            amount: self.amount.calculate(level) as f64,
            operation: self.operation,
        }
    }
}
