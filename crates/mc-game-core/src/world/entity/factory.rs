//! `EntityType` factories (`EntityType.EntityFactory`) and the loading/creation entry points
//! (`EntityType.create`, `loadEntityRecursive`, `loadEntitiesRecursive`).
//!
//! Concrete entity classes register a factory through [`ENTITY_FACTORIES`] from the module that implements them.
//! Every entity type but the player has a class registered this way. `PlainEntity` is the fallback for a type without a
//! registered factory (the player type reaching `instantiate`); it keeps the raw NBT it was loaded from.

use super::plain::PlainEntity;
use super::{Entity, EntityId};
use crate::server::level::ServerLevel;
use crate::world::block_entity::problem_reporter;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::CompoundTag;
use std::collections::HashMap;
use std::sync::OnceLock;

/// `EntityType.EntityFactory<T>`: `create(entityType, level)`.
pub struct EntityRegistration {
    pub entity_type: EntityType,
    /// `(type, entity id, registries)`: mob constructors read their default variants from the registries.
    pub create: fn(EntityType, EntityId, &crate::registry::RegistryAccess) -> Box<dyn Entity>,
}

#[distributed_slice]
pub static ENTITY_FACTORIES: [EntityRegistration];

fn factories() -> &'static HashMap<EntityType, &'static EntityRegistration> {
    static MAP: OnceLock<HashMap<EntityType, &'static EntityRegistration>> = OnceLock::new();
    MAP.get_or_init(|| {
        ENTITY_FACTORIES
            .iter()
            .map(|r| (r.entity_type, r))
            .collect()
    })
}

/// `EntitySpawnRequest`.
#[derive(Clone, Copy, Debug)]
pub struct EntitySpawnRequest {
    pub reason: EntitySpawnReason,
    pub ignore_checks: bool,
}

/// Whether a concrete class (not the [`PlainEntity`] fallback) is registered for `ty`.
pub fn has_factory(ty: EntityType) -> bool {
    factories().contains_key(&ty)
}

/// `EntityType.create(level, request)`: `None` when the factory returns null (the player type) or the spawn checks fail.
pub fn create(
    ty: EntityType,
    level: &ServerLevel,
    request: EntitySpawnRequest,
) -> Option<Box<dyn Entity>> {
    if ty == EntityType::PLAYER {
        return None; // `EntityType.PLAYER` is `createNothing`
    }
    if !request.ignore_checks && !level.can_spawn_entity_type(ty) {
        return None;
    }
    let id = level.next_entity_id();
    Some(instantiate_with(ty, id, &level.registries, &mut || {
        level.next_entity_id()
    }))
}

/// `factory.create(type, level)` with an explicit entity id and the registries: usable without a `ServerLevel`
/// (worldgen worker threads); no spawn checks.
pub fn instantiate(
    ty: EntityType,
    id: EntityId,
    registries: &crate::registry::RegistryAccess,
) -> Box<dyn Entity> {
    instantiate_with(
        ty,
        id,
        registries,
        &mut crate::server::level::next_detached_entity_id,
    )
}

/// [`instantiate`] with the id source of the hitboxes of a multipart entity (the constructors of its parts take the next ids).
pub fn instantiate_with(
    ty: EntityType,
    id: EntityId,
    registries: &crate::registry::RegistryAccess,
    next_id: &mut dyn FnMut() -> EntityId,
) -> Box<dyn Entity> {
    let mut entity: Box<dyn Entity> = match factories().get(&ty) {
        Some(reg) => (reg.create)(ty, id, registries),
        // Only the player type has no class (vanilla `createNothing`): an inert entity that keeps its NBT.
        None => Box::new(PlainEntity::new(ty, id)),
    };
    entity.allocate_sub_entity_ids(next_id);
    entity
}

/// `EntityType.by(input)`: the type named by the `id` tag.
pub fn by(tag: &CompoundTag) -> Option<EntityType> {
    let id = tag.get_string("id")?;
    let ident = Identifier::parse(id).ok()?;
    EntityType::by_name(&ident.to_string())
}

/// The entity half of `StructureTemplate.placeEntities`: `EntityType.create(input, level, STRUCTURE)` from the tag (the
/// constructor draws the id, seed and UUID), the body and head rotation of the tag, `Mob.finalizeSpawn` when `difficulty`
/// is given, then `addFreshEntityWithPassengers`. False when the entity cannot be created (vanilla skips it).
pub fn add_structure_entity(
    tag: &CompoundTag,
    level: &mut dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
    difficulty: Option<crate::world::difficulty_instance::DifficultyInstance>,
) -> bool {
    let Some(ty) = by(tag) else {
        return false;
    };
    let Some(mut entity) = level.create_entity(
        ty,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Structure,
            ignore_checks: false,
        },
    ) else {
        return false;
    };
    if super::entity::load(entity.as_mut(), level.as_server_level(), tag).is_err() {
        return false;
    }
    entity.resolve_registry_refs(level.registry_access());
    let y_rot = entity.base().y_rot();
    if let Some(living) = entity.living_mut() {
        living.y_body_rot = y_rot;
        living.y_head_rot = y_rot;
    }
    if let (Some(difficulty), true) = (difficulty, entity.is_mob()) {
        entity.finalize_spawn(level, difficulty, EntitySpawnReason::Structure, None);
    }
    level.add_fresh_entity_with_passengers(entity)
}

/// A mob a structure piece creates while generating: `EntityType.create(level, STRUCTURE)`, then with `difficulty`
/// `setPersistenceRequired()`, `snapTo(pos, 0, 0)` (the constructor's random head and body rotation stay) and
/// `finalizeSpawn`, otherwise `setPos(pos)`; then `addFreshEntityWithPassengers`. False when it cannot be created.
pub fn add_structure_mob(
    level: &mut dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
    ty: EntityType,
    pos: mc_core::pos::BlockPos,
    difficulty: Option<crate::world::difficulty_instance::DifficultyInstance>,
) -> bool {
    let Some(mut entity) = level.create_entity(
        ty,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Structure,
            ignore_checks: false,
        },
    ) else {
        return false;
    };
    let (x, y, z) = (
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()),
        f64::from(pos.z()) + 0.5,
    );
    if let Some(difficulty) = difficulty {
        entity.set_persistence_required();
        entity.snap_to(x, y, z, 0.0, 0.0);
        entity.finalize_spawn(level, difficulty, EntitySpawnReason::Structure, None);
    } else {
        entity.set_pos(x, y, z);
    }
    // vanilla ignores the add result (a jockey rider is saved with its vehicle, not on its own)
    level.add_fresh_entity_with_passengers(entity);
    true
}

/// A loaded entity tree: `entity` first, then every passenger depth-first, with `vehicle`/`passengers` links set.
pub type LoadedEntities = Vec<Box<dyn Entity>>;

/// `EntityType.create(input, level, request)` + `load`: `None` (logged) when the id is unknown / not creatable /
/// the tag is invalid.
pub fn create_and_load(
    tag: &CompoundTag,
    level: &ServerLevel,
    request: EntitySpawnRequest,
) -> Option<Box<dyn Entity>> {
    // an id that does not create (unknown, or a type the level's difficulty does not allow) is skipped with the same warning
    let Some(mut entity) = by(tag).and_then(|ty| create(ty, level, request)) else {
        log::warn!(
            "Skipping Entity with id {}",
            tag.get_string("id").unwrap_or("[invalid]")
        );
        return None;
    };
    match entity.load(level, tag) {
        Ok(()) => {
            entity.resolve_registry_refs(&level.registries);
            Some(entity)
        }
        Err(message) => {
            log::warn!("Exception loading entity: {message}");
            None
        }
    }
}

/// `EntityType.loadEntityRecursive(tag, level, request, postLoad)` with the flattening `postLoad` of
/// `loadEntitiesRecursive`: root first, then its passengers (each followed by theirs).
pub fn load_entity_recursive(
    tag: &CompoundTag,
    level: &ServerLevel,
    request: EntitySpawnRequest,
    out: &mut LoadedEntities,
) {
    let Some(entity) = create_and_load(tag, level, request) else {
        return;
    };
    let root_index = out.len();
    let root_id = entity.id();
    out.push(entity);
    for (index, passenger_tag) in tag
        .get_list_or_empty("Passengers")
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.as_compound().map(|t| (i, t)))
    {
        let start = out.len();
        problem_reporter::with_child(
            || format!(".Passengers[{index}]"),
            || load_entity_recursive(passenger_tag, level, request, out),
        );
        if out.len() > start {
            // `passenger.startRiding(entity, true, false)`: the first loaded entity of the subtree is the passenger
            let passenger_id = out[start].id();
            out[start].base_mut().vehicle = Some(root_id);
            add_passenger_link(out, root_index, passenger_id);
        }
    }
}

/// `addPassenger`: players boarding a player-less vehicle go first (server side).
fn add_passenger_link(out: &mut LoadedEntities, vehicle_index: usize, passenger_id: EntityId) {
    let passenger_is_player = out
        .iter()
        .find(|e| e.id() == passenger_id)
        .is_some_and(|e| e.is_player());
    let first_is_player = out[vehicle_index]
        .base()
        .passengers
        .first()
        .and_then(|&id| out.iter().find(|e| e.id() == id))
        .is_some_and(|e| e.is_player());
    let vehicle = out[vehicle_index].base_mut();
    let first = if passenger_is_player && !first_is_player {
        vehicle.passengers.insert(0, passenger_id);
        true
    } else {
        vehicle.passengers.push(passenger_id);
        vehicle.passengers.len() == 1
    };
    if let Some(m) = out[vehicle_index].mob_mut() {
        m.passengers_loaded = true;
    }
    if first {
        let (is_mob, controls) = out
            .iter()
            .find(|e| e.id() == passenger_id)
            .map_or((false, false), |e| {
                (e.mob().is_some(), e.can_control_vehicle())
            });
        if let Some(m) = out[vehicle_index].mob_mut() {
            m.first_passenger_is_mob = is_mob;
            m.first_passenger_controls = is_mob && controls;
        }
    }
}

/// `EntityType.loadEntitiesRecursive(entities, level, reason)`: all entities of a chunk's entity list, flattened.
pub fn load_entities_recursive(
    tags: &[CompoundTag],
    level: &ServerLevel,
    reason: EntitySpawnReason,
) -> LoadedEntities {
    load_entity_list(tags, level, reason, None)
}

/// [`load_entities_recursive`] for a list that a problem collector watches: `name` is the field the list was read from
/// (`.Entities[3]`), none for a bare list (`[3]`).
pub fn load_entity_list(
    tags: &[CompoundTag],
    level: &ServerLevel,
    reason: EntitySpawnReason,
    name: Option<&str>,
) -> LoadedEntities {
    let request = EntitySpawnRequest {
        reason,
        ignore_checks: false,
    };
    let mut out = Vec::new();
    for (index, tag) in tags.iter().enumerate() {
        problem_reporter::with_child(
            || match name {
                Some(name) => format!(".{name}[{index}]"),
                None => format!("[{index}]"),
            },
            || load_entity_recursive(tag, level, request, &mut out),
        );
    }
    out
}

/// `readAdditionalSaveData` parts that need the level, for entities of a chunk load before they are added.
pub fn read_level_dependent_data(entities: &mut LoadedEntities, level: &mut ServerLevel) {
    for entity in entities {
        entity.read_level_dependent_data(level);
    }
}

/// [`read_level_dependent_data`] for a loader that moves the entities afterwards (summon, spawners): the part of loading that
/// `Entity.load` does runs now, while the `addPassenger` of attached passengers (`startRiding`, after the `postLoad` move) is
/// left for the add to the level.
pub fn read_level_dependent_data_before_move(
    entities: &mut LoadedEntities,
    level: &mut ServerLevel,
) {
    for entity in entities {
        let held = entity
            .mob_mut()
            .map(|m| std::mem::take(&mut m.passengers_loaded));
        entity.read_level_dependent_data(level);
        if let (Some(held), Some(m)) = (held, entity.mob_mut()) {
            m.passengers_loaded = held;
        }
    }
}
