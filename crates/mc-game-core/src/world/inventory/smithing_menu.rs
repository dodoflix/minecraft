//! `SmithingMenu` (`extends ItemCombinerMenu`): template, base and addition slots of the smithing table; the result is the
//! first smithing recipe (netherite upgrade or armor trim) that matches the three inputs.

use super::item_combiner_menu::CombinerResultContainer;
use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, DataSlot, ItemCombinerMenu,
    ItemCombinerMenuCore, ItemCombinerMenuSlotDefinition, MenuBase, MenuPlayer,
    RecipeCraftingHolder, Slot, SlotBehavior, SlotEnv,
};
use crate::world::container::Container;
use crate::world::item::crafting::{RecipeInput, RecipePropertySet, SmithingRecipeInput};
use crate::world::item::{ItemStack, on_crafted_by};
use crate::world::level::LevelAccessor;
use crate::world::level::block::BlockStateExt;
use mc_data::block::{Block, BlockState};
use mc_data::{MenuType, RecipeType};
use std::any::Any;
use std::rc::Rc;

pub const TEMPLATE_SLOT: usize = 0;
pub const BASE_SLOT: usize = 1;
pub const ADDITIONAL_SLOT: usize = 2;
pub const RESULT_SLOT: usize = 3;

/// The three `RecipePropertySet`s the menu reads at construction (`baseItemTest`, `templateItemTest`, `additionItemTest`).
#[derive(Default)]
struct PropertySets {
    base: RecipePropertySet,
    template: RecipePropertySet,
    addition: RecipePropertySet,
}

/// The smithing table's result slot: `onTake` consumes one of each input.
struct SmithingResultSlot {
    access: ContainerLevelAccess,
    input_local: usize,
    result_local: usize,
}

impl SlotBehavior for SmithingResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn on_take(&self, _slot: &Slot, env: &mut SlotEnv, carried: &mut ItemStack) {
        let count = carried.count();
        on_crafted_by(carried, &mut *env.player, count);
        let relevant: Vec<ItemStack> = (TEMPLATE_SLOT..=ADDITIONAL_SLOT)
            .map(|i| env.locals[self.input_local].get_item(i).clone())
            .collect();
        env.locals[self.result_local]
            .as_any_mut()
            .downcast_mut::<CombinerResultContainer>()
            .expect("smithing result container")
            .award_used_recipes(&mut *env.player, &relevant);
        for slot in TEMPLATE_SLOT..=ADDITIONAL_SLOT {
            let input = env.locals[self.input_local].as_mut();
            let mut stack = input.get_item(slot).clone();
            if !stack.is_empty() {
                stack.shrink(1);
                input.set_item(slot, stack);
            }
        }
        if let Some(pos) = self.access.pos()
            && let Some(level) = env.player.level()
        {
            level.level_event(None, 1044, pos, 0);
        }
    }
}

pub struct SmithingMenu {
    core: ItemCombinerMenuCore,
    sets: Rc<PropertySets>,
}

impl SmithingMenu {
    /// `SmithingMenu(containerId, inventory, access)`.
    pub fn new(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        access: ContainerLevelAccess,
    ) -> Self {
        let sets = player
            .level_ref()
            .map_or_else(PropertySets::default, |level| {
                let recipes = level.recipe_access();
                PropertySets {
                    base: recipes
                        .property_set(RecipePropertySet::SMITHING_BASE)
                        .clone(),
                    template: recipes
                        .property_set(RecipePropertySet::SMITHING_TEMPLATE)
                        .clone(),
                    addition: recipes
                        .property_set(RecipePropertySet::SMITHING_ADDITION)
                        .clone(),
                }
            });
        let sets = Rc::new(sets);
        let (template_sets, base_sets, addition_sets) = (sets.clone(), sets.clone(), sets.clone());
        let definition = ItemCombinerMenuSlotDefinition::create()
            .with_slot(TEMPLATE_SLOT, 8, 48, move |s| {
                template_sets.template.test(s)
            })
            .with_slot(BASE_SLOT, 26, 48, move |s| base_sets.base.test(s))
            .with_slot(ADDITIONAL_SLOT, 44, 48, move |s| {
                addition_sets.addition.test(s)
            })
            .with_result_slot(RESULT_SLOT, 98, 48)
            .build();
        // the input container is local 0 of the core and the result container local 1
        let result = SmithingResultSlot {
            access,
            input_local: 0,
            result_local: 1,
        };
        let mut core = ItemCombinerMenuCore::new(
            Some(MenuType::SMITHING),
            container_id,
            access,
            definition,
            Box::new(result),
        );
        debug_assert_eq!((core.input_local, core.result_local), (0, 1));
        core.base.add_data_slot(DataSlot::standalone());
        Self { core, sets }
    }

    fn input_stack(&self, slot: usize) -> &ItemStack {
        self.core.base.locals[self.core.input_local].get_item(slot)
    }

    fn results(&mut self) -> &mut CombinerResultContainer {
        let local = self.core.result_local;
        self.core.base.locals[local]
            .as_any_mut()
            .downcast_mut::<CombinerResultContainer>()
            .expect("smithing result container")
    }
}

impl ItemCombinerMenu for SmithingMenu {
    fn core(&self) -> &ItemCombinerMenuCore {
        &self.core
    }
    fn core_mut(&mut self) -> &mut ItemCombinerMenuCore {
        &mut self.core
    }

    fn is_valid_block(&self, state: BlockState) -> bool {
        state.is_block(Block::SMITHING_TABLE)
    }

    fn create_result(&mut self, player: &mut dyn MenuPlayer) {
        let input = SmithingRecipeInput {
            template: self.input_stack(TEMPLATE_SLOT).clone(),
            base: self.input_stack(BASE_SLOT).clone(),
            addition: self.input_stack(ADDITIONAL_SLOT).clone(),
        };
        let found = player.level_ref().and_then(|level| {
            level
                .recipe_access()
                .get_recipe_for(RecipeType::SMITHING, &input, level, None)
        });
        match found {
            Some(recipe) => {
                let result = input.assemble(&recipe.value);
                let results = self.results();
                results.set_recipe_used(Some(recipe));
                results.set_item(0, result);
            }
            None => {
                let results = self.results();
                results.set_recipe_used(None);
                results.set_item(0, ItemStack::empty());
            }
        }
    }

    fn can_move_into_input_slots(&self, stack: &ItemStack) -> bool {
        (self.sets.template.test(stack) && self.input_stack(TEMPLATE_SLOT).is_empty())
            || (self.sets.base.test(stack) && self.input_stack(BASE_SLOT).is_empty())
            || (self.sets.addition.test(stack) && self.input_stack(ADDITIONAL_SLOT).is_empty())
    }
}

impl AbstractContainerMenu for SmithingMenu {
    fn base(&self) -> &MenuBase {
        &self.core.base
    }
    fn base_mut(&mut self) -> &mut MenuBase {
        &mut self.core.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        self.combiner_slots_changed(player, container);
        if player.level_ref().is_some() {
            let has_error = !self.input_stack(TEMPLATE_SLOT).is_empty()
                && !self.input_stack(BASE_SLOT).is_empty()
                && !self.input_stack(ADDITIONAL_SLOT).is_empty()
                && self.results().is_empty();
            self.core.base.set_data(0, i32::from(has_error));
        }
    }
    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.combiner_removed(player);
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.combiner_still_valid(player)
    }
    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        self.combiner_quick_move_stack(player, slot_index)
    }
    /// `canTakeItemForPickAll`: the result slot's container never gives items to a pick-all.
    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, target: &Slot) -> bool {
        !matches!(target.container, ContainerKey::Local(i) if i == self.core.result_local)
    }
}
