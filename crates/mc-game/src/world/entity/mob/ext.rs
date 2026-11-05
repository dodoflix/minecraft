//! `Entity`/`LivingEntity`/`Level` helpers the mob code needs and that have no home elsewhere yet (each is a vanilla
//! method of the same name; they move to their owners' modules if those grow them).

use crate::server::level::ServerLevel;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
use mc_core::ident::Identifier;
use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;
use mc_protocol::components::common::RegistrySet;

/// `LivingEntity.hasLineOfSight(target)`.
pub fn has_line_of_sight(level: &ServerLevel, viewer: &dyn Entity, target: &dyn Entity) -> bool {
    has_line_of_sight_with(level, viewer, target, ClipBlock::Collider, ClipFluid::None, target.base().get_eye_y())
}

/// `LivingEntity.hasLineOfSight(target, blockCollidingContext, fluidCollidingContext, eyeHeight)`.
pub fn has_line_of_sight_with(level: &ServerLevel, viewer: &dyn Entity, target: &dyn Entity, block: ClipBlock, fluid: ClipFluid, eye_height: f64) -> bool {
    let from = Vec3::new(viewer.base().x(), viewer.base().get_eye_y(), viewer.base().z());
    let to = Vec3::new(target.base().x(), eye_height, target.base().z());
    if to.distance_to(from) > 128.0 {
        return false;
    }
    let context = ClipContext::new(from, to, block, fluid, crate::world::entity::movement::collision_context(viewer));
    level.clip(&context).miss
}

/// `Entity.isAlliedTo(other)` (`this == other || considersEntityAsAlly(other) || other.considersEntityAsAlly(this)`), with
/// `TamableAnimal.getTeam()/considersEntityAsAlly` (tamed pets share their root owner's team).
pub fn is_allied_to(level: &ServerLevel, source: &dyn Entity, other: &dyn Entity) -> bool {
    source.id() == other.id() || considers_entity_as_ally(level, source, other) || considers_entity_as_ally(level, other, source)
}

/// The scoreboard name of the team entity `e` plays for (`Entity.getTeam()`, tamed pets: their root owner's).
fn team_member_name(level: &ServerLevel, e: &dyn Entity) -> Option<String> {
    let scoreboard = level.scoreboard();
    if scoreboard.get_players_team(&e.get_scoreboard_name()).is_some() {
        return Some(e.get_scoreboard_name());
    }
    if e.mob().and_then(|m| m.tamable.as_ref()).is_some() && super::tamable::is_tame(e) {
        let owner = super::tamable::get_root_owner(level, e)?;
        let owner = level.get_entity(owner)?;
        if scoreboard.get_players_team(&owner.get_scoreboard_name()).is_some() {
            return Some(owner.get_scoreboard_name());
        }
    }
    None
}

/// `Entity.considersEntityAsAlly(other)`: `isAlliedTo(other.getTeam())`, tamed pets: the root owner is an ally and
/// delegates to the owner's own rule.
fn considers_entity_as_ally(level: &ServerLevel, e: &dyn Entity, other: &dyn Entity) -> bool {
    if e.mob().and_then(|m| m.tamable.as_ref()).is_some() && super::tamable::is_tame(e) {
        if let Some(owner_id) = super::tamable::get_root_owner(level, e) {
            if owner_id == other.id() {
                return true;
            }
            if let Some(owner) = level.get_entity(owner_id) {
                return considers_entity_as_ally(level, owner, other);
            }
        }
    }
    let scoreboard = level.scoreboard();
    let (Some(own), other_name) = (team_member_name(level, e), team_member_name(level, other)) else { return false };
    scoreboard
        .get_players_team(&own)
        .is_some_and(|team| team.is_allied_to(other_name.as_deref().and_then(|n| scoreboard.get_players_team(n))))
}

/// `RegistrySet<EntityType>.contains(type)`.
pub fn entity_set_contains(set: &RegistrySet<EntityType>, ty: EntityType) -> bool {
    match set {
        RegistrySet::Entries(v) => v.contains(&ty),
        RegistrySet::Tag(id) => mc_data::tags::get::<EntityType>(&id.to_string()).is_some_and(|t| t.contains(ty)),
    }
}

/// `LivingEntity.getVisibilityPercent(level, targetingEntity)`.
pub fn get_visibility_percent(entity: &dyn Entity, targeting_entity: Option<&dyn Entity>) -> f64 {
    use mc_protocol::components::values::basic::MOB_VISIBILITY;
    use mc_protocol::components::values::combat::EQUIPPABLE;
    let mut visibility_percent = 1.0;
    if entity.is_discrete() {
        visibility_percent *= 0.8;
    }
    if entity.is_invisible() {
        let mut cover_percentage = entity.get_armor_cover_percentage();
        if cover_percentage < 0.1 {
            cover_percentage = 0.1;
        }
        visibility_percent *= 0.7 * f64::from(cover_percentage);
    }
    if let Some(targeting) = targeting_entity {
        for slot in EquipmentSlot::VALUES {
            let item = entity.get_item_by_slot(slot);
            if let Some(visibility) = item.get(&MOB_VISIBILITY) {
                if let Some(equippable) = item.get(&EQUIPPABLE) {
                    if slot == EquipmentSlot::from_protocol(equippable.slot) && entity_set_contains(&visibility.targeting_entity_types, targeting.get_type()) {
                        visibility_percent *= f64::from(visibility.visibility);
                    }
                }
            }
        }
    }
    mc_core::math::mth::clamp_f64(visibility_percent, 0.0, 10.0)
}

/// `EntityGetter.getNearbyEntities(class, conditions, source, box)`: entities in `bb` accepted by `class_test` and
/// `conditions` (the source itself is never returned: it is taken out of the level while it runs).
pub fn get_nearby_entities(
    level: &ServerLevel,
    class_test: impl Fn(&dyn Entity) -> bool,
    conditions: &TargetingConditions,
    source: &dyn Entity,
    bb: &AABB,
) -> Vec<EntityId> {
    level.get_entities_of_class(bb, |e| class_test(e) && e.living().is_some(), |e| conditions.test(level, Some(source), e))
}

/// `LevelReader`/`EntityGetter.getNearestEntity(entities, conditions, source, x, y, z)`.
pub fn get_nearest_entity(level: &ServerLevel, entities: &[EntityId], conditions: &TargetingConditions, source: &dyn Entity, x: f64, y: f64, z: f64) -> Option<EntityId> {
    let mut best = -1.0;
    let mut result = None;
    for &id in entities {
        let Some(e) = level.get_entity(id) else { continue };
        if conditions.test(level, Some(source), e) {
            let dist = e.base().distance_to_sqr_xyz(x, y, z);
            if best == -1.0 || dist < best {
                best = dist;
                result = Some(id);
            }
        }
    }
    result
}

/// `getNearestPlayer(conditions, source, x, y, z)`.
pub fn get_nearest_player(level: &ServerLevel, conditions: &TargetingConditions, source: &dyn Entity, x: f64, y: f64, z: f64) -> Option<EntityId> {
    let players: Vec<EntityId> = level.players().to_vec();
    get_nearest_entity(level, &players, conditions, source, x, y, z)
}

/// `LivingEntity.dropFromGiftLootTable(level, table, consumer)` (`dropFromLootTable`): the table's drops (no loot seed) go
/// to `consume`; returns whether there were any.
pub fn drop_from_gift_loot_table(
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    table: &Identifier,
    mut consume: impl FnMut(&mut ServerLevel, &mut dyn Entity, crate::world::item::ItemStack),
) -> bool {
    let loot = loading::get_loot_table(&level.registries, table);
    let items = {
        let this: &dyn Entity = &*entity;
        let params = LootParams::builder(level).this_entity(this).origin(this.position()).create(&LootContextParamSets::GIFT);
        loot.get_random_items(&params)
    };
    if items.is_empty() {
        return false;
    }
    for stack in items {
        consume(level, entity, stack);
    }
    true
}

/// `player.isCreative()` for an entity that may be a player.
pub fn is_creative_player(e: &dyn Entity) -> bool {
    e.as_any().downcast_ref::<crate::world::entity::ServerPlayer>().is_some_and(|p| p.is_creative())
}

/// `Entity.getLightLevelDependentMagicValue()`.
pub fn get_light_level_dependent_magic_value_at_entity(level: &ServerLevel, e: &dyn Entity) -> f32 {
    use crate::world::level::LevelReader;
    let b = e.base();
    if level.has_chunk(b.block_position().x() >> 4, b.block_position().z() >> 4) {
        level.get_light_level_dependent_magic_value(mc_core::pos::BlockPos::containing(b.x(), b.get_eye_y(), b.z()))
    } else {
        0.0
    }
}

/// `EnvironmentAttributes.MONSTERS_BURN` at `pos`.
pub fn monsters_burn_at(level: &ServerLevel, pos: Vec3) -> bool {
    level.attribute_bool_vec(crate::world::attribute::attributes::MONSTERS_BURN, &pos)
}

/// `CriteriaTriggers.BRED_ANIMALS.trigger(cause, parent, partner, offspring)`. SEAM(ImAdvancements): the criteria
/// trigger engine lives in the advancements slice; wired by replacing this body.
pub fn trigger_bred_animals(_level: &mut ServerLevel, _cause: EntityId, _parent: &dyn Entity, _partner: EntityId, _offspring: Option<&dyn Entity>) {}

/// `LeashFenceKnotEntity.getOrCreateKnot(level, pos)`. SEAM(ItemsMenusScope): the knot entity belongs to the items slice;
/// they replace this body.
pub fn get_or_create_leash_knot(_level: &mut ServerLevel, _pos: mc_core::pos::BlockPos) -> Option<EntityId> {
    None
}

/// `CriteriaTriggers.TAME_ANIMAL.trigger(player, animal)`. SEAM(ImAdvancements).
pub fn trigger_tame_animal(_level: &mut ServerLevel, _player: EntityId, _animal: EntityId) {}

/// `oldBrain` `ANGRY_AT` memory copy of `ConversionType.convertCommon`. SEAM(BrainFramework): the brain memories are
/// owned by the brain slice; they replace this body once `Brain` lands.
pub fn copy_angry_at_memory(_from: &dyn Entity, _to: &mut dyn Entity) {}

/// `CriteriaTriggers.PLAYER_SHEARED_EQUIPMENT.trigger(player, item, mob)`. SEAM(ImAdvancements).
pub fn trigger_player_sheared_equipment(_level: &mut ServerLevel, _player: EntityId, _mob: EntityId) {}

/// `SpawnEggItem.spawnOffspringFromSpawnEgg(...)` when `stack` is a spawn egg: `None` = not a spawn egg, `Some(None)` = no
/// offspring was created. SEAM(ItemsMenusScope): the spawn egg item lives in the items slice.
pub fn spawn_offspring_from_spawn_egg(_level: &mut ServerLevel, _player: &mut crate::world::entity::ServerPlayer, _mob: &mut dyn Entity, _stack: &crate::world::item::ItemStack) -> Option<Option<EntityId>> {
    None
}

// ==== [GoalsA] additions ====

/// `EntitySelector.NO_CREATIVE_OR_SPECTATOR`.
pub fn no_creative_or_spectator(e: &dyn Entity) -> bool {
    !(e.is_player() && (e.is_spectator() || is_creative_player(e)))
}
