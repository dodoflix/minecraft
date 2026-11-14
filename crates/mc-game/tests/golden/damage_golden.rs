//! Damage pipeline / death / respawn golden: the scenarios `tests/oracle/scenarios/{damage_*,death_*,hurt_*}.txt` run on the
//! real vanilla server (`scripts/entity-oracle.sh`, fixtures in `tests/fixtures/entity_oracle/`) are replayed on our
//! `ServerPlayer`s + `ServerLevel` (real vanilla registries: damage types + tags, enchantments, loot tables) and compared line
//! by line, nothing ignored but the `PKT` types listed in [`PKT_TYPES`] complement (the join burst, chunk/tracker traffic).
//!
//! Replay: `level`, `difficulty`, `gamerule`, `cmd difficulty`, `entity_counter` (the level's own id counter; the next entity gets
//! id N + 1), `spawn` (the real entity class, created with the command spawn reason), `spawn_player`,
//! `give` (item components via the command item parser), `effect`, `set`, `snap`, `use`, `hurt` (`by=`/`direct=`/`at=`),
//! `respawn` (`PlayerList.respawn` core), `tick`, `watch`, `dump` (`player:`/`entity:`/`entities`/`inventory:`/`packets:`).
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, GameType};
use mc_data::{EntityType, Item, MobEffect};
use mc_game::server::level::ServerLevel;
use mc_game::server::level::chunk_map::PlayerTrackingInfo;
use mc_game::server::level::chunk_send_sink::ChunkSendSink;
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::server::players::PlayerHandle;
use mc_game::world::damagesource::DamageSource;
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::{InteractionHand, ItemStack};
use mc_game::world::level::entity::Visibility;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_nbt::CompoundTag;
use mc_protocol::codec::GameProfile;
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_names};
use mc_world::level_storage::Difficulty;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

static ACCESS: std::sync::LazyLock<mc_game::registry::RegistryAccess> =
    std::sync::LazyLock::new(|| {
        let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = common::load_world(
            dir.path(),
            mc_world::level_storage::WorldDataConfiguration::DEFAULT,
            true,
        )
        .expect("default world loads");
        stem.registries.composite_access().clone()
    });

fn registries() -> mc_game::registry::RegistryAccess {
    ACCESS.clone()
}

fn f(v: f32) -> String {
    mc_core::math::java::java_float_to_string(v)
}

fn d(v: f64) -> String {
    mc_core::math::java::java_double_to_string(v)
}

fn overworld() -> Identifier {
    Identifier::parse("minecraft:overworld").unwrap()
}

/// The oracle driver's `tokens`: whitespace separated, `[..]`, `{..}` and `".."` keep their spaces.
fn tokens(s: &str) -> Vec<String> {
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

/// `EntityOracleDriver.hash`.
fn driver_hash(s: &str) -> i64 {
    let mut h: i64 = 1125899906842597;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(i64::from(c));
    }
    h
}

fn item_string(s: &ItemStack) -> String {
    if s.is_empty() {
        return "empty".into();
    }
    let mut b = format!("{}*{}", s.item().name(), s.count());
    if s.is_damageable_item() && s.get_damage_value() != 0 {
        b.push_str(&format!("/dmg={}", s.get_damage_value()));
    }
    b
}

fn pose_name(pose: mc_data::enums::Pose) -> String {
    format!("{pose:?}").chars().fold(String::new(), |mut s, c| {
        if c.is_uppercase() && !s.is_empty() {
            s.push('_');
        }
        s.push(c.to_ascii_lowercase());
        s
    })
}

fn parse_state(text: &str) -> mc_data::block::BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, Some(p.trim_end_matches(']'))),
        None => (text, None),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("block {name}"));
    let props: Vec<(&str, &str)> = props
        .map(|p| p.split(',').filter_map(|kv| kv.split_once('=')).collect())
        .unwrap_or_default();
    block
        .state_from_properties(props)
        .expect("valid properties")
}

fn parse_stack(spec: &str, count: i32) -> ItemStack {
    use mc_game::commands::CommandBuildContext;
    use mc_game::commands::command_build_context::StaticRegistries;
    use std::sync::Arc;
    let context = CommandBuildContext::simple(
        Arc::new(StaticRegistries),
        mc_world::level_storage::FeatureFlagSet::ALL,
    );
    let mut reader = mc_brigadier::StringReader::new(spec);
    let mut stack = mc_game::commands::arguments::item::item_parser::ItemParser::new(&context)
        .parse(&mut reader)
        .unwrap_or_else(|e| panic!("stack {spec}: {e:?}"))
        .create_item_stack(1)
        .unwrap_or_else(|e| panic!("stack {spec}: {e:?}"));
    stack.set_count(count);
    stack
}

// ---------------------------------------------------------------------------------------------------- replay

/// Packet types compared (`PKT` lines of other types are dropped from both sides: join/respawn bursts, chunk and tracker traffic).
const PKT_TYPES: &[&str] = &[
    "minecraft:damage_event",
    "minecraft:hurt_animation",
    "minecraft:entity_event",
    "minecraft:player_combat_kill",
    "minecraft:player_combat_enter",
    "minecraft:player_combat_end",
    "minecraft:set_health",
    "minecraft:set_experience",
    "minecraft:update_mob_effect",
    "minecraft:remove_mob_effect",
    "minecraft:take_item_entity",
];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_varint(bytes: &[u8]) -> i32 {
    let (mut value, mut shift) = (0i32, 0);
    for &b in bytes {
        value |= i32::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    value
}

struct Replay {
    level: ServerLevel,
    refs: HashMap<String, EntityId>,
    names: Vec<String>,
    players: Vec<String>,
    rx: HashMap<String, UnboundedReceiver<Outbound>>,
    out: Vec<String>,
    ticks: i64,
    known: std::collections::HashSet<EntityId>,
    auto_counter: HashMap<String, i32>,
    watches: Vec<Vec<String>>,
    world: mc_game::server::level::level_data::SharedWorldData,
    packet_names: HashMap<i32, &'static str>,
    /// the oracle world exists (first world command ran): every later command restarts the entity seed sequence
    booted: bool,
    /// players whose first chunk batch goes out at the end of the current tick
    unsent_chunks: Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Replay {
    fn new() -> Self {
        let mut level = ServerLevel::detached(overworld());
        // `Level.random` is seeded at boot like the oracle (`seed ^ 0x5DEECE66DL`, default world seed 4242); an unseeded one
        // made `ExperienceOrb.award`'s merge id (a draw from it) differ from run to run
        {
            use mc_core::random::RandomSource as _;
            level.tick_state.random.set_seed(4242 ^ 0x5DEECE66D);
        }
        // the oracle's references keep a removed entity
        level.removed_entities = Some(Default::default());
        level.registries = registries();
        // the level's own counter (the shared static drifts with every test in the binary): ids start at 1 like a pinned oracle
        level.entity_id_counter = Some(std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0)));
        let world = mc_game::server::level::level_data::default_world_data();
        level.level_data = Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
            world.clone(),
        ));
        for x in -3..=130 {
            for z in -4..=12 {
                level.set_block_and_update(
                    BlockPos::new(x, -61, z),
                    Block::GRASS_BLOCK.default_state(),
                );
            }
        }
        for cx in -1..=8 {
            for cz in -1..=1 {
                level
                    .entity_manager
                    .update_chunk_status(ChunkPos::new(cx, cz), Visibility::Ticking);
            }
        }
        let packet_names = packet_names(ConnectionState::Play, PacketFlow::Clientbound)
            .iter()
            .copied()
            .collect();
        Replay {
            level,
            refs: HashMap::new(),
            names: Vec::new(),
            players: Vec::new(),
            rx: HashMap::new(),
            out: Vec::new(),
            ticks: 0,
            known: Default::default(),
            auto_counter: HashMap::new(),
            watches: Vec::new(),
            world,
            packet_names,
            booted: false,
            unsent_chunks: Vec::new(),
        }
    }

    /// `ChunkMap.addEntity(player)`: the player's chunk view, so entity trackers pair with it. The oracle's connections never
    /// acknowledge a chunk batch: after the first tick only the nine chunks around the player were ever sent, every other
    /// chunk of the view stays pending, so a tracker pairs with the player only inside them.
    fn track_chunks(&mut self, p: &ServerPlayer) {
        struct FirstBatch {
            center: ChunkPos,
            sent: std::sync::Arc<std::sync::atomic::AtomicBool>,
        }
        impl ChunkSendSink for FirstBatch {
            fn mark_chunk_pending_to_send(&mut self, _: ChunkPos) {}
            fn drop_chunk(&mut self, _: ChunkPos, _: bool) {}
            fn is_pending(&self, pos: i64) -> bool {
                let near = (ChunkPos::get_x(pos) - self.center.x).abs() <= 1
                    && (ChunkPos::get_z(pos) - self.center.z).abs() <= 1;
                !(near && self.sent.load(std::sync::atomic::Ordering::Relaxed))
            }
        }
        let sent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.unsent_chunks.push(sent.clone());
        self.level
            .chunk_source
            .chunk_map
            .add_player(PlayerTrackingInfo {
                entity_id: p.base_id(),
                uuid: p.uuid(),
                connection: p.connection.clone().expect("connection"),
                sink: Box::new(FirstBatch {
                    center: p.chunk_position(),
                    sent,
                }),
                position: p.position(),
                requested_view_distance: p.requested_view_distance,
                spectator: p.is_spectator(),
                alive: p.is_alive(),
            });
    }

    fn id_of(&self, name: &str) -> EntityId {
        *self
            .refs
            .get(name)
            .unwrap_or_else(|| panic!("unknown ref {name}"))
    }

    fn entity(&self, name: &str) -> &dyn Entity {
        let id = self.id_of(name);
        self.level
            .get_entity(id)
            .or_else(|| self.level.get_dead_player(id))
            .unwrap_or_else(|| panic!("{name} is not in the level"))
    }

    fn with_player<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
    ) -> R {
        let id = self.id_of(name);
        self.level
            .with_player(id, f)
            .unwrap_or_else(|| panic!("{name} is not a player"))
    }

    fn with_entity<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
    ) -> R {
        let id = self.id_of(name);
        self.level
            .with_entity(id, f)
            .unwrap_or_else(|| panic!("{name} is not in the level"))
    }

    fn set_difficulty(&mut self, name: &str) {
        let d = match name {
            "peaceful" => Difficulty::Peaceful,
            "easy" => Difficulty::Easy,
            "hard" => Difficulty::Hard,
            _ => Difficulty::Normal,
        };
        self.world.lock().settings.difficulty_settings.difficulty = d;
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let t = tokens(line);
        if self.booted && t[0] != "reseed" {
            mc_core::random::reseed_uniquifier(driver_hash(line));
        }
        match t[0].as_str() {
            "level" => {}
            "difficulty" => self.set_difficulty(&t[1]),
            "cmd" => {
                let c = tokens(line[3..].trim());
                assert!(
                    c.len() == 2 && c[0] == "difficulty",
                    "unsupported cmd `{line}`"
                );
                self.set_difficulty(&c[1]);
            }
            "gamerule" => {
                let v = t[2] == "true";
                self.level
                    .server_state
                    .rules
                    .update(|r| match t[1].as_str() {
                        "keep_inventory" => r.keep_inventory = v,
                        "fall_damage" => r.fall_damage = v,
                        "fire_damage" => r.fire_damage = v,
                        "drowning_damage" => r.drowning_damage = v,
                        "freeze_damage" => r.freeze_damage = v,
                        "show_death_messages" => r.show_death_messages = v,
                        "forgive_dead_players" => r.forgive_dead_players = v,
                        other => panic!("gamerule {other}"),
                    });
            }
            "block" => {
                let state = parse_state(&t[4]);
                self.level.set_block(
                    BlockPos::new(
                        t[1].parse().unwrap(),
                        t[2].parse().unwrap(),
                        t[3].parse().unwrap(),
                    ),
                    state,
                    3,
                    512,
                );
            }
            "fill" => {
                let state = parse_state(&t[7]);
                let n = |i: usize| t[i].parse::<i32>().unwrap();
                for x in n(1).min(n(4))..=n(1).max(n(4)) {
                    for y in n(2).min(n(5))..=n(2).max(n(5)) {
                        for z in n(3).min(n(6))..=n(3).max(n(6)) {
                            self.level.set_block(BlockPos::new(x, y, z), state, 3, 512);
                        }
                    }
                }
            }
            "spawn_player" => self.spawn_player(&t),
            "spawn" => self.spawn(&t),
            "give" => self.give(&t),
            "effect" => {
                let effect = MobEffect::by_name(&t[2]).unwrap_or_else(|| panic!("effect {}", t[2]));
                let (amp, dur) = (t[3].parse().unwrap(), t[4].parse().unwrap());
                let hidden = t.iter().any(|s| s == "hidden");
                let added = self.with_entity(&t[1], |level, e| {
                    e.add_effect(
                        level,
                        MobEffectInstance::new(effect, dur, amp, false, !hidden, !hidden, None),
                        None,
                    )
                });
                self.out
                    .push(format!("EFFECT {} {} added={}", t[1], t[2], added));
            }
            "set" => {
                let v: f64 = t[3].parse().unwrap_or(0.0);
                let what = t[2].clone();
                self.with_entity(&t[1], |_, e| match what.as_str() {
                    "health" => e.living_mut().expect("living").set_health(v as f32),
                    "absorption" => e.set_absorption_amount(v as f32),
                    "xp_level" => e
                        .as_any_mut()
                        .downcast_mut::<ServerPlayer>()
                        .expect("player")
                        .set_experience_levels(v as i32),
                    "food" => e
                        .player_mut()
                        .expect("player")
                        .food_data
                        .set_food_level(v as i32),
                    "saturation" => e
                        .player_mut()
                        .expect("player")
                        .food_data
                        .set_saturation(v as f32),
                    "exhaustion" => {
                        e.player_mut().expect("player").food_data.exhaustion_level = v as f32
                    }
                    "air" => e.base_mut().set_air_supply(v as i32),
                    "fire" => e.base_mut().remaining_fire_ticks = v as i32,
                    "fall_distance" => e.base_mut().fall_distance = v,
                    "ticks_frozen" => e.base_mut().set_ticks_frozen(v as i32),
                    other => panic!("set {other}"),
                });
            }
            "snap" => {
                let n = |i: usize| t[i].parse::<f64>().unwrap();
                let (x, y, z) = (n(2), n(3), n(4));
                self.with_entity(&t[1], |_, e| {
                    let yaw = t
                        .get(5)
                        .map_or(e.base().y_rot(), |s| s.parse::<f64>().unwrap() as f32);
                    let pitch = t
                        .get(6)
                        .map_or(e.base().x_rot(), |s| s.parse::<f64>().unwrap() as f32);
                    e.snap_to(x, y, z, yaw, pitch);
                });
            }
            "use" => {
                let hand = if t.get(2).is_some_and(|h| h == "off") {
                    InteractionHand::OffHand
                } else {
                    InteractionHand::MainHand
                };
                self.with_player(&t[1], |level, p| {
                    p.use_item(level, hand);
                });
            }
            "entity_counter" => self
                .level
                .entity_id_counter
                .as_ref()
                .expect("per-level counter")
                .store(t[1].parse().unwrap(), std::sync::atomic::Ordering::Relaxed),
            "hurt" => self.hurt(&t),
            "respawn" => self.respawn(&t[1]),
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| n.parse().unwrap()) {
                    self.tick_once();
                }
            }
            "watch" => {
                if t[1] == "off" {
                    self.watches.clear();
                } else {
                    self.watches.push(t[1..].to_vec());
                }
            }
            "dump" => self.dump(&t[1..]),
            other => panic!("unsupported scenario command `{other}`"),
        }
        self.name_new_entities();
        self.booted |= !matches!(
            t[0].as_str(),
            "level" | "seed" | "difficulty" | "preload" | "note"
        );
    }

    fn spawn_player(&mut self, t: &[String]) {
        let id = self.level.next_entity_id();
        let name = t[1].clone();
        mc_core::random::reseed_uniquifier(driver_hash(&name));
        let profile = GameProfile {
            id: mc_core::uuid_util::create_offline_player_uuid(&name),
            name: name.clone(),
            properties: vec![],
        };
        let mut p = ServerPlayer::new(id, overworld(), profile.clone(), Default::default());
        let mode = match t[2].as_str() {
            "creative" => GameType::Creative,
            "spectator" => GameType::Spectator,
            "adventure" => GameType::Adventure,
            _ => GameType::Survival,
        };
        p.set_game_mode_for_player(mode, None);
        let n = |i: usize| t[i].parse::<f64>().unwrap();
        let rot = |i: usize| {
            t.get(i)
                .filter(|s| s.parse::<f64>().is_ok())
                .map_or(0.0, |s| s.parse::<f64>().unwrap() as f32)
        };
        p.snap_to(n(3), n(4), n(5), rot(6), rot(7));
        // the oracle derives onGround once from the collision below (`!noCollision(box.move(0, -1.0E-3, 0))`)
        let below = BlockPos::new(
            mc_core::math::mth::floor_f64(n(3)),
            mc_core::math::mth::floor_f64(n(4) - 1.0e-3),
            mc_core::math::mth::floor_f64(n(5)),
        );
        p.base_mut()
            .set_on_ground_raw(!self.level.get_block_state(below).is_air());
        p.simulate_local_player = t.last().is_some_and(|l| l == "sim");
        // the connection: every packet the player (and the player list on its behalf) sends lands in `rx`
        let (tx, rx) = unbounded_channel();
        let sender = ConnectionSender::new(tx);
        p.connection = Some(sender.clone());
        let handle = PlayerHandle::new(
            id,
            profile,
            sender,
            "127.0.0.1".into(),
            mc_game::world::level::DimensionKey::try_parse("minecraft:overworld").unwrap(),
        );
        p.handle = Some(handle.clone());
        p.sync_handle();
        self.level.server_state.roster.add(handle);
        self.track_chunks(&p);
        self.level.add_player(Box::new(p));
        self.refs.insert(name.clone(), id);
        self.players.push(name.clone());
        self.names.push(name.clone());
        self.rx.insert(name, rx);
        self.known.insert(id);
    }

    fn spawn(&mut self, t: &[String]) {
        mc_core::random::reseed_uniquifier(driver_hash(&t[1]));
        let ty = EntityType::by_name(&t[2]).unwrap_or_else(|| panic!("entity type {}", t[2]));
        let request = EntitySpawnRequest {
            reason: EntitySpawnReason::Command,
            ignore_checks: true,
        };
        let mut mob = factory::create(ty, &self.level, request)
            .unwrap_or_else(|| panic!("cannot create {}", t[2]));
        let id = mob.id();
        let n = |i: usize| t[i].parse::<f64>().unwrap();
        let snbt = t
            .last()
            .filter(|s| s.starts_with('{') && t.len() > 6)
            .cloned();
        let rotated = t.len() - usize::from(snbt.is_some()) > 7;
        mob.snap_to(
            n(3),
            n(4),
            n(5),
            if rotated { n(6) as f32 } else { 0.0 },
            if rotated { n(7) as f32 } else { 0.0 },
        );
        if let Some(snbt) = snbt {
            let mut tag = CompoundTag::new();
            tag.merge(&mc_nbt::snbt::parse_compound(&snbt).expect("snbt"));
            mob.read_additional_save_data(Some(&self.level), &tag);
            mob.resolve_registry_refs(&self.level.registries);
            // `Entity.load`: a string tag is a literal component (`'"Bob"'` is the text `"Bob"` with its quotes)
            if let Some(name) = tag.get("CustomName") {
                mob.base_mut().set_custom_name(Some(
                    mc_text::Component::from_nbt(name).expect("custom name component"),
                ));
            }
        }
        self.level.add_fresh_entity(mob);
        self.refs.insert(t[1].clone(), id);
        self.names.push(t[1].clone());
        self.known.insert(id);
    }

    fn give(&mut self, t: &[String]) {
        let count = t.get(4).map_or(1, |c| c.parse().unwrap());
        let stack = parse_stack(&t[3], count);
        let slot = t[2].clone();
        self.with_entity(&t[1], |level, e| {
            if let Some(i) = slot
                .strip_prefix("hotbar")
                .or_else(|| slot.strip_prefix("inv"))
                .and_then(|i| i.parse::<usize>().ok())
            {
                e.as_any_mut()
                    .downcast_mut::<ServerPlayer>()
                    .expect("player")
                    .player
                    .inventory
                    .get_non_equipment_items_mut()[i] = stack;
                return;
            }
            let es = match slot.as_str() {
                "mainhand" => EquipmentSlot::MainHand,
                "offhand" => EquipmentSlot::OffHand,
                "head" => EquipmentSlot::Head,
                "chest" => EquipmentSlot::Chest,
                "legs" => EquipmentSlot::Legs,
                "feet" => EquipmentSlot::Feet,
                other => panic!("slot {other}"),
            };
            match e.as_any_mut().downcast_mut::<ServerPlayer>() {
                Some(p) => p.set_item_slot(level, es, stack),
                None => {
                    e.living_mut().expect("living").equipment.set(es, stack);
                }
            }
        });
    }

    /// An attacker named by a script: in the level, or removed earlier (the oracle's references keep it).
    fn known_entity(&self, id: EntityId) -> &dyn Entity {
        self.level
            .get_entity_or_removed(id)
            .expect("attacker entity")
    }

    fn hurt(&mut self, t: &[String]) {
        let key = Identifier::parse(&if t[2].contains(':') {
            t[2].clone()
        } else {
            format!("minecraft:{}", t[2])
        })
        .unwrap();
        let data = self
            .level
            .registries
            .get::<mc_game::registry::typed::damage_type::DamageType>("minecraft:damage_type", &key)
            .expect("damage type")
            .clone();
        let kv: HashMap<&str, &str> = t[4..].iter().filter_map(|s| s.split_once('=')).collect();
        let by = kv.get("by").map(|n| self.id_of(n));
        let direct = kv.get("direct").map(|n| self.id_of(n)).or(by);
        let at = kv.get("at").map(|s| {
            let v: Vec<f64> = s.split(',').map(|x| x.parse().unwrap()).collect();
            Vec3::new(v[0], v[1], v[2])
        });
        let source = DamageSource::new(
            key,
            std::sync::Arc::new(data),
            direct.map(|id| self.known_entity(id)),
            by.map(|id| self.known_entity(id)),
            at,
        );
        let amount = t[3].parse::<f64>().unwrap() as f32;
        let r = self.with_entity(&t[1], |level, e| e.hurt_server(level, &source, amount));
        self.out
            .push(format!("HURT {} {} {} -> {}", t[1], t[2], t[3], r));
    }

    /// `ServerboundClientCommandPacket(PERFORM_RESPAWN)` -> `PlayerList.respawn(player, false)`: the level half of it (the
    /// packet burst that is not compared is skipped).
    fn respawn(&mut self, name: &str) {
        use mc_game::server::players::respawn::{
            create_respawned_player, find_spawn_position, take_old_player,
        };
        let id = self.id_of(name);
        let keep_inventory = self.level.keep_inventory();
        let old = take_old_player(
            &mut self.level,
            id,
            mc_game::world::entity::RemovalReason::Killed,
        )
        .expect("player");
        let position = find_spawn_position(&mut self.level, false, 10, BlockPos::new(0, -60, 0));
        let transition = mc_game::server::players::teleport_transition::TeleportTransition::new(
            old.dimension.clone(),
            position,
            Vec3::ZERO,
            0.0,
            0.0,
            mc_game::server::players::teleport_transition::PostTeleportTransition::DO_NOTHING,
        );
        let new = create_respawned_player(old, &mut self.level, &transition, false, keep_inventory);
        new.send(
            &mc_protocol::packets::play::ClientboundSetExperiencePacket {
                experience_progress: new.player.experience_progress,
                experience_level: new.player.experience_level,
                total_experience: new.player.total_experience,
            },
        );
        mc_game::server::players::join::send_active_player_effects(&new);
        // `PlayerList.sendPlayerPermissionLevel`: entity event 24 (op level 0)
        new.send(&mc_protocol::packets::play::ClientboundEntityEventPacket {
            entity_id: id,
            event_id: 24,
        });
        self.track_chunks(&new);
        self.level.add_player(Box::new(new));
        self.with_player(name, |level, p| p.init_inventory_menu(level));
    }

    fn tick_once(&mut self) {
        self.ticks += 1;
        // `VanillaLevel.tick`: every tick restarts the entity-seed sequence
        mc_core::random::reseed_uniquifier(
            8682522807148012i64 ^ self.ticks.wrapping_mul(0x9E3779B97F4A7C15u64 as i64),
        );
        for name in self.players.clone() {
            self.with_player(&name, |_, p| {
                if p.simulate_local_player {
                    p.apply_input();
                }
            });
        }
        self.level.tick_entity_tracker();
        self.level.for_each_ticking(|level, e| {
            if e.is_removed() {
                return;
            }
            e.check_despawn(level);
            if !e.is_removed() {
                mc_game::world::entity::entity::common_tick(e);
                e.tick(level);
            }
        });
        for name in self.players.clone() {
            // `ServerGamePacketListenerImpl.tickPlayer`: `doTick()`, then a client-authoritative player is snapped back to
            // the position it had when the connection tick started (its server-side motion is thrown away)
            self.with_player(&name, |level, p| {
                let (pos, y_rot, x_rot) = (p.position(), p.get_y_rot(), p.get_x_rot());
                p.base_mut().set_old_pos(pos);
                p.do_tick(level);
                if !p.simulate_local_player {
                    p.abs_snap_to(pos.x, pos.y, pos.z, y_rot, x_rot);
                }
            });
        }
        for sent in self.unsent_chunks.drain(..) {
            sent.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.name_new_entities();
        if !self.watches.is_empty() {
            self.out.push(format!("T {}", self.ticks));
            for w in self.watches.clone() {
                self.dump(&w);
            }
        }
    }

    /// Entities created by game logic get stable names `type_path#n` in creation order (the driver's `nameNewEntities`).
    fn name_new_entities(&mut self) {
        let big = mc_core::phys::AABB::new(-1.0e6, -1.0e4, -1.0e6, 1.0e6, 1.0e4, 1.0e6);
        let mut ids = self.level.get_entities(None, &big, |_| true);
        ids.sort_unstable();
        for id in ids {
            if self.known.insert(id) {
                let path = self
                    .level
                    .get_entity(id)
                    .map(|e| {
                        e.get_type()
                            .name()
                            .trim_start_matches("minecraft:")
                            .to_owned()
                    })
                    .unwrap_or_default();
                let n = self
                    .auto_counter
                    .entry(path.clone())
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
                let name = format!("{path}#{n}");
                self.refs.insert(name.clone(), id);
                self.names.push(name);
            }
        }
    }

    fn dump(&mut self, what: &[String]) {
        for w in what {
            let (kind, arg) = w.split_once(':').map_or((w.as_str(), ""), |(k, a)| (k, a));
            match kind {
                "player" | "entity" => {
                    let line = self.describe(arg);
                    self.out.push(line);
                }
                "all_players" => {
                    for name in self.players.clone() {
                        let line = self.describe(&name);
                        self.out.push(line);
                    }
                }
                "entities" => {
                    self.name_new_entities();
                    let mut all: Vec<(EntityId, String)> = self
                        .names
                        .iter()
                        .filter_map(|n| {
                            let id = self.refs[n];
                            let e = self.level.get_entity(id)?;
                            (!e.is_player() && e.is_alive()).then(|| (id, n.clone()))
                        })
                        .collect();
                    all.sort();
                    for (_, name) in all {
                        let line = self.describe(&name);
                        self.out.push(line);
                    }
                }
                "inventory" => {
                    let lines: Vec<String> = {
                        let p = self.entity(arg).player().expect("player");
                        let inv = &p.inventory;
                        let mut v = Vec::new();
                        // `Inventory.getItem(i)`: 0..35 hotbar + main, 36..39 feet legs chest head, 40 offhand
                        let items = inv.get_non_equipment_items();
                        for (i, s) in items.iter().enumerate() {
                            if !s.is_empty() {
                                v.push(format!(
                                    "I {arg} {i} {}",
                                    common::entity_scenario::item_full(s)
                                ));
                            }
                        }
                        for (slot_no, slot) in [
                            (36, EquipmentSlot::Feet),
                            (37, EquipmentSlot::Legs),
                            (38, EquipmentSlot::Chest),
                            (39, EquipmentSlot::Head),
                            (40, EquipmentSlot::OffHand),
                        ] {
                            let s = inv.equipment_get(slot);
                            if !s.is_empty() {
                                v.push(format!(
                                    "I {arg} {slot_no} {}",
                                    common::entity_scenario::item_full(s)
                                ));
                            }
                        }
                        v
                    };
                    self.out.extend(lines);
                }
                "packets" => {
                    let rx = self
                        .rx
                        .get_mut(arg)
                        .unwrap_or_else(|| panic!("no player {arg}"));
                    while let Ok(msg) = rx.try_recv() {
                        let Outbound::Payload(p) = msg else { continue };
                        let kind = self
                            .packet_names
                            .get(&read_varint(&p))
                            .copied()
                            .unwrap_or("?");
                        if PKT_TYPES.contains(&kind) {
                            self.out.push(format!("PKT {arg} {kind} {}", hex(&p)));
                        }
                    }
                }
                other => panic!("unsupported dump {other}"),
            }
        }
    }

    fn describe(&self, name: &str) -> String {
        let e = self.entity(name);
        let b = e.base();
        let is_player = e.is_player();
        let mut s = format!(
            "{} {} {}",
            if is_player { "P" } else { "E" },
            name,
            e.get_type().name()
        );
        let (pos, dm) = (b.position(), b.delta_movement());
        s.push_str(&format!(" pos={},{},{}", d(pos.x), d(pos.y), d(pos.z)));
        s.push_str(&format!(" delta={},{},{}", d(dm.x), d(dm.y), d(dm.z)));
        s.push_str(&format!(" rot={},{}", f(b.y_rot()), f(b.x_rot())));
        s.push_str(&format!(
            " og={} fall={} fire={} air={} alive={} pose={}",
            u8::from(b.on_ground()),
            d(b.fall_distance),
            b.remaining_fire_ticks,
            b.get_air_supply(),
            u8::from(e.is_alive()),
            pose_name(b.get_pose())
        ));
        if let Some(l) = e.living() {
            s.push_str(&format!(
                " hp={} abs={} hurt={}/{} death={} fx=",
                f(l.get_health()),
                f(e.get_absorption_amount()),
                l.hurt_time,
                l.damage_cooldown_time,
                l.death_time
            ));
            let mut fx: Vec<&MobEffectInstance> = l.active_effects.values().collect();
            fx.sort_by_key(|m| m.effect.name());
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
        if let Some(p) = e.player() {
            let sp = e
                .as_any()
                .downcast_ref::<ServerPlayer>()
                .expect("server player");
            s.push_str(&format!(
                " gm={}",
                sp.game_mode.game_mode_for_player.name().to_lowercase()
            ));
            s.push_str(&format!(
                " food={} sat={} exh={}",
                p.food_data.get_food_level(),
                f(p.food_data.get_saturation_level()),
                f(p.food_data.exhaustion_level)
            ));
            s.push_str(&format!(
                " xp={}/{}/{}",
                p.experience_level,
                p.total_experience,
                f(p.experience_progress)
            ));
            s.push_str(&format!(
                " sel={} main={}",
                p.inventory.get_selected_slot(),
                common::entity_scenario::item_full(&e.get_item_by_slot(EquipmentSlot::MainHand))
            ));
        } else if let Some(i) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::item::ItemEntity>()
        {
            s.push_str(&format!(
                " item={} age={}",
                item_string(&i.get_item()),
                i.age
            ));
            s.push_str(&format!(
                " pickup={} health={} owner={} thrower={}",
                i.pickup_delay,
                i.health,
                self.uuid_name(i.target),
                self.uuid_name(i.thrower)
            ));
        } else if let Some(o) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::experience_orb::ExperienceOrb>()
        {
            s.push_str(&format!(
                " value={} count={} age={} health={}",
                o.get_value(),
                o.count,
                o.age,
                o.health
            ));
        }
        s
    }

    fn uuid_name(&self, u: Option<uuid::Uuid>) -> String {
        match u {
            None => "-".into(),
            Some(u) => self
                .names
                .iter()
                .find(|n| {
                    self.level
                        .get_entity(self.refs[*n])
                        .is_some_and(|e| e.uuid() == u)
                })
                .cloned()
                .unwrap_or_else(|| u.to_string()),
        }
    }
}

fn replay(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(scenario_dir().join(format!("{name}.txt"))).unwrap();
    let mut r = Replay::new();
    for line in text.lines() {
        r.run_line(line);
    }
    r.out
}

/// `removeAllEffects` walks a `HashMap` keyed by registry holders, whose order the real server leaves to the identity hash
/// of each run: a run of consecutive `remove_mob_effect` packets compares sorted.
fn with_sorted_effect_removals(mut lines: Vec<String>) -> Vec<String> {
    let removal = |l: &String| l.contains(" minecraft:remove_mob_effect ");
    for run in lines.chunk_by_mut(|a, b| removal(a) && removal(b)) {
        run.sort_unstable();
    }
    lines
}

/// The fixture without the `PKT` lines of types the replay does not produce.
fn check(name: &str) {
    let mut fixture = Fixture::load(name);
    fixture.lines.retain(|l| {
        let mut it = l.split_whitespace();
        it.next() != Some("PKT") || PKT_TYPES.contains(&it.nth(1).unwrap_or(""))
    });
    fixture.lines = with_sorted_effect_removals(fixture.lines);
    let opts = Options {
        ignore_kinds: vec!["#", "C"],
        ..Default::default()
    };
    fixture.assert_matches(&with_sorted_effect_removals(replay(name)), &opts);
}

#[test]
fn invulnerability_frames() {
    check("damage_frames");
}

#[test]
fn armor_materials() {
    check("damage_armor");
}

#[test]
fn resistance_absorption_fire_resistance() {
    check("damage_effects");
}

#[test]
fn damage_sources_by_game_mode_and_rules() {
    check("damage_sources");
}

#[test]
fn enchantment_protections() {
    check("damage_enchant");
}

#[test]
fn shield_blocking() {
    check("damage_shield");
}

#[test]
fn difficulty_scaling() {
    check("damage_difficulty");
}

#[test]
fn zombie_hits_player() {
    check("hurt_basic");
}

#[test]
fn player_death_drops_xp_totem_and_respawn() {
    check("death_basic");
}

#[test]
fn dead_player_experience_orbs() {
    check("ent_death_orbs");
}

#[test]
fn death_messages_for_every_damage_type() {
    check("death_messages");
}

#[test]
fn drowning_and_freezing() {
    check("damage_drown_freeze");
}

#[test]
fn starvation_void_and_fall_by_difficulty() {
    check("damage_starve_void");
}

#[test]
fn totem_of_undying() {
    check("death_totem");
}

/// `/gamerule keepInventory true` then `/kill` in the same tick: levels read the live rule at the point of use (no per-tick
/// snapshot), so the very next death already keeps the inventory (and a flip back drops it).
#[test]
fn keep_inventory_rule_is_live_within_a_tick() {
    use mc_game::world::level::gamerules::GameRules;
    let mut rules = GameRules::new(mc_world::level_storage::FeatureFlagSet::DEFAULT_FLAGS);
    let mut r = Replay::new();
    r.level.server_state.rules = rules.live();
    for (name, keep, expect_kept) in [("A", true, true), ("B", false, false)] {
        r.run_line(&format!("spawn_player {name} survival 0.5 -60 0.5 0 0"));
        rules.set(GameRules::KEEP_INVENTORY, keep, None);
        let mut stack = ItemStack::new(Item::by_name("minecraft:cobblestone").unwrap());
        r.with_player(name, |_, p| {
            p.player.inventory.add(&mut stack, false);
        });
        r.run_line(&format!("hurt {name} generic_kill 1000"));
        let kept = !r
            .entity(name)
            .player()
            .unwrap()
            .inventory
            .get_selected_item()
            .is_empty();
        assert_eq!(kept, expect_kept, "keepInventory={keep}");
    }
}

/// `Player.remove(KILLED)` removes the player before `AbstractContainerMenu.removed`: the stack on the cursor is dropped as an
/// item entity (a living player gets it back in the inventory), also when `keepInventory` carries the inventory over.
#[test]
fn respawn_drops_the_cursor_stack() {
    let mut r = Replay::new();
    r.run_line("gamerule keep_inventory true");
    r.run_line("spawn_player A survival 0.5 -60 0.5 0 0");
    r.with_player("A", |_, p| {
        let stack = ItemStack::new(Item::by_name("minecraft:cobblestone").unwrap());
        p.player.menus.current_mut().base_mut().set_carried(stack);
    });
    r.run_line("hurt A generic_kill 1000");
    r.run_line("respawn A");
    let cobblestone = ItemStack::new(Item::by_name("minecraft:cobblestone").unwrap());
    assert!(
        !r.entity("A")
            .player()
            .unwrap()
            .inventory
            .contains(&cobblestone)
    );
    r.run_line("dump entities");
    assert!(
        r.out
            .iter()
            .any(|l| l.contains("item=minecraft:cobblestone")),
        "no dropped item in {:?}",
        r.out
    );
}
