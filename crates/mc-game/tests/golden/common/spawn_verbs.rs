//! Spawning, raid, difficulty and brain-isolation verbs of the scenario DSL (grammar of `tests/oracle/EntityOracleDriver.java`), shared
//! by the replays: `spawnpass`, `custom_spawner`, `raid_waves`, `raid_start`, `rest`, `memory`, `activity` and the dumps
//! `difficulty:X,Y,Z`, `spawn_state`, `raids`. A replay implements [`SpawnHost`] and forwards unknown commands to [`command`] and
//! unknown dump kinds to [`dump`]. The verbs that need the ticking level (natural spawning, custom spawners, raids with a village, local
//! difficulty) are only offered by a host with [`SpawnHost::TICKING`]; the others run in every replay.
use super::player_actions::ActionHost;
use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_core::pos::BlockPos;
use mc_data::enums::MobCategory;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::EntityId;
use mc_game::world::entity::raid::raid::Raid;
use mc_game::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game::world::level::spawner::MobCategoryExt;
use mc_game::world::level::spawner::natural_spawner::{self, SpawnState};
use mc_world::level_storage::Difficulty;

/// `key=value` options after the positional arguments (the driver's `kv`).
pub fn options(a: &[String], from: usize) -> std::collections::BTreeMap<String, String> {
    a[from.min(a.len())..]
        .iter()
        .filter_map(|t| t.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn bool_text(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

fn category(name: &str) -> MobCategory {
    MobCategory::ALL
        .iter()
        .copied()
        .find(|c| c.get_name() == name)
        .unwrap_or_else(|| panic!("unknown mob category {name}"))
}

fn difficulty(name: &str) -> Difficulty {
    match name {
        "peaceful" => Difficulty::Peaceful,
        "easy" => Difficulty::Easy,
        "normal" => Difficulty::Normal,
        "hard" => Difficulty::Hard,
        other => panic!("difficulty {other}"),
    }
}

/// `dump difficulty:X,Y,Z`: `level.getCurrentDifficultyAt(pos)` and the inhabited time of the chunk.
pub fn difficulty_line(level: &ServerLevel, arg: &str) -> String {
    let c: Vec<i32> = arg
        .split(',')
        .map(|n| n.parse().expect("difficulty:X,Y,Z"))
        .collect();
    let pos = BlockPos::new(c[0], c[1], c[2]);
    let di = ServerLevelAccessor::get_current_difficulty_at(level, pos);
    let inhabited = level
        .chunk_now(pos.x() >> 4, pos.z() >> 4)
        .map_or(0, |chunk| chunk.data.access.inhabited_time());
    format!(
        "DIFFICULTY {} {} {} base={} effective={} special={} hard={} inhabited={inhabited}",
        c[0],
        c[1],
        c[2],
        match di.get_difficulty() {
            Difficulty::Peaceful => "peaceful",
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        },
        java_float_to_string(di.get_effective_difficulty()),
        java_float_to_string(di.get_special_multiplier()),
        bool_text(di.is_hard()),
    )
}

/// `createSpawnState()`: the state of `ServerChunkCache.tickChunks` for the level as it is now.
pub fn create_spawn_state(level: &mut ServerLevel) -> SpawnState {
    let count = level
        .chunk_source
        .chunk_map
        .distance_manager
        .get_natural_spawn_chunk_count();
    natural_spawner::create_state(count as i32, level)
}

/// `printSpawnState(state)`.
pub fn spawn_state_line(state: &SpawnState) -> String {
    let mut counts = Vec::new();
    let mut caps = Vec::new();
    for c in MobCategory::ALL.iter().copied() {
        counts.push(format!(
            "{}:{}",
            c.get_name(),
            state.get_mob_category_count(c)
        ));
        caps.push(format!(
            "{}:{}",
            c.get_name(),
            c.get_max_instances_per_chunk() * state.get_spawnable_chunk_count() / 289
        ));
    }
    let charges: Vec<String> = state
        .spawn_potential_charges()
        .into_iter()
        .map(|(p, charge)| {
            format!(
                "{},{},{}:{}",
                p.x(),
                p.y(),
                p.z(),
                java_double_to_string(charge)
            )
        })
        .collect();
    format!(
        "SPAWNSTATE chunks={} counts={} caps={} charges={}",
        state.get_spawnable_chunk_count(),
        counts.join(";"),
        caps.join(";"),
        if charges.is_empty() {
            "-".to_string()
        } else {
            charges.join(";")
        }
    )
}

/// `dump spawn_state`.
pub fn spawn_state_dump(level: &mut ServerLevel) -> String {
    spawn_state_line(&create_spawn_state(level))
}

/// `spawnpass SEED [chunk=CX,CZ] [cats=..] [enemies=B] [persistent=B]`: seeds `Level.random`, creates the state and runs one
/// `spawnForChunk`. Returns the `SPAWNPASS` header; the state (after the pass) is `level.spawning.last_spawn_state()`.
pub fn spawn_pass(level: &mut ServerLevel, a: &[String]) -> String {
    use mc_core::random::RandomSource as _;
    level
        .tick_state
        .random
        .set_seed(a[1].parse().expect("spawnpass SEED"));
    let o = options(a, 2);
    let (cx, cz) = o.get("chunk").map_or((0, 0), |c| {
        let (x, z) = c.split_once(',').expect("chunk=CX,CZ");
        (x.parse().expect("chunk x"), z.parse().expect("chunk z"))
    });
    let state = create_spawn_state(level);
    let cats: Vec<MobCategory> = match o.get("cats") {
        Some(c) => c.split(',').map(category).collect(),
        None => natural_spawner::get_filtered_spawning_categories(
            &state,
            o.get("enemies").is_none_or(|v| v == "true"),
            o.get("persistent")
                .map_or(level.game_time() % 400 == 0, |v| v == "true"),
        ),
    };
    let chunk = mc_core::pos::ChunkPos::new(cx, cz);
    let allowed = level.can_spawn_entities_in_chunk(chunk);
    let header = format!(
        "SPAWNPASS seed={} chunk={cx},{cz} cats={} allowed={}",
        a[1],
        if cats.is_empty() {
            "-".to_string()
        } else {
            cats.iter()
                .map(|c| c.get_name())
                .collect::<Vec<_>>()
                .join(",")
        },
        bool_text(allowed)
    );
    let run = !cats.is_empty() && allowed;
    level.spawning.set_spawn_state(state, cats);
    if run {
        natural_spawner::spawn_for_chunk(level, chunk);
    }
    header
}

/// `custom_spawner NAME SEED [enemies=B]`: seeds `Level.random` and ticks one custom spawner once. Returns the header line.
pub fn custom_spawner(level: &mut ServerLevel, a: &[String]) -> String {
    use mc_core::random::RandomSource as _;
    use mc_game::world::level::spawner::custom_spawner::order;
    level
        .tick_state
        .random
        .set_seed(a[2].parse().expect("custom_spawner SEED"));
    let enemies = options(a, 3).get("enemies").is_none_or(|v| v == "true");
    let order = match a[1].as_str() {
        "phantom" => order::PHANTOM,
        "patrol" => order::PATROL,
        "cat" => order::CAT,
        "siege" => order::VILLAGE_SIEGE,
        "trader" => order::WANDERING_TRADER,
        other => panic!("custom_spawner {other} (phantom patrol cat siege trader)"),
    };
    // the replay's level is not built by the server's level factory
    if level.spawning.custom_spawner_orders.is_empty() {
        level.install_custom_spawners();
    }
    assert!(
        level.tick_custom_spawner(order, enemies),
        "this level has no {} spawner",
        a[1]
    );
    format!(
        "CUSTOMSPAWNER {} seed={} enemies={}",
        a[1],
        a[2],
        bool_text(enemies)
    )
}

/// `raid_waves DIFFICULTY OMEN SEED`: the `RAID_WAVE` lines.
pub fn raid_wave_lines(a: &[String]) -> Vec<String> {
    let diff = difficulty(&a[1]);
    let omen: i32 = a[2].parse().expect("raid_waves OMEN");
    let mut raid = Raid::new(BlockPos::new(0, 0, 0), diff);
    raid.set_raid_omen_level(omen);
    raid.set_random_seed(a[3].parse().expect("raid_waves SEED"));
    let groups = raid.get_num_groups(diff);
    let last = if omen > 1 { groups + 1 } else { groups };
    let mut lines = Vec::new();
    for g in 1..=last {
        let is_bonus = g == groups + 1;
        let mut line = format!(
            "RAID_WAVE diff={} omen={omen} wave={g} bonus={}",
            a[1],
            bool_text(is_bonus)
        );
        for (name, n) in raid.wave_spawn_counts(g, diff, is_bonus) {
            line.push_str(&format!(" {name}={n}"));
        }
        line.push_str(&format!(
            " enchant={}",
            java_float_to_string(raid.get_enchant_odds())
        ));
        lines.push(line);
    }
    lines
}

/// `printRaid(id, raid)`; `name_of` is the driver's `entName`.
pub fn raid_line(raid: &Raid, id: i32, name_of: &dyn Fn(EntityId) -> String) -> String {
    let mut raiders: Vec<String> = raid.get_all_raiders().into_iter().map(name_of).collect();
    raiders.sort();
    let c = raid.get_center();
    format!(
        "RAID {id} center={},{},{} started={} active={} status={} omen={} groups={}/{} health={} raiders={}",
        c.x(),
        c.y(),
        c.z(),
        bool_text(raid.is_started()),
        bool_text(raid.is_active()),
        raid.status_name(),
        raid.get_raid_omen_level(),
        raid.get_groups_spawned(),
        raid.get_num_groups_field(),
        java_float_to_string(raid.get_total_health()),
        if raiders.is_empty() {
            "-".to_string()
        } else {
            raiders.join(";")
        }
    )
}

/// `dump raids`: every raid by id.
pub fn raid_lines(level: &ServerLevel, name_of: &dyn Fn(EntityId) -> String) -> Vec<String> {
    let Some(raids) = level.get_raids() else {
        return Vec::new();
    };
    let mut ids = raids.ids();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| raids.get(id).map(|r| raid_line(r, id, name_of)))
        .collect()
}

/// `memory REF NAME VALUE|-` on the brain of `e` (`entity` = the entity `entity:REF` names): erases the memory, stores an entity, or
/// decodes the SNBT `{value:..,ttl:..}` with the memory type's codec.
pub fn set_memory(
    e: &mut dyn mc_game::world::entity::Entity,
    name: &str,
    value: &str,
    entity: Option<EntityId>,
) {
    use mc_game::world::entity::ai::memory::MemoryTypeId;
    use mc_game::world::entity::ai::memory::codec::decode_expirable;
    use mc_game::world::entity::ai::memory::value::{EntityRef, MemoryValue};
    let key = mc_core::ident::Identifier::parse(&if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{name}")
    })
    .unwrap_or_else(|_| panic!("memory type {name}"));
    let id = MemoryTypeId::by_id(&key).unwrap_or_else(|| panic!("unknown memory type {name}"));
    let brain = e.brain_mut();
    if value == "-" {
        brain.erase_memory_id(id);
    } else if let Some(target) = entity {
        brain.set_memory_value(id, Some(MemoryValue::Entity(EntityRef(target))), None);
    } else {
        let tag = mc_nbt::snbt::parse_compound(value)
            .unwrap_or_else(|err| panic!("memory {name}: bad SNBT: {}", err.message()));
        let ev = decode_expirable(id, &mc_nbt::Tag::Compound(tag))
            .unwrap_or_else(|| panic!("memory {name}: {value} does not decode"));
        brain.set_memory_value(id, Some(ev.value), ev.time_to_live);
    }
}

/// `activity REF NAME`: `brain.setActiveActivityIfPossible(Activity NAME)`.
pub fn set_activity(e: &mut dyn mc_game::world::entity::Entity, name: &str) {
    use mc_game::world::entity::schedule::{ACTIVITIES, Activity};
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let activity: Activity = ACTIVITIES
        .iter()
        .copied()
        .find(|a| a.get_name() == name)
        .unwrap_or_else(|| panic!("unknown activity {name}"));
    e.brain_mut().set_active_activity_if_possible(activity);
}

/// `rest PLAYER TICKS`: the `time_since_rest` statistic `PhantomSpawner` reads.
pub fn set_time_since_rest(p: &mut mc_game::world::entity::ServerPlayer, ticks: i32) {
    use mc_game::stats::stats::Stats;
    p.stats.lock().set_value(Stats::TIME_SINCE_REST, ticks);
}

/// What the spawning verbs need from a replay next to the player actions.
pub trait SpawnHost: ActionHost {
    /// Whether the replay runs a ticking level with chunk loading, a village index and real biomes (the block replay).
    const TICKING: bool;

    /// The scenario names of the entities (the driver's `refs`), naming the entities game logic created first.
    fn entity_names(&mut self) -> std::collections::BTreeMap<EntityId, String>;

    /// The ids of every entity in the level (the driver's `getAllEntities`).
    fn all_entity_ids(&mut self) -> std::collections::BTreeSet<EntityId>;

    /// The entities created since `before`, named `type#n`, as rich `E` lines in id order (the driver's `printFreshEntities`).
    fn fresh_entity_lines(&mut self, before: &std::collections::BTreeSet<EntityId>) -> Vec<String>;
}

fn entity_name_fn(
    names: std::collections::BTreeMap<EntityId, String>,
) -> impl Fn(EntityId) -> String {
    move |e| names.get(&e).cloned().unwrap_or_else(|| "-".to_string())
}

/// The verbs of this module (`false` = not one of them, or one the replay cannot run).
pub fn command<H: SpawnHost>(h: &mut H, a: &[String]) -> bool {
    match a[0].as_str() {
        "memory" => {
            h.boot();
            let target = a[3].strip_prefix("entity:").map(|r| h.action_ref(r));
            let id = h.action_ref(&a[1]);
            h.action_level()
                .with_entity(id, |_, e| set_memory(e, &a[2], &a[3], target));
        }
        "activity" => {
            h.boot();
            let id = h.action_ref(&a[1]);
            h.action_level()
                .with_entity(id, |_, e| set_activity(e, &a[2]));
        }
        "raid_waves" => {
            h.boot();
            for line in raid_wave_lines(a) {
                h.emit(line);
            }
        }
        "rest" if H::TICKING => {
            h.boot();
            let id = h.action_ref(&a[1]);
            let ticks = a[2].parse().expect("rest TICKS");
            h.action_level()
                .with_player(id, |_, p| set_time_since_rest(p, ticks));
        }
        "spawnpass" | "custom_spawner" if H::TICKING => {
            h.boot();
            let before = h.all_entity_ids();
            let header = if a[0] == "spawnpass" {
                spawn_pass(h.action_level(), a)
            } else {
                custom_spawner(h.action_level(), a)
            };
            h.emit(header);
            for line in h.fresh_entity_lines(&before) {
                h.emit(line);
            }
            if a[0] == "spawnpass" {
                let line = spawn_state_line(
                    h.action_level()
                        .spawning
                        .last_spawn_state()
                        .expect("spawn state"),
                );
                h.emit(line);
            }
        }
        // raid_start PLAYER X Y Z [omen=N]: `Raids.createOrExtendRaid` (a village = the POI blocks the script places)
        "raid_start" if H::TICKING => {
            h.boot();
            let pos = BlockPos::new(
                a[2].parse().expect("x"),
                a[3].parse().expect("y"),
                a[4].parse().expect("z"),
            );
            let player = h.action_ref(&a[1]);
            let id = h
                .action_level()
                .with_player(player, |level, p| {
                    mc_game::world::entity::raid::raids::Raids::create_or_extend_raid(level, p, pos)
                })
                .expect("raid_start: the player is in the level");
            match id {
                None => h.emit("RAID none".to_string()),
                Some(id) => {
                    if let Some(omen) = options(a, 5).get("omen") {
                        let omen = omen.parse().expect("omen=N");
                        h.action_level()
                            .with_raid(id, |_, raid| raid.set_raid_omen_level(omen));
                    }
                    let name_of = entity_name_fn(h.entity_names());
                    let line =
                        raid_line(h.action_level().get_raid(id).expect("raid"), id, &name_of);
                    h.emit(line);
                }
            }
        }
        _ => return false,
    }
    true
}

/// The dump kinds of this module (`false` = not one of them, or one the replay cannot run).
pub fn dump<H: SpawnHost>(h: &mut H, kind: &str, arg: Option<&str>) -> bool {
    if !H::TICKING {
        return false;
    }
    match kind {
        "difficulty" => {
            let line = difficulty_line(h.action_level(), arg.expect("difficulty:X,Y,Z"));
            h.emit(line);
        }
        "spawn_state" => {
            let line = spawn_state_dump(h.action_level());
            h.emit(line);
        }
        "raids" => {
            let name_of = entity_name_fn(h.entity_names());
            for line in raid_lines(h.action_level(), &name_of) {
                h.emit(line);
            }
        }
        _ => return false,
    }
    true
}
