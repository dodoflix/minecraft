#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]
//! Dual-server survival scenarios: the SAME bot script against the real vanilla server (observations committed as
//! `tests/fixtures/entity_dual/*.txt`, regenerate with `ENTITY_DUAL_UPDATE=1 cargo test -p mc-server --test oracle
//! entity_dual_vanilla -- --ignored`) and OUR server (`entity_dual_ours` diffs its observations against those fixtures and writes
//! `run/entity-dual-ours.diff`; `ENTITY_DUAL_STRICT=1` makes any difference a test failure).
//!
//! Setup/assertion channel is the server console (`give`, `tp`, `damage`, `summon`, `setblock`, `effect`, `kill`), observation channel
//! is only what a vanilla client sees (the bot's state: health/food/air, block updates, tracked entities, events). The world
//! is the flat generator, difficulty normal, no natural spawning.
//!
//! Observation lines are `scenario key=value ...`; numeric values that depend on timing are written `~VALUE±TOL` (absolute) or
//! `~VALUE±PCT%` and match any number within the tolerance of the FIXTURE side (see [`compare`]).
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::EntityType;
use mc_protocol::packets::play::{EntityDataValue, InteractionHand};
use mc_testbot::Bot;
use mc_testbot::state::Observed;

use super::Console;

pub const PROPS: &[&str] = &[
    "level-type=minecraft:flat",
    "difficulty=normal",
    "spawn-monsters=false",
    "spawn-animals=false",
    "spawn-npcs=false",
    "allow-nether=false",
    "gamemode=survival",
    "force-gamemode=true",
    "generate-structures=false",
];

/// Console setup run once after the server started: frozen quiet world (game rules of 26.x; unknown rules are harmless).
pub const SETUP: &[&str] = &[
    "gamerule advance_time false",
    "gamerule advance_weather false",
    "gamerule spawn_mobs false",
    "gamerule spawn_monsters false",
    "gamerule spawn_patrols false",
    "gamerule spawn_phantoms false",
    "gamerule spawn_wandering_traders false",
    "gamerule spawn_wardens false",
    "gamerule raids false",
    "gamerule mob_drops false",
    "gamerule natural_health_regeneration false", // scenarios needing regeneration switch it on explicitly
    "gamerule send_command_feedback false",
    "time set 6000",
    "weather clear",
    "kill @e[type=!minecraft:player]",
];

pub async fn setup(bot: &Bot, srv: &(dyn Console + Sync)) {
    for c in SETUP {
        srv.cmd(c);
        bot.wait_ticks(2).await.expect("setup");
    }
}

/// Flat world: grass top at y=-61, standing y=-60.
pub const STAND_Y: f64 = -60.0;
const NAME: &str = "Bot";

pub struct Ctx {
    pub bot: Arc<Bot>,
    pub srv: Arc<dyn Console + Send + Sync>,
}

fn slot36(s: &mc_testbot::state::State) -> mc_protocol::components::ItemStack {
    s.inventory
        .slots
        .get(36)
        .cloned()
        .unwrap_or_else(mc_protocol::components::ItemStack::empty)
}

/// Observation builder.
pub struct Obs {
    name: &'static str,
    parts: Vec<String>,
}

impl Obs {
    pub fn new(name: &'static str) -> Self {
        Obs {
            name,
            parts: Vec::new(),
        }
    }
    pub fn exact(mut self, key: &str, v: impl std::fmt::Display) -> Self {
        self.parts.push(format!("{key}={v}"));
        self
    }
    /// Timing dependent number, compared with an absolute tolerance.
    pub fn abs(mut self, key: &str, v: f64, tol: f64) -> Self {
        self.parts.push(format!("{key}=~{v}±{tol}"));
        self
    }
    /// Number compared with a relative tolerance in percent.
    pub fn pct(mut self, key: &str, v: f64, pct: f64) -> Self {
        self.parts.push(format!("{key}=~{v}±{pct}%"));
        self
    }
    pub fn line(self) -> String {
        format!("{} {}", self.name, self.parts.join(" "))
    }
}

impl Ctx {
    pub async fn ticks(&self, n: u64) {
        self.bot
            .wait_ticks(n)
            .await
            .unwrap_or_else(|e| panic!("wait_ticks: {e}\n{}", self.bot.log_tail(30)));
    }

    /// Console command followed by a short pause so the effect reaches the client.
    pub async fn cmd(&self, c: &str) {
        self.srv.cmd(c);
        self.ticks(3).await;
    }

    /// Fresh survival player standing on the surface at the origin, full health/food, empty inventory, no effects, empty world.
    pub async fn reset(&self) {
        if self.bot.with_state(|s| s.dead) {
            self.bot.respawn();
            self.bot
                .wait_for(Duration::from_secs(20), |s| {
                    (!s.dead && s.health > 0.0).then_some(())
                })
                .await
                .unwrap_or_else(|e| panic!("respawn: {e}\n{}", self.bot.log_tail(30)));
        }
        for c in [
            "gamemode survival Bot",
            "effect clear Bot",
            "kill @e[type=!minecraft:player]", // before `clear`: lying drops would be picked up in between
            "clear Bot",
            "fill -8 -60 -8 8 -40 8 minecraft:air",
            "fill -8 -61 -8 8 -61 8 minecraft:grass_block",
            "tp Bot 0.5 -60 0.5 0 0",
            "effect give Bot minecraft:instant_health 1 10 true",
            "effect give Bot minecraft:saturation 1 30 true",
        ] {
            self.srv.cmd(c);
            self.ticks(2).await;
            if c == "clear Bot" {
                let _ = self
                    .bot
                    .wait_for(Duration::from_secs(5), |s| {
                        s.inventory
                            .slots
                            .iter()
                            .all(|st| st.is_empty())
                            .then_some(())
                    })
                    .await;
            }
        }
        self.settle().await;
    }

    /// On the ground, full health, one second without changes.
    pub async fn settle(&self) {
        self.bot
            .wait_for(Duration::from_secs(20), |s| {
                (s.body.on_ground && s.health >= 20.0 && s.food >= 20 && !s.dead).then_some(())
            })
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "settle: {e}; health={} food={}\n{}",
                    self.bot.health(),
                    self.bot.food(),
                    self.bot.log_tail(30)
                )
            });
        self.ticks(20).await;
    }

    pub async fn give_held(&self, stack: &str) {
        self.srv.cmd(&format!("give {NAME} {stack}"));
        let id = stack.split(['[', ' ']).next().unwrap_or(stack).to_string();
        self.bot
            .wait_for(Duration::from_secs(10), |s| {
                format!("{:?}", slot36(s).item())
                    .contains(&id)
                    .then_some(())
            })
            .await
            .unwrap_or_else(|e| panic!("give {stack}: {e}\n{}", self.bot.log_tail(30)));
        self.bot.select_hotbar(0);
        self.ticks(2).await;
    }

    pub fn item_entities(&self) -> Vec<String> {
        self.bot.with_state(|s| {
            let mut v: Vec<String> = s
                .entities
                .values()
                .filter(|e| e.entity_type == EntityType::ITEM)
                .filter_map(|e| match e.data.get(&8) {
                    Some(EntityDataValue::ItemStack(i)) => {
                        Some(format!("{:?}x{}", i.item(), i.count()))
                    }
                    _ => None,
                })
                .collect();
            v.sort();
            v
        })
    }
}

fn block_at(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

// ---------------------------------------------------------------------------------------------------------------------------

/// Survival digging time (client ticks until the server confirms the block is gone) per tool/block, plus effects.
pub async fn dig_timing(c: &Ctx) -> Vec<String> {
    let cases: &[(&str, Option<&str>, &str, &[&str])] = &[
        ("hand", None, "minecraft:dirt", &[]),
        ("hand", None, "minecraft:stone", &[]),
        ("hand", None, "minecraft:glass", &[]),
        (
            "wooden_pickaxe",
            Some("minecraft:wooden_pickaxe"),
            "minecraft:stone",
            &[],
        ),
        (
            "iron_pickaxe",
            Some("minecraft:iron_pickaxe"),
            "minecraft:stone",
            &[],
        ),
        (
            "diamond_pickaxe",
            Some("minecraft:diamond_pickaxe"),
            "minecraft:obsidian",
            &[],
        ),
        (
            "iron_shovel",
            Some("minecraft:iron_shovel"),
            "minecraft:sand",
            &[],
        ),
        (
            "iron_axe",
            Some("minecraft:iron_axe"),
            "minecraft:oak_log",
            &[],
        ),
        (
            "shears",
            Some("minecraft:shears"),
            "minecraft:white_wool",
            &[],
        ),
        (
            "iron_pickaxe+eff3",
            Some("minecraft:iron_pickaxe[enchantments={\"minecraft:efficiency\":3}]"),
            "minecraft:stone",
            &[],
        ),
        (
            "iron_pickaxe+haste2",
            Some("minecraft:iron_pickaxe"),
            "minecraft:stone",
            &["effect give Bot minecraft:haste 60 1 true"],
        ),
        (
            "iron_pickaxe+fatigue1",
            Some("minecraft:iron_pickaxe"),
            "minecraft:stone",
            &["effect give Bot minecraft:mining_fatigue 60 0 true"],
        ),
        (
            "iron_pickaxe,wrong_tool_block",
            Some("minecraft:iron_pickaxe"),
            "minecraft:dirt",
            &[],
        ),
    ];
    let mut out = Vec::new();
    for (label, tool, block, pre) in cases {
        c.reset().await;
        if let Some(t) = tool {
            c.give_held(t).await;
        }
        for p in *pre {
            c.cmd(p).await;
        }
        c.cmd(&format!("setblock 2 -60 0 {block}")).await;
        let pos = block_at(2, -60, 0);
        let start = c.bot.tick();
        let r = c
            .bot
            .dig_block(pos, Direction::West, Duration::from_secs(40))
            .await;
        let ticks = c.bot.tick() - start;
        c.ticks(4).await;
        let drops = c.item_entities().join(",");
        out.push(match r {
            Ok(()) => Obs::new("dig")
                .exact("case", format!("{label}/{block}"))
                .abs("ticks", ticks as f64, 2.0)
                .exact("drops", if drops.is_empty() { "-".into() } else { drops })
                .line(),
            Err(e) => Obs::new("dig")
                .exact("case", format!("{label}/{block}"))
                .exact("error", format!("{e}").replace(' ', "_"))
                .line(),
        });
    }
    out
}

/// Fall damage per height (health lost when landing on grass) and landing in water.
pub async fn fall_damage(c: &Ctx) -> Vec<String> {
    let mut out = Vec::new();
    for h in [3.0f64, 4.0, 5.0, 8.0, 12.0, 20.0] {
        c.reset().await;
        let before = c.bot.health();
        c.srv.cmd(&format!("tp Bot 0.5 {} 0.5", STAND_Y + h));
        c.ticks(3).await;
        let r = c
            .bot
            .wait_for(Duration::from_secs(20), |s| {
                (s.body.on_ground && s.body.pos.y <= STAND_Y + 0.01).then_some(())
            })
            .await;
        c.ticks(8).await;
        let lost = before - c.bot.health();
        out.push(match r {
            Ok(()) => Obs::new("fall")
                .exact("height", h)
                .exact("damage", lost)
                .line(),
            Err(e) => Obs::new("fall")
                .exact("height", h)
                .exact("error", format!("{e}").replace(' ', "_"))
                .line(),
        });
    }
    // water softens the fall completely
    c.reset().await;
    c.cmd("fill 0 -61 0 1 -55 1 minecraft:water").await;
    let before = c.bot.health();
    c.srv.cmd("tp Bot 0.9 -45 0.9");
    c.ticks(60).await;
    out.push(
        Obs::new("fall")
            .exact("height", "water_from_15")
            .exact("damage", before - c.bot.health())
            .line(),
    );
    out
}

/// Air supply in water, drowning damage rhythm (damage events of the bot, health deltas).
pub async fn drowning(c: &Ctx) -> Vec<String> {
    c.reset().await;
    c.cmd("fill -3 -61 -3 3 -50 3 minecraft:water").await;
    let mark = c.bot.event_mark();
    c.srv.cmd("tp Bot 0.5 -56 0.5");
    let _ = c
        .bot
        .wait_for(Duration::from_secs(10), |s| s.body.in_water.then_some(()))
        .await;
    c.ticks(20).await;
    let air20 = c.bot.air();
    let own = c.bot.with_state(|s| s.entity_id);
    let h0 = c.bot.health();
    let t0 = c.bot.tick();
    let own_damage = |e: &Observed| match e {
        Observed::Damage {
            tick,
            entity_id,
            source_type,
            ..
        } if *entity_id == own => Some((*tick, *source_type)),
        _ => None,
    };
    let first = c
        .bot
        .wait_event(mark, Duration::from_secs(40), own_damage)
        .await;
    let health_after_first = c.bot.health();
    let air_at_damage = c.bot.air();
    c.ticks(70).await;
    let ticks: Vec<u64> = c
        .bot
        .events_since(mark)
        .iter()
        .filter_map(own_damage)
        .map(|d| d.0)
        .collect();
    let gaps: Vec<String> = ticks
        .windows(2)
        .map(|w| (w[1] - w[0]).to_string())
        .collect();
    c.cmd("fill -3 -61 -3 3 -50 3 minecraft:air").await;
    vec![
        Obs::new("drown")
            .abs("air_after_20_ticks", air20.unwrap_or(-9999) as f64, 25.0)
            .abs(
                "ticks_until_first_damage",
                first.as_ref().map_or(-1.0, |f| (f.0 - t0) as f64),
                25.0,
            )
            .exact(
                "damage_events_in_70_ticks_after",
                ticks.len().saturating_sub(1),
            )
            .exact(
                "gap_between_damage_events",
                gaps.first().cloned().unwrap_or_default(),
            )
            .exact("first_damage", h0 - health_after_first)
            .abs(
                "air_at_first_damage",
                air_at_damage.map_or(-9999, |a| a) as f64,
                12.0,
            )
            .line(),
    ]
}

/// Natural regeneration at full food: heal pace and the food/saturation it costs.
pub async fn regen(c: &Ctx) -> Vec<String> {
    c.reset().await;
    c.cmd("gamerule natural_health_regeneration true").await;
    c.cmd("damage Bot 10 minecraft:generic").await;
    let (h0, food0, sat0) = c.bot.with_state(|s| (s.health, s.food, s.saturation));
    let mut last = h0;
    let t0 = c.bot.tick();
    let mut heals: Vec<u64> = Vec::new();
    for _ in 0..240 {
        c.ticks(1).await;
        let h = c.bot.health();
        if h > last {
            heals.push(c.bot.tick() - t0);
        }
        last = h;
    }
    let (h1, food1, sat1) = c.bot.with_state(|s| (s.health, s.food, s.saturation));
    c.cmd("gamerule natural_health_regeneration false").await;
    let gap = |i: usize| {
        heals
            .get(i + 1)
            .zip(heals.get(i))
            .map_or(-1.0, |(b, a)| (*b - *a) as f64)
    };
    vec![
        Obs::new("regen")
            .exact("health_after_damage", h0)
            .abs(
                "first_heal_after_ticks",
                heals.first().map_or(-1.0, |t| *t as f64),
                8.0,
            )
            .abs("gap1", gap(0), 2.0)
            .abs("gap2", gap(1), 2.0)
            .abs("gap3", gap(2), 2.0)
            .abs("health_gain_240_ticks", f64::from(h1 - h0), 2.0)
            .abs("food_lost", f64::from(food0 - food1), 1.0)
            .abs("saturation_lost", f64::from(sat0 - sat1), 2.0)
            .line(),
    ]
}

/// Hunger drains with a hunger effect; starvation damage rhythm at difficulty normal.
pub async fn starving(c: &Ctx) -> Vec<String> {
    c.reset().await;
    c.cmd("effect give Bot minecraft:hunger 600 255 true").await;
    let t0 = c.bot.tick();
    let zero = c
        .bot
        .wait_for(Duration::from_secs(60), |s| (s.food == 0).then_some(()))
        .await;
    let t_food0 = c.bot.tick() - t0;
    let h0 = c.bot.health();
    let t1 = c.bot.tick();
    let first = c
        .bot
        .wait_for(Duration::from_secs(20), |s| {
            (s.health < h0).then_some(s.health)
        })
        .await
        .ok();
    let t_first = c.bot.tick() - t1;
    let t2 = c.bot.tick();
    let second = c
        .bot
        .wait_for(Duration::from_secs(20), |s| {
            first.filter(|f| s.health < *f).map(|_| s.health)
        })
        .await
        .ok();
    let t_second = c.bot.tick() - t2;
    c.cmd("effect clear Bot").await;
    vec![
        Obs::new("starve")
            .exact("food_reached_zero", zero.is_ok())
            .abs("ticks_until_food_zero", t_food0 as f64, 30.0)
            .abs("ticks_until_first_damage", t_first as f64, 4.0)
            .exact("first_damage", first.map_or(-1.0, |f| f64::from(h0 - f)))
            .abs("ticks_between_damage", t_second as f64, 4.0)
            .exact(
                "second_damage",
                match (first, second) {
                    (Some(f), Some(s)) => f64::from(f - s),
                    _ => -1.0,
                },
            )
            .line(),
    ]
}

/// Death message + respawn for several damage sources.
pub async fn death_and_respawn(c: &Ctx) -> Vec<String> {
    let mut out = Vec::new();
    for (label, setup, cmd) in [
        ("generic", "", "damage Bot 1000 minecraft:generic"),
        ("fall", "", "damage Bot 1000 minecraft:fall"),
        ("in_fire", "", "damage Bot 1000 minecraft:in_fire"),
        ("drown", "", "damage Bot 1000 minecraft:drown"),
        (
            "zombie",
            "summon minecraft:zombie 3.5 -60 0.5 {NoAI:1b,Silent:1b}",
            "damage Bot 1000 minecraft:mob_attack by @e[type=minecraft:zombie,limit=1]",
        ),
        ("kill_command", "", "kill Bot"),
    ] {
        c.reset().await;
        if !setup.is_empty() {
            c.cmd(setup).await;
        }
        let mark = c.bot.event_mark();
        c.srv.cmd(cmd);
        let death = c
            .bot
            .wait_event(mark, Duration::from_secs(10), |e| match e {
                Observed::Death { message, .. } => Some(message.clone()),
                _ => None,
            })
            .await;
        let dead = c.bot.with_state(|s| s.dead);
        let health_dead = c.bot.health();
        c.ticks(10).await;
        c.bot.respawn();
        let respawned = c
            .bot
            .wait_for(Duration::from_secs(20), |s| {
                (!s.dead && s.health > 0.0).then_some(())
            })
            .await;
        c.ticks(10).await;
        let (health, food, y) = c.bot.with_state(|s| (s.health, s.food, s.body.pos.y));
        out.push(
            Obs::new("death")
                .exact("source", label)
                .exact(
                    "message",
                    death.map_or_else(
                        |e| format!("ERROR_{e}").replace(' ', "_"),
                        |m| m.replace(' ', "_"),
                    ),
                )
                .exact("dead_flag", dead)
                .exact("health_when_dead", health_dead)
                .exact("respawned", respawned.is_ok())
                .exact("health_after", health)
                .exact("food_after", food)
                .abs("y_after", y, 5.0)
                .line(),
        );
    }
    out
}

/// Kill a zombie (no AI) with an iron sword: damage per hit, hits needed, death event.
pub async fn kill_mob(c: &Ctx) -> Vec<String> {
    c.reset().await;
    c.give_held("minecraft:iron_sword").await;
    c.cmd("summon minecraft:zombie 2.5 -60 0.5 {NoAI:1b,Silent:1b,PersistenceRequired:1b,DeathLootTable:\"minecraft:empty\"}").await;
    let id = c
        .bot
        .wait_for(Duration::from_secs(5), |s| {
            s.entities
                .values()
                .find(|e| e.entity_type == EntityType::ZOMBIE)
                .map(|e| e.id)
        })
        .await;
    let Ok(id) = id else {
        return vec![
            Obs::new("kill_mob")
                .exact("error", "zombie_not_tracked")
                .line(),
        ];
    };
    c.ticks(30).await; // attack strength fully recovered after the item switch
    let mark = c.bot.event_mark();
    let mut healths = Vec::new();
    for _ in 0..8 {
        c.bot.look_at(Vec3::new(2.5, -59.0, 0.5));
        c.bot.attack(id);
        c.bot.swing();
        c.ticks(24).await; // beyond the 20 tick invulnerability frames and the sword's attack cooldown
        let h = c
            .bot
            .with_state(|s| match s.entities.get(&id).and_then(|e| e.data.get(&9)) {
                Some(EntityDataValue::Float(f)) => Some(*f),
                _ => None,
            });
        healths.push(h);
        if c.bot.with_state(|s| !s.entities.contains_key(&id)) {
            break;
        }
    }
    let died = c.bot.events_since(mark).iter().any(
        |e| matches!(e, Observed::EntityEvent { entity_id, event_id: 3, .. } if *entity_id == id),
    );
    let removed = c.bot.with_state(|s| !s.entities.contains_key(&id));
    let first = healths
        .first()
        .copied()
        .flatten()
        .map_or(-1.0, |h| f64::from(20.0 - h));
    vec![
        Obs::new("kill_mob")
            .pct("first_hit_damage", first, 40.0)
            .abs("hits_to_kill", healths.len() as f64, 1.0)
            .exact("death_event", died)
            .exact("removed", removed)
            .line(),
    ]
}

/// Pick up dropped items: ticks until the take animation, inventory afterwards.
pub async fn pickup(c: &Ctx) -> Vec<String> {
    c.reset().await;
    let mark = c.bot.event_mark();
    let t0 = c.bot.tick();
    c.srv
        .cmd("summon minecraft:item 1.5 -60 0.5 {Item:{id:\"minecraft:diamond\",count:3}}");
    let taken = c
        .bot
        .wait_event(mark, Duration::from_secs(10), |e| match e {
            Observed::TakeItem { amount, tick, .. } => Some((*amount, *tick)),
            _ => None,
        })
        .await;
    c.ticks(5).await;
    let inv = c.bot.with_state(|s| {
        s.inventory
            .slots
            .iter()
            .filter(|st| !st.is_empty())
            .map(|st| format!("{:?}x{}", st.item(), st.count()))
            .collect::<Vec<_>>()
            .join(",")
    });
    vec![
        Obs::new("pickup")
            .exact("taken_amount", taken.as_ref().map_or(-1, |t| t.0))
            .abs(
                "ticks_until_take",
                taken.as_ref().map_or(-1.0, |t| (t.1 - t0) as f64),
                4.0,
            )
            .exact("inventory", inv)
            .line(),
    ]
}

/// TNT at 3 blocks: explosion event (radius, knockback, block count), damage taken.
pub async fn tnt(c: &Ctx) -> Vec<String> {
    c.reset().await;
    c.give_held("minecraft:stone 1").await; // irrelevant item: makes the bot's slot 36 nonempty for later scenarios
    let before = c.bot.health();
    let mark = c.bot.event_mark();
    let t0 = c.bot.tick();
    c.srv.cmd("summon minecraft:tnt 3.5 -60 0.5 {fuse:40s}");
    let ex = c
        .bot
        .wait_event(mark, Duration::from_secs(10), |e| match e {
            Observed::Explosion {
                radius,
                block_count,
                knockback,
                tick,
                ..
            } => Some((*radius, *block_count, *knockback, *tick)),
            _ => None,
        })
        .await;
    c.ticks(10).await;
    let after = c.bot.health();
    vec![match ex {
        Ok((radius, blocks, kb, tick)) => Obs::new("tnt")
            .exact("radius", radius)
            .pct("blocks", f64::from(blocks), 35.0)
            .exact("knockback_to_player", kb.is_some_and(|k| k.x < 0.0))
            .abs("fuse_ticks", (tick - t0) as f64, 3.0)
            .pct("damage", f64::from(before - after), 40.0)
            .line(),
        Err(e) => Obs::new("tnt")
            .exact("error", format!("{e}").replace(' ', "_"))
            .line(),
    }]
}

/// Eating: ticks held, food/saturation gained, stack consumed.
pub async fn eating(c: &Ctx) -> Vec<String> {
    c.reset().await;
    c.cmd("effect give Bot minecraft:hunger 30 255 true").await;
    let _ = c
        .bot
        .wait_for(Duration::from_secs(30), |s| {
            (s.food <= 12 && s.food > 0).then_some(())
        })
        .await;
    c.cmd("effect clear Bot").await;
    c.give_held("minecraft:bread 3").await;
    let (food0, sat0) = c.bot.with_state(|s| (s.food, s.saturation));
    let mark = c.bot.event_mark();
    let t0 = c.bot.tick();
    c.bot.use_item(InteractionHand::MainHand);
    let done = c
        .bot
        .wait_for(Duration::from_secs(10), |s| {
            (slot36(s).count() < 3).then_some(())
        })
        .await;
    let ticks = c.bot.tick() - t0;
    c.ticks(5).await;
    let (food1, sat1) = c.bot.with_state(|s| (s.food, s.saturation));
    let finished = c
        .bot
        .events_since(mark)
        .iter()
        .any(|e| matches!(e, Observed::EntityEvent { event_id: 9, .. }));
    vec![
        Obs::new("eat")
            .exact("consumed", done.is_ok())
            .abs("ticks", ticks as f64, 3.0)
            .exact("food_gain", food1 - food0)
            .exact("saturation_gain", sat1 - sat0)
            .exact("count_after", c.bot.with_state(|s| slot36(s).count()))
            .exact("finish_event", finished)
            .line(),
    ]
}

pub type ScenarioFn = fn(Arc<Ctx>) -> Pin<Box<dyn Future<Output = Vec<String>> + Send>>;

macro_rules! scenarios {
    ($($name:ident),*) => {
        pub const SCENARIOS: &[(&str, ScenarioFn)] = &[$((stringify!($name), |c| Box::pin(async move { $name(&c).await }))),*];
    };
}
scenarios!(
    dig_timing,
    fall_damage,
    drowning,
    regen,
    starving,
    death_and_respawn,
    kill_mob,
    pickup,
    tnt,
    eating
);

// ---------------------------------------------------------------------------------------------------------------------------
// comparison

/// One token `key=value`; fixture values `~N±T` / `~N±P%` accept any number within the tolerance of N.
fn token_matches(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    let (Some(ev), Some(av)) = (expected.strip_prefix('~'), actual.strip_prefix('~')) else {
        return false;
    };
    let parse = |s: &str| -> Option<(f64, f64, bool)> {
        let (n, t) = s.split_once('±')?;
        let (t, pct) = t.strip_suffix('%').map_or((t, false), |t| (t, true));
        Some((n.parse().ok()?, t.parse().ok()?, pct))
    };
    let (Some((en, et, epct)), Some((an, _, _))) = (parse(ev), parse(av)) else {
        return false;
    };
    let tol = if epct { en.abs() * et / 100.0 } else { et };
    (en - an).abs() <= tol
}

/// Compare fixture lines against observed lines (same scenario order); returns the differences.
pub fn compare(expected: &[String], actual: &[String]) -> Vec<String> {
    let mut diffs = Vec::new();
    for i in 0..expected.len().max(actual.len()) {
        match (expected.get(i), actual.get(i)) {
            (Some(e), Some(a)) => {
                let (et, at): (Vec<&str>, Vec<&str>) =
                    (e.split(' ').collect(), a.split(' ').collect());
                let bad: Vec<String> = et
                    .iter()
                    .zip(at.iter())
                    .filter(|(x, y)| {
                        !token_matches(
                            x.split_once('=').map_or(x, |p| p.1),
                            y.split_once('=').map_or(y, |p| p.1),
                        ) || x.split_once('=').map(|p| p.0) != y.split_once('=').map(|p| p.0)
                    })
                    .map(|(x, y)| format!("    vanilla {x}   ours {y}"))
                    .collect();
                if !bad.is_empty() || et.len() != at.len() {
                    diffs.push(format!(
                        "{}\n{}{}",
                        e.split(' ').next().unwrap_or(""),
                        bad.join("\n"),
                        if et.len() != at.len() {
                            "\n    (different number of fields)"
                        } else {
                            ""
                        }
                    ));
                }
            }
            (Some(e), None) => diffs.push(format!("missing line: {e}")),
            (None, Some(a)) => diffs.push(format!("extra line: {a}")),
            (None, None) => {}
        }
    }
    diffs
}

/// Run every scenario (a panic/timeout in one is recorded as an `ERROR` line, the others still run).
pub async fn run_all(
    bot: &Arc<Bot>,
    srv: Arc<dyn Console + Send + Sync>,
    only: Option<&str>,
) -> Vec<(String, Vec<String>)> {
    let mut all = Vec::new();
    for (name, f) in SCENARIOS {
        if only.is_some_and(|o| o != *name) {
            continue;
        }
        let ctx = Arc::new(Ctx {
            bot: bot.clone(),
            srv: srv.clone(),
        });
        let task = tokio::spawn(f(ctx));
        let lines = match tokio::time::timeout(Duration::from_secs(300), task).await {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                let msg = if e.is_panic() {
                    let p = e.into_panic();
                    p.downcast_ref::<String>()
                        .cloned()
                        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default()
                } else {
                    e.to_string()
                };
                vec![format!(
                    "{name} ERROR={}",
                    msg.lines().next().unwrap_or("panic").replace(' ', "_")
                )]
            }
            Err(_) => vec![format!("{name} ERROR=timeout")],
        };
        println!("SCENARIO {name}:\n  {}", lines.join("\n  "));
        all.push((name.to_string(), lines));
    }
    all
}
