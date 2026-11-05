//! `LivingEntity` movement/tick golden: the scenario scripts of `tests/oracle/scenarios/ll_*.txt` are replayed on the real vanilla
//! server (`scripts/entity-oracle.sh`, fixtures in `tests/fixtures/entity_oracle/ll_*.txt`, `sim` players = a locally controlled
//! player whose input drives `xxa/zza/jumping/sprinting/sneaking`) and, line by line, on our `ServerPlayer` through the generic
//! `LivingEntity` code. Positions, deltas, rotation, ground flag, fall distance, air supply and pose are compared bit-exactly per
//! tick. Regenerate the fixtures with `scripts/entity-oracle.sh --regen`.
//!
//! The replay understands the subset of the driver DSL these scenarios use (`level flat`, `fill`, `block`, `spawn_player`, `give`,
//! `effect`, `attribute`, `set`, `nbt {FallFlying}`, `entity_seed`, `velocity`, `input`, `tick`, `watch`, `cmd effect clear`).
//! Effects are added to the active-effect map together with the attribute modifiers the vanilla `MobEffect` registers
//! (effect ticking/expiry belongs to the effects slice; durations here never run out).
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::LegacyRandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{GameType, Pose};
use mc_data::{Attribute, Item, MobEffect};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::level::LevelAccessor;
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::play::Input;

struct Replay {
    level: ServerLevel,
    players: Vec<(String, ServerPlayer)>,
    watched: Vec<String>,
    out: Vec<String>,
    tick: u64,
    next_id: i32,
}

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    block.state_from_properties(props.split(',').filter(|p| !p.is_empty()).map(|p| p.split_once('=').expect("prop=value"))).unwrap_or_else(|| panic!("bad state {text}"))
}

fn effect_modifiers(effect: MobEffect, amplifier: i32) -> Vec<(Attribute, AttributeModifier)> {
    let (attribute, path, base) = match effect {
        e if e == MobEffect::SPEED => (Attribute::MOVEMENT_SPEED, "effect.speed", 0.2f32),
        e if e == MobEffect::SLOWNESS => (Attribute::MOVEMENT_SPEED, "effect.slowness", -0.15f32),
        e if e == MobEffect::JUMP_BOOST => (Attribute::SAFE_FALL_DISTANCE, "effect.jump_boost", 1.0f32),
        _ => return Vec::new(),
    };
    let operation = if attribute == Attribute::SAFE_FALL_DISTANCE { AttributeModifierOperation::AddValue } else { AttributeModifierOperation::AddMultipliedTotal };
    vec![(attribute, AttributeModifier { id: Identifier::new("minecraft", path).unwrap(), amount: f64::from(base) * f64::from(amplifier + 1), operation })]
}

fn pose_name(pose: Pose) -> String {
    format!("{pose:?}").chars().fold(String::new(), |mut s, c| {
        if c.is_uppercase() && !s.is_empty() {
            s.push('_');
        }
        s.push(c.to_ascii_lowercase());
        s
    })
}

impl Replay {
    fn new() -> Self {
        let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = common::load_world(dir.path(), mc_world::level_storage::WorldDataConfiguration::DEFAULT, true).expect("default world loads");
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = stem.registries.composite_access().clone();
        Replay { level, players: Vec::new(), watched: Vec::new(), out: Vec::new(), tick: 0, next_id: 1 }
    }

    fn set(&mut self, x: i32, y: i32, z: i32, state: BlockState) {
        self.level.set_block(BlockPos::new(x, y, z), state, mc_game::world::level::flags::UPDATE_CLIENTS | mc_game::world::level::flags::UPDATE_KNOWN_SHAPE, 512);
    }

    fn fill(&mut self, a: [i32; 3], b: [i32; 3], state: BlockState) {
        for x in a[0].min(b[0])..=a[0].max(b[0]) {
            for y in a[1].min(b[1])..=a[1].max(b[1]) {
                for z in a[2].min(b[2])..=a[2].max(b[2]) {
                    self.set(x, y, z, state);
                }
            }
        }
    }

    fn flat_world(&mut self) {
        let (bedrock, dirt, grass) = (Block::BEDROCK.default_state(), Block::DIRT.default_state(), Block::GRASS_BLOCK.default_state());
        self.fill([-40, -64, -40], [100, -64, 100], bedrock);
        self.fill([-40, -63, -40], [100, -62, 100], dirt);
        self.fill([-40, -61, -40], [100, -61, 100], grass);
    }

    fn player(&mut self, name: &str) -> &mut ServerPlayer {
        &mut self.players.iter_mut().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no player {name}")).1
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let t: Vec<&str> = line.splitn(if line.starts_with("nbt ") { 3 } else { 64 }, ' ').collect();
        let num = |s: &str| s.parse::<f64>().unwrap_or_else(|_| panic!("number {s} in `{line}`"));
        match t[0] {
            "level" => self.flat_world(),
            "fill" => self.fill([num(t[1]) as i32, num(t[2]) as i32, num(t[3]) as i32], [num(t[4]) as i32, num(t[5]) as i32, num(t[6]) as i32], parse_state(t[7])),
            "block" => self.set(num(t[1]) as i32, num(t[2]) as i32, num(t[3]) as i32, parse_state(t[4])),
            "spawn_player" => {
                let id = self.next_id;
                self.next_id += 1;
                let profile = GameProfile { id: uuid::Uuid::from_u128(id as u128), name: t[1].into(), properties: vec![] };
                let mut p = ServerPlayer::new(id, Identifier::parse("minecraft:overworld").unwrap(), profile, Default::default());
                let mode = match t[2] {
                    "creative" => GameType::Creative,
                    "spectator" => GameType::Spectator,
                    "adventure" => GameType::Adventure,
                    _ => GameType::Survival,
                };
                p.set_game_mode_for_player(mode, None);
                let (yaw, pitch) = if t.len() > 6 && t[6] != "sim" { (num(t[6]) as f32, num(t[7]) as f32) } else { (0.0, 0.0) };
                p.snap_to(num(t[3]), num(t[4]), num(t[5]), yaw, pitch);
                p.base_mut().first_tick = true;
                p.simulate_local_player = t.last() == Some(&"sim");
                self.players.push((t[1].to_string(), p));
            }
            "give" => {
                let slot = match t[2] {
                    "chest" => EquipmentSlot::Chest,
                    "head" => EquipmentSlot::Head,
                    "legs" => EquipmentSlot::Legs,
                    "feet" => EquipmentSlot::Feet,
                    other => panic!("slot {other}"),
                };
                let item = Item::by_name(t[3]).unwrap_or_else(|| panic!("item {}", t[3]));
                self.player(t[1]).player.inventory.equipment_set(slot, ItemStack::new(item));
            }
            "effect" => {
                let effect = MobEffect::by_name(t[2]).unwrap_or_else(|| panic!("effect {}", t[2]));
                let (amplifier, duration) = (num(t[3]) as i32, num(t[4]) as i32);
                let p = self.player(t[1]);
                p.player.living.active_effects.insert(effect, MobEffectInstance::new(effect, duration, amplifier, false, true, true, None));
                for (attribute, modifier) in effect_modifiers(effect, amplifier) {
                    p.player.living.attributes.get_instance(attribute).unwrap().add_transient_modifier(modifier);
                }
            }
            "attribute" => {
                let attribute = Attribute::by_name(t[2]).unwrap_or_else(|| panic!("attribute {}", t[2]));
                let value = num(t[3]);
                self.player(t[1]).player.living.attributes.get_instance(attribute).unwrap().set_base_value(value);
            }
            "set" => {
                let value = num(t[3]);
                let p = self.player(t[1]);
                match t[2] {
                    "air" => p.base_mut().set_air_supply(value as i32),
                    "fall_distance" => p.base_mut().fall_distance = value,
                    other => panic!("set {other}"),
                }
            }
            "nbt" => {
                assert_eq!(t[2], "{FallFlying:1b}", "only the glide flag is replayed");
                self.player(t[1]).base_mut().set_shared_flag(mc_game::world::entity::entity::FLAG_FALL_FLYING, true);
            }
            "entity_seed" => self.player(t[1]).base_mut().random = LegacyRandomSource::new(num(t[2]) as i64),
            "velocity" => self.player(t[1]).base_mut().set_delta_movement_xyz(num(t[2]), num(t[3]), num(t[4])),
            "input" => {
                let flags: Vec<&str> = t[2].split(',').collect();
                let has = |f: &str| flags.contains(&f);
                let p = self.player(t[1]);
                p.last_client_input = Input { forward: has("forward"), backward: has("back"), left: has("left"), right: has("right"), jump: has("jump"), shift: has("sneak"), sprint: has("sprint") };
                // `handlePlayerInput`
                let shift = p.last_client_input.shift;
                p.base_mut().set_shared_flag(mc_game::world::entity::entity::FLAG_SHIFT_KEY_DOWN, shift);
            }
            "cmd" => {
                // `effect clear PLAYER EFFECT`
                assert_eq!(&t[1..3], ["effect", "clear"], "only `effect clear` is replayed");
                let effect = MobEffect::by_name(t[4]).unwrap();
                let p = self.player(t[3]);
                if let Some(instance) = p.player.living.active_effects.shift_remove(&effect) {
                    for (attribute, modifier) in effect_modifiers(effect, instance.amplifier) {
                        p.player.living.attributes.get_instance(attribute).unwrap().remove_modifier(&modifier.id);
                    }
                }
            }
            "watch" => self.watched = t[1..].iter().map(|w| w.strip_prefix("player:").expect("player watch").to_string()).collect(),
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| num(n) as usize) {
                    self.tick_once();
                }
            }
            other => panic!("unsupported scenario command `{other}`"),
        }
    }

    /// One server tick: `SimPlayer.applyInput` (before the tick), the level's entity tick (`ServerPlayer.tick`), then the
    /// connection tick (`tickPlayer` -> `doTick`, whose position snap-back a simulated player skips).
    fn tick_once(&mut self) {
        let level = &mut self.level;
        for (_, p) in &mut self.players {
            if p.simulate_local_player {
                p.apply_input();
            }
            mc_game::world::entity::entity::common_tick(p);
            p.tick(level);
            let pos = p.base().position();
            p.base_mut().set_old_pos(pos);
            p.do_tick(level);
        }
        self.tick += 1;
        self.out.push(format!("T {}", self.tick));
        for name in self.watched.clone() {
            let p = self.player(&name);
            let b = p.base();
            let (pos, dm) = (b.position(), b.delta_movement());
            let line = format!(
                "P {name} minecraft:player pos={},{},{} delta={},{},{} rot={},{} og={} fall={} air={} pose={}",
                pos.x, pos.y, pos.z, dm.x, dm.y, dm.z, b.y_rot(), b.x_rot(), u8::from(b.on_ground()), b.fall_distance, b.get_air_supply(), pose_name(b.get_pose())
            );
            self.out.push(line);
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

fn check(name: &str) {
    let fixture = Fixture::load(name);
    let opts = Options {
        ignore_kinds: vec!["I", "PKT", "C", "#", "B", "EFFECT", "DIG", "HURT"],
        ignore_fields: vec!["fire", "alive", "hp", "abs", "hurt", "death", "fx", "gm", "food", "sat", "exh", "xp", "sel", "main"],
        ..Default::default()
    };
    fixture.assert_matches(&replay(name), &opts);
}

#[test]
fn ground_walk_sprint_jump_sneak() {
    check("ll_ground");
}

#[test]
fn ground_turned_strafe() {
    check("ll_ground_turned");
}

#[test]
fn ice_blue_ice_slime_friction() {
    check("ll_ice");
}

#[test]
fn soul_sand_honey_movement_efficiency() {
    check("ll_sticky");
}

#[test]
fn water_swimming_efficiency_dolphins_grace() {
    check("ll_water");
}

#[test]
fn lava() {
    check("ll_lava");
}

#[test]
fn ladder_vine_scaffolding() {
    check("ll_climb");
}

#[test]
fn speed_slowness_jump_boost_levitation_slow_falling() {
    check("ll_effects");
}

#[test]
fn elytra_glide() {
    check("ll_elytra");
}

#[test]
fn air_supply_under_water() {
    check("ll_air");
}

#[test]
fn cobweb_and_bubble_column() {
    check("ll_cobweb");
}
