//! `Entity`/`LivingEntity`/`Level` helpers the mob code needs and that have no home elsewhere yet (each is a vanilla
//! method of the same name; they move to their owners' modules if those grow them).

use crate::server::level::ServerLevel;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
use mc_core::ident::Identifier;
use mc_core::math::mth;
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
pub(super) fn team_member_name(level: &ServerLevel, e: &dyn Entity) -> Option<String> {
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
/// offspring was created (`stack` is the hand stack: consumed on success).
pub fn spawn_offspring_from_spawn_egg(level: &mut ServerLevel, player: &mut crate::world::entity::ServerPlayer, mob: &mut dyn Entity, stack: &mut crate::world::item::ItemStack) -> Option<Option<EntityId>> {
    if !stack.item().data().classes.contains(&"SpawnEggItem") {
        return None;
    }
    let position = mob.position();
    let ty = mob.get_type();
    Some(crate::world::item::spawn_egg_item::spawn_offspring_from_spawn_egg(player, mob, ty, level, position, stack))
}

// ==== [GoalsA] additions ====

/// `EntitySelector.NO_CREATIVE_OR_SPECTATOR`.
pub fn no_creative_or_spectator(e: &dyn Entity) -> bool {
    !(e.is_player() && (e.is_spectator() || is_creative_player(e)))
}

// ==== [Cubes] additions ====

/// `Mob.lookAt(entity, yMax, xMax)`: turns the mob's own rotation toward `entity` (not through the look control).
pub fn mob_look_at(mob: &mut dyn Entity, entity: &dyn Entity, y_max: f32, x_max: f32) {
    let xd = entity.base().x() - mob.base().x();
    let zd = entity.base().z() - mob.base().z();
    let yd = if entity.living().is_some() {
        entity.base().get_eye_y() - mob.base().get_eye_y()
    } else {
        let bb = entity.base().bounding_box();
        (bb.min_y + bb.max_y) / 2.0 - mob.base().get_eye_y()
    };
    let sd = (xd * xd + zd * zd).sqrt();
    let y_rot_d = (mth::atan2(zd, xd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI)) as f32 - 90.0;
    let x_rot_d = (-(mth::atan2(yd, sd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI))) as f32;
    let x_rot = mob_rotlerp(mob.base().x_rot(), x_rot_d, x_max);
    mob.set_x_rot(x_rot);
    let y_rot = mob_rotlerp(mob.base().y_rot(), y_rot_d, y_max);
    mob.set_y_rot(y_rot);
}

/// `Mob.rotlerp(a, b, max)`.
fn mob_rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut diff = mth::wrap_degrees_f32(b - a);
    if diff > max {
        diff = max;
    }
    if diff < -max {
        diff = -max;
    }
    a + diff
}

/// `Mob.DEFAULT_ATTACK_REACH`.
fn default_attack_reach() -> f64 {
    f64::from(2.04f32).sqrt() - f64::from(0.6f32)
}

/// `Mob.getAttackBoundingBox(horizontalExpansion)`.
pub fn get_attack_bounding_box(level: &ServerLevel, mob: &dyn Entity, horizontal_expansion: f64) -> AABB {
    let own = mob.base().bounding_box();
    let aabb = match mob.base().vehicle.and_then(|v| level.get_entity(v)) {
        Some(vehicle) => {
            let mount = vehicle.base().bounding_box();
            AABB::new(own.min_x.min(mount.min_x), own.min_y, own.min_z.min(mount.min_z), own.max_x.max(mount.max_x), own.max_y, own.max_z.max(mount.max_z))
        }
        None => own,
    };
    aabb.inflate(horizontal_expansion, 0.0, horizontal_expansion)
}

/// `Mob.isWithinMeleeAttackRange(target)`.
pub fn is_within_melee_attack_range(level: &ServerLevel, mob: &dyn Entity, target: &dyn Entity) -> bool {
    let attack_range = mob.living().and_then(|l| l.use_item.get(&mc_protocol::components::ATTACK_RANGE).cloned());
    let (max_range, min_range) = match attack_range {
        None => (default_attack_reach(), 0.0),
        Some(range) => (f64::from(range.max_reach * range.mob_factor), f64::from(range.min_reach * range.mob_factor)),
    };
    let hitbox = target.base().bounding_box();
    get_attack_bounding_box(level, mob, max_range).intersects_aabb(&hitbox) && (min_range <= 0.0 || !get_attack_bounding_box(level, mob, min_range).intersects_aabb(&hitbox))
}

/// `LivingEntity.getDimensions(pose)` (final): `getDefaultDimensions(pose).scale(getScale())`, fixed dimensions while sleeping.
pub fn living_dimensions(e: &dyn Entity, pose: mc_data::enums::Pose, default_dimensions: crate::world::entity::EntityDimensions) -> crate::world::entity::EntityDimensions {
    if pose == mc_data::enums::Pose::Sleeping {
        return crate::world::entity::EntityDimensions::fixed(0.2, 0.2).with_eye_height(0.2);
    }
    let scale = e.living().map_or(1.0, |l| l.get_attribute_value(mc_data::Attribute::SCALE) as f32);
    default_dimensions.scale(scale)
}

/// `LivingEntity.getScale()` (`SCALE` attribute) times `getAgeScale()`.
pub fn get_scale_times_age_scale(e: &dyn Entity) -> f32 {
    let scale = e.living().map_or(1.0, |l| l.get_attribute_value(mc_data::Attribute::SCALE) as f32);
    scale * if e.is_baby() { 0.5 } else { 1.0 }
}

/// `Entity.distanceTo(entity)` (float math).
pub fn distance_to(a: &dyn Entity, b: &dyn Entity) -> f32 {
    let f = (a.base().x() - b.base().x()) as f32;
    let g = (a.base().y() - b.base().y()) as f32;
    let h = (a.base().z() - b.base().z()) as f32;
    mc_core::math::mth::sqrt(f * f + g * g + h * h)
}

/// `Entity.hasMovedHorizontallyRecently()`.
pub fn has_moved_horizontally_recently(level: &ServerLevel, e: &dyn Entity) -> bool {
    e.get_known_speed(level).horizontal_distance().abs() > f64::from(1.0E-5f32)
}

// ==== [FarmAnimals] additions ====

/// `Level.playSound(except, entity, sound, source, volume, pitch)`: the entity-bound sound packet with the level's sound seed.
pub fn play_sound_from_entity(level: &mut ServerLevel, except: Option<EntityId>, entity: EntityId, sound: mc_data::SoundEvent, source: mc_protocol::packets::play::SoundSource, volume: f32, pitch: f32) {
    use mc_core::random::RandomSource;
    let seed = level.tick_state.sound_seed_generator.next_long();
    level.play_seeded_sound_from_entity(except, entity, sound, source, volume, pitch, seed);
}

/// `LivingEntity.dropFromShearingLootTable(level, table, tool, consumer)`: the table's drops (`LootContextParamSets.SHEARING`)
/// go to `consume`; returns whether there were any.
pub fn drop_from_shearing_loot_table(
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    table: &Identifier,
    tool: &crate::world::item::ItemStack,
    mut consume: impl FnMut(&mut ServerLevel, &mut dyn Entity, crate::world::item::ItemStack),
) -> bool {
    let loot = loading::get_loot_table(&level.registries, table);
    let items = {
        let this: &dyn Entity = &*entity;
        let params = LootParams::builder(level).this_entity(this).origin(this.position()).tool(tool).create(&LootContextParamSets::SHEARING);
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

// ==== [Monsters2] additions ====

/// `me.isLookingAtMe(target, coneSize, adjustForDistance, seeThroughTransparentBlocks, gazeHeights...)`: whether `target` (a
/// living entity, e.g. the player) looks at `me`.
pub fn is_looking_at_me(level: &ServerLevel, me: &dyn Entity, target: &dyn Entity, cone_size: f64, adjust_for_distance: bool, see_through_transparent_blocks: bool, gaze_heights: &[f64]) -> bool {
    let look = target.get_view_vector(1.0).normalize();
    for &gaze_height in gaze_heights {
        let dir = Vec3::new(me.base().x() - target.base().x(), gaze_height - target.base().get_eye_y(), me.base().z() - target.base().z());
        let dist = dir.length();
        let dir = dir.normalize();
        let dot = look.dot(dir);
        let block = if see_through_transparent_blocks { ClipBlock::Visual } else { ClipBlock::Collider };
        if dot > 1.0 - cone_size / (if adjust_for_distance { dist } else { 1.0 }) && has_line_of_sight_with(level, target, me, block, ClipFluid::None, gaze_height) {
            return true;
        }
    }
    false
}
