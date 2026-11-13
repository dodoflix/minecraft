//! `SynchedEntityData` / `EntityDataAccessor` / `EntityDataSerializers`.
//!
//! Accessor ids are fixed by vanilla's class chain order and come from the extracted table
//! (`mc_data::misc::ENTITY_DATA_ACCESSORS`): `define_accessor!` looks the id up at compile time and checks the
//! serializer, so a wrong class/field/serializer is a build error. Values are `mc_protocol`'s [`EntityDataValue`]
//! (the wire form); an accessor is typed by a serializer marker (`serializer::Byte`, ...).

use mc_protocol::packets::play::{DataValue, EntityDataValue};
use std::marker::PhantomData;

/// A serializer of `EntityDataSerializers`: the Rust value type and its [`EntityDataValue`] variant.
pub trait Serializer: 'static {
    type Value: Clone + PartialEq;
    /// `EntityDataSerializers` constant name as in the extracted table.
    const NAME: &'static str;
    fn wrap(value: Self::Value) -> EntityDataValue;
    fn unwrap(value: &EntityDataValue) -> Option<&Self::Value>;
}

macro_rules! serializers {
    ($($marker:ident($ty:ty) = $name:literal;)*) => {
        /// Markers for every `EntityDataSerializers` constant.
        pub mod serializer {
            use super::*;
            $(
                pub struct $marker;
                impl Serializer for $marker {
                    type Value = $ty;
                    const NAME: &'static str = $name;
                    fn wrap(value: $ty) -> EntityDataValue { EntityDataValue::$marker(value) }
                    fn unwrap(value: &EntityDataValue) -> Option<&$ty> {
                        match value { EntityDataValue::$marker(v) => Some(v), _ => None }
                    }
                }
            )*
        }
    };
}

serializers! {
    Byte(i8) = "BYTE";
    ItemStack(mc_protocol::components::ItemStack) = "ITEM_STACK";
    ResolvableProfile(mc_protocol::packets::play::ResolvableProfile) = "RESOLVABLE_PROFILE";
    Int(i32) = "INT";
    Long(i64) = "LONG";
    Float(f32) = "FLOAT";
    String(std::string::String) = "STRING";
    Component(mc_text::Component) = "COMPONENT";
    OptionalComponent(Option<mc_text::Component>) = "OPTIONAL_COMPONENT";
    Boolean(bool) = "BOOLEAN";
    Rotations(mc_protocol::packets::play::Rotations) = "ROTATIONS";
    BlockPos(mc_core::pos::BlockPos) = "BLOCK_POS";
    OptionalBlockPos(Option<mc_core::pos::BlockPos>) = "OPTIONAL_BLOCK_POS";
    Direction(mc_core::direction::Direction) = "DIRECTION";
    OptionalLivingEntityReference(Option<uuid::Uuid>) = "OPTIONAL_LIVING_ENTITY_REFERENCE";
    BlockState(mc_data::block::BlockState) = "BLOCK_STATE";
    OptionalBlockState(Option<mc_data::block::BlockState>) = "OPTIONAL_BLOCK_STATE";
    Particle(mc_protocol::packets::play::ParticleOptions) = "PARTICLE";
    Particles(Vec<mc_protocol::packets::play::ParticleOptions>) = "PARTICLES";
    VillagerData(mc_protocol::packets::play::VillagerData) = "VILLAGER_DATA";
    OptionalUnsignedInt(Option<i32>) = "OPTIONAL_UNSIGNED_INT";
    Pose(mc_data::enums::Pose) = "POSE";
    CatVariant(i32) = "CAT_VARIANT";
    CatSoundVariant(i32) = "CAT_SOUND_VARIANT";
    CowVariant(i32) = "COW_VARIANT";
    CowSoundVariant(i32) = "COW_SOUND_VARIANT";
    WolfVariant(i32) = "WOLF_VARIANT";
    WolfSoundVariant(i32) = "WOLF_SOUND_VARIANT";
    FrogVariant(i32) = "FROG_VARIANT";
    PigVariant(i32) = "PIG_VARIANT";
    PigSoundVariant(i32) = "PIG_SOUND_VARIANT";
    ChickenVariant(i32) = "CHICKEN_VARIANT";
    ChickenSoundVariant(i32) = "CHICKEN_SOUND_VARIANT";
    ZombieNautilusVariant(i32) = "ZOMBIE_NAUTILUS_VARIANT";
    OptionalGlobalPos(Option<mc_protocol::codec::GlobalPos>) = "OPTIONAL_GLOBAL_POS";
    PaintingVariant(mc_protocol::codec::Holder<mc_protocol::packets::play::PaintingVariant>) = "PAINTING_VARIANT";
    SnifferState(mc_protocol::packets::play::SnifferState) = "SNIFFER_STATE";
    ArmadilloState(mc_protocol::packets::play::ArmadilloState) = "ARMADILLO_STATE";
    CopperGolemState(mc_protocol::packets::play::CopperGolemState) = "COPPER_GOLEM_STATE";
    WeatheringCopperState(mc_protocol::packets::play::WeatheringCopperState) = "WEATHERING_COPPER_STATE";
    Vector3([f32; 3]) = "VECTOR3";
    Quaternion([f32; 4]) = "QUATERNION";
    HumanoidArm(mc_protocol::packets::common::HumanoidArm) = "HUMANOID_ARM";
    DyeColor(mc_protocol::packets::play::DyeColor) = "DYE_COLOR";
}

/// `EntityDataAccessor<T>`: the id of a data slot (+ the serializer marker).
pub struct EntityDataAccessor<S: Serializer> {
    id: u8,
    _serializer: PhantomData<S>,
}

impl<S: Serializer> Clone for EntityDataAccessor<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: Serializer> Copy for EntityDataAccessor<S> {}
impl<S: Serializer> std::fmt::Debug for EntityDataAccessor<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntityDataAccessor<{}>({})", S::NAME, self.id)
    }
}

impl<S: Serializer> EntityDataAccessor<S> {
    pub const fn new(id: u8) -> Self {
        Self {
            id,
            _serializer: PhantomData,
        }
    }

    pub const fn id(&self) -> u8 {
        self.id
    }
}

impl<S: Serializer, T: Serializer> PartialEq<EntityDataAccessor<T>> for EntityDataAccessor<S> {
    fn eq(&self, other: &EntityDataAccessor<T>) -> bool {
        self.id == other.id && S::NAME == T::NAME
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Compile-time lookup in the extracted accessor table; panics (build error in const context) if the field is
/// unknown or its serializer isn't `serializer_name`.
pub const fn accessor_id(class: &str, field: &str, serializer_name: &str) -> u8 {
    let table = &mc_data::misc::ENTITY_DATA_ACCESSORS;
    let mut i = 0;
    while i < table.len() {
        if str_eq(table[i].class, class) && str_eq(table[i].field, field) {
            assert!(
                str_eq(table[i].serializer, serializer_name),
                "entity data accessor has a different serializer"
            );
            return table[i].id;
        }
        i += 1;
    }
    panic!("unknown entity data accessor");
}

/// `static final EntityDataAccessor<T> NAME = SynchedEntityData.defineId(Class.class, SERIALIZER)`:
/// `define_accessor!(pub DATA_BABY_ID: Boolean = AgeableMob);` (class = Java simple class name, field = the constant name).
#[macro_export]
macro_rules! define_accessor {
    ($vis:vis $name:ident : $ser:ident = $class:ident) => {
        $vis const $name: $crate::world::entity::synched_data::EntityDataAccessor<$crate::world::entity::synched_data::serializer::$ser> =
            $crate::world::entity::synched_data::EntityDataAccessor::new($crate::world::entity::synched_data::accessor_id(
                stringify!($class),
                stringify!($name),
                <$crate::world::entity::synched_data::serializer::$ser as $crate::world::entity::synched_data::Serializer>::NAME,
            ));
    };
}

/// `Objects.equals(old, new)` of `SynchedEntityData`: floats (also those of a `Vector3f` and a `Quaternionf`) are equal
/// when their `Float.floatToIntBits` are, so `-0.0` differs from `0.0` and a NaN equals every NaN.
fn same_value(a: &EntityDataValue, b: &EntityDataValue) -> bool {
    fn bits(f: f32) -> u32 {
        if f.is_nan() { 0x7fc0_0000 } else { f.to_bits() }
    }
    fn same(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(a, b)| bits(*a) == bits(*b))
    }
    match (a, b) {
        (EntityDataValue::Float(a), EntityDataValue::Float(b)) => bits(*a) == bits(*b),
        (EntityDataValue::Vector3(a), EntityDataValue::Vector3(b)) => same(a, b),
        (EntityDataValue::Quaternion(a), EntityDataValue::Quaternion(b)) => same(a, b),
        _ => a == b,
    }
}

struct DataItem {
    value: EntityDataValue,
    initial_value: EntityDataValue,
    dirty: bool,
    /// Set by a `set` of a non-empty `ItemStack`: vanilla compares stacks by identity, so the new object is never the
    /// default one even when both stacks are equal.
    replaced_object: bool,
}

impl DataItem {
    fn is_set_to_default(&self) -> bool {
        !self.replaced_object && same_value(&self.initial_value, &self.value)
    }
}

/// `SynchedEntityData.Builder`.
#[derive(Default)]
pub struct SynchedEntityDataBuilder {
    items: Vec<Option<DataItem>>,
}

impl SynchedEntityDataBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// `define(accessor, value)`; panics on duplicate ids like vanilla's `IllegalArgumentException`.
    pub fn define<S: Serializer>(
        &mut self,
        accessor: EntityDataAccessor<S>,
        value: S::Value,
    ) -> &mut Self {
        let id = accessor.id as usize;
        if self.items.len() <= id {
            self.items.resize_with(id + 1, || None);
        }
        assert!(self.items[id].is_none(), "Duplicate id value for {id}!");
        let value = S::wrap(value);
        self.items[id] = Some(DataItem {
            initial_value: value.clone(),
            value,
            dirty: false,
            replaced_object: false,
        });
        self
    }

    /// `build()`; panics if an id below the highest one was never defined ("has not defined synched data value").
    pub fn build(self) -> SynchedEntityData {
        let items = self
            .items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                item.unwrap_or_else(|| panic!("Entity has not defined synched data value {i}"))
            })
            .collect();
        SynchedEntityData {
            items,
            is_dirty: false,
        }
    }
}

/// `SynchedEntityData`. `set` reports whether the value changed; the owner runs vanilla's
/// `onSyncedDataUpdated(accessor)` reaction (see `EntityExt::set_data`).
pub struct SynchedEntityData {
    items: Vec<DataItem>,
    is_dirty: bool,
}

impl SynchedEntityData {
    pub fn get<S: Serializer>(&self, accessor: EntityDataAccessor<S>) -> S::Value {
        match S::unwrap(&self.items[accessor.id as usize].value) {
            Some(v) => v.clone(),
            None => panic!("entity data id {} does not hold a {}", accessor.id, S::NAME),
        }
    }

    pub fn get_ref<S: Serializer>(&self, accessor: EntityDataAccessor<S>) -> &S::Value {
        match S::unwrap(&self.items[accessor.id as usize].value) {
            Some(v) => v,
            None => panic!("entity data id {} does not hold a {}", accessor.id, S::NAME),
        }
    }

    /// `set(accessor, value)`; returns whether the value was changed (marked dirty). `ItemStack` has no `equals`, so
    /// vanilla's `ObjectUtils.notEqual` sees every new stack object as a change; only the shared empty stack equals itself.
    pub fn set<S: Serializer>(&mut self, accessor: EntityDataAccessor<S>, value: S::Value) -> bool {
        self.set_force(accessor, value, false)
    }

    /// `set(accessor, value, force)`.
    pub fn set_force<S: Serializer>(
        &mut self,
        accessor: EntityDataAccessor<S>,
        value: S::Value,
        force_dirty: bool,
    ) -> bool {
        let item = &mut self.items[accessor.id as usize];
        let value = S::wrap(value);
        let new_stack_object = matches!(&value, EntityDataValue::ItemStack(s) if !s.is_empty());
        if force_dirty || new_stack_object || !same_value(&value, &item.value) {
            item.value = value;
            item.dirty = true;
            item.replaced_object = new_stack_object;
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    /// Replaces the stored value without marking it dirty: vanilla code that mutates a stored object in place
    /// (`ItemEntity.merge`'s `fromStack.shrink`, `Inventory.add(itemEntity.getItem())`) is never noticed by
    /// `SynchedEntityData`.
    pub fn set_untracked<S: Serializer>(
        &mut self,
        accessor: EntityDataAccessor<S>,
        value: S::Value,
    ) {
        self.items[accessor.id as usize].value = S::wrap(value);
    }

    /// `define(accessor, value)` of an already built set: replaces both the value and the default it is compared with
    /// (a constructor whose `defineSynchedData` value depends on the entity's random, `ZombieVillager`'s profession).
    pub fn redefine_initial<S: Serializer>(
        &mut self,
        accessor: EntityDataAccessor<S>,
        value: S::Value,
    ) {
        let item = &mut self.items[accessor.id as usize];
        let value = S::wrap(value);
        item.initial_value = value.clone();
        item.value = value;
        item.dirty = false;
        item.replaced_object = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    /// `packDirty()`: the dirty values (accessor id order), clearing the flags; `None` if nothing is dirty.
    pub fn pack_dirty(&mut self) -> Option<Vec<DataValue>> {
        if !self.is_dirty {
            return None;
        }
        self.is_dirty = false;
        let mut result = Vec::new();
        for (id, item) in self.items.iter_mut().enumerate() {
            if item.dirty {
                item.dirty = false;
                result.push(DataValue {
                    id: id as u8,
                    value: item.value.clone(),
                });
            }
        }
        Some(result)
    }

    /// `getNonDefaultValues()`.
    pub fn get_non_default_values(&self) -> Option<Vec<DataValue>> {
        let result: Vec<DataValue> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| !i.is_set_to_default())
            .map(|(id, i)| DataValue {
                id: id as u8,
                value: i.value.clone(),
            })
            .collect();
        (!result.is_empty()).then_some(result)
    }

    /// `packAll` of vanilla's `ServerEntity.sendPairingData`/`ClientboundSetEntityDataPacket` for every item.
    pub fn pack_all(&self) -> Vec<DataValue> {
        self.items
            .iter()
            .enumerate()
            .map(|(id, i)| DataValue {
                id: id as u8,
                value: i.value.clone(),
            })
            .collect()
    }

    /// `assignValues(items)`: returns the accessor ids that were assigned (vanilla's `onSyncedDataUpdated` calls);
    /// `Err` with vanilla's "Invalid entity data item type" message on a serializer mismatch.
    pub fn assign_values(&mut self, items: &[DataValue]) -> Result<Vec<u8>, String> {
        let mut updated = Vec::with_capacity(items.len());
        for item in items {
            let Some(data_item) = self.items.get_mut(item.id as usize) else {
                return Err(format!("Invalid entity data item id {}", item.id));
            };
            if std::mem::discriminant(&data_item.value) != std::mem::discriminant(&item.value) {
                return Err(format!(
                    "Invalid entity data item type for field {}: old={:?}, new={:?}",
                    item.id, data_item.value, item.value
                ));
            }
            data_item.value = item.value.clone();
            updated.push(item.id);
        }
        Ok(updated)
    }

    /// Number of defined slots.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
