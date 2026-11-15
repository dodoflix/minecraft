//! The smelting recipe lookup of `furnace_smelt` (`level.recipeAccess().getRecipeFor(RecipeType.SMELTING, input, level)`).

use mc_data::RecipeType;
use mc_protocol::components::ItemStack;

use crate::server::level::ServerLevel;
use crate::world::item::crafting::SingleRecipeInput;

/// `recipe.assemble(input)` of the first smelting recipe the server's recipe manager (feature flags applied) matches to
/// `stack`.
pub fn smelting_result(level: &ServerLevel, stack: &ItemStack) -> Option<ItemStack> {
    let input = SingleRecipeInput(stack.clone());
    let recipe = level
        .recipe_access()
        .get_recipe_for(RecipeType::SMELTING, &input, level, None)?;
    recipe.value.as_cooking().map(|c| c.result().create())
}
