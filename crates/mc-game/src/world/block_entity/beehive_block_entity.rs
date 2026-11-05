//! `BeehiveBlockEntity`: the bees inside a hive / nest (`BeeData` = occupant + ticks in hive), `flower_pos`, the per-tick
//! release logic and the NBT / `bees` component forms.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec, set_changed};
use crate::server::level::ServerLevel;
use crate::world::level::LevelAccessor;
use mc_core::random::RandomSource;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{Entity, EntityId, entity};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::EntitySpawnReason;
use mc_data::{BlockEntityType, EntityType, GameEvent, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::{BeeOccupant, Bees, TypedEntityData, block_pos_codec};
use mc_protocol::packets::play::SoundSource;

/// `IGNORED_BEE_TAGS`: removed from a bee's saved data when it enters the hive and again when it is released.
pub const IGNORED_BEE_TAGS: [&str; 25] = [
    "Air",
    "drop_chances",
    "equipment",
    "Brain",
    "CanPickUpLoot",
    "DeathTime",
    "fall_distance",
    "FallFlying",
    "Fire",
    "HurtTime",
    "LeftHanded",
    "Motion",
    "NoGravity",
    "OnGround",
    "PortalCooldown",
    "Pos",
    "Rotation",
    "sleeping_pos",
    "CannotEnterHiveTicks",
    "TicksSincePollination",
    "CropsGrownSincePollination",
    "hive_pos",
    "Passengers",
    "leash",
    "UUID",
];

pub const MAX_OCCUPANTS: usize = 3;
pub const MIN_OCCUPATION_TICKS_NECTAR: i32 = 2400;
pub const MIN_OCCUPATION_TICKS_NECTARLESS: i32 = 600;

/// `BeehiveBlockEntity.BeeReleaseStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeeReleaseStatus {
    HoneyDelivered,
    BeeReleased,
    Emergency,
}

/// `BeehiveBlockEntity.BeeData`.
#[derive(Clone, Debug)]
struct BeeData {
    occupant: BeeOccupant,
    ticks_in_hive: i32,
}

impl BeeData {
    fn new(occupant: BeeOccupant) -> Self {
        let ticks_in_hive = occupant.ticks_in_hive;
        Self { occupant, ticks_in_hive }
    }
    /// `tick()`: `this.ticksInHive++ > this.occupant.minTicksInHive`.
    fn tick(&mut self) -> bool {
        let before = self.ticks_in_hive;
        self.ticks_in_hive = self.ticks_in_hive.wrapping_add(1);
        before > self.occupant.min_ticks_in_hive
    }
    /// `toOccupant()`.
    fn to_occupant(&self) -> BeeOccupant {
        BeeOccupant { entity_data: self.occupant.entity_data.clone(), ticks_in_hive: self.ticks_in_hive, min_ticks_in_hive: self.occupant.min_ticks_in_hive }
    }
    /// `hasNectar()`.
    fn has_nectar(&self) -> bool {
        self.occupant.entity_data.tag.get_boolean_or("HasNectar", false)
    }
}

pub struct BeehiveBlockEntity {
    base: BlockEntityData,
    stored: Vec<BeeData>,
    pub saved_flower_pos: Option<BlockPos>,
}

impl BeehiveBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::BEEHIVE, pos, state), stored: Vec::new(), saved_flower_pos: None }
    }

    pub fn is_empty(&self) -> bool {
        self.stored.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.stored.len() == MAX_OCCUPANTS
    }

    /// `getOccupantCount()`.
    pub fn get_occupant_count(&self) -> usize {
        self.stored.len()
    }

    /// `BeehiveBlockEntity.getHoneyLevel(state)`.
    pub fn get_honey_level(state: BlockState) -> i32 {
        state.get(props::LEVEL_HONEY)
    }

    pub fn has_saved_flower_pos(&self) -> bool {
        self.saved_flower_pos.is_some()
    }

    /// `storeBee(occupant)`.
    pub fn store_bee(&mut self, occupant: BeeOccupant) {
        self.stored.push(BeeData::new(occupant));
    }

    /// `getBees()`: the occupants as saved (`ticks_in_hive` current).
    pub fn get_bees(&self) -> Vec<BeeOccupant> {
        self.stored.iter().map(BeeData::to_occupant).collect()
    }

    /// `applyImplicitComponents`: the `bees` component replaces the stored bees.
    pub fn set_bees(&mut self, bees: &Bees) {
        self.stored.clear();
        for occupant in &bees.bees {
            self.store_bee(occupant.clone());
        }
    }

    /// `isFireNearby()`: any `FireBlock` within one block (`findBlocksIn(pos - 1, pos + 1)`).
    pub fn is_fire_nearby(&self, level: &dyn crate::world::level::BlockGetter) -> bool {
        let pos = self.get_block_pos();
        for x in pos.x() - 1..=pos.x() + 1 {
            for y in pos.y() - 1..=pos.y() + 1 {
                for z in pos.z() - 1..=pos.z() + 1 {
                    if crate::world::level::block::helpers::is_instance_of(level.get_block_state(BlockPos::new(x, y, z)), "FireBlock") {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `isSedated()`: a lit campfire below.
    pub fn is_sedated(&self, level: &dyn crate::world::level::BlockGetter) -> bool {
        is_smokey_pos(level, self.get_block_pos())
    }

    /// `emptyAllLivingFromHive(player, state, releaseReason)`.
    pub fn empty_all_living_from_hive(&mut self, level: &mut ServerLevel, player: Option<EntityId>, state: BlockState, status: BeeReleaseStatus) {
        let released = self.release_all_occupants(level, state, status);
        let Some(player_id) = player else { return };
        let Some(player_pos) = level.get_entity(player_id).map(|p| p.position()) else { return };
        let sedated = self.is_sedated(&*level);
        for id in released {
            let Some(bee_pos) = level.get_entity(id).map(|e| e.position()) else { continue };
            if player_pos.distance_to_sqr(bee_pos) <= 16.0 {
                level.with_entity(id, |_, entity| {
                    if let Some(bee) = entity.as_bee_access() {
                        if !sedated {
                            bee.set_target(Some(player_id));
                        } else {
                            bee.set_stay_out_of_hive_countdown(400);
                        }
                    }
                });
            }
        }
    }

    /// `releaseAllOccupants(state, releaseStatus)`: the released bees.
    fn release_all_occupants(&mut self, level: &mut ServerLevel, state: BlockState, status: BeeReleaseStatus) -> Vec<EntityId> {
        let pos = self.get_block_pos();
        let saved_flower_pos = self.saved_flower_pos;
        let mut spawned = Vec::new();
        let changed = self.remove_released(|occupant| release_occupant(level, pos, state, occupant, Some(&mut spawned), status, saved_flower_pos));
        if !spawned.is_empty() {
            // `super.setChanged()`
            set_changed(level, pos, self.get_block_state());
        }
        let _ = changed;
        spawned
    }

    /// `addOccupant(bee)`: the bee enters the hive (`EnterBeehiveGoal`, `Bee` code); the entity is discarded.
    pub fn add_occupant(&mut self, level: &mut ServerLevel, bee: &mut dyn Entity) {
        if self.stored.len() >= MAX_OCCUPANTS {
            return;
        }
        level.stop_riding(bee);
        level.eject_passengers(bee);
        if let Some(access) = bee.as_bee_access() {
            access.drop_leash(level);
        }
        let Some(occupant) = occupant_of(bee) else { return };
        self.store_bee(occupant);
        let bee_flower_pos = bee.as_bee_access().and_then(|a| a.get_saved_flower_pos());
        if let Some(flower_pos) = bee_flower_pos
            && (!self.has_saved_flower_pos() || level.random().next_boolean())
        {
            self.saved_flower_pos = Some(flower_pos);
        }
        let pos = self.get_block_pos();
        level.play_sound_at(None, f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), SoundEvent::BLOCK_BEEHIVE_ENTER, SoundSource::Blocks, 1.0, 1.0);
        level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of(Some(bee.id()), Some(self.get_block_state())));
        bee.discard();
        set_changed(level, pos, self.get_block_state());
    }

    /// `tickOccupants(level, pos, state, stored, savedFlowerPos)`.
    fn tick_occupants(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        let saved_flower_pos = self.saved_flower_pos;
        let mut changed = false;
        let mut i = 0;
        while i < self.stored.len() {
            if self.stored[i].tick() {
                let status = if self.stored[i].has_nectar() { BeeReleaseStatus::HoneyDelivered } else { BeeReleaseStatus::BeeReleased };
                let occupant = self.stored[i].to_occupant();
                if release_occupant(level, pos, state, &occupant, None, status, saved_flower_pos) {
                    changed = true;
                    self.stored.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        if changed {
            set_changed(level, pos, state);
        }
    }

    /// `stored.removeIf(release)`: `release` returns whether the occupant was released; every occupant is offered.
    fn remove_released(&mut self, mut release: impl FnMut(&BeeOccupant) -> bool) -> bool {
        let before = self.stored.len();
        self.stored.retain(|d| !release(&d.to_occupant()));
        before != self.stored.len()
    }
}

/// What `BeehiveBlockEntity` / `BeehiveBlock` call on a `Bee` (implemented by the bee entity; see
/// `Entity::as_bee_access`).
pub trait BeeAccess {
    /// `hasSavedFlowerPos()`.
    fn has_saved_flower_pos(&self) -> bool {
        self.get_saved_flower_pos().is_some()
    }
    /// `getSavedFlowerPos()`.
    fn get_saved_flower_pos(&self) -> Option<BlockPos>;
    /// `setSavedFlowerPos(pos)`.
    fn set_saved_flower_pos(&mut self, pos: BlockPos);
    /// `setHivePos(pos)`.
    fn set_hive_pos(&mut self, pos: BlockPos);
    /// `dropOffNectar()`.
    fn drop_off_nectar(&mut self);
    /// `setStayOutOfHiveCountdown(ticks)`.
    fn set_stay_out_of_hive_countdown(&mut self, ticks: i32);
    /// `getTarget()` (`Mob.getTarget`).
    fn get_target(&self) -> Option<EntityId>;
    /// `setTarget(target)` (`Mob.setTarget`).
    fn set_target(&mut self, target: Option<EntityId>);
    /// `isAgeLocked()` (`AgeableMob`).
    fn is_age_locked(&self) -> bool;
    /// `getAge()`.
    fn get_age(&self) -> i32;
    /// `setAge(age)`.
    fn set_age(&mut self, age: i32);
    /// `getInLoveTime()` (`Animal`).
    fn get_in_love_time(&self) -> i32;
    /// `setInLoveTime(ticks)`.
    fn set_in_love_time(&mut self, ticks: i32);
    /// `dropLeash()` (`Leashable`).
    fn drop_leash(&mut self, level: &mut ServerLevel);
}

/// `BeehiveBlockEntity.Occupant.of(entity)`: the entity's saved data minus [`IGNORED_BEE_TAGS`].
pub fn occupant_of(entity: &dyn Entity) -> Option<BeeOccupant> {
    let mut tag = entity::save(entity, &|_| None)?;
    for ignored in IGNORED_BEE_TAGS {
        tag.remove(ignored);
    }
    let has_nectar = tag.get_boolean_or("HasNectar", false);
    Some(BeeOccupant {
        entity_data: TypedEntityData::of(entity.get_type(), tag),
        ticks_in_hive: 0,
        min_ticks_in_hive: if has_nectar { MIN_OCCUPATION_TICKS_NECTAR } else { MIN_OCCUPATION_TICKS_NECTARLESS },
    })
}

/// `Occupant.create(ticksInHive)`: a bare bee (structures' pre-filled nests).
pub fn occupant_create(ticks_in_hive: i32) -> BeeOccupant {
    BeeOccupant { entity_data: TypedEntityData::of(EntityType::BEE, CompoundTag::new()), ticks_in_hive, min_ticks_in_hive: MIN_OCCUPATION_TICKS_NECTARLESS }
}

/// `Occupant.createEntity(level, hivePos)`: the stored bee as a live entity (not yet in the level).
fn create_entity(level: &ServerLevel, occupant: &BeeOccupant, hive_pos: BlockPos) -> Option<Box<dyn Entity>> {
    let mut tag = occupant.entity_data.tag.clone();
    for ignored in IGNORED_BEE_TAGS {
        tag.remove(ignored);
    }
    let mut entity = factory::create(occupant.entity_data.ty, level, EntitySpawnRequest { reason: EntitySpawnReason::Load, ignore_checks: false })?;
    if entity.load(&tag).is_err() || !mc_data::tags::get::<EntityType>("beehive_inhabitors").is_some_and(|t| t.contains(entity.get_type())) {
        return None;
    }
    entity.base_mut().set_no_gravity(true);
    if let Some(bee) = entity.as_bee_access() {
        bee.set_hive_pos(hive_pos);
        // `setBeeReleaseData(ticksInHive, bee)`
        let ticks = occupant.ticks_in_hive;
        if !bee.is_age_locked() {
            let age = bee.get_age();
            if age < 0 {
                bee.set_age(0.min(age.wrapping_add(ticks)));
            } else if age > 0 {
                bee.set_age(0.max(age.wrapping_sub(ticks)));
            }
        }
        bee.set_in_love_time(0.max(bee.get_in_love_time().wrapping_sub(ticks)));
    }
    Some(entity)
}

/// `BeehiveBlockEntity.releaseOccupant(level, pos, state, occupant, spawned, releaseStatus, savedFlowerPos)`: whether the
/// bee left the hive.
fn release_occupant(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    occupant: &BeeOccupant,
    spawned: Option<&mut Vec<EntityId>>,
    release_status: BeeReleaseStatus,
    saved_flower_pos: Option<BlockPos>,
) -> bool {
    if level.attribute_bool(crate::world::attribute::attributes::BEES_STAY_IN_HIVE, pos) && release_status != BeeReleaseStatus::Emergency {
        return false;
    }
    let facing = state.get(props::HORIZONTAL_FACING);
    let facing_pos = pos.relative(facing);
    let front_blocked = !level.get_block_state(facing_pos).get_collision_shape(&*level, facing_pos).is_empty();
    if front_blocked && release_status != BeeReleaseStatus::Emergency {
        return false;
    }
    let Some(mut entity) = create_entity(level, occupant, pos) else { return false };
    let id = entity.id();
    if entity.as_bee_access().is_some() {
        // `RandomSource random = level.getRandom()`: `nextFloat` only runs when the first two conditions hold
        let adopt_flower_pos = match saved_flower_pos {
            Some(flower_pos) if !entity.as_bee_access().expect("bee").has_saved_flower_pos() && level.random().next_float() < 0.9 => Some(flower_pos),
            _ => None,
        };
        if let Some(flower_pos) = adopt_flower_pos {
            entity.as_bee_access().expect("bee").set_saved_flower_pos(flower_pos);
        }
        if release_status == BeeReleaseStatus::HoneyDelivered {
            entity.as_bee_access().expect("bee").drop_off_nectar();
            if state.block().is_in_tag("beehives") && state.has_property(props::LEVEL_HONEY) {
                let honey_level = BeehiveBlockEntity::get_honey_level(state);
                if honey_level < 5 {
                    let mut level_increase = if level.random().next_int_bounded(100) == 0 { 2 } else { 1 };
                    if honey_level + level_increase > 5 {
                        level_increase -= 1;
                    }
                    level.set_block_and_update(pos, state.set(props::LEVEL_HONEY, honey_level + level_increase));
                }
            }
        }
        if let Some(spawned) = spawned {
            spawned.push(id);
        }
        let bb_width = entity.base().get_bb_width();
        let bb_height = entity.base().get_bb_height();
        let delta = if front_blocked { 0.0 } else { 0.55 + f64::from(bb_width / 2.0) };
        let spawn_x = f64::from(pos.x()) + 0.5 + delta * f64::from(facing.get_step_x());
        let spawn_y = f64::from(pos.y()) + 0.5 - f64::from(bb_height / 2.0);
        let spawn_z = f64::from(pos.z()) + 0.5 + delta * f64::from(facing.get_step_z());
        let (y_rot, x_rot) = (entity.base().y_rot(), entity.base().x_rot());
        entity.snap_to(spawn_x, spawn_y, spawn_z, y_rot, x_rot);
    }
    level.play_sound(None, pos, SoundEvent::BLOCK_BEEHIVE_EXIT, SoundSource::Blocks, 1.0, 1.0);
    level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of(Some(id), Some(level.get_block_state(pos))));
    level.add_fresh_entity(entity)
}

/// `CampfireBlock.isSmokeyPos(level, pos)`.
// TEMP: replaced by `CampfireBlock::is_smokey_pos` once BxFireBedCauldron's campfire lands on main
fn is_smokey_pos(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    use crate::world::level::collision::CollisionContext;
    use mc_core::phys::shapes::{BooleanOp, Shapes};
    let lit_campfire = |state: BlockState| state.has_property(props::LIT) && state.is_tag("campfires") && state.get(props::LIT);
    let post = crate::world::level::shapes::column(4.0, 0.0, 16.0);
    for i in 1..=5 {
        let pos_to_check = pos.below_n(i);
        let state = level.get_block_state(pos_to_check);
        if lit_campfire(state) {
            return true;
        }
        if Shapes::join_is_not_empty(&post, &state.get_collision_shape_ctx(level, pos, &CollisionContext::empty()), BooleanOp::AND) {
            return lit_campfire(level.get_block_state(pos_to_check.below()));
        }
    }
    false
}

impl BlockEntity for BeehiveBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.stored.clear();
        if let Some(bees) = read_codec(tag, "bees", &Bees::codec()) {
            for occupant in bees.bees {
                self.store_bee(occupant);
            }
        }
        self.saved_flower_pos = read_codec(tag, "flower_pos", &block_pos_codec());
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Ok(bees) = mc_dfu::encode_nbt(&Bees::codec(), &Bees { bees: self.get_bees() }).into_result() {
            tag.put("bees", bees);
        }
        if let Some(pos) = &self.saved_flower_pos
            && let Ok(encoded) = mc_dfu::encode_nbt(&block_pos_codec(), pos).into_result()
        {
            tag.put("flower_pos", encoded);
        }
    }

    fn apply_components_from_item_stack(&mut self, stack: &crate::world::item::ItemStack) {
        let bees = stack.get(&mc_protocol::components::BEES).cloned().unwrap_or_default();
        self.set_bees(&bees);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `BeehiveBlockEntity.serverTick`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let state = level.get_block_state(pos);
        self.tick_occupants(level, pos, state);
        if !self.stored.is_empty() && level.random().next_double() < 0.005 {
            let x = f64::from(pos.x()) + 0.5;
            let y = f64::from(pos.y());
            let z = f64::from(pos.z()) + 0.5;
            level.play_sound_at(None, x, y, z, SoundEvent::BLOCK_BEEHIVE_WORK, SoundSource::Blocks, 1.0, 1.0);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BEEHIVE: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::BEEHIVE, create: |pos, state| Box::new(BeehiveBlockEntity::new(pos, state)) };
