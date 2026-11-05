//! `MemoryModuleType<U>`: every vanilla memory type as a typed constant, in registry order
//! (`BuiltInRegistries.MEMORY_MODULE_TYPE`). Use them as `memory_module_type::HOME`, `&memory_module_type::WALK_TARGET`.

use super::value::{EntityRef, GlobalPosSet, MemoryData, SharedDamageSource, SpearStatus, Unit, Void};
use super::walk_target::WalkTarget;
use super::NearestVisibleLivingEntities;
use crate::world::entity::ai::behavior::position_tracker::PositionTracker;
use crate::world::level::pathfinder::Path;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_protocol::codec::GlobalPos;
use std::marker::PhantomData;
use uuid::Uuid;

/// How a memory is persisted (`MemoryModuleType.getCodec()`): `None` = not serializable (`canSerialize()` false).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryCodec {
    None,
    Bool,
    Int,
    Long,
    Unit,
    GlobalPos,
    BlockPos,
    Uuid,
    /// `Codec.list(GlobalPos.CODEC)`.
    GlobalPosList,
    /// `GlobalPos.CODEC.listOf().xmap(Sets::newHashSet, Lists::newArrayList)`.
    GlobalPosSet,
}

/// Registry entry of a memory type.
#[derive(Clone, Copy, Debug)]
pub struct MemoryTypeInfo {
    pub name: &'static str,
    pub codec: MemoryCodec,
}

/// Untyped `MemoryModuleType<?>`: the registry index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct MemoryTypeId(pub u16);

impl MemoryTypeId {
    pub fn info(self) -> &'static MemoryTypeInfo {
        &REGISTRY[self.0 as usize]
    }

    /// `toString()`: the registry key (`minecraft:home`).
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// `canSerialize()`.
    pub fn can_serialize(self) -> bool {
        self.info().codec != MemoryCodec::None
    }

    pub fn id(self) -> Identifier {
        Identifier::with_default_namespace(self.name()).expect("valid memory type name")
    }

    /// `BuiltInRegistries.MEMORY_MODULE_TYPE.getValue(id)`.
    pub fn by_id(id: &Identifier) -> Option<MemoryTypeId> {
        if id.namespace() != "minecraft" {
            return None;
        }
        REGISTRY.iter().position(|i| i.name == id.path()).map(|i| MemoryTypeId(i as u16))
    }
}

impl std::fmt::Display for MemoryTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "minecraft:{}", self.name())
    }
}

/// Typed `MemoryModuleType<U>`.
pub struct MemoryModuleType<U> {
    id: MemoryTypeId,
    _marker: PhantomData<fn() -> U>,
}

impl<U> Clone for MemoryModuleType<U> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<U> Copy for MemoryModuleType<U> {}
impl<U> PartialEq for MemoryModuleType<U> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<U> Eq for MemoryModuleType<U> {}
impl<U> std::fmt::Debug for MemoryModuleType<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MemoryModuleType({})", self.id)
    }
}

impl<U> MemoryModuleType<U> {
    const fn new(index: u16) -> Self {
        Self { id: MemoryTypeId(index), _marker: PhantomData }
    }

    /// The untyped id (`MemoryModuleType<?>`).
    pub const fn id(&self) -> MemoryTypeId {
        self.id
    }

    /// `canSerialize()`.
    pub fn can_serialize(&self) -> bool {
        self.id.can_serialize()
    }
}

impl<U> From<MemoryModuleType<U>> for MemoryTypeId {
    fn from(t: MemoryModuleType<U>) -> Self {
        t.id
    }
}
impl<U> From<&MemoryModuleType<U>> for MemoryTypeId {
    fn from(t: &MemoryModuleType<U>) -> Self {
        t.id
    }
}

macro_rules! memory_types {
    ($($ident:ident: $ty:ty = $name:literal, $codec:ident;)*) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[repr(u16)]
        enum Index { $($ident),* }
        $(pub const $ident: MemoryModuleType<$ty> = MemoryModuleType::new(Index::$ident as u16);)*
        /// `BuiltInRegistries.MEMORY_MODULE_TYPE` in registry order.
        pub static REGISTRY: &[MemoryTypeInfo] = &[$(MemoryTypeInfo { name: $name, codec: MemoryCodec::$codec }),*];
    };
}

memory_types! {
    DUMMY: Void = "dummy", None;
    HOME: GlobalPos = "home", GlobalPos;
    JOB_SITE: GlobalPos = "job_site", GlobalPos;
    POTENTIAL_JOB_SITE: GlobalPos = "potential_job_site", GlobalPos;
    MEETING_POINT: GlobalPos = "meeting_point", GlobalPos;
    SECONDARY_JOB_SITE: Vec<GlobalPos> = "secondary_job_site", None;
    NEAREST_LIVING_ENTITIES: Vec<EntityRef> = "mobs", None;
    NEAREST_VISIBLE_LIVING_ENTITIES: NearestVisibleLivingEntities = "visible_mobs", None;
    VISIBLE_VILLAGER_BABIES: Vec<EntityRef> = "visible_villager_babies", None;
    NEAREST_PLAYERS: Vec<EntityRef> = "nearest_players", None;
    NEAREST_VISIBLE_PLAYER: EntityRef = "nearest_visible_player", None;
    NEAREST_VISIBLE_ATTACKABLE_PLAYER: EntityRef = "nearest_visible_targetable_player", None;
    NEAREST_VISIBLE_ATTACKABLE_PLAYERS: Vec<EntityRef> = "nearest_visible_targetable_players", None;
    WALK_TARGET: WalkTarget = "walk_target", None;
    LOOK_TARGET: PositionTracker = "look_target", None;
    ATTACK_TARGET: EntityRef = "attack_target", None;
    ATTACK_COOLING_DOWN: bool = "attack_cooling_down", None;
    INTERACTION_TARGET: EntityRef = "interaction_target", None;
    BREED_TARGET: EntityRef = "breed_target", None;
    RIDE_TARGET: EntityRef = "ride_target", None;
    PATH: Path = "path", None;
    DOORS_TO_CLOSE: GlobalPosSet = "doors_to_close", None;
    NEAREST_BED: BlockPos = "nearest_bed", None;
    HURT_BY: SharedDamageSource = "hurt_by", None;
    HURT_BY_ENTITY: EntityRef = "hurt_by_entity", None;
    AVOID_TARGET: EntityRef = "avoid_target", None;
    NEAREST_HOSTILE: EntityRef = "nearest_hostile", None;
    NEAREST_ATTACKABLE: EntityRef = "nearest_attackable", None;
    HIDING_PLACE: GlobalPos = "hiding_place", None;
    HEARD_BELL_TIME: i64 = "heard_bell_time", None;
    CANT_REACH_WALK_TARGET_SINCE: i64 = "cant_reach_walk_target_since", None;
    GOLEM_DETECTED_RECENTLY: bool = "golem_detected_recently", Bool;
    DANGER_DETECTED_RECENTLY: bool = "danger_detected_recently", Bool;
    LAST_SLEPT: i64 = "last_slept", Long;
    LAST_WOKEN: i64 = "last_woken", Long;
    LAST_WORKED_AT_POI: i64 = "last_worked_at_poi", Long;
    NEAREST_VISIBLE_ADULT: EntityRef = "nearest_visible_adult", None;
    NEAREST_VISIBLE_WANTED_ITEM: EntityRef = "nearest_visible_wanted_item", None;
    NEAREST_VISIBLE_NEMESIS: EntityRef = "nearest_visible_nemesis", None;
    PLAY_DEAD_TICKS: i32 = "play_dead_ticks", Int;
    TEMPTING_PLAYER: EntityRef = "tempting_player", None;
    TEMPTATION_COOLDOWN_TICKS: i32 = "temptation_cooldown_ticks", Int;
    GAZE_COOLDOWN_TICKS: i32 = "gaze_cooldown_ticks", Int;
    LONG_JUMP_COOLDOWN_TICKS: i32 = "long_jump_cooling_down", Int;
    LONG_JUMP_MID_JUMP: bool = "long_jump_mid_jump", None;
    HAS_HUNTING_COOLDOWN: bool = "has_hunting_cooldown", Bool;
    RAM_COOLDOWN_TICKS: i32 = "ram_cooldown_ticks", Int;
    RAM_TARGET: Vec3 = "ram_target", None;
    IS_IN_WATER: Unit = "is_in_water", Unit;
    IS_PREGNANT: Unit = "is_pregnant", Unit;
    IS_PANICKING: bool = "is_panicking", Bool;
    UNREACHABLE_TONGUE_TARGETS: Vec<Uuid> = "unreachable_tongue_targets", None;
    VISITED_BLOCK_POSITIONS: GlobalPosSet = "visited_block_positions", GlobalPosSet;
    UNREACHABLE_TRANSPORT_BLOCK_POSITIONS: GlobalPosSet = "unreachable_transport_block_positions", GlobalPosSet;
    TRANSPORT_ITEMS_COOLDOWN_TICKS: i32 = "transport_items_cooldown_ticks", None;
    CHARGE_COOLDOWN_TICKS: i32 = "charge_cooldown_ticks", Int;
    ATTACK_TARGET_COOLDOWN: i32 = "attack_target_cooldown", Int;
    SPEAR_FLEEING_TIME: i32 = "spear_fleeing_time", None;
    SPEAR_FLEEING_POSITION: Vec3 = "spear_fleeing_position", None;
    SPEAR_CHARGE_POSITION: Vec3 = "spear_charge_position", None;
    SPEAR_ENGAGE_TIME: i32 = "spear_engage_time", None;
    SPEAR_STATUS: SpearStatus = "spear_status", None;
    ANGRY_AT: Uuid = "angry_at", Uuid;
    UNIVERSAL_ANGER: bool = "universal_anger", Bool;
    ADMIRING_ITEM: bool = "admiring_item", Bool;
    TIME_TRYING_TO_REACH_ADMIRE_ITEM: i32 = "time_trying_to_reach_admire_item", None;
    DISABLE_WALK_TO_ADMIRE_ITEM: bool = "disable_walk_to_admire_item", None;
    ADMIRING_DISABLED: bool = "admiring_disabled", Bool;
    HUNTED_RECENTLY: bool = "hunted_recently", Bool;
    CELEBRATE_LOCATION: BlockPos = "celebrate_location", None;
    DANCING: bool = "dancing", None;
    NEAREST_VISIBLE_HUNTABLE_HOGLIN: EntityRef = "nearest_visible_huntable_hoglin", None;
    NEAREST_VISIBLE_BABY_HOGLIN: EntityRef = "nearest_visible_baby_hoglin", None;
    NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD: EntityRef = "nearest_targetable_player_not_wearing_gold", None;
    NEARBY_ADULT_PIGLINS: Vec<EntityRef> = "nearby_adult_piglins", None;
    NEAREST_VISIBLE_ADULT_PIGLINS: Vec<EntityRef> = "nearest_visible_adult_piglins", None;
    NEAREST_VISIBLE_ADULT_HOGLINS: Vec<EntityRef> = "nearest_visible_adult_hoglins", None;
    NEAREST_VISIBLE_ADULT_PIGLIN: EntityRef = "nearest_visible_adult_piglin", None;
    NEAREST_VISIBLE_ZOMBIFIED: EntityRef = "nearest_visible_zombified", None;
    VISIBLE_ADULT_PIGLIN_COUNT: i32 = "visible_adult_piglin_count", None;
    VISIBLE_ADULT_HOGLIN_COUNT: i32 = "visible_adult_hoglin_count", None;
    NEAREST_PLAYER_HOLDING_WANTED_ITEM: EntityRef = "nearest_player_holding_wanted_item", None;
    ATE_RECENTLY: bool = "ate_recently", None;
    NEAREST_REPELLENT: BlockPos = "nearest_repellent", None;
    PACIFIED: bool = "pacified", None;
    ROAR_TARGET: EntityRef = "roar_target", None;
    DISTURBANCE_LOCATION: BlockPos = "disturbance_location", None;
    RECENT_PROJECTILE: Unit = "recent_projectile", Unit;
    IS_SNIFFING: Unit = "is_sniffing", Unit;
    IS_EMERGING: Unit = "is_emerging", Unit;
    ROAR_SOUND_DELAY: Unit = "roar_sound_delay", Unit;
    DIG_COOLDOWN: Unit = "dig_cooldown", Unit;
    ROAR_SOUND_COOLDOWN: Unit = "roar_sound_cooldown", Unit;
    SNIFF_COOLDOWN: Unit = "sniff_cooldown", Unit;
    TOUCH_COOLDOWN: Unit = "touch_cooldown", Unit;
    VIBRATION_COOLDOWN: Unit = "vibration_cooldown", Unit;
    SONIC_BOOM_COOLDOWN: Unit = "sonic_boom_cooldown", Unit;
    SONIC_BOOM_SOUND_COOLDOWN: Unit = "sonic_boom_sound_cooldown", Unit;
    SONIC_BOOM_SOUND_DELAY: Unit = "sonic_boom_sound_delay", Unit;
    LIKED_PLAYER: Uuid = "liked_player", Uuid;
    LIKED_NOTEBLOCK_POSITION: GlobalPos = "liked_noteblock", GlobalPos;
    LIKED_NOTEBLOCK_COOLDOWN_TICKS: i32 = "liked_noteblock_cooldown_ticks", Int;
    ITEM_PICKUP_COOLDOWN_TICKS: i32 = "item_pickup_cooldown_ticks", Int;
    SNIFFER_EXPLORED_POSITIONS: Vec<GlobalPos> = "sniffer_explored_positions", GlobalPosList;
    SNIFFER_SNIFFING_TARGET: BlockPos = "sniffer_sniffing_target", None;
    SNIFFER_DIGGING: bool = "sniffer_digging", None;
    SNIFFER_HAPPY: bool = "sniffer_happy", None;
    BREEZE_JUMP_COOLDOWN: Unit = "breeze_jump_cooldown", Unit;
    BREEZE_SHOOT: Unit = "breeze_shoot", Unit;
    BREEZE_SHOOT_CHARGING: Unit = "breeze_shoot_charging", Unit;
    BREEZE_SHOOT_RECOVERING: Unit = "breeze_shoot_recover", Unit;
    BREEZE_SHOOT_COOLDOWN: Unit = "breeze_shoot_cooldown", Unit;
    BREEZE_JUMP_INHALING: Unit = "breeze_jump_inhaling", Unit;
    BREEZE_JUMP_TARGET: BlockPos = "breeze_jump_target", BlockPos;
    BREEZE_LEAVING_WATER: Unit = "breeze_leaving_water", Unit;
}

/// Compile-time check that every `U` above is a [`MemoryData`].
const _: () = {
    const fn assert_memory_data<U: MemoryData>(_: &MemoryModuleType<U>) {}
    assert_memory_data(&HOME);
};
