//! `net.minecraft.util.debug.DebugBrainDump.takeBrainDump`: the brain state a debug client shows over a mob.

use super::entity_name::{get_entity_name, get_entity_name_for_uuid};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, SimpleContainer};
use crate::world::entity::Entity;
use crate::world::entity::EntityId;
use crate::world::entity::ai::behavior::position_tracker::PositionTracker;
use crate::world::entity::ai::brain::java_hash_map_order;
use crate::world::entity::ai::gossip::gossip_type::GossipType;
use crate::world::entity::ai::memory::memory_module_type::{
    HEARD_BELL_TIME, HOME, JOB_SITE, MEETING_POINT, MemoryModuleType, POTENTIAL_JOB_SITE,
};
use crate::world::entity::ai::memory::value::MemoryValue;
use mc_core::java_collections::{JavaHashMap, block_pos_hash};
use mc_core::math::java::{java_double_to_string, java_string_hash};
use mc_core::pos::BlockPos;
use mc_protocol::codec::GlobalPos;
use mc_protocol::packets::play::DebugBrainDump;

/// What `takeBrainDump` reads off the mob besides its brain: the `Villager` profession, experience, gossips and golem
/// wish, the `Warden` anger level and the contents of an `InventoryCarrier`. A mob without these keeps the defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct BrainDumpExtras {
    pub profession: String,
    pub xp: i32,
    pub inventory: String,
    pub wants_golem: bool,
    pub anger_level: i32,
    pub gossips: Vec<String>,
}

impl Default for BrainDumpExtras {
    fn default() -> Self {
        Self {
            profession: String::new(),
            xp: 0,
            inventory: String::new(),
            wants_golem: false,
            anger_level: -1,
            gossips: Vec::new(),
        }
    }
}

/// `takeBrainDump(level, entity)`; `None` for an entity without a living part.
pub fn take_brain_dump(level: &ServerLevel, entity: &dyn Entity) -> Option<DebugBrainDump> {
    let living = entity.living()?;
    let brain = &living.brain;
    let extras = entity.debug_brain_dump_extras(level);
    let game_time = level.game_time();
    let activity_names: Vec<&str> = brain
        .get_active_activities()
        .iter()
        .map(|a| a.get_name())
        .collect();
    // `activeActivities` is a `HashSet<Activity>` whose hash is the name's `String.hashCode`
    let hashes: Vec<i32> = activity_names.iter().map(|n| java_string_hash(n)).collect();
    let activities = java_hash_map_order(&hashes)
        .into_iter()
        .map(|i| activity_names[i].to_owned())
        .collect();
    Some(DebugBrainDump {
        name: get_entity_name(entity),
        profession: extras.profession,
        xp: extras.xp,
        health: living.get_health(),
        max_health: living.get_max_health(),
        inventory: extras.inventory,
        wants_golem: extras.wants_golem,
        anger_level: extras.anger_level,
        activities,
        behaviors: brain.get_running_behaviors(),
        memories: memory_descriptions(level, entity, game_time),
        gossips: extras.gossips,
        pois: known_block_positions(entity, &[&JOB_SITE, &HOME, &MEETING_POINT]),
        potential_pois: known_block_positions(entity, &[&POTENTIAL_JOB_SITE]),
    })
}

/// `getKnownBlockPositions(brain, memories..)`: a `Collectors.toSet()` (`HashSet<BlockPos>`) in its iteration order.
fn known_block_positions(
    entity: &dyn Entity,
    memories: &[&MemoryModuleType<GlobalPos>],
) -> Vec<BlockPos> {
    let brain = entity.brain();
    let mut set: JavaHashMap<BlockPos, ()> = JavaHashMap::new(block_pos_hash);
    for memory in memories {
        if let Some(Some(global)) = brain.get_memory_internal(memory) {
            set.insert(global.pos, ());
        }
    }
    set.keys().copied().collect()
}

/// `getMemoryDescriptions(level, body, timestamp)`: sorted `name: value` lines. The `(ttl: ..)` suffix prints the game
/// time, not the time to live (vanilla passes `timestamp` to `collectResult` for expiring memories).
fn memory_descriptions(level: &ServerLevel, entity: &dyn Entity, timestamp: i64) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    entity.brain().for_each(|id, value| {
        let name = id.name();
        let description = match value {
            None => "-".to_owned(),
            Some((value, ttl)) => {
                if id == HEARD_BELL_TIME.id() {
                    let since = match value {
                        MemoryValue::Long(t) => timestamp - t,
                        _ => 0,
                    };
                    format!("{since} ticks ago")
                } else if ttl.is_some() {
                    format!("{} (ttl: {timestamp})", short_description(level, value))
                } else {
                    short_description(level, value)
                }
            }
        };
        result.push(truncate(format!("{name}: {description}"), 255));
    });
    result.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    result
}

/// `StringUtil.truncateStringIfNecessary(s, max, true)` (lengths in UTF-16 units).
fn truncate(s: String, max: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    if units.len() <= max {
        return s;
    }
    let mut out = String::from_utf16_lossy(&units[..max - 3]);
    out.push_str("...");
    out
}

/// `BlockPos.toString()`.
fn block_pos_text(p: BlockPos) -> String {
    format!("BlockPos{{x={}, y={}, z={}}}", p.x(), p.y(), p.z())
}

fn entity_name_of(level: &ServerLevel, id: EntityId) -> String {
    level
        .get_entity_or_recently_removed(id)
        .map_or_else(|| "-".to_owned(), get_entity_name)
}

fn collection(level: &ServerLevel, items: impl Iterator<Item = String>) -> String {
    let _ = level;
    format!("[{}]", items.collect::<Vec<_>>().join(", "))
}

/// `getShortDescription(level, obj)`.
fn short_description(level: &ServerLevel, value: &MemoryValue) -> String {
    let global = |g: &GlobalPos| block_pos_text(g.pos);
    let tracker = |t: &PositionTracker| match t {
        PositionTracker::Entity(e) => entity_name_of(level, e.get_entity()),
        PositionTracker::Block(_) => block_pos_text(t.current_block_position(level)),
    };
    match value {
        MemoryValue::Bool(b) => b.to_string(),
        MemoryValue::Int(i) => i.to_string(),
        MemoryValue::Long(i) => i.to_string(),
        MemoryValue::Unit(_) => "INSTANCE".to_owned(),
        MemoryValue::GlobalPos(g) => global(g),
        MemoryValue::GlobalPosList(l) => collection(level, l.iter().map(global)),
        MemoryValue::GlobalPosSet(s) => collection(level, s.iter().map(global)),
        MemoryValue::BlockPos(p) => block_pos_text(*p),
        MemoryValue::Vec3(v) => format!(
            "({}, {}, {})",
            java_double_to_string(v.x),
            java_double_to_string(v.y),
            java_double_to_string(v.z)
        ),
        MemoryValue::Uuid(u) => level
            .get_entity_by_uuid(*u)
            .map_or_else(|| "-".to_owned(), get_entity_name),
        MemoryValue::UuidList(l) => collection(
            level,
            l.iter().map(|u| {
                level
                    .get_entity_by_uuid(*u)
                    .map_or_else(|| "-".to_owned(), get_entity_name)
            }),
        ),
        MemoryValue::Entity(e) => entity_name_of(level, e.0),
        MemoryValue::EntityList(l) => {
            collection(level, l.iter().map(|e| entity_name_of(level, e.0)))
        }
        MemoryValue::DamageSource(d) => match d.0.get_entity() {
            Some(id) => entity_name_of(level, id),
            None => format!("DamageSource ({})", d.0.get_msg_id()),
        },
        MemoryValue::WalkTarget(w) => tracker(w.get_target()),
        MemoryValue::PositionTracker(t) => tracker(t),
        MemoryValue::Path(p) => p.to_string(),
        MemoryValue::NearestVisibleLivingEntities(n) => collection(
            level,
            n.nearby_entities()
                .iter()
                .map(|e| entity_name_of(level, e.0)),
        ),
        MemoryValue::SpearStatus(s) => format!("{s:?}").to_uppercase(),
    }
}

/// `inventory.isEmpty() ? "" : inventory.toString()` (`SimpleContainer.toString`: the non-empty stacks).
pub fn inventory_string(container: &SimpleContainer) -> String {
    let stacks: Vec<String> = (0..container.get_container_size())
        .map(|i| container.get_item(i))
        .filter(|s| !s.is_empty())
        .map(|s| format!("{} {}", s.count(), s.item().name()))
        .collect();
    if stacks.is_empty() {
        String::new()
    } else {
        format!("[{}]", stacks.join(", "))
    }
}

/// `getVillagerGossips(villager)`: `gossipee: TYPE: value` per entry.
pub fn gossip_lines(entries: Vec<(uuid::Uuid, Vec<(GossipType, i32)>)>) -> Vec<String> {
    let mut out = Vec::new();
    for (uuid, types) in entries {
        let name = get_entity_name_for_uuid(uuid);
        for (gossip_type, value) in types {
            out.push(format!(
                "{name}: {}: {value}",
                gossip_type.id().to_ascii_uppercase()
            ));
        }
    }
    out
}
