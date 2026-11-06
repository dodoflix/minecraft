//! `EquipmentSlot` and `EntityEquipment` (vanilla 26.3 `net.minecraft.world.entity`).
//!
//! `EntityEquipment` is an `EnumMap<EquipmentSlot, ItemStack>`; absent entries read as empty. A player's main hand is
//! NOT stored here: vanilla's `PlayerEquipment` redirects `MAINHAND` to the inventory's selected slot, which in Rust is
//! done by `Inventory::equipment_get/equipment_set` (the inventory owns the player's `EntityEquipment`).

use crate::world::item::ItemStack;
use crate::world::item::{ItemStackExt, load_item_stack};
use mc_nbt::{CompoundTag, Tag};

/// `EquipmentSlot.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EquipmentSlotType {
    Hand,
    HumanoidArmor,
    AnimalArmor,
    Saddle,
}

/// `EquipmentSlot`; the discriminant is the Java ordinal (`EnumMap` order), [`EquipmentSlot::get_id`] the wire/filter id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

impl EquipmentSlot {
    /// From the packet-level slot enum used by the `equippable` component.
    pub fn from_protocol(slot: mc_protocol::packets::play::EquipmentSlot) -> EquipmentSlot {
        use mc_protocol::packets::play::EquipmentSlot as P;
        match slot {
            P::MainHand => Self::MainHand,
            P::OffHand => Self::OffHand,
            P::Feet => Self::Feet,
            P::Legs => Self::Legs,
            P::Chest => Self::Chest,
            P::Head => Self::Head,
            P::Body => Self::Body,
            P::Saddle => Self::Saddle,
        }
    }

    /// `EquipmentSlot.VALUES`.
    pub const VALUES: [EquipmentSlot; 8] = [
        EquipmentSlot::MainHand,
        EquipmentSlot::OffHand,
        EquipmentSlot::Feet,
        EquipmentSlot::Legs,
        EquipmentSlot::Chest,
        EquipmentSlot::Head,
        EquipmentSlot::Body,
        EquipmentSlot::Saddle,
    ];

    pub fn to_protocol(self) -> mc_protocol::packets::play::EquipmentSlot {
        use mc_protocol::packets::play::EquipmentSlot as P;
        match self {
            Self::MainHand => P::MainHand,
            Self::OffHand => P::OffHand,
            Self::Feet => P::Feet,
            Self::Legs => P::Legs,
            Self::Chest => P::Chest,
            Self::Head => P::Head,
            Self::Body => P::Body,
            Self::Saddle => P::Saddle,
        }
    }

    pub fn get_type(self) -> EquipmentSlotType {
        match self {
            Self::MainHand | Self::OffHand => EquipmentSlotType::Hand,
            Self::Feet | Self::Legs | Self::Chest | Self::Head => EquipmentSlotType::HumanoidArmor,
            Self::Body => EquipmentSlotType::AnimalArmor,
            Self::Saddle => EquipmentSlotType::Saddle,
        }
    }

    /// `getIndex()`.
    pub fn get_index(self) -> i32 {
        match self {
            Self::MainHand | Self::Feet | Self::Body | Self::Saddle => 0,
            Self::OffHand | Self::Legs => 1,
            Self::Chest => 2,
            Self::Head => 3,
        }
    }

    /// `getIndex(base)`.
    pub fn get_index_from(self, base: i32) -> i32 {
        base + self.get_index()
    }

    /// `countLimit` (0 = `NO_COUNT_LIMIT`).
    fn count_limit(self) -> i32 {
        match self {
            Self::MainHand | Self::OffHand => 0,
            _ => 1,
        }
    }

    /// `limit(toEquip)`: splits off at most `countLimit` items.
    pub fn limit(self, to_equip: &mut ItemStack) -> ItemStack {
        let limit = self.count_limit();
        if limit > 0 {
            to_equip.split(limit)
        } else {
            std::mem::take(to_equip)
        }
    }

    /// `getId()` (the wire id of `EquipmentSlot.STREAM_CODEC`).
    pub fn get_id(self) -> i32 {
        match self {
            Self::MainHand => 0,
            Self::OffHand => 5,
            Self::Feet => 1,
            Self::Legs => 2,
            Self::Chest => 3,
            Self::Head => 4,
            Self::Body => 6,
            Self::Saddle => 7,
        }
    }

    pub fn from_id(id: i32) -> Option<EquipmentSlot> {
        Self::VALUES.into_iter().find(|s| s.get_id() == id)
    }

    /// `getName()` = `getSerializedName()`.
    pub fn get_name(self) -> &'static str {
        match self {
            Self::MainHand => "mainhand",
            Self::OffHand => "offhand",
            Self::Feet => "feet",
            Self::Legs => "legs",
            Self::Chest => "chest",
            Self::Head => "head",
            Self::Body => "body",
            Self::Saddle => "saddle",
        }
    }

    /// `CODEC.byName`.
    pub fn by_name(name: &str) -> Option<EquipmentSlot> {
        Self::VALUES.into_iter().find(|s| s.get_name() == name)
    }

    pub fn is_armor(self) -> bool {
        matches!(
            self.get_type(),
            EquipmentSlotType::HumanoidArmor | EquipmentSlotType::AnimalArmor
        )
    }

    pub fn can_increase_experience(self) -> bool {
        self.get_type() != EquipmentSlotType::Saddle
    }
}

/// `EntityEquipment`.
#[derive(Clone, Debug, Default)]
pub struct EntityEquipment {
    items: [ItemStack; 8],
}

impl EntityEquipment {
    pub fn new() -> Self {
        Self::default()
    }

    /// `set(slot, stack)`: returns the previous stack.
    pub fn set(&mut self, slot: EquipmentSlot, stack: ItemStack) -> ItemStack {
        std::mem::replace(&mut self.items[slot as usize], stack)
    }

    pub fn get(&self, slot: EquipmentSlot) -> &ItemStack {
        &self.items[slot as usize]
    }

    /// In-place access (vanilla hands out the stored `ItemStack` object and callers mutate it).
    pub fn get_mut(&mut self, slot: EquipmentSlot) -> &mut ItemStack {
        &mut self.items[slot as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }

    /// `setAll(other)`.
    pub fn set_all(&mut self, other: &EntityEquipment) {
        self.items = other.items.clone();
    }

    pub fn clear(&mut self) {
        self.items.fill(ItemStack::empty());
    }

    /// `EntityEquipment.CODEC` (`unboundedMap(EquipmentSlot.CODEC, ItemStack.CODEC)` minus empty stacks), as the
    /// compound stored under `equipment`.
    pub fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        for slot in EquipmentSlot::VALUES {
            let stack = self.get(slot);
            if !stack.is_empty() {
                let mut item = CompoundTag::new();
                stack.save_into(&mut item);
                tag.put(slot.get_name(), Tag::Compound(item));
            }
        }
        tag
    }

    /// Decoding side of `CODEC`: unknown slot names and undecodable stacks are skipped.
    pub fn load(tag: &CompoundTag) -> EntityEquipment {
        let mut eq = EntityEquipment::new();
        for (name, value) in tag.iter() {
            if let (Some(slot), Some(item)) = (
                EquipmentSlot::by_name(name),
                value.as_compound().and_then(load_item_stack),
            ) {
                eq.set(slot, item);
            }
        }
        eq
    }
}
