//! Damage pipeline golden: the scenarios `tests/oracle/scenarios/{damage_*,death_*}.txt` run on the real vanilla server
//! (`scripts/entity-oracle.sh`, fixtures in `tests/fixtures/entity_oracle/`) are replayed on our `ServerPlayer` over a
//! `ServerLevel` with the real vanilla registries (damage types + tags, enchantments, loot tables) and compared line by line:
//! `P` lines (health, absorption, hurt time / damage cooldown, death time, food, saturation, exhaustion, alive) and `HURT`
//! results.
//!
//! Replay subset: `level`, `difficulty`, `gamerule`, `spawn_player`, `give` (plain item ids), `effect`, `set health|absorption`,
//! `hurt` (no `by=`/`direct=`/`at=`), `tick`, `dump player:`. Scenarios needing mobs, item components, the connection packets
//! (`PKT` lines), inventories or respawn are not replayed yet (see STATUS: EcDamage known gaps).
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::enums::GameType;
use mc_data::{Item, MobEffect};
use mc_game::server::level::ServerLevel;
use mc_game::world::damagesource::DamageSource;
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::level::LevelAccessor;
use mc_protocol::codec::GameProfile;
use mc_world::level_storage::Difficulty;

static ACCESS: std::sync::LazyLock<mc_game::registry::RegistryAccess> = std::sync::LazyLock::new(|| {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(dir.path(), mc_world::level_storage::WorldDataConfiguration::DEFAULT, true).expect("default world loads");
    stem.registries.composite_access().clone()
});

fn registries() -> mc_game::registry::RegistryAccess {
    ACCESS.clone()
}

struct Replay {
    level: ServerLevel,
    players: Vec<(String, ServerPlayer)>,
    out: Vec<String>,
    next_id: i32,
    world: mc_game::server::level::level_data::SharedWorldData,
}

fn f(v: f32) -> String {
    mc_core::math::java::java_float_to_string(v)
}

impl Replay {
    fn new() -> Self {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = registries();
        let world = mc_game::server::level::level_data::default_world_data();
        level.level_data = Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(world.clone()));
        for x in -3..=60 {
            for z in -4..=4 {
                level.set_block_and_update(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state());
            }
        }
        Replay { level, players: Vec::new(), out: Vec::new(), next_id: 1, world }
    }

    fn player(&mut self, name: &str) -> &mut ServerPlayer {
        &mut self.players.iter_mut().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no player {name}")).1
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        match t[0] {
            "level" => {}
            "difficulty" => {
                let d = match t[1] {
                    "peaceful" => Difficulty::Peaceful,
                    "easy" => Difficulty::Easy,
                    "hard" => Difficulty::Hard,
                    _ => Difficulty::Normal,
                };
                self.world.lock().settings.difficulty_settings.difficulty = d;
            }
            "gamerule" => {
                let v = t[2] == "true";
                self.level.server_state.rules.update(|r| match t[1] {
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
                let n = |i: usize| t[i].parse::<f64>().unwrap();
                p.snap_to(n(3), n(4), n(5), 0.0, 0.0);
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
                let (amp, dur) = (t[3].parse().unwrap(), t[4].parse().unwrap());
                let (level, players) = (&mut self.level, &mut self.players);
                let p = &mut players.iter_mut().find(|(n, _)| n == t[1]).unwrap().1;
                p.add_effect(level, MobEffectInstance::new(effect, dur, amp, false, true, true, None), None);
            }
            "set" => {
                let v: f32 = t[3].parse().unwrap();
                let p = self.player(t[1]);
                match t[2] {
                    "health" => p.player.living.set_health(v),
                    "absorption" => p.player.internal_set_absorption_amount(v),
                    other => panic!("set {other}"),
                }
            }
            "hurt" => {
                let (level, players) = (&mut self.level, &mut self.players);
                let p = &mut players.iter_mut().find(|(n, _)| n == t[1]).unwrap().1;
                let key = Identifier::parse(&format!("minecraft:{}", t[2])).unwrap();
                let data = level.registries.get::<mc_game::registry::typed::damage_type::DamageType>("minecraft:damage_type", &key).expect("damage type").clone();
                let source = DamageSource::new(key, std::sync::Arc::new(data), None, None, None);
                let r = p.hurt_server(level, &source, t[3].parse::<f64>().unwrap() as f32);
                self.out.push(format!("HURT {} {} {} -> {}", t[1], t[2], t[3], r));
            }
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| n.parse().unwrap()) {
                    let level = &mut self.level;
                    for (_, p) in &mut self.players {
                        mc_game::world::entity::entity::common_tick(p);
                        p.tick(level);
                        p.do_tick(level);
                    }
                }
            }
            "dump" => {
                for w in &t[1..] {
                    if let Some(name) = w.strip_prefix("player:") {
                        let p = self.player(name);
                        let l = &p.player.living;
                        let line = format!(
                            "P {name} minecraft:player alive={} hp={} abs={} hurt={}/{} death={} food={} sat={} exh={}",
                            u8::from(p.is_alive()),
                            f(l.get_health()),
                            f(p.player.get_absorption_amount()),
                            l.hurt_time,
                            l.damage_cooldown_time,
                            l.death_time,
                            p.player.food_data.get_food_level(),
                            f(p.player.food_data.get_saturation_level()),
                            f(p.player.food_data.exhaustion_level),
                        );
                        self.out.push(line);
                    }
                }
            }
            other => panic!("unsupported scenario command `{other}`"),
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

/// Keep only the lines (and fields) the replay models.
fn check(name: &str) {
    let fixture = Fixture::load(name);
    let keep = |l: &&str| l.starts_with("P ") || l.starts_with("HURT ");
    let _ = keep;
    let opts = Options {
        ignore_kinds: vec!["I", "PKT", "C", "#", "B", "EFFECT", "DIG", "T", "E"],
        ignore_fields: vec!["pos", "delta", "rot", "og", "fall", "fire", "air", "pose", "gm", "fx", "xp", "sel", "main"],
        ..Default::default()
    };
    fixture.assert_matches(&replay(name), &opts);
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
        r.player(name).player.inventory.add(&mut stack, false);
        r.run_line(&format!("hurt {name} generic_kill 1000"));
        let kept = !r.player(name).player.inventory.get_selected_item().is_empty();
        assert_eq!(kept, expect_kept, "keepInventory={keep}");
    }
}
