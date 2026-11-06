//! `net.minecraft.world.level.BaseSpawner`: the spawn timer and spawn attempt shared by the monster spawner block
//! entity and the spawner minecart. The client-only display entity and spin are not modelled.

use super::spawn_data::{SpawnData, has_no_configuration};
use super::spawner_mob_seam::{self, MobSpawnSteps};
use super::{LevelAccessor, LevelReader};
use crate::server::level::ServerLevel;
use crate::util::weighted_list::{Weighted, WeightedList};
use crate::world::block_entity::read_codec;
use crate::world::entity::factory::{self, EntitySpawnRequest, LoadedEntities};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::enchantment::codecs::vec3_codec;
use crate::world::level::collision::CollisionGetter;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::GameEvent;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use parking_lot::Mutex;
use std::sync::Arc;

/// A `SpawnData` instance shared between the spawn potentials and `nextSpawnData`, as the same Java object is: a spawn
/// egg that edits the next spawn data edits the matching potential too.
pub type SharedSpawnData = Arc<Mutex<SpawnData>>;

/// What differs between the `BaseSpawner` anonymous subclasses (`SpawnerBlockEntity`, `MinecartSpawner`).
pub trait BaseSpawnerHost: Send + Sync {
    /// `broadcastEvent(level, pos, id)`.
    fn broadcast_event(&self, level: &mut ServerLevel, pos: BlockPos, id: i32);
    /// The part of `setNextSpawnData(level, pos, data)` after `this.nextSpawnData = data` (the block entity sends a
    /// block update, flags 260).
    fn next_spawn_data_changed(&self, _level: &mut ServerLevel, _pos: BlockPos) {}
}

pub struct BaseSpawner {
    host: Box<dyn BaseSpawnerHost>,
    pub spawn_delay: i32,
    pub spawn_potentials: WeightedList<SharedSpawnData>,
    pub next_spawn_data: Option<SharedSpawnData>,
    pub min_spawn_delay: i32,
    pub max_spawn_delay: i32,
    pub spawn_count: i32,
    pub max_nearby_entities: i32,
    pub required_player_range: i32,
    pub spawn_range: i32,
}

/// `EntityType.getSpawnAABB(x, y, z)`.
pub fn spawn_aabb(ty: EntityType, x: f64, y: f64, z: f64) -> AABB {
    let data = ty.data();
    let half_width = data.spawn_dimensions_scale * data.width / 2.0;
    let height = data.spawn_dimensions_scale * data.height;
    AABB::new(
        x - f64::from(half_width),
        y,
        z - f64::from(half_width),
        x + f64::from(half_width),
        y + f64::from(height),
        z + f64::from(half_width),
    )
}

/// `level.hasNearbyAlivePlayer(x, y, z, range)`.
pub fn has_nearby_alive_player(level: &ServerLevel, x: f64, y: f64, z: f64, range: f64) -> bool {
    level
        .get_nearest_player(x, y, z, range, |p| {
            !p.is_spectator() && p.is_alive() && p.living().is_some()
        })
        .is_some()
}

/// `Vec3 spawnPos = input.read("Pos", Vec3.CODEC)` or the random position around the spawner
/// (`x: nextDouble - nextDouble`, `y: nextInt(3) - 1`, `z: nextDouble - nextDouble`, in that draw order).
pub fn read_or_random_spawn_pos(
    entity_tag: &CompoundTag,
    level: &mut ServerLevel,
    pos: BlockPos,
    spawn_range: i32,
) -> mc_core::phys::Vec3 {
    if let Some(p) = read_codec(entity_tag, "Pos", &vec3_codec()) {
        return p;
    }
    let random = level.random();
    let x = f64::from(pos.x())
        + (random.next_double() - random.next_double()) * f64::from(spawn_range)
        + 0.5;
    let y = f64::from(pos.y() + random.next_int_bounded(3) - 1);
    let z = f64::from(pos.z())
        + (random.next_double() - random.next_double()) * f64::from(spawn_range)
        + 0.5;
    mc_core::phys::Vec3::new(x, y, z)
}

/// `EntityType.loadEntityRecursive(input, level, reason, postLoad)` without `postLoad`: the loaded tree, root first. The
/// caller applies its post-load step to every entity in this order (a passenger is post-processed after its vehicle).
pub fn load_entity_tree(
    tag: &CompoundTag,
    level: &ServerLevel,
    reason: EntitySpawnReason,
) -> Option<LoadedEntities> {
    let mut out = Vec::new();
    factory::load_entity_recursive(
        tag,
        level,
        EntitySpawnRequest {
            reason,
            ignore_checks: false,
        },
        &mut out,
    );
    if out.is_empty() { None } else { Some(out) }
}

impl BaseSpawner {
    pub fn new(host: Box<dyn BaseSpawnerHost>) -> Self {
        Self {
            host,
            spawn_delay: 20,
            spawn_potentials: WeightedList::empty(),
            next_spawn_data: None,
            min_spawn_delay: 200,
            max_spawn_delay: 800,
            spawn_count: 4,
            max_nearby_entities: 6,
            required_player_range: 16,
            spawn_range: 4,
        }
    }

    /// `setEntityId(type, level, random, pos)` (the random is the level's).
    pub fn set_entity_id(&mut self, ty: EntityType, level: &mut ServerLevel, pos: BlockPos) {
        let next = self.get_or_create_next_spawn_data(level, pos);
        next.lock().entity_to_spawn.put_string("id", ty.data().name);
    }

    /// `setEntityData(entityData, level, random, pos)`: `TypedEntityData.loadInto(spawnData, registry)`.
    pub fn set_entity_data(
        &mut self,
        ty: EntityType,
        data: &CompoundTag,
        level: &mut ServerLevel,
        pos: BlockPos,
    ) {
        let next = self.get_or_create_next_spawn_data(level, pos);
        let mut spawn_data = next.lock();
        spawn_data.entity_to_spawn.put_string("id", ty.data().name);
        spawn_data.entity_to_spawn.merge(data);
    }

    fn is_near_player(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        has_nearby_alive_player(
            level,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
            f64::from(self.required_player_range),
        )
    }

    /// `serverTick(level, pos)`.
    pub fn server_tick(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        if !(self.is_near_player(level, pos) && level.spawner_blocks_work()) {
            return;
        }
        if self.spawn_delay == -1 {
            self.delay(level, pos);
        }
        if self.spawn_delay > 0 {
            self.spawn_delay -= 1;
            return;
        }
        let mut delay = false;
        let next_spawn_data = self.get_or_create_next_spawn_data(level, pos);
        let next = next_spawn_data.lock().clone();
        for _ in 0..self.spawn_count {
            let Some(entity_type) = factory::by(&next.entity_to_spawn) else {
                self.delay(level, pos);
                return;
            };
            let spawn_pos =
                read_or_random_spawn_pos(&next.entity_to_spawn, level, pos, self.spawn_range);
            let spawn_aabb = spawn_aabb(entity_type, spawn_pos.x, spawn_pos.y, spawn_pos.z);
            if !level.no_collision(None, None, &spawn_aabb) {
                continue;
            }
            let spawn_block_pos = BlockPos::containing(spawn_pos.x, spawn_pos.y, spawn_pos.z);
            if let Some(custom_spawn_rules) = &next.custom_spawn_rules {
                if !entity_type.data().category.data().friendly
                    && level.difficulty() == Difficulty::Peaceful
                {
                    continue;
                }
                if !custom_spawn_rules.is_valid_position(spawn_block_pos, &*level) {
                    continue;
                }
            } else if !spawner_mob_seam::check_spawn_placements(
                level,
                entity_type,
                EntitySpawnReason::Spawner,
                spawn_block_pos,
            ) {
                continue;
            }
            let Some(mut entities) =
                load_entity_tree(&next.entity_to_spawn, level, EntitySpawnReason::Spawner)
            else {
                self.delay(level, pos);
                return;
            };
            for entity in &mut entities {
                let (y_rot, x_rot) = (entity.base().y_rot(), entity.base().x_rot());
                entity.snap_to(spawn_pos.x, spawn_pos.y, spawn_pos.z, y_rot, x_rot);
            }
            let entity_type_loaded = entities[0].get_type();
            let area = AABB::new(
                f64::from(pos.x()),
                f64::from(pos.y()),
                f64::from(pos.z()),
                f64::from(pos.x() + 1),
                f64::from(pos.y() + 1),
                f64::from(pos.z() + 1),
            )
            .inflate_scalar(f64::from(self.spawn_range));
            let near_by = level
                .get_entities_of_class(
                    &area,
                    |e| e.get_type() == entity_type_loaded,
                    |e| !e.is_spectator(),
                )
                .len() as i32;
            if near_by >= self.max_nearby_entities {
                self.delay(level, pos);
                return;
            }
            let root = entities[0].as_mut();
            let (x, y, z) = (root.base().x(), root.base().y(), root.base().z());
            root.snap_to(x, y, z, level.random().next_float() * 360.0, 0.0);
            let steps = MobSpawnSteps {
                check_spawn_rules: next.custom_spawn_rules.is_none(),
                finalize_spawn: has_no_configuration(&next.entity_to_spawn),
                set_persistence_required: false,
                equipment: next.equipment.as_ref(),
            };
            if !spawner_mob_seam::prepare_mob(root, level, EntitySpawnReason::Spawner, &steps) {
                continue;
            }
            let root_id: EntityId = entities[0].id();
            if !level.try_add_fresh_entity_with_passengers(entities) {
                self.delay(level, pos);
                return;
            }
            level.level_event(None, 2004, pos, 0);
            level.game_event_at_block(
                GameEvent::ENTITY_PLACE,
                spawn_block_pos,
                &crate::world::level::gameevent::GameEventContext::of_entity(Some(root_id)),
            );
            spawner_mob_seam::spawn_anim(level, root_id);
            delay = true;
        }
        if delay {
            self.delay(level, pos);
        }
    }

    /// `delay(level, pos)`.
    fn delay(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        if self.max_spawn_delay <= self.min_spawn_delay {
            self.spawn_delay = self.min_spawn_delay;
        } else {
            self.spawn_delay = self.min_spawn_delay
                + level
                    .random()
                    .next_int_bounded(self.max_spawn_delay - self.min_spawn_delay);
        }
        let picked = self.spawn_potentials.get_random(level.random()).cloned();
        if let Some(entry) = picked {
            self.set_next_spawn_data(level, pos, entry);
        }
        self.host.broadcast_event(level, pos, 1);
    }

    /// `load(level, pos, input)`: the level only matters for the block update of `setNextSpawnData`, which the caller of
    /// a plain load (chunk load, `/data merge`) already sends.
    pub fn load(&mut self, tag: &CompoundTag) {
        self.spawn_delay = i32::from(tag.get_short_or("Delay", 20));
        if let Some(next) = read_codec(tag, "SpawnData", &SpawnData::codec()) {
            self.next_spawn_data = Some(Arc::new(Mutex::new(next)));
        }
        self.spawn_potentials = match read_codec(tag, "SpawnPotentials", &SpawnData::list_codec()) {
            Some(list) => list.map(|d| Arc::new(Mutex::new(d))),
            None => WeightedList::of_value(
                self.next_spawn_data
                    .clone()
                    .unwrap_or_else(|| Arc::new(Mutex::new(SpawnData::empty()))),
            ),
        };
        self.min_spawn_delay = tag.get_int_or("MinSpawnDelay", 200);
        self.max_spawn_delay = tag.get_int_or("MaxSpawnDelay", 800);
        self.spawn_count = tag.get_int_or("SpawnCount", 4);
        self.max_nearby_entities = tag.get_int_or("MaxNearbyEntities", 6);
        self.required_player_range = tag.get_int_or("RequiredPlayerRange", 16);
        self.spawn_range = tag.get_int_or("SpawnRange", 4);
    }

    /// `save(output)`.
    pub fn save(&self, tag: &mut CompoundTag) {
        tag.put_short("Delay", self.spawn_delay as i16);
        tag.put_short("MinSpawnDelay", self.min_spawn_delay as i16);
        tag.put_short("MaxSpawnDelay", self.max_spawn_delay as i16);
        tag.put_short("SpawnCount", self.spawn_count as i16);
        tag.put_short("MaxNearbyEntities", self.max_nearby_entities as i16);
        tag.put_short("RequiredPlayerRange", self.required_player_range as i16);
        tag.put_short("SpawnRange", self.spawn_range as i16);
        if let Some(next) = &self.next_spawn_data
            && let Ok(encoded) = mc_dfu::encode_nbt(&SpawnData::codec(), &next.lock()).into_result()
        {
            tag.put("SpawnData", encoded);
        }
        let potentials = WeightedList::of(
            self.spawn_potentials
                .unwrap()
                .iter()
                .map(|w| Weighted {
                    value: w.value.lock().clone(),
                    weight: w.weight,
                })
                .collect(),
        );
        if let Some(potentials) = potentials
            && let Ok(encoded) =
                mc_dfu::encode_nbt(&SpawnData::list_codec(), &potentials).into_result()
        {
            tag.put("SpawnPotentials", encoded);
        }
    }

    /// `onEventTriggered(level, id)` on the server: event 1 is accepted (it is only acted upon on the client).
    pub fn on_event_triggered(&self, id: i32) -> bool {
        id == 1
    }

    fn set_next_spawn_data(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        next: SharedSpawnData,
    ) {
        self.next_spawn_data = Some(next);
        self.host.next_spawn_data_changed(level, pos);
    }

    fn get_or_create_next_spawn_data(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
    ) -> SharedSpawnData {
        if let Some(next) = &self.next_spawn_data {
            return next.clone();
        }
        let picked = self
            .spawn_potentials
            .get_random(level.random())
            .cloned()
            .unwrap_or_else(|| Arc::new(Mutex::new(SpawnData::empty())));
        self.set_next_spawn_data(level, pos, picked.clone());
        picked
    }
}
