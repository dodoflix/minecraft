//! The smelting recipe lookup of `furnace_smelt` (`level.recipeAccess().getRecipeFor(RecipeType.SMELTING, input, level)`),
//! answered from the loaded `recipe` registry until a recipe manager exists.

use mc_core::ident::Identifier;
use mc_dfu::{Dyn, JSON};
use mc_protocol::components::{ItemStack, ItemStackTemplate};
use serde_json::Value;

use crate::server::level::ServerLevel;

/// Whether the recipe `ingredient` JSON (an item id, `#tag` or a list of those) accepts `stack`.
fn ingredient_matches(level: &ServerLevel, ingredient: &Value, stack: &ItemStack) -> bool {
    match ingredient {
        Value::String(s) => match s.strip_prefix('#') {
            Some(tag) => Identifier::try_parse(tag).is_some_and(|tag| {
                let item_id = stack.item().id();
                level
                    .registries
                    .registry("minecraft:item")
                    .is_some_and(|r| r.is_in_tag(item_id, &tag))
            }),
            None => {
                Identifier::try_parse(s).is_some_and(|id| id.to_string() == stack.item().name())
            }
        },
        Value::Array(list) => list.iter().any(|i| ingredient_matches(level, i, stack)),
        _ => false,
    }
}

/// The first `minecraft:smelting` recipe (registry order) whose ingredient accepts `stack`: `recipe.assemble(input)`.
pub fn smelting_result(level: &ServerLevel, stack: &ItemStack) -> Option<ItemStack> {
    let registry = level.registries.registry("minecraft:recipe")?;
    for entry in registry.entries() {
        let json = &*entry.json;
        if json.get("type").and_then(Value::as_str) != Some("minecraft:smelting") {
            continue;
        }
        if !json
            .get("ingredient")
            .is_some_and(|i| ingredient_matches(level, i, stack))
        {
            continue;
        }
        let result = json.get("result")?;
        let dyn_value = Dyn::from_json_str(&result.to_string()).ok()?;
        let template = ItemStackTemplate::codec()
            .parse(&JSON, &dyn_value)
            .into_result()
            .ok()?;
        return Some(template.create());
    }
    None
}
