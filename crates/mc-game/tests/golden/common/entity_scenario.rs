//! Scenario replay harness: runs the SAME scenario scripts the vanilla oracle consumes (`tests/oracle/scenarios/NAME.txt`, DSL in the
//! header of `tests/oracle/EntityOracleDriver.java`) through the Rust server and prints lines in the driver's grammar, so
//! [`Fixture::assert_matches`] compares the whole scenario against the committed vanilla fixture (`tests/fixtures/entity_oracle/NAME.txt`).
//!
//! # Adding a mob golden (about 10 minutes)
//! 1. Write `tests/oracle/scenarios/zombie_chase.txt` (see the header of the Java driver; `mobspawn`, `nav`, `target`, `watch mob:Z`, `tick`, ...).
//! 2. `scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/zombie_chase.txt -o crates/mc-game/tests/fixtures/entity_oracle/zombie_chase.txt`
//! 3. A test next to your mob (any `tests/*.rs`):
//! ```ignore
//! mod common;
//! use common::entity_scenario::run_scenario_test;
//! #[test]
//! fn zombie_chase_matches_vanilla() {
//!     run_scenario_test("zombie_chase");
//! }
//! ```
//! 4. Fields your mob does not model yet: `run_scenario_test_with("zombie_chase", &Options { ignore_fields: vec!["mem", "acts"], ..Default::default() })`
//!    (documented gaps, remove the entries as the implementation catches up). Mob-specific `k=v` fields of the rich `E` lines are
//!    produced by registrations of `mc_game::world::entity::scenario_dump::SCENARIO_DUMP_EXTRA` (next to the mob, not here).
//!
//! # What the replay does
//! * Level: `ServerLevel::detached` + the vanilla registries, flat world (bedrock -64, dirt -63..-62, grass -61) filled for
//!   `x,z in [-64, 64]`; `level void` leaves it empty. `level normal` is not supported.
//! * Time: one `tick` = `HarnessEnv`-less entity pass of `ServerLevel.tick`'s entity part in creation order (no range checks),
//!   then the connection tick of the players (`doTick`). Block ticks / fluid flow / random ticks / weather / chunk loading do NOT run
//!   (scenarios enclose fluids in stone like the item scenarios do). `time` and `gamerule` only feed what the harness models.
//! * Randomness: before every command the entity-seed sequence is restarted with `hash(line)`, `spawn*` with `hash(name)` and every tick
//!   with the oracle's per-tick seed (`mc_core::random::reseed_uniquifier`, the Rust twin of the oracle's `RandomSupport` shadow), so
//!   freshly created entities draw the same random seeds as in the oracle. Entity ids are assigned `1, 2, 3, ...` in creation order
//!   (the oracle's `Entity` counter) for entities the scenario creates.
//! * Commands implemented: see [`Harness::run`] plus the families in `entity_scenario/` (`mob_cmds.rs`: target love age breed tame leash;
//!   `nav_cmds.rs`: nav path_query nav_stop; `dump_cmds.rs`: goals: brain: path: attrs: nbt:) and the player actions shared with the block replay
//!   (`player_actions.rs`: use use_on release interact drop swing swap stab dispense select, dumps use: cooldowns: stats: inventory:), the `events` dump (EV lines). Add a new family as a new file with
//!   `pub(super) fn run(&mut Harness, &[String]) -> bool` (+ a `mod` line and one `||` in the fallback arm of `run`). Entity ids come from the
//!   level's private counter (`ServerLevel::entity_id_counter`, starts at 1 like the oracle's `Entity.ENTITY_COUNTER`), so auto-created entities
//!   (babies, arrows, orbs) get the oracle's ids; ids matter for `(tickCount + id) % n` throttles. An unsupported command or dump kind PANICS with `scenario:line`; there is no
//!   silent skipping (`ignore_kinds` of [`Options`] only drops output lines at comparison time).
//! * Output: `T n`, `P`/`E` lines for `dump`/`watch` (`player:`, `entity:`, `mob:`, `entities`, `mobs`, `all_players`), `HURT`, `EFFECT`,
//!   `# note`. Base/living/player/item fields are written by this module, extra fields of the rich mob dumps by the
//!   `SCENARIO_DUMP_EXTRA` registry.
use super::entity_oracle::{Fixture, Options, scenario_dir};

// Command families of the DSL (one file each, `pub(super) fn run(&mut Harness, &[String]) -> bool` = "handled"): mob state
// (love/age/breed/target/tame/leash), navigation (nav/path_query/nav_stop), extra dump kinds (goals/brain/path/attrs/nbt/difficulty).
mod dump_cmds;
mod mob_cmds;
mod nav_cmds;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::GameType;
use mc_data::{Attribute, EntityType, MobEffect};
use mc_game::server::level::ServerLevel;
use mc_game::world::damagesource::DamageSource;
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::scenario_dump::{
    SCENARIO_DUMP_EXTRA, SCENARIO_FINALIZE_SPAWN, ScenarioDumpCtx,
};
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::level::LevelAccessor;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::play::Input;
use mc_world::level_storage::Difficulty;
use std::collections::{BTreeMap, BTreeSet};

// ------------------------------------------------------------------------------------------------ entry points

/// Replay `tests/oracle/scenarios/NAME.txt` and compare with `tests/fixtures/entity_oracle/NAME.txt` (nothing ignored).
pub fn run_scenario_test(name: &str) {
    run_scenario_test_with(name, &Options::default());
}

pub fn run_scenario_test_with(name: &str, opts: &Options) {
    let actual = run_scenario(name);
    Fixture::load(name).assert_matches(&actual, opts);
}

/// Replay a scenario and return the lines the Java driver would have printed.
pub fn run_scenario(name: &str) -> Vec<String> {
    let path = scenario_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("scenario {}: {e}", path.display()));
    run_scenario_text(name, &text)
}

pub fn run_scenario_text(label: &str, text: &str) -> Vec<String> {
    let mut h = Harness::new();
    h.label = label.to_string();
    for (i, raw) in text.lines().enumerate() {
        h.line_no = i + 1;
        h.line = raw.trim().to_string();
        if h.line.is_empty() || h.line.starts_with('#') {
            continue;
        }
        h.run();
    }
    h.out
}

// ------------------------------------------------------------------------------------------------ small helpers

/// The driver's `hash(String)` (Java `char` = UTF-16 unit).
pub fn java_hash(s: &str) -> i64 {
    let mut h: i64 = 1125899906842597;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(i64::from(c));
    }
    h
}

/// The driver's `tokens()`: whitespace separated, `[..]`/`{..}` and `"..."` keep their spaces.
pub fn tokens(s: &str) -> Vec<String> {
    let mut r = Vec::new();
    let mut cur = String::new();
    let (mut depth, mut quote) = (0i32, false);
    for c in s.chars() {
        if c == '"' {
            quote = !quote;
        }
        if !quote {
            if c == '[' || c == '{' {
                depth += 1;
            } else if c == ']' || c == '}' {
                depth -= 1;
            }
        }
        if c.is_whitespace() && depth == 0 && !quote {
            if !cur.is_empty() {
                r.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        r.push(cur);
    }
    r
}

fn jf32(v: f32) -> String {
    mc_core::math::java::java_float_to_string(v)
}

/// The `OFFERS` / `OFFER` lines of the driver's `offers:REF` dump.
pub fn offer_lines(
    name: &str,
    offers: &mc_game::world::item::trading::MerchantOffers,
) -> Vec<String> {
    let mut lines = vec![format!("OFFERS {name} count={}", offers.0.len())];
    for (i, o) in offers.0.iter().enumerate() {
        let cost_b = o.get_cost_b();
        lines.push(format!(
            "OFFER {name} {i} buyA={} buyB={} sell={} uses={} max={} xp={} mult={} demand={} special={}",
            item_full(o.get_base_cost_a()),
            if cost_b.is_empty() { "-".to_string() } else { item_full(&cost_b) },
            item_full(o.get_result()),
            o.get_uses(),
            o.get_max_uses(),
            o.get_xp(),
            jf32(o.get_price_multiplier()),
            o.get_demand(),
            o.get_special_price_diff()
        ));
    }
    lines
}

fn jf64(v: f64) -> String {
    format!("{v:?}")
}

fn vec3(x: f64, y: f64, z: f64) -> String {
    format!("{},{},{}", jf64(x), jf64(y), jf64(z))
}

fn snake(s: &str) -> String {
    s.chars().fold(String::new(), |mut r, c| {
        if c.is_uppercase() && !r.is_empty() {
            r.push('_');
        }
        r.push(c.to_ascii_lowercase());
        r
    })
}

pub fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    block
        .state_from_properties(
            props
                .split(',')
                .filter(|p| !p.is_empty())
                .map(|p| p.split_once('=').expect("prop=value")),
        )
        .unwrap_or_else(|| panic!("bad state {text}"))
}

/// The driver's `item()`: `id*count[/dmg=N]`.
pub fn item_text(s: &ItemStack) -> String {
    if s.is_empty() {
        return "empty".into();
    }
    let mut r = format!("{}*{}", s.item().name(), s.count());
    if s.is_damageable_item() && s.get_damage_value() != 0 {
        r.push_str(&format!("/dmg={}", s.get_damage_value()));
    }
    r
}

/// The driver's `itemFull(stack)`: `item()` plus the canonical components patch.
pub fn item_full(s: &ItemStack) -> String {
    let base = item_text(s);
    let patch = s.get_components_patch();
    if s.is_empty() || patch.is_empty() {
        return base;
    }
    let tag = mc_dfu::encode_nbt(
        &mc_protocol::components::DataComponentPatch::codec(),
        &patch,
    )
    .into_result()
    .expect("patch encodes");
    format!("{base}{}", canon(&tag))
}

/// The driver's `canon(Tag)`: sorted compound keys printed raw, everything else as the SNBT of the tag.
pub fn canon(tag: &Tag) -> String {
    match tag {
        Tag::Compound(c) => {
            let mut keys: Vec<&String> = c.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .iter()
                .map(|k| format!("{k}:{}", canon(c.get(k).expect("key exists"))))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Tag::List(l) => format!("[{}]", l.iter().map(canon).collect::<Vec<_>>().join(",")),
        other => other.to_string(),
    }
}

// ------------------------------------------------------------------------------------------------ the harness

static REGISTRIES: std::sync::LazyLock<mc_game::registry::RegistryAccess> =
    std::sync::LazyLock::new(|| {
        let _serial = super::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = super::load_world(
            dir.path(),
            mc_world::level_storage::WorldDataConfiguration::DEFAULT,
            true,
        )
        .expect("default world loads");
        stem.registries.composite_access().clone()
    });

fn registries() -> mc_game::registry::RegistryAccess {
    REGISTRIES.clone()
}

pub struct Harness {
    pub level: ServerLevel,
    pub world: mc_game::server::level::level_data::SharedWorldData,
    /// scenario name -> entity id (the driver's `refs`).
    pub refs: BTreeMap<String, EntityId>,
    pub names: BTreeMap<EntityId, String>,
    pub uuids: BTreeMap<uuid::Uuid, EntityId>,
    /// Tick order (creation order, like the entity tick list).
    pub order: Vec<EntityId>,
    known: BTreeSet<EntityId>,
    auto: BTreeMap<String, i32>,
    pub players: Vec<(String, EntityId)>,
    watches: Vec<Vec<String>>,
    pub out: Vec<String>,
    pub ticks: u64,
    booted: bool,
    pub label: String,
    pub line_no: usize,
    pub line: String,
    flat: bool,
    pub day_time: i64,
    sequences: super::player_actions::Sequences,
    /// The level's recorded broadcasts (sounds, level events) since the last `dump events`.
    broadcast_log: std::sync::Arc<
        parking_lot::Mutex<Vec<mc_game::server::level::level_events::BroadcastRecord>>,
    >,
}

impl Harness {
    pub fn new() -> Self {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = registries();
        // item behaviour that reads registries without a level (`Item.getUseDuration` of a goat horn)
        mc_game::world::item::set_item_registries(level.registries.clone());
        let world = mc_game::server::level::level_data::default_world_data();
        level.level_data = Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
            world.clone(),
        ));
        mc_core::random::reseed_uniquifier(8682522807148012);
        level.entity_id_counter = Some(std::sync::atomic::AtomicI32::new(0));
        // `Level.random` is seeded at boot like the oracle (`o.seed ^ 0x5DEECE66DL`, default world seed 4242)
        {
            use mc_core::random::RandomSource as _;
            level.tick_state.random.set_seed(4242 ^ 0x5DEECE66D);
        }
        // loot random sequences mix in the world seed (the oracle's default)
        level.server_state.loot.world_seed = 4242;
        let broadcast_log = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        level.tick_state.broadcast_log = Some(broadcast_log.clone());
        Harness {
            level,
            world,
            refs: BTreeMap::new(),
            names: BTreeMap::new(),
            uuids: BTreeMap::new(),
            order: Vec::new(),
            known: BTreeSet::new(),
            auto: BTreeMap::new(),
            players: Vec::new(),
            watches: Vec::new(),
            out: Vec::new(),
            ticks: 0,
            booted: false,
            label: String::new(),
            line_no: 0,
            line: String::new(),
            flat: true,
            day_time: 0,
            sequences: Default::default(),
            broadcast_log,
        }
    }

    fn fail(&self, msg: &str) -> ! {
        panic!(
            "scenario {}:{}: `{}`: {msg}",
            self.label, self.line_no, self.line
        )
    }

    fn num(&self, s: &str) -> f64 {
        s.parse::<f64>()
            .unwrap_or_else(|_| self.fail(&format!("not a number: {s}")))
    }

    fn int(&self, s: &str) -> i32 {
        s.parse::<i32>()
            .unwrap_or_else(|_| self.fail(&format!("not an integer: {s}")))
    }

    // ---- world --------------------------------------------------------------------------------------------

    /// First world command: build the terrain (the driver boots its server lazily the same way).
    fn world(&mut self) {
        if self.booted {
            return;
        }
        self.booted = true;
        if self.flat {
            let (bedrock, dirt, grass) = (
                Block::BEDROCK.default_state(),
                Block::DIRT.default_state(),
                Block::GRASS_BLOCK.default_state(),
            );
            self.fill([-64, -64, -64], [64, -64, 64], bedrock);
            self.fill([-64, -63, -64], [64, -62, 64], dirt);
            self.fill([-64, -61, -64], [64, -61, 64], grass);
        }
        mc_core::random::reseed_uniquifier(8682522807148012);
    }

    fn set_block(&mut self, x: i32, y: i32, z: i32, state: BlockState) {
        self.level
            .set_block_and_update(BlockPos::new(x, y, z), state);
    }

    fn fill(&mut self, a: [i32; 3], b: [i32; 3], state: BlockState) {
        for x in a[0].min(b[0])..=a[0].max(b[0]) {
            for y in a[1].min(b[1])..=a[1].max(b[1]) {
                for z in a[2].min(b[2])..=a[2].max(b[2]) {
                    self.set_block(x, y, z, state);
                }
            }
        }
    }

    // ---- entities -------------------------------------------------------------------------------------------

    fn entity_ref(&self, name: &str) -> EntityId {
        *self.refs.get(name).unwrap_or_else(|| {
            let mut known: Vec<&str> = self.refs.keys().map(String::as_str).collect();
            known.sort_unstable();
            self.fail(&format!("unknown ref {name} (known: {})", known.join(" ")))
        })
    }

    /// Register a created entity (its id came from the level's private counter = the oracle's `Entity.ENTITY_COUNTER`), name it, add it to the level and the tick order.
    fn add_entity(&mut self, name: &str, e: Box<dyn Entity>) -> EntityId {
        let id = e.id();
        let uuid = e.uuid();
        self.level.add_fresh_entity(e);
        self.refs.insert(name.to_string(), id);
        self.names.insert(id, name.to_string());
        self.uuids.insert(uuid, id);
        self.known.insert(id);
        self.order.push(id);
        id
    }

    /// Entities created by game logic get stable names `type_path#n` in id order (the driver's `nameNewEntities`).
    fn name_new_entities(&mut self) {
        if !self.booted {
            return;
        }
        let mut ids = self.level.entity_manager.all_visible_ids();
        ids.sort_unstable();
        for id in ids {
            if self.known.insert(id) {
                let Some(e) = self.level.get_entity(id) else {
                    continue;
                };
                let path = e
                    .get_type()
                    .name()
                    .trim_start_matches("minecraft:")
                    .to_string();
                let n = self.auto.entry(path.clone()).or_insert(0);
                *n += 1;
                let name = format!("{path}#{n}");
                let uuid = e.uuid();
                self.refs.insert(name.clone(), id);
                self.names.insert(id, name);
                self.uuids.insert(uuid, id);
                self.order.push(id);
            }
        }
    }

    /// `EntityType.create` + `snapTo` (+ merged SNBT) for `spawn`/`mobspawn`; the entity is not added yet.
    pub fn create_entity(
        &mut self,
        type_name: &str,
        reason: mc_data::enums::EntitySpawnReason,
        pos: (f64, f64, f64),
        rot: (f32, f32),
        snbt: Option<&str>,
    ) -> Box<dyn Entity> {
        let ty = EntityType::by_name(type_name)
            .unwrap_or_else(|| self.fail(&format!("unknown entity type {type_name}")));
        let mut e = factory::create(
            ty,
            &self.level,
            EntitySpawnRequest {
                reason,
                ignore_checks: false,
            },
        )
        .unwrap_or_else(|| self.fail(&format!("cannot create {type_name}")));
        e.snap_to(pos.0, pos.1, pos.2, rot.0, rot.1);
        if let Some(s) = snbt {
            self.merge_snbt(e.as_mut(), s);
        }
        e
    }

    fn merge_snbt(&self, e: &mut dyn Entity, snbt: &str) {
        let extra = mc_nbt::snbt::parse_compound(snbt)
            .unwrap_or_else(|err| self.fail(&format!("bad SNBT: {}", err.message())));
        let mut tag = CompoundTag::new();
        e.save_without_id(&mut tag, &|_| None);
        tag.merge(&extra);
        e.load(&tag)
            .unwrap_or_else(|err| self.fail(&format!("load: {err}")));
    }

    fn with_entity<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
    ) -> R {
        let id = self.entity_ref(name);
        let line = self.line.clone();
        self.level.with_entity(id, f).unwrap_or_else(|| {
            panic!(
                "scenario {}:{}: `{line}`: entity {name} is not in the level",
                self.label, self.line_no
            )
        })
    }

    fn with_player<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
    ) -> R {
        let id = self.entity_ref(name);
        let line = self.line.clone();
        self.level.with_player(id, f).unwrap_or_else(|| {
            panic!(
                "scenario {}:{}: `{line}`: {name} is not a player",
                self.label, self.line_no
            )
        })
    }

    // ---- commands -------------------------------------------------------------------------------------------

    /// One scenario command. Implemented: `level seed difficulty preload note gamerule time reseed block fill cmd(effect clear)
    /// spawn spawn_item entity_seed spawn_player give effect attribute set nbt snap tp velocity kill input hurt tick watch dump
    /// mobspawn`, the families in `entity_scenario/` and the player actions of `player_actions.rs`. Everything else panics.
    pub fn run(&mut self) {
        let a = tokens(&self.line.clone());
        let cmd = a[0].clone();
        if self.booted && cmd != "reseed" {
            mc_core::random::reseed_uniquifier(java_hash(&self.line));
        }
        match cmd.as_str() {
            "level" => {
                if self.booted {
                    self.fail("`level` is an option: it must come before the first world command");
                }
                match a[1].as_str() {
                    "flat" => self.flat = true,
                    "void" => self.flat = false,
                    other => self.fail(&format!(
                        "level type {other} is not supported by the replay"
                    )),
                }
            }
            "seed" => {
                use mc_core::random::RandomSource as _;
                let n = a[1].parse::<i64>().unwrap_or_else(|_| self.fail("seed"));
                self.level.tick_state.random.set_seed(n ^ 0x5DEECE66D);
            }
            "level_seed" => {
                use mc_core::random::RandomSource as _;
                let n = a[1]
                    .parse::<i64>()
                    .unwrap_or_else(|_| self.fail("level_seed"));
                self.level.tick_state.random.set_seed(n);
            }
            "preload" => {}
            "difficulty" => {
                let d = match a[1].as_str() {
                    "peaceful" => Difficulty::Peaceful,
                    "easy" => Difficulty::Easy,
                    "normal" => Difficulty::Normal,
                    "hard" => Difficulty::Hard,
                    other => self.fail(&format!("difficulty {other}")),
                };
                self.world.lock().settings.difficulty_settings.difficulty = d;
            }
            "note" => self.out.push(format!("# {}", self.line[4..].trim())),
            "gamerule" => {
                self.world();
                let v = a[2] == "true";
                let name = a[1].clone();
                let modelled = [
                    "keep_inventory",
                    "fall_damage",
                    "fire_damage",
                    "drowning_damage",
                    "freeze_damage",
                    "show_death_messages",
                    "forgive_dead_players",
                ];
                // quiet-world defaults of the oracle that the replay never contradicts
                let quiet = [
                    "advance_time",
                    "advance_weather",
                    "spawn_mobs",
                    "spawn_monsters",
                    "spawn_patrols",
                    "spawn_phantoms",
                    "spawn_wandering_traders",
                    "spawn_wardens",
                    "raids",
                    "log_admin_commands",
                    "send_command_feedback",
                    "random_tick_speed",
                ];
                if modelled.contains(&name.as_str()) {
                    self.level
                        .server_state
                        .rules
                        .update(|r| match name.as_str() {
                            "keep_inventory" => r.keep_inventory = v,
                            "fall_damage" => r.fall_damage = v,
                            "fire_damage" => r.fire_damage = v,
                            "drowning_damage" => r.drowning_damage = v,
                            "freeze_damage" => r.freeze_damage = v,
                            "show_death_messages" => r.show_death_messages = v,
                            _ => r.forgive_dead_players = v,
                        });
                } else if !quiet.contains(&name.as_str()) {
                    self.fail(&format!("gamerule {name} is not modelled by the replay"));
                }
            }
            "time" => {
                self.world();
                self.day_time = self.num(&a[1]) as i64;
            }
            "reseed" => {
                self.world();
                mc_core::random::reseed_uniquifier(self.num(&a[1]) as i64);
            }
            "block" => {
                self.world();
                let st = parse_state(&a[4]);
                self.set_block(self.int(&a[1]), self.int(&a[2]), self.int(&a[3]), st);
            }
            "fill" => {
                self.world();
                let st = parse_state(&a[7]);
                let n = |i: usize| self.int(&a[i]);
                self.fill([n(1), n(2), n(3)], [n(4), n(5), n(6)], st);
            }
            "cmd" => {
                self.world();
                // `effect clear TARGET EFFECT`
                if a.len() == 5 && a[1] == "effect" && a[2] == "clear" {
                    let effect = MobEffect::by_name(&a[4]).unwrap_or_else(|| self.fail("effect"));
                    self.with_entity(&a[3].clone(), |level, e| {
                        e.remove_effect(level, effect);
                    });
                } else {
                    self.fail("only `cmd effect clear TARGET EFFECT` is replayed");
                }
            }
            "spawn" => {
                self.world();
                self.spawn(&a);
            }
            "mobspawn" => {
                self.world();
                self.mob_spawn(&a);
            }
            "trades" => {
                self.world();
                self.trades(&a);
            }
            "offers" => {
                self.world();
                let name = a[1].clone();
                self.offers_dump(&name);
            }
            "spawn_item" => {
                self.world();
                mc_core::random::reseed_uniquifier(java_hash(&a[1]));
                let count = if a.len() > 9 { self.int(&a[9]) } else { 1 };
                let stack = super::player_actions::item_stack(&a[2], count);
                let (x, y, z) = (self.num(&a[3]), self.num(&a[4]), self.num(&a[5]));
                let ie = if a.len() >= 9 {
                    ItemEntity::with_stack_and_motion(
                        &self.level,
                        x,
                        y,
                        z,
                        stack,
                        self.num(&a[6]),
                        self.num(&a[7]),
                        self.num(&a[8]),
                    )
                } else {
                    ItemEntity::with_stack(&self.level, x, y, z, stack)
                };
                self.add_entity(&a[1], Box::new(ie));
            }
            "entity_seed" => {
                self.world();
                let seed = self.num(&a[2]) as i64;
                self.with_entity(&a[1], |_, e| {
                    e.base_mut().random = mc_core::random::LegacyRandomSource::new(seed)
                });
            }
            "spawn_player" => {
                self.world();
                self.spawn_player(&a);
            }
            "give" => {
                self.world();
                self.give(&a);
            }
            "effect" => {
                self.world();
                let effect = MobEffect::by_name(&a[2])
                    .unwrap_or_else(|| self.fail(&format!("effect {}", a[2])));
                let (amp, dur) = (self.int(&a[3]), self.int(&a[4]));
                let hidden = a.iter().any(|t| t == "hidden");
                let added = self.with_entity(&a[1], |level, e| {
                    e.add_effect(
                        level,
                        MobEffectInstance::new(effect, dur, amp, false, !hidden, !hidden, None),
                        None,
                    )
                });
                self.out
                    .push(format!("EFFECT {} {} added={added}", a[1], a[2]));
            }
            "attribute" => {
                self.world();
                let attribute = Attribute::by_name(&a[2])
                    .unwrap_or_else(|| self.fail(&format!("attribute {}", a[2])));
                let value = self.num(&a[3]);
                self.with_entity(&a[1], |_, e| {
                    let l = e
                        .living_mut()
                        .unwrap_or_else(|| panic!("attribute on a non-living entity"));
                    l.attributes
                        .get_instance(attribute)
                        .unwrap_or_else(|| panic!("entity has no attribute {attribute:?}"))
                        .set_base_value(value);
                });
            }
            "set" => {
                self.world();
                self.set(&a);
            }
            "nbt" => {
                self.world();
                let rest = self.line[self.line.find(&a[2]).expect("snbt token")..].to_string();
                let line = self.line.clone();
                let (label, no) = (self.label.clone(), self.line_no);
                self.with_entity(&a[1], |_, e| {
                    let extra = mc_nbt::snbt::parse_compound(&rest).unwrap_or_else(|err| {
                        panic!(
                            "scenario {label}:{no}: `{line}`: bad SNBT: {}",
                            err.message()
                        )
                    });
                    let mut tag = CompoundTag::new();
                    e.save_without_id(&mut tag, &|_| None);
                    tag.merge(&extra);
                    e.load(&tag)
                        .unwrap_or_else(|err| panic!("scenario {label}:{no}: load: {err}"));
                });
            }
            "snap" => {
                self.world();
                let (x, y, z) = (self.num(&a[2]), self.num(&a[3]), self.num(&a[4]));
                let rot = (
                    a.get(5).map(|s| self.num(s) as f32),
                    a.get(6).map(|s| self.num(s) as f32),
                );
                self.with_entity(&a[1], |_, e| {
                    let (yr, xr) = (
                        rot.0.unwrap_or_else(|| e.base().y_rot()),
                        rot.1.unwrap_or_else(|| e.base().x_rot()),
                    );
                    e.snap_to(x, y, z, yr, xr);
                });
            }
            "tp" => {
                self.world();
                let (x, y, z) = (self.num(&a[2]), self.num(&a[3]), self.num(&a[4]));
                if a.len() > 5 {
                    self.fail("`tp` with rotation is not replayed");
                }
                self.with_entity(&a[1], |level, e| e.teleport_to(level, x, y, z));
            }
            "velocity" => {
                self.world();
                let (x, y, z) = (self.num(&a[2]), self.num(&a[3]), self.num(&a[4]));
                self.with_entity(&a[1], |_, e| e.base_mut().set_delta_movement_xyz(x, y, z));
            }
            "kill" => {
                self.world();
                // `Entity.discard()` of a removed entity (a picked-up item) does nothing
                let id = self.entity_ref(&a[1]);
                self.level.with_entity(id, |_, e| e.discard());
            }
            "input" => {
                self.world();
                let flags: Vec<&str> = a[2].split(',').collect();
                let has = |f: &str| flags.contains(&f);
                let input = Input {
                    forward: has("forward"),
                    backward: has("back"),
                    left: has("left"),
                    right: has("right"),
                    jump: has("jump"),
                    shift: has("sneak"),
                    sprint: has("sprint"),
                };
                self.with_player(&a[1], |_, p| {
                    p.last_client_input = input;
                    // `handlePlayerInput`
                    p.base_mut().set_shared_flag(
                        mc_game::world::entity::entity::FLAG_SHIFT_KEY_DOWN,
                        input.shift,
                    );
                });
            }
            "hurt" => {
                self.world();
                self.hurt(&a);
            }
            "tick" => {
                self.world();
                let n = a.get(1).map_or(1, |s| self.int(s));
                for _ in 0..n {
                    self.tick_once();
                }
            }
            "watch" => {
                if a[1] == "off" {
                    self.watches.clear();
                } else {
                    self.watches.push(a[1..].to_vec());
                }
            }
            "dump" => {
                self.world();
                let what: Vec<String> = if a.len() > 1 {
                    a[1..].to_vec()
                } else {
                    vec!["entities".into(), "all_players".into()]
                };
                self.dump(&what);
            }
            other => {
                if !(mob_cmds::run(self, &a)
                    || nav_cmds::run(self, &a)
                    || super::player_actions::command(self, &a))
                {
                    self.fail(&format!("unsupported scenario command `{other}` (see the Harness::run doc and entity_scenario/*.rs for the implemented subset)"))
                }
            }
        }
        self.name_new_entities();
    }

    fn spawn(&mut self, a: &[String]) {
        let name = a[1].clone();
        mc_core::random::reseed_uniquifier(java_hash(&name));
        let mut n = a.len();
        let snbt = if n > 6 && a[n - 1].starts_with('{') {
            n -= 1;
            Some(a[n].clone())
        } else {
            None
        };
        let rot = if n > 7 {
            (self.num(&a[6]) as f32, self.num(&a[7]) as f32)
        } else {
            (0.0, 0.0)
        };
        let e = self.create_entity(
            &a[2],
            mc_data::enums::EntitySpawnReason::Command,
            (self.num(&a[3]), self.num(&a[4]), self.num(&a[5])),
            rot,
            snbt.as_deref(),
        );
        self.add_entity(&name, e);
    }

    /// `mobspawn REF TYPE X Y Z [yaw pitch] [reason=NAME] [nofinalize] [SNBT]`: creation, SNBT, `finalizeSpawn` (when the type is a
    /// Mob that implements it), add.
    fn mob_spawn(&mut self, a: &[String]) {
        let name = a[1].clone();
        mc_core::random::reseed_uniquifier(java_hash(&name));
        let mut reason = mc_data::enums::EntitySpawnReason::Command;
        let (mut finalize, mut snbt, mut rot) = (true, None, Vec::new());
        for t in &a[6..] {
            if t.starts_with('{') {
                snbt = Some(t.as_str());
            } else if let Some(r) = t.strip_prefix("reason=") {
                reason = spawn_reason(r).unwrap_or_else(|| self.fail(&format!("spawn reason {r}")));
            } else if t == "nofinalize" {
                finalize = false;
            } else {
                rot.push(self.num(t) as f32);
            }
        }
        let rot = if rot.len() > 1 {
            (rot[0], rot[1])
        } else {
            (0.0, 0.0)
        };
        let mut e = self.create_entity(
            &a[2],
            reason,
            (self.num(&a[3]), self.num(&a[4]), self.num(&a[5])),
            rot,
            snbt,
        );
        if finalize {
            for hook in SCENARIO_FINALIZE_SPAWN.iter() {
                hook(&mut self.level, e.as_mut(), reason);
            }
        }
        self.add_entity(&name, e);
    }

    /// `trades REF PROFESSION LEVEL SEED [type=..]`: a villager with that data, its entity random seeded, offers generated by `getOffers()`.
    fn trades(&mut self, a: &[String]) {
        let name = a[1].clone();
        let ty = a[5..]
            .iter()
            .find_map(|t| t.strip_prefix("type="))
            .unwrap_or("minecraft:plains");
        mc_core::random::reseed_uniquifier(java_hash(&name));
        let snbt = format!(
            "{{VillagerData:{{type:\"{ty}\",profession:\"{}\",level:{}}}}}",
            a[2], a[3]
        );
        let e = self.create_entity(
            "minecraft:villager",
            mc_data::enums::EntitySpawnReason::Command,
            (0.5, -60.0, 0.5),
            (0.0, 0.0),
            Some(&snbt),
        );
        self.add_entity(&name, e);
        let seed = a[4]
            .parse::<i64>()
            .unwrap_or_else(|_| self.fail("trades seed"));
        self.with_entity(&name, |level, e| {
            e.base_mut().random = mc_core::random::LegacyRandomSource::new(seed);
            e.as_merchant_mut().expect("merchant").get_offers(level);
        });
        self.offers_dump(&name);
    }

    fn offers_dump(&mut self, name: &str) {
        let offers = self.with_entity(name, |level, e| {
            e.as_merchant_mut()
                .expect("merchant")
                .get_offers(level)
                .clone()
        });
        self.out.extend(offer_lines(name, &offers));
    }

    fn spawn_player(&mut self, a: &[String]) {
        mc_core::random::reseed_uniquifier(java_hash(&a[1]));
        let id = self.level.next_entity_id();
        let profile = GameProfile {
            id: mc_core::uuid_util::create_offline_player_uuid(&a[1]),
            name: a[1].clone(),
            properties: vec![],
        };
        let mut p = ServerPlayer::new(
            id,
            Identifier::parse("minecraft:overworld").unwrap(),
            profile,
            Default::default(),
        );
        let mode = match a[2].as_str() {
            "creative" => GameType::Creative,
            "spectator" => GameType::Spectator,
            "adventure" => GameType::Adventure,
            _ => GameType::Survival,
        };
        p.set_game_mode_for_player(mode, None);
        let sim = a.last().is_some_and(|s| s == "sim");
        let n = a.len() - usize::from(sim);
        let (yaw, pitch) = if n > 7 {
            (self.num(&a[6]) as f32, self.num(&a[7]) as f32)
        } else {
            (0.0, 0.0)
        };
        p.snap_to(
            self.num(&a[3]),
            self.num(&a[4]),
            self.num(&a[5]),
            yaw,
            pitch,
        );
        p.base_mut().first_tick = true;
        p.simulate_local_player = sim;
        let uuid = p.uuid();
        self.level.add_player(Box::new(p));
        self.refs.insert(a[1].clone(), id);
        self.names.insert(id, a[1].clone());
        self.uuids.insert(uuid, id);
        self.known.insert(id);
        self.order.push(id);
        self.players.push((a[1].clone(), id));
    }

    /// `give REF SLOT STACK [count]`: a player through the shared action, a mob's equipment slot here.
    fn give(&mut self, a: &[String]) {
        let count = a.get(4).map_or(1, |s| self.int(s));
        let stack = super::player_actions::item_stack(&a[3], count);
        let slot = a[2].clone();
        let line = self.line.clone();
        self.with_entity(&a[1], |_, e| {
            if let Some(p) = e.as_any_mut().downcast_mut::<ServerPlayer>() {
                super::player_actions::give(p, &slot, stack);
            } else {
                e.living_mut()
                    .unwrap_or_else(|| panic!("`{line}`: give on a non-living entity"))
                    .equipment
                    .set(super::player_actions::equipment_slot(&slot), stack);
            }
        });
    }

    fn set(&mut self, a: &[String]) {
        let (what, value) = (a[2].clone(), a[3].clone());
        let v: f64 = value.parse().unwrap_or(f64::NAN);
        let line = self.line.clone();
        self.with_entity(&a[1], |_, e| match what.as_str() {
            "health" => e
                .living_mut()
                .unwrap_or_else(|| panic!("`{line}`: not living"))
                .set_health(v as f32),
            "air" => e.base_mut().set_air_supply(v as i32),
            "fire" => e.base_mut().remaining_fire_ticks = v as i32,
            "fall_distance" => e.base_mut().fall_distance = v,
            "absorption" => e.set_absorption_amount(v as f32),
            "no_gravity" => e.base_mut().set_no_gravity(value == "true"),
            "on_ground" => e.base_mut().set_on_ground_raw(value == "true"),
            "invulnerable" => e.base_mut().permanently_invulnerable = value == "true",
            "tag" => {
                e.base_mut().add_tag(&value);
            }
            "food" | "saturation" | "exhaustion" | "xp_level" => {
                let p = e
                    .as_any_mut()
                    .downcast_mut::<ServerPlayer>()
                    .unwrap_or_else(|| panic!("`{line}`: not a player"));
                match what.as_str() {
                    "food" => p.player.food_data.set_food_level(v as i32),
                    "saturation" => p.player.food_data.saturation_level = v as f32,
                    "exhaustion" => p.player.food_data.exhaustion_level = v as f32,
                    _ => p.player.experience_level = v as i32,
                }
            }
            other => panic!("`{line}`: set {other} is not replayed"),
        });
    }

    fn hurt(&mut self, a: &[String]) {
        let (mut by, mut direct, mut at) = (None, None, None);
        for t in &a[4..] {
            match t.split_once('=') {
                Some(("by", r)) => by = Some(self.entity_ref(r)),
                Some(("direct", r)) => direct = Some(self.entity_ref(r)),
                Some(("at", r)) => {
                    let c: Vec<f64> = r.split(',').map(|v| self.num(v)).collect();
                    at = Some(mc_core::phys::Vec3::new(c[0], c[1], c[2]));
                }
                _ => self.fail(&format!(
                    "hurt option {t} is not replayed (by= direct= at=)"
                )),
            }
        }
        let direct = direct.or(by);
        let key = Identifier::parse(&if a[2].contains(':') {
            a[2].clone()
        } else {
            format!("minecraft:{}", a[2])
        })
        .unwrap();
        let data = self
            .level
            .registries
            .get::<mc_game::registry::typed::damage_type::DamageType>("minecraft:damage_type", &key)
            .unwrap_or_else(|| self.fail("unknown damage type"))
            .clone();
        let amount = self.num(&a[3]) as f32;
        let source = DamageSource::new(
            key,
            std::sync::Arc::new(data),
            direct.and_then(|i| self.level.get_entity(i)),
            by.and_then(|i| self.level.get_entity(i)),
            at,
        );
        let r = self.with_entity(&a[1], |level, e| e.hurt_server(level, &source, amount));
        self.out
            .push(format!("HURT {} {} {} -> {r}", a[1], a[2], a[3]));
    }

    // ---- time ----------------------------------------------------------------------------------------------

    /// The entity part of `MinecraftServer.tickServer`: `SimPlayer.applyInput`, the entity tick list in creation order (snapshot at the
    /// start, passengers after their vehicle), then the players' connection tick (`tickPlayer` -> `doTick`).
    pub fn tick_once(&mut self) {
        mc_core::random::reseed_uniquifier(
            8682522807148012i64
                ^ ((self.ticks as i64 + 1).wrapping_mul(0x9E3779B97F4A7C15u64 as i64)),
        );
        for (_, id) in self.players.clone() {
            self.level.with_player(id, |_, p| {
                if p.simulate_local_player {
                    p.apply_input();
                }
            });
        }
        let order = self.order.clone();
        for id in order {
            self.tick_entity(id);
        }
        for (_, id) in self.players.clone() {
            self.level.with_player(id, |level, p| {
                let pos = p.base().position();
                p.base_mut().set_old_pos(pos);
                p.do_tick(level);
            });
        }
        self.ticks += 1;
        self.name_new_entities();
        if !self.watches.is_empty() {
            self.out.push(format!("T {}", self.ticks));
            for w in self.watches.clone() {
                self.dump(&w);
            }
        }
    }

    /// `ServerLevel.tickNonPassenger` without the entity-ticking-range check (the detached level has no chunk tickets).
    fn tick_entity(&mut self, id: EntityId) {
        let Some(mut entity) = self.level.take_entity(id) else {
            return;
        };
        if entity.is_removed() {
            self.level.put_back_entity(id, entity);
            return;
        }
        entity.check_despawn(&mut self.level);
        let riding = entity.base().vehicle.is_some_and(|v| {
            self.level
                .entity_manager
                .get_any(v)
                .is_some_and(|v| !v.is_removed() && v.base().passengers.contains(&id))
        });
        let mut passengers = Vec::new();
        if !riding {
            mc_game::world::entity::entity::common_tick(entity.as_mut());
            entity.tick(&mut self.level);
            passengers = entity.base().passengers.clone();
        }
        self.level.put_back_entity(id, entity);
        for p in passengers {
            self.level.tick_passenger(id, p);
        }
    }

    // ---- dumps ---------------------------------------------------------------------------------------------

    fn dump(&mut self, what: &[String]) {
        for w in what {
            let (kind, arg) = match w.split_once(':') {
                Some((k, a)) => (k, Some(a)),
                None => (w.as_str(), None),
            };
            match (kind, arg) {
                ("player", Some(n)) | ("entity", Some(n)) => self.describe_ref(n, false),
                ("mob", Some(n)) => self.describe_ref(n, true),
                ("all_players", None) => {
                    for (n, _) in self.players.clone() {
                        self.describe_ref(&n, false);
                    }
                }
                // EV lines of the sounds and level events broadcast since the last dump
                ("events", None) => {
                    let records = std::mem::take(&mut *self.broadcast_log.lock());
                    for r in records {
                        self.out.push(super::block_scenario::format_broadcast(&r));
                    }
                }
                ("entities", None) | ("mobs", None) => {
                    self.name_new_entities();
                    let rich = kind == "mobs";
                    let mut all: Vec<(EntityId, String)> = self
                        .names
                        .iter()
                        .filter(|(id, _)| !self.players.iter().any(|(_, p)| p == *id))
                        .map(|(i, n)| (*i, n.clone()))
                        .collect();
                    all.sort();
                    for (id, n) in all {
                        let Some(e) = self.level.get_entity(id) else {
                            continue;
                        };
                        if !e.is_alive() || (rich && !is_mob(e)) {
                            continue;
                        }
                        self.describe_ref(&n, rich);
                    }
                }
                _ => {
                    if !(dump_cmds::dump(self, kind, arg)
                        || super::player_actions::dump(self, kind, arg))
                    {
                        self.fail(&format!("unsupported dump kind `{w}` (replayed: player: entity: mob: all_players entities mobs use: cooldowns: stats: inventory:, see entity_scenario/dump_cmds.rs)"))
                    }
                }
            }
        }
    }

    fn describe_ref(&mut self, name: &str, rich: bool) {
        let id = self.entity_ref(name);
        let line = match self.level.get_entity(id) {
            Some(e) => self.describe(name, e, rich),
            None => self.fail(&format!("entity {name} is not in the level")),
        };
        self.out.push(line);
    }

    /// The driver's `describe(name, entity, rich)`.
    pub fn describe(&self, name: &str, e: &dyn Entity, rich: bool) -> String {
        let b = e.base();
        let is_player = e.as_any().downcast_ref::<ServerPlayer>().is_some();
        let (p, d) = (b.position(), b.delta_movement());
        let mut s = format!(
            "{} {name} {} pos={} delta={} rot={},{} og={} fall={} fire={} air={} alive={} pose={}",
            if is_player { "P" } else { "E" },
            e.get_type().name(),
            vec3(p.x, p.y, p.z),
            vec3(d.x, d.y, d.z),
            jf32(b.y_rot()),
            jf32(b.x_rot()),
            u8::from(b.on_ground()),
            jf64(b.fall_distance),
            b.remaining_fire_ticks,
            b.get_air_supply(),
            u8::from(e.is_alive()),
            snake(&format!("{:?}", b.get_pose())),
        );
        if let Some(l) = e.living() {
            s.push_str(&format!(
                " hp={} abs={} hurt={}/{} death={} fx=",
                jf32(e.get_health()),
                jf32(e.get_absorption_amount()),
                l.hurt_time,
                l.damage_cooldown_time,
                l.death_time
            ));
            let mut fx: Vec<&MobEffectInstance> = l.active_effects.values().collect();
            fx.sort_by_key(|i| i.effect.name());
            if fx.is_empty() {
                s.push('-');
            }
            for (i, m) in fx.iter().enumerate() {
                s.push_str(&format!(
                    "{}{}/{}/{}",
                    if i > 0 { ";" } else { "" },
                    m.effect.name(),
                    m.amplifier,
                    m.duration
                ));
            }
        }
        if let Some(sp) = e.as_any().downcast_ref::<ServerPlayer>() {
            let pd = &sp.player;
            s.push_str(&format!(
                " gm={} food={} sat={} exh={} xp={}/{}/{} sel={} main={}",
                format!("{:?}", sp.game_mode.game_mode_for_player).to_lowercase(),
                pd.food_data.get_food_level(),
                jf32(pd.food_data.get_saturation_level()),
                jf32(pd.food_data.exhaustion_level),
                pd.experience_level,
                pd.total_experience,
                jf32(pd.experience_progress),
                pd.inventory.get_selected_slot(),
                item_full(pd.inventory.get_selected_item()),
            ));
        } else if let Some(ie) = e.as_any().downcast_ref::<ItemEntity>() {
            s.push_str(&format!(
                " item={} age={}",
                item_text(&ie.get_item()),
                ie.age
            ));
        } else if let Some(o) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::experience_orb::ExperienceOrb>()
        {
            s.push_str(&format!(" value={}", o.get_value()));
        } else if let Some(t) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::item::primed_tnt::PrimedTnt>()
        {
            s.push_str(&format!(" fuse={}", t.get_fuse()));
        } else if let Some(a) = abstract_arrow(e) {
            let d = a.arrow_data();
            s.push_str(&format!(
                " ground={} crit={} pierce={} pickup={} shake={} life={} dmg={}",
                u8::from(a.is_in_ground()),
                u8::from(a.is_crit_arrow()),
                a.pierce_level(),
                d.pickup.ordinal(),
                d.shake_time,
                d.life,
                jf64(d.base_damage)
            ));
        }
        let ctx = ScenarioDumpCtx {
            names: &self.names,
            uuids: &self.uuids,
        };
        if let Some(ie) = e.as_any().downcast_ref::<ItemEntity>() {
            s.push_str(&format!(
                " pickup={} health={} owner={} thrower={}",
                ie.pickup_delay,
                ie.health,
                ctx.name_uuid(ie.target),
                ctx.name_uuid(ie.thrower)
            ));
        } else if let Some(o) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::experience_orb::ExperienceOrb>()
        {
            s.push_str(&format!(
                " count={} age={} health={}",
                o.count, o.age, o.health
            ));
        }
        if rich && !is_player {
            let mut extras: Vec<_> = SCENARIO_DUMP_EXTRA
                .iter()
                .filter(|x| (x.applies)(e))
                .collect();
            extras.sort_by_key(|x| x.order);
            for x in extras {
                for (k, v) in (x.fields)(e, &self.level, &ctx) {
                    s.push_str(&format!(" {k}={v}"));
                }
            }
        }
        s
    }
}

impl super::player_actions::ActionHost for Harness {
    fn boot(&mut self) {
        self.world();
    }
    fn action_level(&mut self) -> &mut ServerLevel {
        &mut self.level
    }
    fn action_ref(&self, name: &str) -> EntityId {
        self.entity_ref(name)
    }
    fn next_sequence(&mut self, player: &str) -> i32 {
        self.sequences.next(player)
    }
    fn emit(&mut self, line: String) {
        self.out.push(line);
    }
}

/// `instanceof AbstractArrow`.
pub fn abstract_arrow(
    e: &dyn Entity,
) -> Option<&dyn mc_game::world::entity::projectile::arrow::AbstractArrow> {
    use mc_game::world::entity::projectile::arrow::{Arrow, SpectralArrow, ThrownTrident};
    let any = e.as_any();
    any.downcast_ref::<Arrow>()
        .map(|a| a as &dyn mc_game::world::entity::projectile::arrow::AbstractArrow)
        .or_else(|| any.downcast_ref::<SpectralArrow>().map(|a| a as _))
        .or_else(|| any.downcast_ref::<ThrownTrident>().map(|a| a as _))
}

/// Is the entity a `Mob`? Defined by the generic mob dump registration (`order 10`, see `scenario_dump`).
fn is_mob(e: &dyn Entity) -> bool {
    SCENARIO_DUMP_EXTRA
        .iter()
        .any(|x| x.order == 10 && (x.applies)(e))
}

fn spawn_reason(name: &str) -> Option<mc_data::enums::EntitySpawnReason> {
    use mc_data::enums::EntitySpawnReason as R;
    Some(match name.to_ascii_lowercase().as_str() {
        "natural" => R::Natural,
        "chunk_generation" => R::ChunkGeneration,
        "spawner" => R::Spawner,
        "structure" => R::Structure,
        "breeding" => R::Breeding,
        "mob_summoned" => R::MobSummoned,
        "jockey" => R::Jockey,
        "event" => R::Event,
        "conversion" => R::Conversion,
        "reinforcement" => R::Reinforcement,
        "triggered" => R::Triggered,
        "bucket" => R::Bucket,
        "spawn_item_use" => R::SpawnItemUse,
        "command" => R::Command,
        "dispenser" => R::Dispenser,
        "patrol" => R::Patrol,
        "trial_spawner" => R::TrialSpawner,
        "load" => R::Load,
        "dimension_travel" => R::DimensionTravel,
        _ => return None,
    })
}
