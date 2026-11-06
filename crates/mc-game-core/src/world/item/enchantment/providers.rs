//! `net.minecraft.world.item.enchantment.providers`: `EnchantmentProvider` (`by_cost`, `by_cost_with_difficulty`, `single`),
//! the `enchantment_provider` registry entries mobs and loot use to enchant their equipment.

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_protocol::components::{Holder, HolderSet, NoDirect};
use mc_worldgen::provider::IntProvider;
use serde_json::Value;

use super::enchantment::Enchantment;
use super::enchantment_instance::EnchantmentInstance;
use super::item_enchantments::{ItemEnchantmentsMutable, holder};
use crate::registry::RegistryAccess;
use crate::registry::codec::{RegistryDecodeContext, Schema, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::item::ItemStack;

/// `EnchantmentProvider`.
#[derive(Clone, Debug)]
pub enum EnchantmentProvider {
    ByCost {
        enchantments: HolderSet<NoDirect>,
        cost: IntProvider,
    },
    ByCostWithDifficulty {
        enchantments: HolderSet<NoDirect>,
        min_cost: i32,
        max_cost_span: i32,
    },
    Single {
        enchantment: Identifier,
        level: IntProvider,
    },
}

fn holder_set_schema() -> Schema {
    Schema::HolderSet {
        registry: keys::ENCHANTMENT,
        always_list: false,
    }
}

fn schema(ty: &str) -> Option<Schema> {
    Some(Schema::Record(match ty {
        "minecraft:by_cost" => vec![
            req("enchantments", holder_set_schema()),
            req("cost", Schema::Raw),
        ],
        "minecraft:by_cost_with_difficulty" => {
            vec![
                req("enchantments", holder_set_schema()),
                req("min_cost", Schema::IntRange(1, 10000)),
                req("max_cost_span", Schema::IntRange(0, 10000)),
            ]
        }
        "minecraft:single" => vec![
            req("enchantment", Schema::Holder(keys::ENCHANTMENT)),
            req("level", Schema::Raw),
        ],
        _ => return None,
    }))
}

fn holder_set_of(v: &Value) -> Option<HolderSet<NoDirect>> {
    use crate::registry::typed::HolderSetRef;
    Some(match HolderSetRef::from_json(v)? {
        HolderSetRef::Tag(t) => HolderSet::Named(t),
        HolderSetRef::Elements(ids) => {
            HolderSet::Direct(ids.into_iter().map(Holder::Reference).collect())
        }
    })
}

impl EnchantmentProvider {
    /// `EnchantmentProvider.DIRECT_CODEC` decode of the JSON of a registry entry.
    pub fn from_json(
        json: &Value,
        ctx: &RegistryDecodeContext,
    ) -> Result<EnchantmentProvider, String> {
        let ty = json
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "No key type in MapLike".to_owned())?;
        let ty = Identifier::read(ty)?.to_string();
        let schema = schema(&ty).ok_or_else(|| format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:enchantment_provider_type]: {ty}"))?;
        schema.convert(json, ctx)?;
        let int_provider =
            |key: &str| IntProvider::from_json(&json[key]).map_err(|e| e.to_string());
        Ok(match ty.as_str() {
            "minecraft:by_cost" => EnchantmentProvider::ByCost {
                enchantments: holder_set_of(&json["enchantments"]).ok_or("invalid enchantments")?,
                cost: int_provider("cost")?,
            },
            "minecraft:by_cost_with_difficulty" => EnchantmentProvider::ByCostWithDifficulty {
                enchantments: holder_set_of(&json["enchantments"]).ok_or("invalid enchantments")?,
                min_cost: json["min_cost"].as_i64().unwrap_or(1) as i32,
                max_cost_span: json["max_cost_span"].as_i64().unwrap_or(0) as i32,
            },
            _ => EnchantmentProvider::Single {
                enchantment: json["enchantment"]
                    .as_str()
                    .and_then(Identifier::try_parse)
                    .ok_or("invalid enchantment")?,
                level: int_provider("level")?,
            },
        })
    }

    /// `enchant(item, itemEnchantments, random, difficulty)`.
    pub fn enchant(
        &self,
        registries: &RegistryAccess,
        item: &ItemStack,
        enchantments: &mut ItemEnchantmentsMutable,
        random: &mut dyn RandomSource,
        difficulty: &DifficultyInstance,
    ) {
        let upgrade_all = |enchantments: &mut ItemEnchantmentsMutable,
                           picked: Vec<EnchantmentInstance>| {
            for instance in picked {
                enchantments.upgrade(&holder(&instance.enchantment), instance.level);
            }
        };
        match self {
            EnchantmentProvider::ByCost {
                enchantments: set,
                cost,
            } => {
                let cost = cost.sample(random);
                let source =
                    crate::world::item::enchantment_helper::candidates_of(registries, Some(set));
                upgrade_all(
                    enchantments,
                    crate::world::item::enchantment_helper::select_enchantment(
                        random, item, cost, registries, &source,
                    ),
                );
            }
            EnchantmentProvider::ByCostWithDifficulty {
                enchantments: set,
                min_cost,
                max_cost_span,
            } => {
                let difficulty_modifier = difficulty.get_special_multiplier();
                let cost = mth::random_between_inclusive(
                    random,
                    *min_cost,
                    min_cost.wrapping_add((difficulty_modifier * *max_cost_span as f32) as i32),
                );
                let source =
                    crate::world::item::enchantment_helper::candidates_of(registries, Some(set));
                upgrade_all(
                    enchantments,
                    crate::world::item::enchantment_helper::select_enchantment(
                        random, item, cost, registries, &source,
                    ),
                );
            }
            EnchantmentProvider::Single { enchantment, level } => {
                if let Some(e) = registries.get::<Enchantment>(keys::ENCHANTMENT, enchantment) {
                    let sampled = level.sample(random);
                    enchantments.upgrade(
                        &holder(enchantment),
                        mth::clamp_i32(sampled, e.get_min_level(), e.get_max_level()),
                    );
                }
            }
        }
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    EnchantmentProvider::from_json(json, ctx).map(DecodedEntry::new)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::ENCHANTMENT_PROVIDER,
    decode,
};
