//! The value kinds a memory can hold ([`MemoryValue`]) and the typed view of them ([`MemoryData`]).

use super::NearestVisibleLivingEntities;
use super::walk_target::WalkTarget;
use crate::world::damagesource::DamageSource;
use crate::world::entity::EntityId;
use crate::world::entity::ai::behavior::position_tracker::PositionTracker;
use crate::world::level::pathfinder::Path as NavPath;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_protocol::codec::GlobalPos;
use std::sync::Arc;
use uuid::Uuid;

/// A shared `DamageSource` (`HURT_BY`); equality is identity (vanilla `DamageSource` has no `equals`).
#[derive(Clone, Debug)]
pub struct SharedDamageSource(pub Arc<DamageSource>);

impl PartialEq for SharedDamageSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// `com.mojang.datafixers.util.Unit` / `net.minecraft.util.Unit`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Unit;

/// `Void` (`MemoryModuleType.DUMMY`): never holds a value.
#[derive(Clone, PartialEq, Debug)]
pub enum Void {}

/// A reference to an entity stored in a memory (`LivingEntity`, `Player`, `Mob`, `ItemEntity`, ... in vanilla,
/// where the memory holds the object itself).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EntityRef(pub EntityId);

impl From<EntityId> for EntityRef {
    fn from(id: EntityId) -> Self {
        EntityRef(id)
    }
}

/// `SpearAttack.SpearStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpearStatus {
    Approach,
    Charging,
    Retreat,
}

/// A `Set<GlobalPos>` (vanilla `HashSet`; `GlobalPos.hashCode` depends on the identity hash of the dimension key, so
/// its iteration order is not reproducible either way: insertion order is used).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GlobalPosSet(Vec<GlobalPos>);

impl GlobalPosSet {
    pub fn new() -> Self {
        Self::default()
    }
    /// `add`: `true` if the element was not yet present.
    pub fn insert(&mut self, pos: GlobalPos) -> bool {
        if self.0.contains(&pos) {
            false
        } else {
            self.0.push(pos);
            true
        }
    }
    pub fn remove(&mut self, pos: &GlobalPos) -> bool {
        match self.0.iter().position(|p| p == pos) {
            Some(i) => {
                self.0.remove(i);
                true
            }
            None => false,
        }
    }
    pub fn contains(&self, pos: &GlobalPos) -> bool {
        self.0.contains(pos)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, GlobalPos> {
        self.0.iter()
    }
}

impl FromIterator<GlobalPos> for GlobalPosSet {
    fn from_iter<T: IntoIterator<Item = GlobalPos>>(iter: T) -> Self {
        let mut set = GlobalPosSet::new();
        for p in iter {
            set.insert(p);
        }
        set
    }
}

/// The dynamically typed content of a memory slot.
#[derive(Clone, Debug)]
pub enum MemoryValue {
    Bool(bool),
    Int(i32),
    Long(i64),
    Unit(Unit),
    GlobalPos(GlobalPos),
    GlobalPosList(Vec<GlobalPos>),
    GlobalPosSet(GlobalPosSet),
    BlockPos(BlockPos),
    Vec3(Vec3),
    Uuid(Uuid),
    UuidList(Vec<Uuid>),
    Entity(EntityRef),
    EntityList(Vec<EntityRef>),
    DamageSource(SharedDamageSource),
    WalkTarget(WalkTarget),
    PositionTracker(PositionTracker),
    Path(NavPath),
    NearestVisibleLivingEntities(NearestVisibleLivingEntities),
    SpearStatus(SpearStatus),
}

impl MemoryValue {
    /// `Brain.isEmptyCollection`: an empty `Collection` value clears the slot instead of being stored.
    pub fn is_empty_collection(&self) -> bool {
        match self {
            MemoryValue::GlobalPosList(v) => v.is_empty(),
            MemoryValue::GlobalPosSet(v) => v.is_empty(),
            MemoryValue::UuidList(v) => v.is_empty(),
            MemoryValue::EntityList(v) => v.is_empty(),
            _ => false,
        }
    }
}

/// `Objects.equals` semantics (identity for Java objects without `equals`: damage sources, paths, the visible
/// entities cache; structural for values, ids for entities).
impl PartialEq for MemoryValue {
    fn eq(&self, other: &Self) -> bool {
        use MemoryValue::*;
        match (self, other) {
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Long(a), Long(b)) => a == b,
            (Unit(_), Unit(_)) => true,
            (GlobalPos(a), GlobalPos(b)) => a == b,
            (GlobalPosList(a), GlobalPosList(b)) => a == b,
            (GlobalPosSet(a), GlobalPosSet(b)) => a == b,
            (BlockPos(a), BlockPos(b)) => a == b,
            (Vec3(a), Vec3(b)) => a == b,
            (Uuid(a), Uuid(b)) => a == b,
            (UuidList(a), UuidList(b)) => a == b,
            (Entity(a), Entity(b)) => a == b,
            (EntityList(a), EntityList(b)) => a == b,
            (DamageSource(a), DamageSource(b)) => a == b,
            (WalkTarget(a), WalkTarget(b)) => a == b,
            (PositionTracker(a), PositionTracker(b)) => a == b,
            (Path(a), Path(b)) => a == b,
            (NearestVisibleLivingEntities(a), NearestVisibleLivingEntities(b)) => a == b,
            (SpearStatus(a), SpearStatus(b)) => a == b,
            _ => false,
        }
    }
}

/// A Rust type usable as `U` of a `MemoryModuleType<U>`: maps to exactly one [`MemoryValue`] variant.
pub trait MemoryData: Clone + PartialEq + 'static {
    fn into_value(self) -> MemoryValue;
    fn from_value(value: &MemoryValue) -> Option<&Self>;
    fn from_value_mut(value: &mut MemoryValue) -> Option<&mut Self>;
}

macro_rules! memory_data {
    ($($ty:ty => $variant:ident;)*) => {$(
        impl MemoryData for $ty {
            fn into_value(self) -> MemoryValue {
                MemoryValue::$variant(self)
            }
            fn from_value(value: &MemoryValue) -> Option<&Self> {
                match value {
                    MemoryValue::$variant(v) => Some(v),
                    _ => None,
                }
            }
            fn from_value_mut(value: &mut MemoryValue) -> Option<&mut Self> {
                match value {
                    MemoryValue::$variant(v) => Some(v),
                    _ => None,
                }
            }
        }
    )*};
}

memory_data! {
    bool => Bool;
    i32 => Int;
    i64 => Long;
    Unit => Unit;
    GlobalPos => GlobalPos;
    Vec<GlobalPos> => GlobalPosList;
    GlobalPosSet => GlobalPosSet;
    BlockPos => BlockPos;
    Vec3 => Vec3;
    Uuid => Uuid;
    Vec<Uuid> => UuidList;
    EntityRef => Entity;
    Vec<EntityRef> => EntityList;
    SharedDamageSource => DamageSource;
    NavPath => Path;
    WalkTarget => WalkTarget;
    PositionTracker => PositionTracker;
    NearestVisibleLivingEntities => NearestVisibleLivingEntities;
    SpearStatus => SpearStatus;
}

impl MemoryData for Void {
    fn into_value(self) -> MemoryValue {
        match self {}
    }
    fn from_value(_: &MemoryValue) -> Option<&Self> {
        None
    }
    fn from_value_mut(_: &mut MemoryValue) -> Option<&mut Self> {
        None
    }
}
