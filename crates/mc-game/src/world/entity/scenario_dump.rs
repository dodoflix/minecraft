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
        self.names.get(&id).cloned().unwrap_or_else(|| "-".to_string())
    }

    pub fn name_opt(&self, id: Option<EntityId>) -> String {
        id.map_or_else(|| "-".to_string(), |i| self.name(i))
    }

    /// Scenario name of an entity UUID (the driver's `uuidName`: the raw UUID when it is not a named entity).
    pub fn name_uuid(&self, uuid: Option<Uuid>) -> String {
        match uuid {
            None => "-".to_string(),
            Some(u) => self.uuids.get(&u).map_or_else(|| u.to_string(), |i| self.name(*i)),
        }
    }
}

pub struct ScenarioDumpExtra {
    pub order: i32,
    pub applies: fn(&dyn Entity) -> bool,
    pub fields: fn(&dyn Entity, &ServerLevel, &ScenarioDumpCtx<'_>) -> Vec<(String, String)>,
}

#[distributed_slice]
pub static SCENARIO_DUMP_EXTRA: [ScenarioDumpExtra];

/// `Mob.finalizeSpawn(level, level.getCurrentDifficultyAt(pos), reason, null)` hooks of the `mobspawn` command (called for every entity it
/// creates, before it is added to the level; a hook ignores entities it does not handle). The mob spine registers the real
/// `finalize_spawn` pipeline here.
#[distributed_slice]
pub static SCENARIO_FINALIZE_SPAWN: [fn(&mut ServerLevel, &mut dyn Entity, mc_data::enums::EntitySpawnReason)];
