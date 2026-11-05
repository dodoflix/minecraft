//! Differential test of `advancements::predicates` against the real vanilla classes.
//!
//! Every `tests/oracle/scenarios/predicates/NAME.txt` is a scenario for the in-process vanilla oracle
//! (`tests/oracle/PredicateOracle.java`, run through `scripts/predicate-oracle.sh`), whose output is committed as
//! `tests/fixtures/predicate_oracle/NAME.txt`. Here the same script builds the same world and entities in OUR level
//! (`ServerLevel`, `ServerPlayer`, a test-only living `TestMob`) and every `pred_*` line is evaluated by our predicates:
//! `R id true|false|ERR` must equal vanilla's result and `J id json` (the encoding of the decoded predicate) must equal
//! vanilla's JSON.

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{GameType, Pose};
use mc_data::{EntityType, MobEffect};
use mc_game::advancements::predicates::{DamagePredicate, DamageSourcePredicate, EntityPredicate, LocationPredicate};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::living::LivingEntityData;
use mc_game::world::entity::plain::PlainEntity;
use mc_game::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::play::Input;
use std::any::Any;
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------------------------------- test entity

/// A living non-player entity of any type (vanilla's `Mob`s are not ported yet): carries the state the oracle scripts
/// set (baby, sheared, cube size, raider flags, target) and answers the predicates' seam hooks with it.
struct TestMob {
    living: LivingEntityData,
    baby: bool,
    sheared: Option<bool>,
    cube_size: Option<i32>,
    /// `Raider.isPatrolLeader()`; a captain also needs the ominous banner on the head.
    patrol_leader: Option<bool>,
    target: Option<EntityId>,
}

impl Entity for TestMob {
    fn base(&self) -> &EntityData {
        &self.living.entity
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.living.entity
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    fn living(&self) -> Option<&LivingEntityData> {
        Some(&self.living)
    }
    fn living_mut(&mut self) -> Option<&mut LivingEntityData> {
        Some(&mut self.living)
    }
    fn is_baby(&self) -> bool {
        self.baby
    }
    fn get_target(&self) -> Option<EntityId> {
        self.target
    }
    fn sheep_sheared(&self) -> Option<bool> {
        self.sheared
    }
    fn cube_mob_size(&self) -> Option<i32> {
        self.cube_size
    }
    fn raider_state(&self) -> Option<(bool, bool)> {
        self.patrol_leader.map(|leader| (false, leader && self.living.equipment.get(EquipmentSlot::Head).item() == mc_data::Item::WHITE_BANNER))
    }
}

/// Types the oracle spawns as living entities (everything else is a non-living `PlainEntity`).
const LIVING: &[&str] = &["pig", "cow", "sheep", "zombie", "slime", "pillager", "villager", "skeleton", "armor_stand"];

// ---------------------------------------------------------------------------------------------------- world

struct World {
    level: ServerLevel,
    refs: HashMap<String, EntityId>,
    lines: Vec<String>,
}

fn tokens(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quote) = (Vec::new(), String::new(), 0i32, false);
    for c in s.chars() {
        if c == '"' {
            quote = !quote;
        }
        if !quote {
            match c {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if c.is_whitespace() && depth == 0 && !quote {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn num(s: &str) -> f64 {
    s.parse().unwrap_or_else(|_| panic!("number {s}"))
}

fn id(s: &str) -> Identifier {
    Identifier::parse(if s.contains(':') { s.to_owned() } else { format!("minecraft:{s}") }.as_str()).unwrap()
}

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, Some(p.trim_end_matches(']'))),
        None => (text, None),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("block {name}"));
    let props: Vec<(&str, &str)> = props.map(|p| p.split(',').filter_map(|kv| kv.split_once('=')).collect()).unwrap_or_default();
    block.state_from_properties(props).expect("valid properties")
}

fn parse_stack(spec: &str, count: i32) -> ItemStack {
    use mc_game::commands::command_build_context::StaticRegistries;
    use mc_game::commands::CommandBuildContext;
    use std::sync::Arc;
    let context = CommandBuildContext::simple(Arc::new(StaticRegistries), mc_world::level_storage::FeatureFlagSet::ALL);
    let mut reader = mc_brigadier::StringReader::new(spec);
    mc_game::commands::arguments::item::item_parser::ItemParser::new(&context)
        .parse(&mut reader)
        .unwrap_or_else(|e| panic!("stack {spec}: {e:?}"))
        .create_item_stack(count)
        .unwrap_or_else(|e| panic!("stack {spec}: {e:?}"))
}

fn game_type(name: &str) -> GameType {
    GameType::by_serialized_name(name).unwrap_or_else(|| panic!("game type {name}"))
}

impl World {
    fn new() -> World {
        let mut level = ServerLevel::detached(id("overworld"));
        level.chunk_source.set_view_distance(4);
        level.chunk_source.set_simulation_distance(4);
        World { level, refs: HashMap::new(), lines: Vec::new() }
    }

    fn entity(&self, name: &str) -> &dyn Entity {
        let id = *self.refs.get(name).unwrap_or_else(|| panic!("unknown ref {name}"));
        self.level.get_entity(id).expect("entity in level")
    }

    fn entity_mut(&mut self, name: &str) -> &mut dyn Entity {
        let id = *self.refs.get(name).unwrap_or_else(|| panic!("unknown ref {name}"));
        self.level.get_entity_mut(id).expect("entity in level")
    }

    fn load_chunk_at(&mut self, pos: BlockPos) {
        self.level.chunk_blocking_mut(pos.x() >> 4, pos.z() >> 4);
    }

    fn set_block(&mut self, pos: BlockPos, state: BlockState) {
        self.load_chunk_at(pos);
        self.level.set_block(pos, state, 3, 512);
    }

    fn place(&mut self, e: Box<dyn Entity>, name: &str) {
        let id = e.id();
        let pos = e.base().block_position();
        self.load_chunk_at(pos);
        assert!(self.level.add_fresh_entity(e), "{name} was not added");
        self.refs.insert(name.to_owned(), id);
    }

    fn spawn_player(&mut self, a: &[String]) {
        let id = self.level.next_entity_id();
        let uuid = Uuid::from_u128(0x1000 + id as u128);
        let mut p = ServerPlayer::new(id, self.level.dimension.clone(), GameProfile { id: uuid, name: a[1].clone(), properties: vec![] }, Default::default());
        p.set_game_mode_for_player(game_type(&a[2]), None);
        p.set_pos(num(&a[3]), num(&a[4]), num(&a[5]));
        let (yaw, pitch) = (a.get(6).map_or(0.0, |s| num(s) as f32), a.get(7).map_or(0.0, |s| num(s) as f32));
        p.set_rot(yaw, pitch);
        p.set_y_head_rot(yaw);
        self.load_chunk_at(p.base().block_position());
        self.level.add_player(Box::new(p));
        let id = self.level.players().iter().copied().last().expect("player added");
        self.refs.insert(a[1].clone(), id);
    }

    fn spawn(&mut self, a: &[String]) {
        let ty = EntityType::by_name(&id(&a[2]).to_string()).unwrap_or_else(|| panic!("entity type {}", a[2]));
        let nbt = a.last().filter(|s| s.starts_with('{')).map(|s| mc_nbt::snbt::parse_compound(s).expect("snbt")).unwrap_or_default();
        let numeric = a.len() - 1 - usize::from(a.last().is_some_and(|s| s.starts_with('{')));
        let id_ = self.level.next_entity_id();
        let mut entity: Box<dyn Entity> = if LIVING.contains(&ty.name().trim_start_matches("minecraft:")) {
            let age = nbt.get_int_or("Age", 0);
            Box::new(TestMob {
                living: LivingEntityData::new(ty, id_, 300, |_| {}),
                baby: age < 0 || nbt.get_boolean_or("IsBaby", false),
                sheared: (ty == EntityType::SHEEP).then(|| nbt.get_boolean_or("Sheared", false)),
                cube_size: (ty == EntityType::SLIME).then(|| nbt.get_int_or("Size", 1)),
                patrol_leader: (ty == EntityType::PILLAGER).then(|| nbt.get_boolean_or("PatrolLeader", false)),
                target: None,
            })
        } else {
            Box::new(PlainEntity::new(ty, id_))
        };
        entity.set_pos(num(&a[3]), num(&a[4]), num(&a[5]));
        if numeric > 7 {
            entity.set_rot(num(&a[6]) as f32, num(&a[7]) as f32);
        }
        if let Some(tags) = nbt.get_list("Tags") {
            for t in tags.iter() {
                entity.base_mut().add_tag(t.as_string().expect("tag"));
            }
        }
        if nbt.get("Fire").is_some() {
            entity.base_mut().remaining_fire_ticks = nbt.get_short_or("Fire", 0) as i32;
        }
        self.place(entity, &a[1]);
    }

    fn spawn_item(&mut self, a: &[String]) {
        let stack = parse_stack(&a[2], 1);
        let item = ItemEntity::with_stack_and_motion(&self.level, num(&a[3]), num(&a[4]), num(&a[5]), stack, 0.0, 0.0, 0.0);
        self.place(Box::new(item), &a[1]);
    }

    fn give(&mut self, a: &[String]) {
        let stack = parse_stack(&a[3], a.get(4).map_or(1, |c| c.parse().unwrap()));
        let slot = a[2].as_str();
        let e = self.entity_mut(&a[1]);
        let named = match slot {
            "mainhand" => Some(EquipmentSlot::MainHand),
            "offhand" => Some(EquipmentSlot::OffHand),
            "head" => Some(EquipmentSlot::Head),
            "chest" => Some(EquipmentSlot::Chest),
            "legs" => Some(EquipmentSlot::Legs),
            "feet" => Some(EquipmentSlot::Feet),
            _ => None,
        };
        if let Some(player) = e.player_mut() {
            match named {
                Some(s) => {
                    player.inventory.equipment_set(s, stack);
                }
                None => {
                    let n: usize = slot.strip_prefix("hotbar").expect("hotbarN").parse().unwrap();
                    player.inventory.get_non_equipment_items_mut()[n] = stack;
                }
            }
        } else {
            e.living_mut().expect("living").equipment.set(named.expect("named slot"), stack);
        }
    }

    fn effect(&mut self, a: &[String]) {
        let effect = MobEffect::by_name(&id(&a[2]).to_string()).unwrap_or_else(|| panic!("effect {}", a[2]));
        let hidden = a.iter().any(|s| s == "hidden");
        let instance = MobEffectInstance::new(effect, a[4].parse().unwrap(), a[3].parse().unwrap(), false, !hidden, !hidden, None);
        self.entity_mut(&a[1]).living_mut().expect("living").active_effects.insert(effect, instance);
    }

    fn set(&mut self, a: &[String]) {
        let value = a[3].as_str();
        let e = self.entity_mut(&a[1]);
        match a[2].as_str() {
            "health" => e.living_mut().unwrap().set_health(value.parse().unwrap()),
            "food" => e.player_mut().unwrap().food_data.food_level = value.parse().unwrap(),
            "saturation" => e.player_mut().unwrap().food_data.saturation_level = value.parse().unwrap(),
            "xp_level" => e.player_mut().unwrap().experience_level = value.parse().unwrap(),
            "air" => e.base_mut().set_air_supply(value.parse().unwrap()),
            "fire" => e.base_mut().remaining_fire_ticks = value.parse().unwrap(),
            "fall_distance" => e.base_mut().fall_distance = num(value),
            "tag" => {
                e.base_mut().add_tag(value);
            }
            other => panic!("set {other}"),
        }
    }

    fn own(&mut self, a: &[String]) -> bool {
        let flag = |i: usize| a[i] == "true";
        match a[0].as_str() {
            "onground" => self.entity_mut(&a[1]).base_mut().set_on_ground_raw(flag(2)),
            "pose" => {
                let pose = Pose::ALL.iter().copied().find(|p| p.name().eq_ignore_ascii_case(&a[2])).unwrap_or_else(|| panic!("pose {}", a[2]));
                self.entity_mut(&a[1]).set_pose(pose)
            }
            "sprint" => self.entity_mut(&a[1]).base_mut().set_shared_flag(mc_game::world::entity::entity::FLAG_SPRINTING, flag(2)),
            "swimming" => self.entity_mut(&a[1]).base_mut().set_shared_flag(mc_game::world::entity::entity::FLAG_SWIMMING, flag(2)),
            "fallflying" => self.entity_mut(&a[1]).base_mut().set_shared_flag(mc_game::world::entity::entity::FLAG_FALL_FLYING, flag(2)),
            "flying" => self.entity_mut(&a[1]).player_mut().unwrap().abilities.flying = flag(2),
            "tickcount" => self.entity_mut(&a[1]).base_mut().tick_count = a[2].parse().unwrap(),
            "ride" => {
                let (passenger, vehicle) = (self.refs[&a[1]], self.refs[&a[2]]);
                let ok = self.level.with_entity(passenger, |level, e| level.start_riding(e, vehicle, true, false)).expect("passenger");
                assert!(ok, "ride {} {}", a[1], a[2]);
            }
            "target" => {
                // `Mob.asValidTarget`: creative and spectator players are never targets
                let target = self.refs[&a[2]];
                let invalid = self.entity(&a[2]).player().is_some() && (self.entity(&a[2]).is_spectator() || self.entity(&a[2]).player().is_some_and(|p| p.abilities.instabuild));
                let target = (!invalid).then_some(target);
                self.entity_mut(&a[1]).as_any_mut().downcast_mut::<TestMob>().expect("TestMob").target = target;
            }
            "team_join" => {
                let name = self.entity(&a[1]).get_scoreboard_name();
                let mut board = self.level.scoreboard_mut();
                if board.get_player_team(&a[2]).is_none() {
                    board.add_player_team(&a[2]);
                }
                board.add_player_to_team(&name, &a[2]);
            }
            "pinput" => {
                let flags: Vec<&str> = a[2].split(',').collect();
                let f = |n: &str| flags.contains(&n);
                let input = Input { forward: f("forward"), backward: f("back"), left: f("left"), right: f("right"), jump: f("jump"), shift: f("sneak"), sprint: f("sprint") };
                self.entity_mut(&a[1]).as_any_mut().downcast_mut::<ServerPlayer>().unwrap().last_client_input = input;
            }
            "known_movement" => {
                self.entity_mut(&a[1]).as_any_mut().downcast_mut::<ServerPlayer>().unwrap().last_known_client_movement = Vec3::new(num(&a[2]), num(&a[3]), num(&a[4]));
            }
            "ominous_banner" => {
                let banner = parse_stack("minecraft:white_banner", 1);
                self.entity_mut(&a[1]).living_mut().expect("living").equipment.set(EquipmentSlot::Head, banner);
            }
            "recipe" => {
                let p = self.entity_mut(&a[1]).as_any_mut().downcast_mut::<ServerPlayer>().unwrap();
                let mut book = match p.recipe_book.take() {
                    Some(Tag::Compound(c)) => c,
                    _ => CompoundTag::new(),
                };
                let mut recipes = book.get_list("recipes").cloned().unwrap_or_default();
                recipes.push(Tag::String(id(&a[2]).to_string()));
                book.put("recipes", recipes);
                p.recipe_book = Some(Tag::Compound(book));
            }
            _ => return false,
        }
        true
    }

    fn origin(&self, s: &str) -> Option<Vec3> {
        if s == "none" {
            None
        } else if let Some(r) = s.strip_prefix('@') {
            Some(self.entity(r).position())
        } else {
            let p: Vec<f64> = s.split(',').map(num).collect();
            Some(Vec3::new(p[0], p[1], p[2]))
        }
    }

    /// `R id result` + `J id json` for one decoded predicate.
    fn report<A: 'static>(&mut self, id: &str, codec: &mc_dfu::Codec<A>, json: &str, eval: impl FnOnce(&World, &A) -> bool) {
        match mc_dfu::parse_json(codec, json).into_result() {
            Err(_) => self.lines.push(format!("R {id} ERR")),
            Ok(p) => {
                let result = eval(self, &p);
                self.lines.push(format!("R {id} {result}"));
                self.lines.push(format!("J {id} {}", mc_dfu::encode_json(codec, &p).into_result().expect("encodes")));
            }
        }
    }

    fn pred(&mut self, a: &[String]) -> bool {
        match a[0].as_str() {
            "pred_entity" => {
                let origin = self.origin(&a[3]);
                self.report(&a[1], &EntityPredicate::codec(), &a[4], |w, p| p.matches(&w.level, origin, w.entity(&a[2])));
            }
            "pred_location" => {
                let (x, y, z) = (num(&a[2]), num(&a[3]), num(&a[4]));
                self.report(&a[1], &LocationPredicate::codec(), &a[5], |w, p| p.matches(&w.level, x, y, z));
            }
            "pred_damage" => {
                let origin = self.origin(&a[3]).expect("origin");
                let (direct, causing) = self.source_entities(&a[5..]);
                let ty = id(&a[2]);
                self.report(&a[1], &DamageSourcePredicate::codec(), &a[4], |w, p| {
                    p.matches_parts(&w.level, origin, &ty, direct.as_deref().map(|n| w.entity(n)), causing.as_deref().map(|n| w.entity(n)), direct == causing)
                });
            }
            "pred_damage_taken" => {
                let (direct, causing) = self.source_entities(&a[8..]);
                let ty = id(&a[3]);
                let (dealt, taken, blocked) = (a[5].parse::<f32>().unwrap(), a[6].parse::<f32>().unwrap(), a[7] == "true");
                self.report(&a[1], &DamagePredicate::codec(), &a[4], |w, p| {
                    p.matches_parts(&w.level, w.entity(&a[2]), &ty, direct.as_deref().map(|n| w.entity(n)), causing.as_deref().map(|n| w.entity(n)), direct == causing, dealt, taken, blocked)
                });
            }
            _ => return false,
        }
        true
    }

    fn source_entities(&self, kv: &[String]) -> (Option<String>, Option<String>) {
        let get = |key: &str| kv.iter().find_map(|s| s.strip_prefix(&format!("{key}=")).map(str::to_owned));
        (get("direct"), get("by"))
    }

    fn run(&mut self, a: &[String]) {
        match a[0].as_str() {
            "level" | "seed" | "difficulty" | "preload" | "note" | "gamerule" | "time" | "reseed" | "entity_seed" => {}
            "block" => self.set_block(BlockPos::new(a[1].parse().unwrap(), a[2].parse().unwrap(), a[3].parse().unwrap()), parse_state(&a[4])),
            "fill" => {
                let n: Vec<i32> = a[1..7].iter().map(|s| s.parse().unwrap()).collect();
                let state = parse_state(&a[7]);
                for x in n[0].min(n[3])..=n[0].max(n[3]) {
                    for y in n[1].min(n[4])..=n[1].max(n[4]) {
                        for z in n[2].min(n[5])..=n[2].max(n[5]) {
                            self.set_block(BlockPos::new(x, y, z), state);
                        }
                    }
                }
            }
            "spawn_player" => self.spawn_player(a),
            "spawn" => self.spawn(a),
            "spawn_item" => self.spawn_item(a),
            "give" => self.give(a),
            "effect" => self.effect(a),
            "set" => self.set(a),
            "snap" => {
                let e = self.entity_mut(&a[1]);
                e.snap_to_pos(num(&a[2]), num(&a[3]), num(&a[4]));
            }
            _ => {
                if !self.own(a) && !self.pred(a) {
                    panic!("unknown scenario command {}", a[0]);
                }
            }
        }
    }
}

/// JSON values compared like `serde_json` after numbers are normalized to `f64` (vanilla prints `3.0`, we may print `3`).
fn normalize(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Number(n) => Value::from(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        other => other,
    }
}

fn run_scenario(name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
    let script = std::fs::read_to_string(format!("{dir}/oracle/scenarios/predicates/{name}.txt")).expect("scenario");
    let fixture = std::fs::read_to_string(format!("{dir}/fixtures/predicate_oracle/{name}.txt")).expect("fixture (run scripts/predicate-oracle.sh)");
    let mut world = World::new();
    for line in script.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        world.run(&tokens(line));
    }
    // `R id result` / `J id json` per case id
    let by_id = |lines: Vec<&str>| {
        let mut m: Vec<(String, String, Option<serde_json::Value>)> = Vec::new();
        for l in lines {
            let parts: Vec<&str> = l.splitn(3, ' ').collect();
            match parts[0] {
                "R" => m.push((parts[1].to_owned(), parts[2].to_owned(), None)),
                "J" => m.last_mut().filter(|e| e.0 == parts[1]).expect("J after R").2 = Some(normalize(serde_json::from_str(parts[2]).expect("json"))),
                _ => {}
            }
        }
        m
    };
    let expected = by_id(fixture.lines().collect());
    let actual = by_id(world.lines.iter().map(String::as_str).collect());
    let mut failures = Vec::new();
    for (id, result, json) in &expected {
        match actual.iter().find(|(i, ..)| i == id) {
            None => failures.push(format!("  {id}: vanilla {result}, ours: no result")),
            Some((_, r, j)) if r != result || j != json => {
                failures.push(format!("  {id}: vanilla {result} {}\n  {id}: ours    {r} {}", json.as_ref().map_or(String::new(), |j| j.to_string()), j.as_ref().map_or(String::new(), |j| j.to_string())))
            }
            _ => {}
        }
    }
    assert_eq!(expected.len(), actual.len(), "{name}: case count");
    assert!(failures.is_empty(), "{name}: {} of {} cases differ:\n{}", failures.len(), expected.len(), failures.join("\n"));
}

#[test]
fn entity_basic() {
    run_scenario("entity_basic");
}
