//! Extension point of the scenario replay harness (`tests/common/entity_scenario.rs`): extra `key=value` fields of the
//! oracle's `E` lines.
//!
//! The vanilla oracle (`tests/oracle/EntityOracleDriver.java`) prints, for `mob:REF` / `mobs` dumps, the plain entity fields
//! followed by the mob fields (`target= aggr= nav= goals= mem= ...`, see its header) and per-type fields (`vdata=` ...).
//! Everything beyond the base entity/living/player/item fields the harness core knows is produced by registrations of this
//! slice, so the owner of a mob adds its fields next to the mob without touching the harness:
//!
//! ```ignore
//! #[linkme::distributed_slice(SCENARIO_DUMP_EXTRA)]
//! static ZOMBIE_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
//!     order: 20,
//!     applies: |e| e.get_type() == EntityType::ZOMBIE,
//!     fields: |e, _level, ctx| vec![("conv".into(), "0".into())],
//! };
//! ```
//! Registrations run in ascending `order` (ties: registration order); their fields are appended, in the returned order, to the
//! `E` line of the rich dumps (`mob:REF`, `mobs`, `spawnpass`), exactly like the driver's `describeMob`.
//! Values follow the driver's value grammar (see the driver header): booleans `0`/`1`, entity references through
//! [`ScenarioDumpCtx::name`], floats via `java_float_to_string`/`{:?}`, `-` for "none".
//!
//! Order reserved for the mob spine: `order 10` = the generic mob fields (`target hurtby aggr noai left persist canpickup home leash
//! speed zza xxa yya head body nav stuck navspeed ntarget path nodes goals tgoals acts mem equip attrs`), `11` = AgeableMob (`age
//! baby forcedage`), `12` = Animal (`love lovecause`), `13` = TamableAnimal (`tame owner sitting ordered`); concrete types use >= 20.

use super::{Entity, EntityId};
use crate::server::level::ServerLevel;
use linkme::distributed_slice;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Name resolution of the running scenario (`refs` of the DSL: the names given to `spawn`/`spawn_player`/..., or the automatic
/// `type_path#n`).
pub struct ScenarioDumpCtx<'a> {
    pub names: &'a BTreeMap<EntityId, String>,
    pub uuids: &'a BTreeMap<Uuid, EntityId>,
}

impl ScenarioDumpCtx<'_> {
    /// Scenario name of an entity id, `-` when unknown.
    pub fn name(&self, id: EntityId) -> String {
        self.names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "-".to_string())
    }

    pub fn name_opt(&self, id: Option<EntityId>) -> String {
        id.map_or_else(|| "-".to_string(), |i| self.name(i))
    }

    /// Scenario name of an entity UUID (the driver's `uuidName`: the raw UUID when it is not a named entity).
    pub fn name_uuid(&self, uuid: Option<Uuid>) -> String {
        match uuid {
            None => "-".to_string(),
            Some(u) => self
                .uuids
                .get(&u)
                .map_or_else(|| u.to_string(), |i| self.name(*i)),
        }
    }
}

pub struct ScenarioDumpExtra {
    pub order: i32,
    pub applies: fn(&dyn Entity) -> bool,
    pub fields: ScenarioFieldsFn,
}

/// Renders an entity's extra scenario-dump fields as `(name, value)` pairs.
pub type ScenarioFieldsFn =
    fn(&dyn Entity, &ServerLevel, &ScenarioDumpCtx<'_>) -> Vec<(String, String)>;

#[distributed_slice]
pub static SCENARIO_DUMP_EXTRA: [ScenarioDumpExtra];

/// `Mob.finalizeSpawn(level, level.getCurrentDifficultyAt(pos), reason, null)` hooks of the `mobspawn` command (called for every entity it
/// creates, before it is added to the level; a hook ignores entities it does not handle). The mob spine registers the real
/// `finalize_spawn` pipeline here.
#[distributed_slice]
pub static SCENARIO_FINALIZE_SPAWN: [fn(
    &mut ServerLevel,
    &mut dyn Entity,
    mc_data::enums::EntitySpawnReason,
)];

// ==== [MobOracle] shared formatters of the oracle's value grammar (driver header, "Rich mob E line") ====
//
// Used by the harness dump kinds (`goals:` `brain:` `path:`) and available to the mob spine's order-10 registration for `mem=`.

/// `flagsOf(flags)`: `MOVE+LOOK`, `none` for the empty set.
pub fn goal_flags_text(flags: super::ai::goal::GoalFlags) -> String {
    let l: Vec<&str> = flags
        .iter()
        .map(|f| match f {
            super::ai::goal::Flag::Move => "MOVE",
            super::ai::goal::Flag::Look => "LOOK",
            super::ai::goal::Flag::Jump => "JUMP",
            super::ai::goal::Flag::Target => "TARGET",
        })
        .collect();
    if l.is_empty() {
        "none".to_string()
    } else {
        l.join("+")
    }
}

/// `G NAME goal|target PRIO ClassName flags=.. running=0|1` for every registered goal of both selectors (driver `goalsDump`).
pub fn goal_lines(name: &str, mob: &dyn Entity) -> Vec<String> {
    let Some(m) = mob.mob() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (kind, sel) in [("goal", &m.goal_selector), ("target", &m.target_selector)] {
        for g in sel.get_available_goals() {
            out.push(format!(
                "G {name} {kind} {} {} flags={} running={}",
                g.priority(),
                g.goal().name(),
                goal_flags_text(g.goal().flags()),
                u8::from(g.is_running())
            ));
        }
    }
    out
}

fn pos_text(p: mc_core::pos::BlockPos) -> String {
    format!("{},{},{}", p.x(), p.y(), p.z())
}

/// `PATH NAME ...` + `PN NAME i pos= type= malus= walked=` lines of a path (driver `printPath`).
pub fn path_lines(name: &str, path: Option<&crate::world::level::pathfinder::Path>) -> Vec<String> {
    use mc_core::math::java::java_float_to_string as jf;
    let Some(p) = path else {
        return vec![format!("PATH {name} none")];
    };
    let mut out = vec![format!(
        "PATH {name} next={} count={} reached={} target={} dist={}",
        p.get_next_node_index(),
        p.get_node_count(),
        u8::from(p.can_reach()),
        pos_text(p.get_target()),
        jf(p.get_dist_to_target())
    )];
    for i in 0..p.get_node_count() {
        let n = p.get_node(i);
        out.push(format!(
            "PN {name} {i} pos={},{},{} type={} malus={} walked={}",
            n.x,
            n.y,
            n.z,
            n.ty.name(),
            jf(n.cost_malus),
            jf(n.walked_distance)
        ));
    }
    out
}

/// The driver's `memval(Object)` for a brain memory value.
pub fn memory_value_text(
    v: &super::ai::memory::value::MemoryValue,
    level: &ServerLevel,
    ctx: &ScenarioDumpCtx<'_>,
) -> String {
    use super::ai::behavior::position_tracker::PositionTracker;
    use super::ai::memory::value::MemoryValue as V;
    use mc_core::math::java::{java_double_to_string as jd, java_float_to_string as jf};
    let tracker = |t: &PositionTracker| match t {
        PositionTracker::Entity(e) => format!("entity({})", ctx.name(e.get_entity())),
        PositionTracker::Block(_) => format!("pos({})", pos_text(t.current_block_position(level))),
    };
    let glob = |g: &mc_protocol::codec::GlobalPos| format!("{}@{}", g.dimension, pos_text(g.pos));
    let list = |l: Vec<String>| format!("({})", l.join(";"));
    match v {
        V::Bool(b) => u8::from(*b).to_string(),
        V::Int(i) => i.to_string(),
        V::Long(i) => i.to_string(),
        V::Unit(_) => "INSTANCE".to_string(),
        V::GlobalPos(g) => glob(g),
        V::GlobalPosList(l) => list(l.iter().map(glob).collect()),
        V::GlobalPosSet(s) => {
            let mut l: Vec<String> = s.iter().map(glob).collect();
            l.sort();
            list(l)
        }
        V::BlockPos(p) => pos_text(*p),
        V::Vec3(p) => format!("({}, {}, {})", jd(p.x), jd(p.y), jd(p.z)),
        V::Uuid(u) => ctx.name_uuid(Some(*u)),
        V::UuidList(l) => list(l.iter().map(|u| ctx.name_uuid(Some(*u))).collect()),
        V::Entity(e) => ctx.name(e.0),
        V::EntityList(l) => list(l.iter().map(|e| ctx.name(e.0)).collect()),
        V::DamageSource(d) => format!("damage({})", d.0.get_msg_id()),
        V::WalkTarget(w) => format!(
            "walk({};{};{})",
            tracker(w.get_target()),
            jf(w.get_speed_modifier()),
            w.get_close_enough_dist()
        ),
        V::PositionTracker(t) => tracker(t),
        V::Path(p) => format!("path({};{})", p.get_next_node_index(), p.get_node_count()),
        V::NearestVisibleLivingEntities(n) => {
            list(n.nearby_entities().iter().map(|e| ctx.name(e.0)).collect())
        }
        V::SpearStatus(s) => format!("{s:?}").to_uppercase(),
    }
}

/// `(name, value, ttl|inf)` of every memory with a value, sorted by name (driver `memoryEntries`).
pub fn memory_entries(
    e: &dyn Entity,
    level: &ServerLevel,
    ctx: &ScenarioDumpCtx<'_>,
) -> Vec<[String; 3]> {
    let mut out: Vec<[String; 3]> = Vec::new();
    if e.living().is_none() {
        return out;
    }
    e.brain().for_each(|id, value| {
        if let Some((v, ttl)) = value {
            let name = id
                .name()
                .strip_prefix("minecraft:")
                .unwrap_or(id.name())
                .to_string();
            out.push([
                name,
                memory_value_text(v, level, ctx),
                ttl.map_or_else(|| "inf".to_string(), |t| t.to_string()),
            ]);
        }
    });
    out.sort_by(|a, b| a[0].cmp(&b[0]));
    out
}

/// `BRAIN NAME acts=..`, one `BM NAME type= value= ttl=` per memory and `BR NAME debugString` per running behavior (driver `brainDump`).
pub fn brain_lines(
    name: &str,
    e: &dyn Entity,
    level: &ServerLevel,
    ctx: &ScenarioDumpCtx<'_>,
) -> Vec<String> {
    let mut acts: Vec<&str> = e
        .brain()
        .get_active_activities()
        .iter()
        .map(|a| a.get_name())
        .collect();
    acts.sort_unstable();
    let mut out = vec![format!(
        "BRAIN {name} acts={}",
        if acts.is_empty() {
            "-".to_string()
        } else {
            acts.join(";")
        }
    )];
    for [t, v, ttl] in memory_entries(e, level, ctx) {
        out.push(format!("BM {name} type={t} value={v} ttl={ttl}"));
    }
    for b in e.brain().get_running_behaviors() {
        out.push(format!("BR {name} {b}"));
    }
    out
}
