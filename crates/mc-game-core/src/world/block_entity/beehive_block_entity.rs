//! `BeehiveBlockEntity`: the bees inside a hive / nest (`BeeData` = occupant + ticks in hive), `flower_pos`, the per-tick
//! release logic and the NBT / `bees` component forms.

use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
    set_changed,
};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor;
use crate::world::level::block::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::EntitySpawnReason;
use mc_data::{BlockEntityType, EntityType, GameEvent, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::{BeeOccupant, Bees, block_pos_codec};
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
        Self {
            occupant,
            ticks_in_hive,
        }
    }
    /// `tick()`: `this.ticksInHive++ > this.occupant.minTicksInHive`.
    fn tick(&mut self) -> bool {
        let before = self.ticks_in_hive;
        self.ticks_in_hive = self.ticks_in_hive.wrapping_add(1);
        before > self.occupant.min_ticks_in_hive
    }
    /// `toOccupant()`.
    fn to_occupant(&self) -> BeeOccupant {
        BeeOccupant {
            entity_data: self.occupant.entity_data.clone(),
            ticks_in_hive: self.ticks_in_hive,
            min_ticks_in_hive: self.occupant.min_ticks_in_hive,
        }
    }
    /// `hasNectar()`.
    fn has_nectar(&self) -> bool {
        self.occupant
            .entity_data
            .tag
            .get_boolean_or("HasNectar", false)
    }
}

pub struct BeehiveBlockEntity {
    base: BlockEntityData,
    stored: Vec<BeeData>,
    pub saved_flower_pos: Option<BlockPos>,
}

impl BeehiveBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::BEEHIVE, pos, state),
            stored: Vec::new(),
            saved_flower_pos: None,
        }
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

    /// `emptyAllLivingFromHive(player, state, releaseReason)`: every occupant that may leave does. The player and the bee
    /// reactions (target, stay-out countdown) are absent because no `Bee` entity exists.
    pub fn empty_all_living_from_hive(
        &mut self,
        level: &mut ServerLevel,
        state: BlockState,
        status: BeeReleaseStatus,
    ) {
        let pos = self.get_block_pos();
        self.remove_released(|occupant| release_occupant(level, pos, state, occupant, status));
    }

    /// `tickOccupants(level, pos, state, stored)`.
    fn tick_occupants(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        let mut changed = false;
        let mut i = 0;
        while i < self.stored.len() {
            if self.stored[i].tick() {
                let status = if self.stored[i].has_nectar() {
                    BeeReleaseStatus::HoneyDelivered
                } else {
                    BeeReleaseStatus::BeeReleased
                };
                let occupant = self.stored[i].to_occupant();
                if release_occupant(level, pos, state, &occupant, status) {
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
    fn remove_released(&mut self, mut release: impl FnMut(&BeeOccupant) -> bool) {
        self.stored.retain(|d| !release(&d.to_occupant()));
    }
}

/// `Occupant.createEntity(level, hivePos)`: the stored bee as a live entity (not yet in the level; the hive position
/// only matters to a `Bee`).
fn create_entity(level: &ServerLevel, occupant: &BeeOccupant) -> Option<Box<dyn Entity>> {
    let mut tag = occupant.entity_data.tag.clone();
    for ignored in IGNORED_BEE_TAGS {
        tag.remove(ignored);
    }
    let mut entity = factory::create(
        occupant.entity_data.ty,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Load,
            ignore_checks: false,
        },
    )?;
    if entity.load(&tag).is_err()
        || !mc_data::tags::get::<EntityType>("beehive_inhabitors")
            .is_some_and(|t| t.contains(entity.get_type()))
    {
        return None;
    }
    entity.base_mut().set_no_gravity(true);
    Some(entity)
}

/// `BeehiveBlockEntity.releaseOccupant`: whether the occupant left the hive. The `Bee`-only follow-up (flower position,
/// honey delivery, spawn offset) is absent because no `Bee` entity exists.
fn release_occupant(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    occupant: &BeeOccupant,
    release_status: BeeReleaseStatus,
) -> bool {
    if level.attribute_bool(crate::world::attribute::attributes::BEES_STAY_IN_HIVE, pos)
        && release_status != BeeReleaseStatus::Emergency
    {
        return false;
    }
    let facing = state.get(props::HORIZONTAL_FACING);
    let facing_pos = pos.relative(facing);
    let front_blocked = !level
        .get_block_state(facing_pos)
        .get_collision_shape(&*level, facing_pos)
        .is_empty();
    if front_blocked && release_status != BeeReleaseStatus::Emergency {
        return false;
    }
    let Some(entity) = create_entity(level, occupant) else {
        return false;
    };
    let id = entity.id();
    level.play_sound(
        None,
        pos,
        SoundEvent::BLOCK_BEEHIVE_EXIT,
        SoundSource::Blocks,
        1.0,
        1.0,
    );
    level.game_event_at_block(
        GameEvent::BLOCK_CHANGE,
        pos,
        &GameEventContext::of(Some(id), Some(level.get_block_state(pos))),
    );
    level.add_fresh_entity(entity)
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
        if let Ok(bees) = mc_dfu::encode_nbt(
            &Bees::codec(),
            &Bees {
                bees: self.get_bees(),
            },
        )
        .into_result()
        {
            tag.put("bees", bees);
        }
        if let Some(pos) = &self.saved_flower_pos
            && let Ok(encoded) = mc_dfu::encode_nbt(&block_pos_codec(), pos).into_result()
        {
            tag.put("flower_pos", encoded);
        }
    }

    fn apply_components_from_item_stack(&mut self, stack: &crate::world::item::ItemStack) {
        let bees = stack
            .get(&mc_protocol::components::BEES)
            .cloned()
            .unwrap_or_default();
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
            level.play_sound_at(
                None,
                x,
                y,
                z,
                SoundEvent::BLOCK_BEEHIVE_WORK,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BEEHIVE: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::BEEHIVE,
    create: |pos, state| Box::new(BeehiveBlockEntity::new(pos, state)),
};
