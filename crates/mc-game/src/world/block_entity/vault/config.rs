//! `VaultConfig`.

use crate::world::item::ItemStack;
use mc_core::ident::Identifier;
use mc_core::math::java::java_double_to_string;
use mc_data::Item;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DOUBLE, DataResult, record};
use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub struct VaultConfig {
    pub loot_table: Identifier,
    pub activation_range: f64,
    pub deactivation_range: f64,
    pub key_item: ItemStack,
    pub override_loot_table_to_display: Option<Identifier>,
}

/// `VaultConfig.DEFAULT`.
pub static DEFAULT: LazyLock<VaultConfig> = LazyLock::new(|| VaultConfig {
    loot_table: Identifier::parse("minecraft:chests/trial_chambers/reward").expect("constant identifier"),
    activation_range: 4.0,
    deactivation_range: 4.5,
    key_item: ItemStack::new(Item::TRIAL_KEY),
    override_loot_table_to_display: None,
});

impl VaultConfig {
    /// `VaultConfig.CODEC`: absent fields read as the defaults, except `key_item` (empty).
    pub fn codec() -> Codec<VaultConfig> {
        static C: LazyLock<Codec<VaultConfig>> = LazyLock::new(|| {
            let d = &*DEFAULT;
            record!(VaultConfig {
                loot_table: IDENTIFIER.lenient_optional_field_of_default("loot_table", d.loot_table.clone()),
                activation_range: DOUBLE.lenient_optional_field_of_default("activation_range", d.activation_range),
                deactivation_range: DOUBLE.lenient_optional_field_of_default("deactivation_range", d.deactivation_range),
                key_item: ItemStack::codec().lenient_optional_field_of("key_item").xmap(
                    |item: Option<ItemStack>| item.unwrap_or_else(ItemStack::empty),
                    |item: &ItemStack| if item.is_empty() { None } else { Some(item.clone()) },
                ),
                override_loot_table_to_display: IDENTIFIER.lenient_optional_field_of("override_loot_table_to_display"),
            })
            .codec()
            .validate(|config| {
                if config.activation_range > config.deactivation_range {
                    let (a, b) = (java_double_to_string(config.activation_range), java_double_to_string(config.deactivation_range));
                    DataResult::error(move || format!("Activation range must ({a}) be less or equal to deactivation range ({b})"))
                } else {
                    DataResult::success(config)
                }
            })
        });
        C.clone()
    }
}
