//! `TamableAnimal` + `OwnableEntity`: tame flag, owner, sitting, teleport-to-owner.

use super::{DATA_FLAGS_ID, DATA_OWNERUUID_ID, ext, navigation};
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::BlockGetter;
use crate::world::level::collision::CollisionGetter;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_nbt::CompoundTag;
use uuid::Uuid;

pub const TELEPORT_WHEN_DISTANCE_IS_SQ: f64 = 144.0;

/// Fields of `TamableAnimal` that are not synched data.
#[derive(Default, Debug)]
pub struct TamableData {
    pub ordered_to_sit: bool,
}

fn td_mut(e: &mut dyn Entity) -> &mut TamableData {
    e.mob_mut()
        .and_then(|m| m.tamable.as_mut())
        .expect("tamable animal")
}

/// `isTame()`.
pub fn is_tame(e: &dyn Entity) -> bool {
    (e.base().entity_data.get(DATA_FLAGS_ID) & 4) != 0
}

/// `setTame(isTame, includeSideEffects)`.
pub fn set_tame(e: &mut dyn Entity, is_tame: bool, include_side_effects: bool) {
    let current = e.base().entity_data.get(DATA_FLAGS_ID);
    let new = if is_tame { current | 4 } else { current & !4 };
    e.base_mut().entity_data.set(DATA_FLAGS_ID, new);
    if include_side_effects {
        e.apply_taming_side_effects();
    }
}

/// `isInSittingPose()`.
pub fn is_in_sitting_pose(e: &dyn Entity) -> bool {
    (e.base().entity_data.get(DATA_FLAGS_ID) & 1) != 0
}

/// `setInSittingPose(value)`.
pub fn set_in_sitting_pose(e: &mut dyn Entity, value: bool) {
    let current = e.base().entity_data.get(DATA_FLAGS_ID);
    let new = if value { current | 1 } else { current & !1 };
    e.base_mut().entity_data.set(DATA_FLAGS_ID, new);
}

/// `getOwnerReference()`.
pub fn get_owner_reference(e: &dyn Entity) -> Option<Uuid> {
    e.base().entity_data.get(DATA_OWNERUUID_ID)
}

/// `setOwnerReference(owner)`.
pub fn set_owner_reference(e: &mut dyn Entity, owner: Option<Uuid>) {
    e.base_mut().entity_data.set(DATA_OWNERUUID_ID, owner);
}

/// `setOwner(owner)`.
pub fn set_owner(e: &mut dyn Entity, owner: Option<&dyn Entity>) {
    set_owner_reference(e, owner.map(|o| o.uuid()));
}

/// `OwnableEntity.getOwner()`: the living owner while it is in the level.
pub fn get_owner(e: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    let uuid = get_owner_reference(e)?;
    level
        .get_entity_by_uuid(uuid)
        .filter(|o| o.living().is_some())
        .map(|o| o.id())
}

/// `OwnableEntity.getRootOwner()`.
pub fn get_root_owner(level: &ServerLevel, e: &dyn Entity) -> Option<EntityId> {
    let mut seen: Vec<EntityId> = vec![e.id()];
    let mut owner = get_owner(e, level)?;
    loop {
        let owner_entity = level.get_entity(owner)?;
        if owner_entity
            .mob()
            .and_then(|m| m.tamable.as_ref())
            .is_none()
        {
            return Some(owner);
        }
        let owners_owner = get_owner(owner_entity, level);
        if owners_owner.is_some_and(|o| seen.contains(&o)) {
            return None;
        }
        seen.push(owner);
        match owners_owner {
            Some(o) => owner = o,
            None => return None,
        }
    }
}

/// `isOwnedBy(entity)`.
pub fn is_owned_by(e: &dyn Entity, level: &ServerLevel, entity: &dyn Entity) -> bool {
    get_owner(e, level) == Some(entity.id())
}

/// `tame(player)`.
pub fn tame(e: &mut dyn Entity, level: &mut ServerLevel, player: &ServerPlayer) {
    e.set_tame(Some(&mut *level), true, true);
    set_owner(e, Some(player));
    ext::trigger_tame_animal(level, player.id(), &*e);
}

/// `isOrderedToSit()`.
pub fn is_ordered_to_sit(e: &dyn Entity) -> bool {
    e.mob()
        .and_then(|m| m.tamable.as_ref())
        .is_some_and(|t| t.ordered_to_sit)
}

/// `setOrderedToSit(orderedToSit)`.
pub fn set_ordered_to_sit(e: &mut dyn Entity, ordered_to_sit: bool) {
    td_mut(e).ordered_to_sit = ordered_to_sit;
}

/// `TamableAnimal.addAdditionalSaveData`.
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    if let Some(owner) = get_owner_reference(e) {
        out.put_int_array(
            "Owner",
            mc_core::uuid_util::uuid_to_int_array(&owner).to_vec(),
        );
    }
    out.put_boolean("Sitting", is_ordered_to_sit(e));
}

/// `TamableAnimal.readAdditionalSaveData`.
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    let owner = crate::world::entity::entity::read_uuid(input.get("Owner"));
    match owner {
        Some(owner) => {
            set_owner_reference(e, Some(owner));
            e.set_tame(None, true, false);
        }
        None => {
            set_owner_reference(e, None);
            e.set_tame(None, false, true);
        }
    }
    let sitting = input.get_boolean_or("Sitting", false);
    td_mut(e).ordered_to_sit = sitting;
    set_in_sitting_pose(e, sitting);
}

/// `shouldTryTeleportToOwner()`.
pub fn should_try_teleport_to_owner(e: &dyn Entity, level: &ServerLevel) -> bool {
    get_owner(e, level)
        .and_then(|o| level.get_entity(o))
        .is_some_and(|o| e.base().distance_to_sqr_vec(o.position()) >= TELEPORT_WHEN_DISTANCE_IS_SQ)
}

/// `unableToMoveToOwner()`.
pub fn unable_to_move_to_owner(e: &dyn Entity, level: &ServerLevel) -> bool {
    is_ordered_to_sit(e)
        || e.base().is_passenger()
        || super::leashable::may_be_leashed(e)
        || get_owner(e, level)
            .and_then(|o| level.get_entity(o))
            .is_some_and(|o| o.is_spectator())
}

/// `tryToTeleportToOwner()`.
pub fn try_to_teleport_to_owner(e: &mut dyn Entity, level: &mut ServerLevel) {
    if let Some(owner_pos) = get_owner(e, level)
        .and_then(|o| level.get_entity(o))
        .map(|o| o.block_position())
    {
        teleport_to_around_block_pos(e, level, owner_pos);
    }
}

fn teleport_to_around_block_pos(e: &mut dyn Entity, level: &mut ServerLevel, target_pos: BlockPos) {
    for _ in 0..10 {
        let xd = e.base_mut().random.next_int_between_inclusive(-3, 3);
        let zd = e.base_mut().random.next_int_between_inclusive(-3, 3);
        if xd.abs() >= 2 || zd.abs() >= 2 {
            let yd = e.base_mut().random.next_int_between_inclusive(-1, 1);
            if maybe_teleport_to(
                e,
                level,
                target_pos.x() + xd,
                target_pos.y() + yd,
                target_pos.z() + zd,
            ) {
                return;
            }
        }
    }
}

fn maybe_teleport_to(e: &mut dyn Entity, level: &mut ServerLevel, x: i32, y: i32, z: i32) -> bool {
    if !can_teleport_to(e, &*level, BlockPos::new(x, y, z)) {
        return false;
    }
    let (y_rot, x_rot) = (e.base().y_rot(), e.base().x_rot());
    e.snap_to(
        f64::from(x) + 0.5,
        f64::from(y),
        f64::from(z) + 0.5,
        y_rot,
        x_rot,
    );
    navigation::with_own_navigation_stop(e);
    true
}

fn can_teleport_to(e: &mut dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
    use crate::world::level::pathfinder::PathType;
    if navigation::path_type_static(e, level, pos) != PathType::Walkable {
        return false;
    }
    let below = level.get_block_state(pos.below());
    if !e.can_fly_to_owner()
        && mc_data::tags::get::<Block>("leaves").is_some_and(|t| t.contains(below.block()))
    {
        return false;
    }
    let delta = pos.subtract(&e.block_position());
    level.no_collision(
        Some(e.id()),
        Some(&crate::world::entity::movement::collision_context(e)),
        &e.base().bounding_box().move_(
            f64::from(delta.x()),
            f64::from(delta.y()),
            f64::from(delta.z()),
        ),
    )
}
/// `TamableAnimal.die(source)`: the combat tracker's death message goes to a player owner (`SHOW_DEATH_MESSAGES`).
pub fn die(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    source: &crate::world::damagesource::DamageSource,
) {
    if level.show_death_messages() {
        if let Some(owner) =
            get_owner(e, level).filter(|o| level.get_entity(*o).is_some_and(|o| o.is_player()))
        {
            let message = e
                .living()
                .expect("living")
                .combat_tracker
                .get_death_message(level, e);
            level.with_player(owner, |_, p| p.send_system_message(message.clone()));
        }
    }
    crate::world::entity::living_entity::damage::die(e, level, source);
}

/// `TamableAnimal.feed(player, hand, itemStack, healingFactor, defaultHeal)`.
pub fn feed(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: crate::world::item::InteractionHand,
    stack: &crate::world::item::ItemStack,
    healing_factor: f32,
    default_heal: f32,
) {
    let nutrition = stack
        .get(&mc_protocol::components::FOOD)
        .map(|f| f.nutrition);
    super::interact::use_player_item(level, player, hand);
    e.heal(
        level,
        nutrition.map_or(default_heal, |n| healing_factor * n as f32),
    );
    e.play_eating_sound(level);
}
