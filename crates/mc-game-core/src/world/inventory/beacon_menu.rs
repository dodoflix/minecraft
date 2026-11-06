//! `BeaconMenu`: one payment slot over the player's inventory, the beacon block entity's `ContainerData`
//! (levels, primary, secondary effect) as data slots, and `ServerboundSetBeaconPacket` handling.

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, MenuBase, MenuPlayer, Prediction,
    Slot, SlotBehavior, SlotEnv, still_valid,
};
use crate::world::container::{Container, SimpleContainer};
use crate::world::item::ItemStack;
use mc_core::ident::Identifier;
use mc_data::{Block, MenuType, MobEffect};
use std::any::Any;

const PAYMENT_SLOT: usize = 0;
const PAYMENT_LOCAL: usize = 0;

/// `BeaconBlockEntity.BEACON_EFFECTS`: the effects offered per pyramid level.
pub const BEACON_EFFECTS: [&[MobEffect]; 4] = [
    &[MobEffect::SPEED, MobEffect::HASTE],
    &[MobEffect::RESISTANCE, MobEffect::JUMP_BOOST],
    &[MobEffect::STRENGTH],
    &[MobEffect::REGENERATION],
];

/// `BeaconBlockEntity.getRequiredLevelsFor(effect)`.
fn get_required_levels_for(effect: Option<MobEffect>) -> i32 {
    match effect {
        None => 0,
        Some(e) => BEACON_EFFECTS
            .iter()
            .position(|level| level.contains(&e))
            .map_or(i32::MAX, |i| i as i32 + 1),
    }
}

/// `BeaconBlockEntity.validateEffects(primary, secondary, levels)`. (Vanilla throws a `NullPointerException` for a
/// secondary effect without a primary one; that is rejected here like any other invalid selection.)
pub fn validate_effects(
    primary: Option<MobEffect>,
    secondary: Option<MobEffect>,
    levels: i32,
) -> bool {
    if secondary.is_some() && levels < 4 {
        return false;
    }
    let primary_level = get_required_levels_for(primary);
    let secondary_level = get_required_levels_for(secondary);
    if primary_level > levels || secondary_level > levels {
        return false;
    }
    primary_level < 4
        && (secondary_level == 0
            || secondary_level >= 4
            || (primary.is_some() && primary == secondary))
}

/// `BeaconMenu.encodeEffect`.
pub fn encode_effect(effect: Option<MobEffect>) -> i32 {
    effect.map_or(0, |e| e.id() as i32 + 1)
}

/// `BeaconMenu.decodeEffect`.
pub fn decode_effect(id: i32) -> Option<MobEffect> {
    if id <= 0 {
        None
    } else {
        MobEffect::from_id(id as u32 - 1)
    }
}

/// `item.is(ItemTags.BEACON_PAYMENT_ITEMS)` against the level's tags.
fn is_beacon_payment(player: &dyn MenuPlayer, stack: &ItemStack) -> bool {
    let Some(tag) = Identifier::try_parse("minecraft:beacon_payment_items") else {
        return false;
    };
    player
        .level_ref()
        .and_then(|l| l.registries.registry("minecraft:item"))
        .is_some_and(|r| r.is_in_tag(stack.item().id(), &tag))
}

/// The anonymous `SimpleContainer(1)` of the menu: `getMaxStackSize() == 1`.
struct BeaconPaymentContainer(SimpleContainer);

impl Container for BeaconPaymentContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.0.get_container_size()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.0.get_item(slot)
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.0.get_item_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        self.0.remove_item(slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.0.remove_item_no_update(slot)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        stack.limit_size(1);
        self.0.set_item(slot, stack);
    }
    fn clear_content(&mut self) {
        self.0.clear_content();
    }
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn get_max_stack_size(&self) -> i32 {
        1
    }
    fn can_place_item(&self, _slot: usize, _stack: &ItemStack) -> bool {
        // needs the level's tags: only reachable through `PaymentSlot.mayPlace` here
        true
    }
}

/// `BeaconMenu.PaymentSlot`.
struct PaymentSlot;

impl SlotBehavior for PaymentSlot {
    fn may_place(&self, _slot: &Slot, env: &mut SlotEnv, stack: &ItemStack) -> bool {
        is_beacon_payment(&*env.player, stack)
    }
    fn get_max_stack_size(&self, _slot: &Slot, _env: &mut SlotEnv) -> i32 {
        1
    }
}

pub struct BeaconMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
}

impl BeaconMenu {
    /// `BeaconMenu(containerId, inventory, beaconData, access)`: `beaconData` is the `dataAccess` of the beacon block
    /// entity at the access position.
    pub fn new(container_id: i32, access: ContainerLevelAccess) -> Self {
        let mut base = MenuBase::new(Some(MenuType::BEACON), container_id);
        let local = base.add_local(Box::new(BeaconPaymentContainer(SimpleContainer::new(1))));
        debug_assert_eq!(local, PAYMENT_LOCAL);
        base.add_slot(Slot::with_behavior(
            ContainerKey::Local(PAYMENT_LOCAL),
            0,
            136,
            110,
            Box::new(PaymentSlot),
        ));
        let pos = access
            .pos()
            .expect("a beacon menu needs the beacon position");
        base.add_block_entity_data_slots(pos, 3);
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 36, 137);
        Self { base, access }
    }

    fn data(&self, player: &dyn MenuPlayer, id: usize) -> i32 {
        use crate::world::level::BlockGetter;
        let pos = self.access.pos();
        pos.and_then(|p| {
            player
                .level_ref()?
                .get_block_entity(p)?
                .container_data()
                .map(|d| d.get(id))
        })
        .unwrap_or(0)
    }

    fn set_data_value(&self, player: &mut dyn MenuPlayer, id: usize, value: i32) {
        if let (Some(pos), Some(level)) = (self.access.pos(), player.level()) {
            if let Some(d) = level
                .get_block_entity_immediate(pos)
                .and_then(|be| be.container_data_mut())
            {
                d.set(id, value);
            }
        }
    }

    /// `getLevels()`.
    pub fn get_levels(&self, player: &dyn MenuPlayer) -> i32 {
        self.data(player, 0)
    }

    pub fn get_primary_effect(&self, player: &dyn MenuPlayer) -> Option<MobEffect> {
        decode_effect(self.data(player, 1))
    }

    pub fn get_secondary_effect(&self, player: &dyn MenuPlayer) -> Option<MobEffect> {
        decode_effect(self.data(player, 2))
    }

    /// `hasPayment()`.
    pub fn has_payment(&self) -> bool {
        !self.base.locals[PAYMENT_LOCAL].get_item(0).is_empty()
    }

    /// `updateEffects(primary, secondary)`: `false` = invalid selection (the listener disconnects the player).
    pub fn update_effects(
        &mut self,
        player: &mut dyn MenuPlayer,
        primary: Option<MobEffect>,
        secondary: Option<MobEffect>,
    ) -> bool {
        if !self.has_payment() {
            return false;
        }
        let levels = self.get_levels(player);
        if !validate_effects(primary, secondary, levels) {
            return false;
        }
        self.set_data_value(player, 1, encode_effect(primary));
        self.set_data_value(player, 2, encode_effect(secondary));
        self.base.locals[PAYMENT_LOCAL].remove_item(0, 1);
        if let (Some(pos), Some(level)) = (self.access.pos(), player.level()) {
            level.block_entity_changed(pos);
        }
        true
    }
}

impl AbstractContainerMenu for BeaconMenu {
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

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        if player.is_server_player() {
            let stack = self.base.locals[PAYMENT_LOCAL].remove_item(0, 1);
            if !stack.is_empty() {
                player.drop(stack, false, Prediction::ServerOnly);
            }
        }
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        still_valid(&self.access, player, Block::BEACON)
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let mut clicked = ItemStack::empty();
        if !self
            .base
            .with_slot(player, slot_index, |s, e| s.has_item(e))
        {
            return clicked;
        }
        let mut stack = self
            .base
            .with_slot(player, slot_index, |s, e| s.get_item(e));
        clicked = stack.copy();
        let payment_empty = !self
            .base
            .with_slot(player, PAYMENT_SLOT, |s, e| s.has_item(e));
        let may_place = self
            .base
            .with_slot(player, PAYMENT_SLOT, |s, e| s.may_place(e, &stack));
        let moved = if slot_index == 0 {
            let moved = self.move_item_stack_to(player, &mut stack, 1, 37, true);
            if moved {
                self.base
                    .with_slot(player, 0, |s, e| s.on_quick_craft(e, &stack, &clicked));
            }
            moved
        } else if payment_empty && may_place && stack.count() == 1 {
            self.move_item_stack_to(player, &mut stack, 0, 1, false)
        } else if (1..28).contains(&slot_index) {
            self.move_item_stack_to(player, &mut stack, 28, 37, false)
        } else if (28..37).contains(&slot_index) {
            self.move_item_stack_to(player, &mut stack, 1, 28, false)
        } else {
            self.move_item_stack_to(player, &mut stack, 1, 37, false)
        };
        if !moved {
            return ItemStack::empty();
        }
        if stack.is_empty() {
            self.base.with_slot(player, slot_index, |s, e| {
                s.set_by_player_with(e, ItemStack::empty(), &clicked)
            });
        } else {
            let rest = stack.clone();
            self.base.with_slot(player, slot_index, |s, e| {
                s.with_item_mut(e, |t| *t = rest);
                s.set_changed(e);
            });
        }
        if stack.count() == clicked.count() {
            return ItemStack::empty();
        }
        self.base
            .with_slot(player, slot_index, |s, e| s.on_take(e, &stack));
        self.flush_container_events(player);
        clicked
    }
}
