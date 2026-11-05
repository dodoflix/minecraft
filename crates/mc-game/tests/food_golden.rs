//! `FoodData.tick` + `ServerPlayer.tickRegeneration` against the real vanilla server (scenarios `tests/oracle/scenarios/food_*.txt`,
//! fixtures `tests/fixtures/entity_oracle/food_*.txt`, regenerate with `scripts/entity-oracle.sh --regen`): health, food level,
//! saturation and exhaustion of a resting survival player per tick, for the natural regeneration branches, the exhaustion
//! thresholds, peaceful regeneration, the `natural_health_regeneration` rule and the hunger / regeneration effects.
//! Starvation damage needs `hurt_server` (EcDamage) and is not part of these scenarios.
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_data::{Block, MobEffect};
use mc_game::server::level::ServerLevel;
use mc_game::world::level::LevelAccessor;
use mc_game::server::level::level_data::{PrimaryServerLevelData, default_world_data};
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_protocol::codec::GameProfile;
use mc_world::level_storage::Difficulty;

fn replay(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(scenario_dir().join(format!("{name}.txt"))).unwrap();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    let world = default_world_data();
    level.level_data = Box::new(PrimaryServerLevelData(world.clone()));
    let flat = |level: &mut ServerLevel| {
        for x in -4..=4 {
            for z in -4..=4 {
                for (y, block) in [(-64, Block::BEDROCK), (-63, Block::DIRT), (-62, Block::DIRT), (-61, Block::GRASS_BLOCK)] {
                    level.set_block(mc_core::pos::BlockPos::new(x, y, z), block.default_state(), 3, 512);
                }
            }
        }
    };
    let mut player: Option<ServerPlayer> = None;
    let (mut out, mut tick) = (Vec::new(), 0);
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let t: Vec<&str> = line.split(' ').collect();
        let num = |s: &str| s.parse::<f64>().unwrap();
        match t[0] {
            "level" => flat(&mut level),
            "difficulty" => {
                world.lock().settings.difficulty_settings.difficulty = match t[1] {
                    "peaceful" => Difficulty::Peaceful,
                    "easy" => Difficulty::Easy,
                    "hard" => Difficulty::Hard,
                    _ => Difficulty::Normal,
                }
            }
            "gamerule" => {
                assert_eq!(t[1], "natural_health_regeneration");
                level.tick_state.rules.natural_health_regeneration = t[2] == "true";
            }
            "spawn_player" => {
                let profile = GameProfile { id: uuid::Uuid::from_u128(1), name: t[1].into(), properties: vec![] };
                let mut p = ServerPlayer::new(1, Identifier::parse("minecraft:overworld").unwrap(), profile, Default::default());
                p.snap_to(num(t[3]), num(t[4]), num(t[5]), 0.0, 0.0);
                p.base_mut().first_tick = true;
                player = Some(p);
            }
            "watch" => {}
            "set" => {
                let p = player.as_mut().unwrap();
                match t[2] {
                    "health" => p.player.living.set_health(num(t[3]) as f32),
                    "food" => p.player.food_data.food_level = num(t[3]) as i32,
                    "saturation" => p.player.food_data.saturation_level = num(t[3]) as f32,
                    "exhaustion" => p.player.food_data.exhaustion_level = num(t[3]) as f32,
                    other => panic!("set {other}"),
                }
            }
            "effect" => {
                let effect = MobEffect::by_name(t[2]).unwrap();
                let instance = MobEffectInstance::new(effect, num(t[4]) as i32, num(t[3]) as i32, false, true, true, None);
                player.as_mut().unwrap().add_effect(&mut level, instance, None);
            }
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| num(n) as usize) {
                    let p = player.as_mut().unwrap();
                    mc_game::world::entity::entity::common_tick(p);
                    p.tick(&mut level);
                    let pos = p.base().position();
                    p.base_mut().set_old_pos(pos);
                    p.do_tick(&mut level);
                    tick += 1;
                    out.push(format!("T {tick}"));
                    let f = &p.player.food_data;
                    out.push(format!("P Steve minecraft:player hp={} food={} sat={} exh={}", p.player.living.get_health(), f.food_level, f.saturation_level, f.exhaustion_level));
                }
            }
            other => panic!("unsupported scenario command `{other}`"),
        }
    }
    out
}

fn check(name: &str) {
    let opts = Options {
        ignore_kinds: vec!["I", "PKT", "C", "#", "B", "EFFECT", "DIG", "HURT"],
        ignore_fields: vec!["pos", "delta", "rot", "og", "fall", "fire", "air", "alive", "pose", "abs", "hurt", "death", "fx", "gm", "xp", "sel", "main"],
        ..Default::default()
    };
    Fixture::load(name).assert_matches(&replay(name), &opts);
}

#[test]
fn fast_regeneration() {
    check("food_fast_regen");
}

#[test]
fn slow_regeneration() {
    check("food_slow_regen");
}

#[test]
fn exhaustion_thresholds() {
    check("food_exhaust");
}

#[test]
fn peaceful_regeneration() {
    check("food_peaceful");
}

#[test]
fn natural_regeneration_rule() {
    check("food_norule");
}

#[test]
fn hunger_effect() {
    check("food_hunger_effect");
}

#[test]
fn regeneration_effect() {
    check("food_regen_effect");
}
