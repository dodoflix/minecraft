//! `EntityType` factories (`EntityType.EntityFactory`) and the loading/creation entry points
//! (`EntityType.create`, `loadEntityRecursive`, `loadEntitiesRecursive`).
//!
//! Concrete entity classes register a factory through [`ENTITY_FACTORIES`] from the module that implements them.
//! Types without an implementation yet are created as `PlainEntity`, which keeps the
//! raw NBT it was loaded from so saved worlds round trip losslessly.

use super::plain::PlainEntity;
use super::{Entity, EntityId};
use crate::server::level::ServerLevel;
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

/// Whether a concrete implementation (not the [`PlainEntity`] fallback) is registered for `ty`.
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
    Some(instantiate(ty, level.next_entity_id(), &level.registries))
}

/// `factory.create(type, level)` with an explicit entity id and the registries: usable without a `ServerLevel`
/// (worldgen worker threads); no spawn checks.
pub fn instantiate(
    ty: EntityType,
    id: EntityId,
    registries: &crate::registry::RegistryAccess,
) -> Box<dyn Entity> {
    match factories().get(&ty) {
        Some(reg) => (reg.create)(ty, id, registries),
        // Not implemented: every entity type without a registered factory (see the README's entity-type gaps). Vanilla
        // creates the type's own class; this creates an inert placeholder that keeps its NBT.
        None => Box::new(PlainEntity::new(ty, id)),
    }
}

/// `EntityType.by(input)`: the type named by the `id` tag.
pub fn by(tag: &CompoundTag) -> Option<EntityType> {
    let id = tag.get_string("id")?;
    let ident = Identifier::parse(id).ok()?;
    EntityType::by_name(&ident.to_string())
}

/// `StructureTemplate.placeEntities` for a template entity with `finalizeEntities` set: `EntityType.create` from the tag
/// (`STRUCTURE`), the body and head rotation of the tag, `Mob.finalizeSpawn`, and the saved result `addFreshEntityWithPassengers`
/// stores in the chunk. `None` when the entity cannot be created (vanilla skips it).
pub fn finalize_structure_entity(
    tag: &CompoundTag,
    level: &mut dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
    difficulty: crate::world::difficulty_instance::DifficultyInstance,
) -> Option<CompoundTag> {
    let ty = by(tag)?;
    let mut entity = level.create_entity(
        ty,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Structure,
            ignore_checks: false,
        },
    )?;
    entity.load(tag).ok()?;
    entity.resolve_registry_refs(level.registry_access());
    let y_rot = entity.base().y_rot();
    if let Some(living) = entity.living_mut() {
        living.y_body_rot = y_rot;
        living.y_head_rot = y_rot;
    }
    if entity.is_mob() {
        entity.finalize_spawn(level, difficulty, EntitySpawnReason::Structure, None);
    }
    let pending = entity
        .mob_mut()
        .map(|m| std::mem::take(&mut m.pending_passengers))
        .unwrap_or_default();
    let lookup = |id| pending.iter().find(|p| p.id() == id).map(|p| &**p);
    super::entity::save(&*entity, &lookup)
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
    let Some(ty) = by(tag) else {
        log::warn!(
            "Skipping Entity with id {}",
            tag.get_string("id").unwrap_or("[invalid]")
        );
        return None;
    };
    let mut entity = create(ty, level, request)?;
    match entity.load(tag) {
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
    for passenger_tag in tag
        .get_list_or_empty("Passengers")
        .iter()
        .filter_map(|t| t.as_compound())
    {
        let start = out.len();
        load_entity_recursive(passenger_tag, level, request, out);
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
    let request = EntitySpawnRequest {
        reason,
        ignore_checks: false,
    };
    let mut out = Vec::new();
    for tag in tags {
        load_entity_recursive(tag, level, request, &mut out);
    }
    out
}

/// `readAdditionalSaveData` parts that need the level, for entities of a chunk load before they are added.
pub fn read_level_dependent_data(entities: &mut LoadedEntities, level: &mut ServerLevel) {
    for entity in entities {
        entity.read_level_dependent_data(level);
    }
}
