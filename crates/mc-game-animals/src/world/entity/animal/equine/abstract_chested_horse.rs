//! `AbstractChestedHorse` (donkeys, mules, llamas): the chest flag and its inventory.

use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{Attribute, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::Container;
use mc_game_core::world::entity::animal::equine::abstract_horse::{
    self, AbstractHorse, add_behaviour_goals, is_tamed, make_mad,
};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::mob::interact::with_hand_stack;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt as _};
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_ID_CHEST: Boolean = AbstractChestedHorse);

/// `AbstractChestedHorse` (the virtual members).
pub trait AbstractChestedHorse: AbstractHorse {
    /// `playChestEquipsSound()`.
    fn play_chest_equips_sound(&mut self, level: &mut ServerLevel) {
        let pitch =
            (self.base_mut().random.next_float() - self.base_mut().random.next_float()) * 0.2 + 1.0;
        self.play_sound(level, SoundEvent::ENTITY_DONKEY_CHEST, 1.0, pitch);
    }
}

/// The chested horse's constructor part: `canGallop = false` and `babyDimensions`.
pub fn baby_dimensions(ty: mc_data::EntityType) -> EntityDimensions {
    let base = EntityDimensions::of_type(ty);
    let attachments = EntityAttachments::create_default(base.width, base.height).with_passenger(
        Vec3::new(0.0, f64::from(ty.data().height + 0.03125), -0.3125),
    );
    base.with_attachments(attachments).scale(0.5)
}

/// `hasChest()`.
pub fn has_chest(e: &(impl AbstractChestedHorse + ?Sized)) -> bool {
    e.base().entity_data.get(DATA_ID_CHEST)
}

/// `setChest(flag)`.
pub fn set_chest(e: &mut (impl AbstractChestedHorse + ?Sized), flag: bool) {
    e.base_mut().entity_data.set(DATA_ID_CHEST, flag);
}

/// `randomizeAttributes(random)` of `AbstractChestedHorse`.
pub fn randomize_attributes(
    e: &mut (impl AbstractChestedHorse + ?Sized),
    random: &mut dyn RandomSource,
) {
    let health = f64::from(abstract_horse::generate_max_health(|bound| {
        random.next_int_bounded(bound)
    }));
    if let Some(i) = e
        .living_mut()
        .and_then(|l| l.attributes.get_instance(Attribute::MAX_HEALTH))
    {
        i.set_base_value(health);
    }
}

/// `dropEquipment(level)` of `AbstractChestedHorse`.
pub fn drop_equipment(e: &mut (impl AbstractChestedHorse + ?Sized), level: &mut ServerLevel) {
    abstract_horse::drop_equipment(e, level);
    if has_chest(e) {
        mc_game_core::world::entity::item::spawn_at_location(
            level,
            e.as_dyn(),
            ItemStack::new(Item::CHEST),
            Vec3::ZERO,
        );
        set_chest(e, false);
    }
}

/// `addAdditionalSaveData(output)` of `AbstractChestedHorse`.
pub fn add_additional_save_data(e: &(impl AbstractChestedHorse + ?Sized), out: &mut CompoundTag) {
    abstract_horse::add_additional_save_data(e, out);
    out.put_boolean("ChestedHorse", has_chest(e));
    if has_chest(e) {
        let inventory = &e.horse().inventory;
        out.put(
            "Items",
            mc_game_core::world::container::save_slots(
                (0..inventory.get_container_size()).map(|i| (i, inventory.get_item(i))),
            ),
        );
    }
}

/// `readAdditionalSaveData(input)` of `AbstractChestedHorse`.
pub fn read_additional_save_data(
    e: &mut (impl AbstractChestedHorse + ?Sized),
    level: Option<&ServerLevel>,
    input: &CompoundTag,
) {
    abstract_horse::read_additional_save_data(e, level, input);
    let chest = input.get_boolean_or("ChestedHorse", false);
    set_chest(e, chest);
    e.create_inventory();
    if has_chest(e) {
        let size = e.horse().inventory.get_container_size();
        for (slot, stack) in
            mc_game_core::world::container::load_slots(input.get_list_or_empty("Items"))
        {
            if slot < size {
                e.horse_mut().inventory.set_item(slot, stack);
            }
        }
    }
}

/// `mobInteract(player, hand)` of `AbstractChestedHorse`.
pub fn mob_interact(
    e: &mut (impl AbstractChestedHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let should_open_inventory = !e.is_baby() && is_tamed(e) && player.is_shift_key_down();
    if !e.base().is_vehicle()
        && !should_open_inventory
        && (!e.is_baby() || !abstract_horse::is_holding(player, Item::GOLDEN_DANDELION))
    {
        let item_stack = player.get_item_in_hand(hand);
        if !item_stack.is_empty() {
            if e.is_food(&item_stack) {
                return abstract_horse::fed_food(e, level, player, hand);
            }
            if !is_tamed(e) {
                make_mad(e, level);
                return InteractionResult::SUCCESS;
            }
            if !has_chest(e) && item_stack.is(Item::CHEST) {
                equip_chest(e, level, player, hand);
                return InteractionResult::SUCCESS;
            }
        }
        abstract_horse::mob_interact(e, level, player, hand)
    } else {
        abstract_horse::mob_interact(e, level, player, hand)
    }
}

/// `equipChest(player, itemStack)`.
fn equip_chest(
    e: &mut (impl AbstractChestedHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) {
    set_chest(e, true);
    e.play_chest_equips_sound(level);
    with_hand_stack(player, hand, |s, p| {
        s.consume(1, p.has_infinite_materials())
    });
    e.create_inventory();
}

/// `getInventoryColumns()` of the donkey/mule: 5 with a chest.
pub fn chested_columns(e: &(impl AbstractChestedHorse + ?Sized)) -> i32 {
    if has_chest(e) { 5 } else { 0 }
}

/// `addBehaviourGoals()` is `AbstractHorse`'s for donkeys and mules.
pub fn add_default_behaviour_goals<E: AbstractChestedHorse + Sized>(e: &mut E) {
    add_behaviour_goals(e);
}

/// The slot-499 `SlotAccess` of `getSlot(slot)`: the chest item.
pub fn chest_slot_get(e: &(impl AbstractChestedHorse + ?Sized)) -> ItemStack {
    if has_chest(e) {
        ItemStack::new(Item::CHEST)
    } else {
        ItemStack::empty()
    }
}

/// `SlotAccess.set(stack)` of the chest slot.
pub fn chest_slot_set(e: &mut (impl AbstractChestedHorse + ?Sized), stack: &ItemStack) -> bool {
    if stack.is_empty() {
        if has_chest(e) {
            set_chest(e, false);
            e.create_inventory();
        }
        true
    } else if stack.is(Item::CHEST) {
        if !has_chest(e) {
            set_chest(e, true);
            e.create_inventory();
        }
        true
    } else {
        false
    }
}
