//! `BeehiveBlockEntity`: the bees inside a hive / nest (`BeeData` = occupant + ticks in hive), `flower_pos`, the per-tick
//! release logic and the NBT / `bees` component forms.

use super::ComponentReader;
use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
    set_changed,
};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::EntityId;
use crate::world::entity::entity;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::mob::leashable;
use crate::world::entity::mob::{ageable, animal};
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers::is_instance_of;
use crate::world::level::block::impls::campfire_block::CampfireBlock;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::EntitySpawnReason;
use mc_data::{BlockEntityType, EntityType, GameEvent, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::map::DataComponentMapBuilder;
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

    /// `isFireNearby()`: any `FireBlock` within one block (`findBlocksIn(pos - 1, pos + 1)`).
    pub fn is_fire_nearby(&self, level: &dyn BlockGetter) -> bool {
        let pos = self.get_block_pos();
        (pos.x() - 1..=pos.x() + 1).any(|x| {
            (pos.y() - 1..=pos.y() + 1).any(|y| {
                (pos.z() - 1..=pos.z() + 1).any(|z| {
                    is_instance_of(level.get_block_state(BlockPos::new(x, y, z)), "FireBlock")
                })
            })
        })
    }

    /// `isSedated()`: a lit campfire below.
    pub fn is_sedated(&self, level: &dyn BlockGetter) -> bool {
        CampfireBlock::is_smokey_pos(level, self.get_block_pos())
    }

    /// `storeBee(occupant)`.
    pub fn store_bee(&mut self, occupant: BeeOccupant) {
        self.stored.push(BeeData::new(occupant));
    }

    /// `addOccupant(bee)`: the bee (an entity that is out of the level while it acts) is stored and leaves the level.
    pub fn add_occupant(&mut self, level: &mut ServerLevel, bee: &mut dyn Entity) {
        if self.stored.len() >= MAX_OCCUPANTS {
            return;
        }
        level.stop_riding(bee);
        level.eject_passengers(bee);
        leashable::drop_leash(bee, level);
        let mut tag = entity::save(&*bee, &|id| level.get_entity(id)).unwrap_or_default();
        for ignored in IGNORED_BEE_TAGS {
            tag.remove(ignored);
        }
        let has_nectar = tag.get_boolean_or("HasNectar", false);
        self.store_bee(BeeOccupant {
            entity_data: TypedEntityData::of(bee.get_type(), tag),
            ticks_in_hive: 0,
            min_ticks_in_hive: if has_nectar { 2400 } else { 600 },
        });
        if let Some(flower_pos) = bee.as_bee().and_then(|b| b.get_saved_flower_pos())
            && (self.saved_flower_pos.is_none() || level.random().next_boolean())
        {
            self.saved_flower_pos = Some(flower_pos);
        }
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        level.play_sound_at(
            None,
            f64::from(pos.x()),
            f64::from(pos.y()),
            f64::from(pos.z()),
            SoundEvent::BLOCK_BEEHIVE_ENTER,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        level.game_event_from_entity_ref_in_state(
            &*bee,
            Some(state),
            GameEvent::BLOCK_CHANGE,
            mc_core::phys::Vec3::at_center_of(&pos),
        );
        bee.discard_in_level(level);
        set_changed(level, pos, state);
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

    /// `emptyAllLivingFromHive(player, state, releaseReason)`: every occupant that may leave does. The released bees
    /// within 4 blocks of `player` (an entity the level can see) target it, or stay out of the hive for 400 ticks when the
    /// hive is sedated.
    pub fn empty_all_living_from_hive(
        &mut self,
        level: &mut ServerLevel,
        player: Option<&dyn Entity>,
        state: BlockState,
        status: BeeReleaseStatus,
    ) {
        let released = self.release_all_occupants(level, state, status);
        let Some(player) = player else { return };
        for id in released {
            let Some(bee_pos) = level.get_entity(id).map(|bee| bee.base().position()) else {
                continue;
            };
            if player.base().distance_to_sqr_vec(bee_pos) > 16.0 {
                continue;
            }
            let sedated = self.is_sedated(&*level);
            let player_id = player.id();
            level.with_entity(id, |level, bee| {
                if !sedated {
                    bee.set_target(level, Some(player_id));
                } else if let Some(bee) = bee.as_bee_mut() {
                    bee.set_stay_out_of_hive_countdown(400);
                }
            });
        }
    }

    /// `releaseAllOccupants(state, releaseStatus)`: the released bees.
    fn release_all_occupants(
        &mut self,
        level: &mut ServerLevel,
        state: BlockState,
        status: BeeReleaseStatus,
    ) -> Vec<EntityId> {
        let pos = self.get_block_pos();
        let flower_pos = self.saved_flower_pos;
        let mut spawned = Vec::new();
        self.remove_released(|occupant| {
            release_occupant(
                level,
                pos,
                state,
                occupant,
                Some(&mut spawned),
                status,
                flower_pos,
            )
        });
        if !spawned.is_empty() {
            set_changed(level, pos, self.get_block_state());
        }
        spawned
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
                let flower_pos = self.saved_flower_pos;
                let occupant = self.stored[i].to_occupant();
                if release_occupant(level, pos, state, &occupant, None, status, flower_pos) {
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

/// `setBeeReleaseData(ticksInHive, bee)`: the time in the hive ages a baby (or rejuvenates a grown-up's breeding cooldown)
/// and runs down the love time.
fn set_bee_release_data(level: &mut ServerLevel, ticks_in_hive: i32, bee: &mut dyn Entity) {
    if !ageable::is_age_locked(bee) {
        let age = ageable::get_age(bee);
        if age < 0 {
            ageable::set_age_in_level(bee, level, 0.min(age.wrapping_add(ticks_in_hive)));
        } else if age > 0 {
            ageable::set_age_in_level(bee, level, 0.max(age.wrapping_sub(ticks_in_hive)));
        }
    }
    let love = animal::get_in_love_time(bee);
    animal::set_in_love_time(bee, 0.max(love.wrapping_sub(ticks_in_hive)));
}

/// `Occupant.createEntity(level, hivePos)`: the stored bee as a live entity (not yet in the level) that knows its hive.
fn create_entity(
    level: &mut ServerLevel,
    hive_pos: BlockPos,
    occupant: &BeeOccupant,
) -> Option<Box<dyn Entity>> {
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
    if entity.load(level, &tag).is_err()
        || !mc_data::tags::get::<EntityType>("beehive_inhabitors")
            .is_some_and(|t| t.contains(entity.get_type()))
    {
        return None;
    }
    entity.base_mut().set_no_gravity(true);
    if let Some(bee) = entity.as_bee_mut() {
        bee.set_hive_pos(hive_pos);
        set_bee_release_data(level, occupant.ticks_in_hive, &mut *entity);
    }
    Some(entity)
}

/// `BeehiveBlockEntity.releaseOccupant`: whether the occupant left the hive. A released bee is added to `spawned`.
fn release_occupant(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    occupant: &BeeOccupant,
    spawned: Option<&mut Vec<EntityId>>,
    release_status: BeeReleaseStatus,
    saved_flower_pos: Option<BlockPos>,
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
    let Some(mut entity) = create_entity(level, pos, occupant) else {
        return false;
    };
    if let Some(bee) = entity.as_bee_mut() {
        if let Some(flower_pos) = saved_flower_pos
            && bee.get_saved_flower_pos().is_none()
            && level.random().next_float() < 0.9
        {
            bee.set_saved_flower_pos(flower_pos);
        }
        if release_status == BeeReleaseStatus::HoneyDelivered {
            bee.drop_off_nectar();
            if state.is_tag("beehives")
                && let Some(honey_level) = state.try_get(props::LEVEL_HONEY)
                && honey_level < 5
            {
                let mut increase = if level.random().next_int_bounded(100) == 0 {
                    2
                } else {
                    1
                };
                if honey_level + increase > 5 {
                    increase -= 1;
                }
                level.set_block_and_update(
                    pos,
                    state.set(props::LEVEL_HONEY, honey_level + increase),
                );
            }
        }
        if let Some(spawned) = spawned {
            spawned.push(entity.id());
        }
        let width = entity.base().get_bb_width();
        let delta = if front_blocked {
            0.0
        } else {
            0.55 + f64::from(width / 2.0)
        };
        let x = f64::from(pos.x()) + 0.5 + delta * f64::from(facing.get_step_x());
        let y = f64::from(pos.y()) + 0.5 - f64::from(entity.base().get_bb_height() / 2.0);
        let z = f64::from(pos.z()) + 0.5 + delta * f64::from(facing.get_step_z());
        let (y_rot, x_rot) = (entity.base().y_rot(), entity.base().x_rot());
        entity.snap_to(x, y, z, y_rot, x_rot);
    }
    level.play_sound(
        None,
        pos,
        SoundEvent::BLOCK_BEEHIVE_EXIT,
        SoundSource::Blocks,
        1.0,
        1.0,
    );
    let state = level.get_block_state(pos);
    level.game_event_from_entity_ref_in_state(
        &*entity,
        Some(state),
        GameEvent::BLOCK_CHANGE,
        mc_core::phys::Vec3::at_center_of(&pos),
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

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = std::mem::take(builder).set(
            mc_protocol::components::BEES,
            Bees {
                bees: self.get_bees(),
            },
        );
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("bees");
    }

    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        let bees = components
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

#[cfg(test)]
mod tests {
    use super::*;
    use mc_protocol::components::map::DataComponentGetter;
    use mc_protocol::components::{BEES, TypedEntityData};

    #[test]
    fn the_bees_component_carries_the_stored_bees() {
        let bees = Bees {
            bees: vec![BeeOccupant {
                entity_data: TypedEntityData::of(EntityType::BEE, CompoundTag::new()),
                ticks_in_hive: 7,
                min_ticks_in_hive: 600,
            }],
        };
        let mut hive = BeehiveBlockEntity::new(
            BlockPos::new(0, 0, 0),
            mc_data::Block::BEEHIVE.default_state(),
        );
        hive.set_bees(&bees);
        assert_eq!(hive.collect_components().get(&BEES), Some(&bees));
        assert_eq!(hive.get_occupant_count(), 1);
    }
}
