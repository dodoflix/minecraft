//! `LoomMenu`: banner + dye (+ pattern item) -> banner with one more layer.

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, DataSlot, MenuBase, MenuPlayer,
    MenuSimpleContainer, Slot, SlotBehavior, SlotEnv, finish_quick_move,
};
use crate::registry::RegistryAccess;
use crate::registry::keys::BANNER_PATTERN as BANNER_PATTERN_REGISTRY;
use crate::world::container::SimpleContainer;
use crate::world::item::ItemStack;
use mc_core::ident::Identifier;
use mc_data::{Block, Item, MenuType, SoundEvent};
use mc_protocol::components::{
    BANNER_PATTERNS, BannerPattern, BannerPatternLayer, BannerPatternLayers, DYE, Holder,
    HolderSet, PROVIDES_BANNER_PATTERNS,
};
use mc_protocol::packets::play::level::SoundSource;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const INPUT_LOCAL: usize = 0;
const OUTPUT_LOCAL: usize = 1;
pub const BANNER_SLOT: usize = 0;
pub const DYE_SLOT: usize = 1;
pub const PATTERN_SLOT: usize = 2;
pub const RESULT_SLOT: usize = 3;
const INV_SLOT_START: usize = 4;
const INV_SLOT_END: usize = 31;
const USE_ROW_SLOT_START: usize = 31;
const USE_ROW_SLOT_END: usize = 40;
const PATTERN_NOT_SET: i32 = -1;
const MAX_LAYERS: usize = 6;

/// `itemStack.getItem() instanceof BannerItem`.
fn is_banner_item(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.item().data().classes.contains(&"BannerItem")
}

fn is_in_item_tag(stack: &ItemStack, tag: &str) -> bool {
    !stack.is_empty() && mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `isPatternItem`: `#loom_patterns` and `provides_banner_patterns`.
fn is_pattern_item(stack: &ItemStack) -> bool {
    is_in_item_tag(stack, "minecraft:loom_patterns")
        && stack.get(&PROVIDES_BANNER_PATTERNS).is_some()
}

/// `isDyeItem`: `#loom_dyes` and `dye`.
fn is_dye_item(stack: &ItemStack) -> bool {
    is_in_item_tag(stack, "minecraft:loom_dyes") && stack.get(&DYE).is_some()
}

/// The members of a banner pattern tag of the player's registries, in tag order.
fn banner_pattern_tag(registries: &RegistryAccess, tag: &Identifier) -> Vec<Holder<BannerPattern>> {
    let registry = registries.registry_or_throw(BANNER_PATTERN_REGISTRY);
    registry
        .tag(tag)
        .unwrap_or_default()
        .iter()
        .filter_map(|&id| registry.name_of(id))
        .map(Holder::Reference)
        .collect()
}

/// One of the three input slots: `mayPlace` is the predicate.
struct InputSlot(fn(&ItemStack) -> bool);

impl SlotBehavior for InputSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        (self.0)(stack)
    }
}

/// The result slot: `onTake` consumes one banner and one dye and plays the take sound once per game tick.
struct LoomResultSlot {
    access: ContainerLevelAccess,
    selected: Rc<RefCell<Vec<i32>>>,
    last_sound_time: Cell<i64>,
}

impl SlotBehavior for LoomResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, _carried: &mut ItemStack) {
        env.locals[INPUT_LOCAL].remove_item(BANNER_SLOT, 1);
        env.locals[INPUT_LOCAL].remove_item(DYE_SLOT, 1);
        if env.locals[INPUT_LOCAL].get_item(BANNER_SLOT).is_empty()
            || env.locals[INPUT_LOCAL].get_item(DYE_SLOT).is_empty()
        {
            self.selected.borrow_mut()[0] = PATTERN_NOT_SET;
        }
        if let Some(pos) = self.access.pos()
            && let Some(level) = env.player.level()
        {
            let game_time = level.game_time();
            if self.last_sound_time.get() != game_time {
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::UI_LOOM_TAKE_RESULT,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                self.last_sound_time.set(game_time);
            }
        }
        slot.set_changed(env);
    }
}

pub struct LoomMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
    selected_banner_pattern_index: Rc<RefCell<Vec<i32>>>,
    selectable_patterns: Vec<Holder<BannerPattern>>,
}

impl LoomMenu {
    /// `LoomMenu(containerId, inventory, access)`.
    pub fn new(container_id: i32, access: ContainerLevelAccess) -> Self {
        let mut base = MenuBase::new(Some(MenuType::LOOM), container_id);
        let input = base.add_local(Box::new(MenuSimpleContainer::new(3)));
        let output = base.add_local(Box::new(SimpleContainer::new(1)));
        debug_assert_eq!((input, output), (INPUT_LOCAL, OUTPUT_LOCAL));
        let selected = Rc::new(RefCell::new(vec![0]));
        let key = ContainerKey::Local(INPUT_LOCAL);
        for (slot, x, y, may_place) in [
            (
                BANNER_SLOT,
                13,
                26,
                is_banner_item as fn(&ItemStack) -> bool,
            ),
            (DYE_SLOT, 33, 26, is_dye_item),
            (PATTERN_SLOT, 23, 45, is_pattern_item),
        ] {
            base.add_slot(Slot::with_behavior(
                key.clone(),
                slot,
                x,
                y,
                Box::new(InputSlot(may_place)),
            ));
        }
        base.add_slot(Slot::with_behavior(
            ContainerKey::Local(OUTPUT_LOCAL),
            0,
            143,
            57,
            Box::new(LoomResultSlot {
                access,
                selected: selected.clone(),
                last_sound_time: Cell::new(0),
            }),
        ));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        base.add_data_slot(DataSlot::shared(selected.clone(), 0));
        Self {
            base,
            access,
            selected_banner_pattern_index: selected,
            selectable_patterns: Vec::new(),
        }
    }

    pub fn get_selected_banner_pattern_index(&self) -> i32 {
        self.selected_banner_pattern_index.borrow()[0]
    }

    fn set_selected(&self, index: i32) {
        self.selected_banner_pattern_index.borrow_mut()[0] = index;
    }

    pub fn get_selectable_patterns(&self) -> &[Holder<BannerPattern>] {
        &self.selectable_patterns
    }

    /// `getSelectablePatterns(patternStack)`.
    fn compute_selectable_patterns(
        registries: &RegistryAccess,
        pattern_stack: &ItemStack,
    ) -> Vec<Holder<BannerPattern>> {
        if pattern_stack.is_empty() {
            let tag = Identifier::parse("minecraft:no_item_required").expect("tag id");
            return banner_pattern_tag(registries, &tag);
        }
        match pattern_stack.get(&PROVIDES_BANNER_PATTERNS) {
            Some(HolderSet::Named(tag)) => banner_pattern_tag(registries, tag),
            Some(HolderSet::Direct(holders)) => holders.clone(),
            None => Vec::new(),
        }
    }

    fn is_valid_pattern_index(&self, selected_pattern: i32) -> bool {
        selected_pattern >= 0 && (selected_pattern as usize) < self.selectable_patterns.len()
    }

    fn slot_item(&mut self, player: &mut dyn MenuPlayer, slot: usize) -> ItemStack {
        self.base.with_slot(player, slot, |s, e| s.get_item(e))
    }

    fn clear_result(&mut self, player: &mut dyn MenuPlayer) {
        self.base
            .with_slot(player, RESULT_SLOT, |s, e| s.set(e, ItemStack::empty()));
    }

    /// `setupResultSlot(pattern)`.
    fn setup_result_slot(&mut self, player: &mut dyn MenuPlayer, pattern: &Holder<BannerPattern>) {
        let banner_stack = self.slot_item(player, BANNER_SLOT);
        let dye_stack = self.slot_item(player, DYE_SLOT);
        let mut result = ItemStack::empty();
        if !banner_stack.is_empty()
            && !dye_stack.is_empty()
            && let Some(&color) = dye_stack.get(&DYE)
        {
            result = banner_stack.copy_with_count(1);
            let mut layers = result
                .get_or_default(&BANNER_PATTERNS, &BannerPatternLayers::default())
                .layers
                .clone();
            layers.push(BannerPatternLayer {
                pattern: pattern.clone(),
                color,
            });
            result.set(BANNER_PATTERNS, BannerPatternLayers { layers });
        }
        let current = self.slot_item(player, RESULT_SLOT);
        if !ItemStack::matches(&result, &current) {
            self.base
                .with_slot(player, RESULT_SLOT, |s, e| s.set(e, result));
        }
    }
}

impl AbstractContainerMenu for LoomMenu {
    fn base(&self) -> &MenuBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MenuBase {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        super::still_valid(&self.access, player, Block::LOOM)
    }

    fn click_menu_button(&mut self, player: &mut dyn MenuPlayer, button_id: i32) -> bool {
        if button_id >= 0 && (button_id as usize) < self.selectable_patterns.len() {
            self.set_selected(button_id);
            let pattern = self.selectable_patterns[button_id as usize].clone();
            self.setup_result_slot(player, &pattern);
            true
        } else {
            false
        }
    }

    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, _container: &ContainerKey) {
        let banner_stack = self.slot_item(player, BANNER_SLOT);
        let dye_stack = self.slot_item(player, DYE_SLOT);
        let pattern_stack = self.slot_item(player, PATTERN_SLOT);
        if banner_stack.is_empty() || dye_stack.is_empty() {
            self.clear_result(player);
            self.selectable_patterns = Vec::new();
            self.set_selected(PATTERN_NOT_SET);
            return;
        }
        let selected_pattern = self.get_selected_banner_pattern_index();
        let valid_pattern_index = self.is_valid_pattern_index(selected_pattern);
        let previous = std::mem::replace(
            &mut self.selectable_patterns,
            Self::compute_selectable_patterns(&player.registries(), &pattern_stack),
        );
        let pattern_to_display = if self.selectable_patterns.len() == 1 {
            self.set_selected(0);
            Some(self.selectable_patterns[0].clone())
        } else if !valid_pattern_index {
            self.set_selected(PATTERN_NOT_SET);
            None
        } else {
            let selected_value = &previous[selected_pattern as usize];
            match self
                .selectable_patterns
                .iter()
                .position(|p| p == selected_value)
            {
                Some(index) => {
                    self.set_selected(index as i32);
                    Some(selected_value.clone())
                }
                None => {
                    self.set_selected(PATTERN_NOT_SET);
                    None
                }
            }
        };
        match pattern_to_display {
            Some(pattern) => {
                let has_max_patterns = banner_stack
                    .get_or_default(&BANNER_PATTERNS, &BannerPatternLayers::default())
                    .layers
                    .len()
                    >= MAX_LAYERS;
                if has_max_patterns {
                    self.set_selected(PATTERN_NOT_SET);
                    self.clear_result(player);
                } else {
                    self.setup_result_slot(player, &pattern);
                }
            }
            None => self.clear_result(player),
        }
        self.broadcast_changes(player);
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        if !self
            .base
            .with_slot(player, slot_index, |s, e| s.has_item(e))
        {
            return ItemStack::empty();
        }
        let mut stack = self.slot_item(player, slot_index);
        let clicked = stack.copy();
        let moved = if slot_index == RESULT_SLOT {
            let moved =
                self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true);
            if moved {
                self.base.with_slot(player, slot_index, |s, e| {
                    s.on_quick_craft(e, &stack, &clicked)
                });
            }
            moved
        } else if slot_index != DYE_SLOT && slot_index != BANNER_SLOT && slot_index != PATTERN_SLOT
        {
            if is_banner_item(&stack) {
                self.move_item_stack_to(player, &mut stack, BANNER_SLOT, BANNER_SLOT + 1, false)
            } else if is_dye_item(&stack) {
                self.move_item_stack_to(player, &mut stack, DYE_SLOT, DYE_SLOT + 1, false)
            } else if is_pattern_item(&stack) {
                self.move_item_stack_to(player, &mut stack, PATTERN_SLOT, PATTERN_SLOT + 1, false)
            } else if (INV_SLOT_START..INV_SLOT_END).contains(&slot_index) {
                self.move_item_stack_to(
                    player,
                    &mut stack,
                    USE_ROW_SLOT_START,
                    USE_ROW_SLOT_END,
                    false,
                )
            } else if (USE_ROW_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index) {
                self.move_item_stack_to(player, &mut stack, INV_SLOT_START, INV_SLOT_END, false)
            } else {
                true
            }
        } else {
            self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, false)
        };
        if !moved {
            return ItemStack::empty();
        }
        finish_quick_move(self, player, slot_index, stack, &clicked)
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        if self.access.pos().is_some() {
            self.clear_container(player, INPUT_LOCAL);
        }
    }
}
