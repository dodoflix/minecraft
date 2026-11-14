//! `Mob.registerDebugValues`: the path, goal selector and brain of a mob as debug subscription values.

use crate::server::level::ServerLevel;
use crate::util::debug::brain_dump::take_brain_dump;
use crate::util::debug::{DebugRegistration, DebugValue, EntityDebugGetter, subscriptions};
use crate::world::entity::Entity;
use crate::world::level::pathfinder::{Node, Path, Target};
use mc_protocol::packets::play::{
    DebugGoalInfo, DebugGoalInfoDebugGoal, DebugPath, DebugPathData, DebugPathInfo, DebugPathNode,
    DebugSubscriptionEvent, PathType,
};
use std::sync::Arc;

/// `Mob.registerDebugValues(level, registration)`.
pub fn register_debug_values(
    entity: &dyn Entity,
    registration: &mut DebugRegistration<EntityDebugGetter>,
) {
    if entity.mob().is_none() {
        return;
    }
    registration.register(subscriptions::entity_paths(), entity_path);
    registration.register(subscriptions::goal_selectors(), goal_selector);
    if !entity.brain().is_brain_dead() {
        registration.register(subscriptions::brains(), brain);
    }
}

fn entity_path(level: &ServerLevel, entity: &dyn Entity) -> Option<DebugValue> {
    // `getNavigation()`: the navigation of the vehicle a mob controls
    let (path, max_node_distance) = super::navigation::with_navigation_ref(entity, level, |n| {
        (n.get_path(), n.get_max_distance_to_waypoint())
    })?;
    let path = path?;
    let debug_data = path.debug_data()?;
    let info = DebugPathInfo {
        path: debug_path(&path, &debug_data)?,
        max_node_distance,
    };
    // `Path.equals` compares `debugData` by reference
    Some(DebugValue::with_identity(
        DebugSubscriptionEvent::EntityPaths(info),
        Arc::as_ptr(&debug_data) as usize,
    ))
}

fn goal_selector(_level: &ServerLevel, entity: &dyn Entity) -> Option<DebugValue> {
    let goals = entity
        .mob()?
        .goal_selector
        .get_available_goals()
        .iter()
        .map(|g| DebugGoalInfoDebugGoal {
            priority: g.priority(),
            is_running: g.is_running(),
            name: simple_name(g.goal().name()).to_owned(),
        })
        .collect();
    Some(DebugValue::new(DebugSubscriptionEvent::GoalSelectors(
        DebugGoalInfo { goals },
    )))
}

/// `getClass().getSimpleName()` of a goal named by its binary name (`Zombie$ZombieAttackTurtleEggGoal`): the part after the
/// last `$` without the digits a local class carries; an anonymous class has none.
fn simple_name(name: &str) -> &str {
    name.rsplit('$')
        .next()
        .unwrap_or(name)
        .trim_start_matches(|c: char| c.is_ascii_digit())
}

fn brain(level: &ServerLevel, entity: &dyn Entity) -> Option<DebugValue> {
    take_brain_dump(level, entity).map(|dump| DebugValue::new(DebugSubscriptionEvent::Brains(dump)))
}

fn debug_node(node: &Node) -> DebugPathNode {
    DebugPathNode {
        x: node.x,
        y: node.y,
        z: node.z,
        walked_distance: node.walked_distance,
        cost_malus: node.cost_malus,
        closed: node.closed,
        r#type: PathType::by_id(node.ty as i32),
        f: node.f,
    }
}

fn debug_target(target: &Target) -> DebugPathNode {
    debug_node(&target.node)
}

/// `Path.DEBUG_STREAM_CODEC` source: the path with its captured search; `None` without target nodes (vanilla throws
/// `IllegalStateException("Missing debug data")` there).
fn debug_path(
    path: &Path,
    data: &crate::world::level::pathfinder::path::DebugData,
) -> Option<DebugPath> {
    if data.target_nodes.is_empty() {
        return None;
    }
    Some(DebugPath {
        reached: path.can_reach(),
        next_node_index: path.get_next_node_index(),
        target: path.get_target(),
        nodes: path.nodes().iter().map(debug_node).collect(),
        debug_data: DebugPathData {
            open_set: data.open_set.iter().map(debug_node).collect(),
            closed_set: data.closed_set.iter().map(debug_node).collect(),
            target_nodes: data.target_nodes.iter().map(debug_target).collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::simple_name;

    #[test]
    fn simple_names_follow_getsimplename() {
        assert_eq!(simple_name("PanicGoal"), "PanicGoal");
        assert_eq!(
            simple_name("Zombie$ZombieAttackTurtleEggGoal"),
            "ZombieAttackTurtleEggGoal"
        );
        assert_eq!(simple_name("Fox$1FoxGoal"), "FoxGoal");
        assert_eq!(simple_name("Mob$1"), "");
    }
}
