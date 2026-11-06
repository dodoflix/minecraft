//! Scenario-harness registrations of the mob spine (`world::entity::scenario_dump`): the generic fields of the rich `E` lines
//! of the vanilla oracle (`describeMob` of `tests/oracle/EntityOracleDriver.java`) and the `finalizeSpawn` hook of `mobspawn`.
//! Order 10 = `Mob`, 11 = `AgeableMob`, 12 = `Animal`, 13 = `TamableAnimal`.

use super::{ageable, animal, leashable, tamable, tick};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::scenario_dump::{
    SCENARIO_DUMP_EXTRA, SCENARIO_FINALIZE_SPAWN, ScenarioDumpCtx, ScenarioDumpExtra,
};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::math::java::java_float_to_string as jf;
use mc_core::pos::BlockPos;

fn b(v: bool) -> String {
    if v { "1" } else { "0" }.to_string()
}

fn pos(p: BlockPos) -> String {
    format!("{},{},{}", p.x(), p.y(), p.z())
}

fn join_or_dash(l: Vec<String>, sep: &str) -> String {
    if l.is_empty() {
        "-".to_string()
    } else {
        l.join(sep)
    }
}

/// `runningGoals(selector)`.
fn running_goals(sel: &crate::world::entity::ai::goal::GoalSelector) -> String {
    let l = sel
        .get_available_goals()
        .iter()
        .filter(|g| g.is_running())
        .map(|g| {
            format!(
                "{}:{}:{}",
                g.priority(),
                g.goal().name(),
                crate::world::entity::scenario_dump::goal_flags_text(g.goal().flags())
            )
        })
        .collect();
    join_or_dash(l, ";")
}

fn mob_fields(
    e: &dyn Entity,
    level: &ServerLevel,
    ctx: &ScenarioDumpCtx<'_>,
) -> Vec<(String, String)> {
    let m = e.mob().expect("mob");
    let l = e.living().expect("living");
    let f = |k: &str, v: String| (k.to_string(), v);
    let mut out = vec![
        f("target", ctx.name_opt(tick::get_target_checked(e, level))),
        f("hurtby", ctx.name_uuid(l.last_hurt_by_mob)),
        f("aggr", b(m.is_aggressive())),
        f("noai", b(m.is_no_ai())),
        f("left", b(m.is_left_handed())),
        f("persist", b(e.is_persistence_required())),
        f("canpickup", b(e.can_pick_up_loot())),
        f(
            "home",
            if m.has_home() {
                format!("{}/{}", pos(m.home_position), m.home_radius)
            } else {
                "-".to_string()
            },
        ),
        f("leash", ctx.name_opt(leashable::get_leash_holder(e))),
        f("speed", jf(e.get_speed())),
        f("zza", jf(l.zza)),
        f("xxa", jf(l.xxa)),
        f("yya", jf(l.yya)),
        f("head", jf(l.y_head_rot)),
        f("body", jf(l.y_body_rot)),
    ];
    // navigation
    let nav = super::navigation::with_navigation_ref(e, level, |n| {
        let data = n.data();
        let path = data.path.clone();
        (
            n.is_done(),
            n.is_stuck(),
            data.speed_modifier,
            n.get_target_pos(),
            path,
        )
    });
    let (done, stuck, speed, ntarget, path) = nav.unwrap_or((true, false, 0.0, None, None));
    out.push(f("nav", if done { "done" } else { "moving" }.to_string()));
    out.push(f("stuck", b(stuck)));
    out.push(f(
        "navspeed",
        mc_core::math::java::java_double_to_string(speed),
    ));
    out.push(f("ntarget", ntarget.map_or("-".to_string(), pos)));
    out.push(f(
        "path",
        path.as_ref().map_or("-".to_string(), |p| {
            format!(
                "{}/{}/{}/{}",
                p.get_next_node_index(),
                p.get_node_count(),
                b(p.can_reach()),
                pos(p.get_target())
            )
        }),
    ));
    let nodes = match &path {
        Some(p) if p.get_node_count() > 0 => (0..p.get_node_count())
            .map(|i| {
                let n = p.get_node(i);
                format!(
                    "{},{},{}:{}:{}",
                    n.x,
                    n.y,
                    n.z,
                    n.ty.name(),
                    jf(n.cost_malus)
                )
            })
            .collect::<Vec<_>>()
            .join(";"),
        _ => "-".to_string(),
    };
    out.push(f("nodes", nodes));
    out.push(f("goals", running_goals(&m.goal_selector)));
    out.push(f("tgoals", running_goals(&m.target_selector)));
    // brain activities (sorted); memories are `-` (brain-less mobs; brain mobs extend this field when their goldens need it)
    let mut acts: Vec<&str> = e
        .brain()
        .get_active_activities()
        .iter()
        .map(|a| a.get_name())
        .collect();
    acts.sort_unstable();
    out.push(f(
        "acts",
        join_or_dash(acts.into_iter().map(String::from).collect(), ";"),
    ));
    let mem: Vec<String> = crate::world::entity::scenario_dump::memory_entries(e, level, ctx)
        .iter()
        .map(|m| format!("{}:{}:{}", m[0], m[1], m[2]))
        .collect();
    out.push(f("mem", join_or_dash(mem, ";")));
    let equip: Vec<String> = EquipmentSlot::VALUES
        .iter()
        .filter_map(|s| {
            let st = e.get_item_by_slot(*s);
            (!st.is_empty()).then(|| {
                use crate::world::item::ItemStackExt as _;
                let mut r = format!("{}*{}", st.item().name(), st.count());
                if st.is_damageable_item() && st.get_damage_value() != 0 {
                    r.push_str(&format!("/dmg={}", st.get_damage_value()));
                }
                format!("{}:{}", s.get_name(), r)
            })
        })
        .collect();
    out.push(f("equip", join_or_dash(equip, ";")));
    let mut attrs: Vec<String> = e
        .get_type()
        .data()
        .default_attributes
        .iter()
        .filter_map(|(a, _)| {
            let value = l.attributes.get_value(*a);
            (value != a.data().default_value).then(|| {
                format!(
                    "{}:{}",
                    a.name(),
                    mc_core::math::java::java_double_to_string(value)
                )
            })
        })
        .collect();
    attrs.sort();
    out.push(f("attrs", join_or_dash(attrs, ";")));
    out
}

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static MOB_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 10,
    applies: |e| e.mob().is_some(),
    fields: mob_fields,
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static AGEABLE_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 11,
    applies: |e| e.mob().is_some_and(|m| m.ageable.is_some()),
    fields: |e, _level, _ctx| {
        let a = e.mob().and_then(|m| m.ageable.as_ref()).expect("ageable");
        vec![
            ("age".into(), a.age.to_string()),
            ("baby".into(), b(e.is_baby())),
            ("forcedage".into(), a.forced_age.to_string()),
        ]
    },
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static ANIMAL_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 12,
    applies: |e| e.mob().is_some_and(|m| m.animal.is_some()),
    fields: |e, level, ctx| {
        vec![
            ("love".into(), animal::get_in_love_time(e).to_string()),
            (
                "lovecause".into(),
                ctx.name_opt(animal::get_love_cause(e, level)),
            ),
        ]
    },
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static TAMABLE_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 13,
    applies: |e| e.mob().is_some_and(|m| m.tamable.is_some()),
    fields: |e, _level, ctx| {
        vec![
            ("tame".into(), b(tamable::is_tame(e))),
            (
                "owner".into(),
                ctx.name_uuid(tamable::get_owner_reference(e)),
            ),
            ("sitting".into(), b(tamable::is_in_sitting_pose(e))),
            ("ordered".into(), b(tamable::is_ordered_to_sit(e))),
        ]
    },
};

/// `Mob.finalizeSpawn(level, level.getCurrentDifficultyAt(pos), reason, null)` of the `mobspawn` command.
#[distributed_slice(SCENARIO_FINALIZE_SPAWN)]
static FINALIZE: fn(&mut ServerLevel, &mut dyn Entity, mc_data::enums::EntitySpawnReason) =
    |level, e, reason| {
        if e.mob().is_some() {
            let difficulty =
                ServerLevelAccessor::get_current_difficulty_at(&*level, e.block_position());
            e.finalize_spawn(level, difficulty, reason, None);
        }
    };
