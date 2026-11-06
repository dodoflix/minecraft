//! `InteractWithDoor`: open the doors on the path, close them behind unless somebody else is coming through.

use super::{BehaviorBuilder, OneShot};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::memory_module_type::{
    DOORS_TO_CLOSE, NEAREST_LIVING_ENTITIES, PATH,
};
use crate::world::entity::ai::memory::{EntityRef, GlobalPosSet};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers;
use crate::world::level::block::impls::door_block;
use crate::world::level::pathfinder::Node;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::props;
use mc_protocol::codec::GlobalPos;

const COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE: i32 = 20;
const SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN: f64 = 3.0;
const MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS: f64 = 2.0;

/// `state.is(BlockTags.MOB_INTERACTABLE_DOORS, s -> s.getBlock() instanceof DoorBlock)`.
fn is_interactable_door(state: BlockState) -> bool {
    state.is_tag("mob_interactable_doors") && helpers::is_instance_of(state, "DoorBlock")
}

pub fn create() -> OneShot {
    let mut last_checked_node: Option<Node> = None;
    let mut remaining_cooldown: i32 = 0;
    let mut i = BehaviorBuilder::instance();
    let path_memory = i.present(&PATH);
    let doors_memory = i.registered(&DOORS_TO_CLOSE);
    let nearest_entities = i.registered(&NEAREST_LIVING_ENTITIES);
    i.create(move |c| {
        let path = c.get(&path_memory).clone();
        let mut doors: Option<GlobalPosSet> = c.try_get(&doors_memory).cloned();
        if path.not_started() || path.is_done() {
            return false;
        }
        if last_checked_node.as_ref() == Some(&path.get_next_node()) {
            remaining_cooldown = COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE;
        } else {
            remaining_cooldown -= 1;
            if remaining_cooldown > 0 {
                return false;
            }
        }
        last_checked_node = Some(path.get_next_node());
        let from_node = path.get_previous_node();
        let to_node = path.get_next_node();
        if let Some(from_node) = &from_node {
            let from_pos = from_node.as_block_pos();
            let from_state = c.level.get_block_state(from_pos);
            if is_interactable_door(from_state) {
                if !from_state.get(props::OPEN) {
                    set_open_at(Some(c.body().id()), c.level, from_state, from_pos, true);
                }
                doors = Some(remember_door_to_close(
                    c,
                    &doors_memory,
                    doors.take(),
                    from_pos,
                ));
            }
        }
        let to_pos = to_node.as_block_pos();
        let to_state = c.level.get_block_state(to_pos);
        if is_interactable_door(to_state) && !to_state.get(props::OPEN) {
            set_open_at(Some(c.body().id()), c.level, to_state, to_pos, true);
            doors = Some(remember_door_to_close(
                c,
                &doors_memory,
                doors.take(),
                to_pos,
            ));
        }
        if let Some(mut door_set) = doors {
            let entities = c.try_get(&nearest_entities).cloned();
            close_doors_that_i_have_opened_or_passed_through(
                c.level,
                c.body,
                from_node.as_ref(),
                Some(&to_node),
                &mut door_set,
                entities.as_deref(),
            );
            // the set is shared with the memory in vanilla: write the pruned set back
            if c.body().brain().has_memory_value(DOORS_TO_CLOSE.id()) || !door_set.is_empty() {
                c.body_mut()
                    .brain_mut()
                    .set_memory(&DOORS_TO_CLOSE, door_set);
            }
        }
        true
    })
}

/// `closeDoorsThatIHaveOpenedOrPassedThrough(level, body, movingFromNode, movingToNode, doors, nearestEntities)`; the set is
/// pruned in place (vanilla's iterator removal).
pub fn close_doors_that_i_have_opened_or_passed_through(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    moving_from_node: Option<&Node>,
    moving_to_node: Option<&Node>,
    doors: &mut GlobalPosSet,
    nearest_entities: Option<&[EntityRef]>,
) {
    let snapshot: Vec<GlobalPos> = doors.iter().cloned().collect();
    for door_global_pos in snapshot {
        let door_pos = door_global_pos.pos;
        if moving_from_node.is_none_or(|n| n.as_block_pos() != door_pos)
            && moving_to_node.is_none_or(|n| n.as_block_pos() != door_pos)
        {
            if !is_door_too_far_away(level, body, &door_global_pos) {
                let state = level.get_block_state(door_pos);
                if is_interactable_door(state)
                    && state.get(props::OPEN)
                    && !are_other_mobs_coming_through_door(level, body, door_pos, nearest_entities)
                {
                    set_open_at(Some(body.id()), level, state, door_pos, false);
                }
            }
            doors.remove(&door_global_pos);
        }
    }
}

/// `closeDoorsThatIHaveOpenedOrPassedThrough` over the brain's own `DOORS_TO_CLOSE` set (`SleepInBed` passes the memory's set).
pub fn close_doors_in_memory(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    nearest_entities: Option<&[EntityRef]>,
) {
    let Some(mut doors) = body.brain().get_memory(&DOORS_TO_CLOSE).cloned() else {
        return;
    };
    close_doors_that_i_have_opened_or_passed_through(
        level,
        body,
        None,
        None,
        &mut doors,
        nearest_entities,
    );
    body.brain_mut().set_memory(&DOORS_TO_CLOSE, doors);
}

fn are_other_mobs_coming_through_door(
    level: &ServerLevel,
    body: &dyn Entity,
    door_pos: BlockPos,
    nearest_entities: Option<&[EntityRef]>,
) -> bool {
    let Some(entities) = nearest_entities else {
        return false;
    };
    entities
        .iter()
        .filter_map(|r| level.get_entity(r.0))
        .filter(|other| other.get_type() == body.get_type())
        .filter(|other| {
            door_pos.closer_to_center_than(
                &other.position(),
                MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS,
            )
        })
        .any(|other| is_mob_coming_through_door(other, door_pos))
}

fn is_mob_coming_through_door(other: &dyn Entity, door_pos: BlockPos) -> bool {
    let Some(path) = other.brain().get_memory(&PATH) else {
        return false;
    };
    if path.is_done() {
        return false;
    }
    let Some(moving_from_node) = path.get_previous_node() else {
        return false;
    };
    let moving_to_node = path.get_next_node();
    door_pos == moving_from_node.as_block_pos() || door_pos == moving_to_node.as_block_pos()
}

fn is_door_too_far_away(
    level: &ServerLevel,
    body: &dyn Entity,
    door_global_pos: &GlobalPos,
) -> bool {
    door_global_pos.dimension != level.dimension
        || !door_global_pos
            .pos
            .closer_to_center_than(&body.position(), SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN)
}

fn remember_door_to_close(
    c: &mut super::TriggerContext<'_>,
    doors_memory: &super::Registered<GlobalPosSet>,
    doors: Option<GlobalPosSet>,
    door_pos: BlockPos,
) -> GlobalPosSet {
    let global_door_pos = GlobalPos {
        dimension: c.level.dimension.clone(),
        pos: door_pos,
    };
    match doors {
        Some(mut set) => {
            set.insert(global_door_pos);
            set
        }
        None => {
            let mut set = GlobalPosSet::new();
            set.insert(global_door_pos);
            c.set(doors_memory, set.clone());
            set
        }
    }
}

/// `DoorBlock.setOpen(sourceEntity, level, state, pos, shouldOpen)` of the door in `state`.
fn set_open_at(
    source_entity: Option<crate::world::entity::EntityId>,
    level: &mut ServerLevel,
    state: BlockState,
    pos: BlockPos,
    should_open: bool,
) {
    door_block::set_open_at(source_entity, level, state, pos, should_open);
}
