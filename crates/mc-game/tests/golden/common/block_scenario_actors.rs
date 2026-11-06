//! Entities and players of the block scenario replay (`spawn`, `spawn_item`, `entity_seed`, `kill`, `tp`, `snap`, `velocity`, `set`,
//! `spawn_player`, `give`, `select`, `use_on`, `use`, `dig`, `explode`, `dump entities|entity:REF|player:NAME|inventory:NAME|packets:NAME`).
//! Included by `block_scenario.rs` (private state of `Scenario`). Differences from the Java driver, all caused by the Rust level having no
//! server: players act through `ServerPlayer::{use_item_on, use_item, handle_block_break_action}` (no packet handler: no acks, no
//! build-limit messages), entity random seeds come from `entity_seed REF N` (Java draws them from a per-thread counter that Rust cannot
//! reproduce: always call `entity_seed` right after `spawn`/`spawn_item` when the entity's randomness matters, and give `spawn_item` an
//! explicit velocity), `explode` has no `source=`, `attack/swing/drop/respawn/pcommand/hurt/effect/attribute/nbt/input/move_packet` are not replayed.
use super::Scenario;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::LegacyRandomSource;
use mc_data::enums::GameType;
use mc_data::{EntityType, Item};
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_explode::Explode;
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::{InteractionHand, ItemStack};
use mc_game::world::level::BlockGetter;
use mc_game::world::level::server_level_accessor::ServerLevelAccessor;
use mc_protocol::codec::{GameProfile, read_varint};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// What the actors need next to the level.
#[derive(Default)]
pub struct Actors {
    /// Named entities (spawned by the script or auto-named `type_path#n`), in naming order.
    pub refs: Vec<(String, EntityId)>,
    pub players: Vec<String>,
    known: HashSet<EntityId>,
    auto: HashMap<String, i32>,
    rx: HashMap<String, UnboundedReceiver<Outbound>>,
    pub registries_loaded: bool,
}

static ACCESS: std::sync::LazyLock<mc_game::registry::RegistryAccess> =
    std::sync::LazyLock::new(|| {
        let _serial = super::super::SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = super::super::load_world(
            dir.path(),
            mc_world::level_storage::WorldDataConfiguration::DEFAULT,
            true,
        )
        .expect("default world loads");
        stem.registries.composite_access().clone()
    });

fn dbl(v: f64) -> String {
    mc_core::math::java::java_double_to_string(v)
}
fn flt(v: f32) -> String {
    mc_core::math::java::java_float_to_string(v)
}
fn vec3(a: f64, b: f64, c: f64) -> String {
    format!("{},{},{}", dbl(a), dbl(b), dbl(c))
}
fn num(s: &str) -> f64 {
    s.parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}
fn item_name(stack: &ItemStack) -> String {
    if stack.is_empty() {
        return "empty".into();
    }
    let dmg = if stack.is_damageable_item() && stack.get_damage_value() != 0 {
        format!("/dmg={}", stack.get_damage_value())
    } else {
        String::new()
    };
    format!("{}*{}{dmg}", stack.item().name(), stack.count())
}

fn pose_name(pose: impl std::fmt::Debug) -> String {
    format!("{pose:?}").chars().fold(String::new(), |mut s, c| {
        if c.is_uppercase() && !s.is_empty() {
            s.push('_');
        }
        s.push(c.to_ascii_lowercase());
        s
    })
}

impl Scenario {
    /// Entity creation needs the data pack registries (damage types, enchantments, ...): loaded once per test binary.
    pub(super) fn load_registries(&mut self) {
        if !self.actors.registries_loaded {
            self.actors.registries_loaded = true;
            self.level().registries = ACCESS.clone();
        }
    }

    pub(super) fn entity_id(&self, name: &str) -> EntityId {
        self.actors
            .refs
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("unknown ref {name}"))
            .1
    }

    fn item_stack(&self, spec: &str, count: i32) -> ItemStack {
        assert!(
            !spec.contains('['),
            "item components are not supported by the Rust replay: {spec}"
        );
        let item = Item::by_name(spec).unwrap_or_else(|| panic!("unknown item {spec}"));
        ItemStack::with_count(item, count)
    }

    /// Entities created by game logic get stable names `type_path#n` in creation (id) order (twin of `nameNewEntities`).
    pub(super) fn name_new_entities(&mut self) {
        let Some(level) = self.level.as_ref() else {
            return;
        };
        let r = (self.opts.preload + 1) * 16;
        let bb = mc_core::phys::AABB::new(
            f64::from(-r),
            -64.0,
            f64::from(-r),
            f64::from(r),
            320.0,
            f64::from(r),
        );
        let mut ids = level.get_entities(None, &bb, |_| true);
        ids.sort_unstable();
        for id in ids {
            if self.actors.known.insert(id) && !self.actors.refs.iter().any(|(_, i)| *i == id) {
                let path = level
                    .get_entity(id)
                    .map(|e| {
                        e.get_type()
                            .name()
                            .trim_start_matches("minecraft:")
                            .to_string()
                    })
                    .unwrap_or_default();
                let n = self.actors.auto.entry(path.clone()).or_insert(0);
                *n += 1;
                let name = format!("{path}#{n}");
                self.actors.refs.push((name, id));
            }
        }
    }

    pub(super) fn actor_command(&mut self, a: &[String]) -> bool {
        match a[0].as_str() {
            "spawn" => {
                self.world();
                self.load_registries();
                self.spawn(a);
            }
            "spawn_item" => {
                self.world();
                self.load_registries();
                let rot = a
                    .iter()
                    .skip(3)
                    .find_map(|t| t.strip_prefix("rot="))
                    .map(|v| v.parse::<f32>().expect("rot=YAW"));
                let a: &[String] = &a
                    .iter()
                    .filter(|t| !t.starts_with("rot="))
                    .cloned()
                    .collect::<Vec<_>>();
                let stack = self.item_stack(&a[2], a.get(9).map_or(1, |c| c.parse().unwrap()));
                assert!(
                    a.len() >= 9,
                    "spawn_item without a velocity draws random motion from the entity random: give `vx vy vz`"
                );
                let e = mc_game::world::entity::item::ItemEntity::with_stack_and_motion(
                    self.level(),
                    num(&a[3]),
                    num(&a[4]),
                    num(&a[5]),
                    stack,
                    num(&a[6]),
                    num(&a[7]),
                    num(&a[8]),
                );
                let mut e = e;
                if let Some(rot) = rot {
                    let x_rot = e.base().x_rot();
                    e.base_mut().set_rot(rot, x_rot);
                }
                let id = e.id();
                assert!(self.level().add_fresh_entity(Box::new(e)));
                self.actors.known.insert(id);
                self.actors.refs.push((a[1].clone(), id));
            }
            "entity_seed" => {
                self.world();
                let id = self.entity_id(&a[1]);
                let seed: i64 = a[2].parse().unwrap();
                self.level().with_entity(id, |_, e| {
                    e.base_mut().random = LegacyRandomSource::new(seed)
                });
            }
            "kill" => {
                self.world();
                let id = self.entity_id(&a[1]);
                self.level().with_entity(id, |_, e| e.discard());
            }
            "snap" => {
                self.world();
                let id = self.entity_id(&a[1]);
                self.level().with_entity(id, |_, e| {
                    let (yr, xr) = (e.base().y_rot(), e.base().x_rot());
                    e.snap_to(
                        num(&a[2]),
                        num(&a[3]),
                        num(&a[4]),
                        a.get(5).map_or(yr, |v| num(v) as f32),
                        a.get(6).map_or(xr, |v| num(v) as f32),
                    );
                });
            }
            "tp" => {
                self.world();
                let id = self.entity_id(&a[1]);
                self.level().with_entity(id, |level, e| {
                    let (yr, xr) = (e.base().y_rot(), e.base().x_rot());
                    e.teleport_to(level, num(&a[2]), num(&a[3]), num(&a[4]));
                    if a.len() > 5 {
                        e.set_rot(
                            a.get(5).map_or(yr, |v| num(v) as f32),
                            a.get(6).map_or(xr, |v| num(v) as f32),
                        );
                    }
                });
            }
            "velocity" => {
                self.world();
                let id = self.entity_id(&a[1]);
                self.level().with_entity(id, |_, e| {
                    e.base_mut()
                        .set_delta_movement_xyz(num(&a[2]), num(&a[3]), num(&a[4]))
                });
            }
            "set" => {
                self.world();
                let id = self.entity_id(&a[1]);
                let v = a[3].clone();
                self.level().with_entity(id, |_, e| match a[2].as_str() {
                    "air" => e.base_mut().set_air_supply(v.parse().unwrap()),
                    "fire" => e.set_remaining_fire_ticks(v.parse().unwrap()),
                    "fall_distance" => e.base_mut().fall_distance = num(&v),
                    "tag" => {
                        e.base_mut().add_tag(&v);
                    }
                    other => panic!("set {other} is not supported by the Rust replay"),
                });
            }
            "spawn_player" => {
                self.world();
                self.load_registries();
                self.spawn_player(a);
            }
            "give" => {
                self.world();
                let count = a.get(4).map_or(1, |c| c.parse().unwrap());
                let stack = self.item_stack(&a[3], count);
                self.with_player_named(&a[1], |_, p| {
                    let slot = a[2].as_str();
                    if let Some(n) = slot
                        .strip_prefix("hotbar")
                        .or_else(|| slot.strip_prefix("inv"))
                    {
                        p.player.inventory.get_non_equipment_items_mut()
                            [n.parse::<usize>().unwrap()] = stack;
                    } else {
                        let es = match slot {
                            "mainhand" => EquipmentSlot::MainHand,
                            "offhand" => EquipmentSlot::OffHand,
                            "head" => EquipmentSlot::Head,
                            "chest" => EquipmentSlot::Chest,
                            "legs" => EquipmentSlot::Legs,
                            "feet" => EquipmentSlot::Feet,
                            other => panic!("slot {other}"),
                        };
                        p.player.inventory.equipment_set(es, stack);
                    }
                });
            }
            "select" => {
                self.world();
                self.with_player_named(&a[1], |_, p| {
                    p.player
                        .inventory
                        .set_selected_slot(a[2].parse().unwrap())
                        .expect("hotbar slot");
                });
            }
            "use" => {
                self.world();
                let hand = hand(a, 2);
                self.with_player_named(&a[1], |level, p| {
                    p.use_item(level, hand);
                });
            }
            "use_on" => {
                self.world();
                self.use_on(a);
            }
            "dig" => {
                self.world();
                self.dig(a);
            }
            "explode" => {
                self.world();
                self.load_registries();
                self.explode(a);
            }
            _ => return false,
        }
        true
    }

    fn with_player_named<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
    ) -> R {
        let id = self.entity_id(name);
        self.level()
            .with_player(id, f)
            .unwrap_or_else(|| panic!("no player {name}"))
    }

    fn spawn(&mut self, a: &[String]) {
        let ty = EntityType::by_name(&if a[2].contains(':') {
            a[2].clone()
        } else {
            format!("minecraft:{}", a[2])
        })
        .unwrap_or_else(|| panic!("entity type {}", a[2]));
        let request = EntitySpawnRequest {
            reason: mc_data::enums::EntitySpawnReason::Command,
            ignore_checks: true,
        };
        let level = self.level();
        let mut e =
            factory::create(ty, level, request).unwrap_or_else(|| panic!("cannot create {}", a[2]));
        let mut n = a.len();
        let snbt = (n > 6 && a[n - 1].starts_with('{')).then(|| {
            n -= 1;
            a[n].clone()
        });
        e.snap_to(
            num(&a[3]),
            num(&a[4]),
            num(&a[5]),
            if n > 7 { num(&a[6]) as f32 } else { 0.0 },
            if n > 7 { num(&a[7]) as f32 } else { 0.0 },
        );
        if let Some(snbt) = snbt {
            let lookup = |_: EntityId| None;
            let mut tag = mc_nbt::CompoundTag::new();
            mc_game::world::entity::entity::save_without_id(&*e, &mut tag, &lookup);
            tag.merge(
                &mc_nbt::snbt::parse_compound(&snbt)
                    .unwrap_or_else(|er| panic!("bad SNBT {snbt}: {er}")),
            );
            e.load(&tag)
                .unwrap_or_else(|er| panic!("entity load: {er}"));
        }
        let id = e.id();
        level.add_fresh_entity_with_passengers(vec![e]);
        self.actors.known.insert(id);
        self.actors.refs.push((a[1].clone(), id));
    }

    fn spawn_player(&mut self, a: &[String]) {
        let name = a[1].clone();
        let sim = a.last().map(String::as_str) == Some("sim");
        assert!(
            !sim,
            "simulated players (`sim`) are not supported by the block replay"
        );
        let level = self.level();
        let id = level.next_entity_id();
        let profile = GameProfile {
            id: uuid::Uuid::from_u128(id as u128),
            name: name.clone(),
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
            "survival" => GameType::Survival,
            other => panic!("game mode {other}"),
        };
        p.set_game_mode_for_player(mode, None);
        let (yaw, pitch) = if a.len() > 7 {
            (num(&a[6]) as f32, num(&a[7]) as f32)
        } else {
            (0.0, 0.0)
        };
        p.snap_to(num(&a[3]), num(&a[4]), num(&a[5]), yaw, pitch);
        p.base_mut().first_tick = true;
        let (tx, rx) = unbounded_channel();
        p.connection = Some(ConnectionSender::new(tx));
        level.add_player(Box::new(p));
        // a non-simulated player has no movement packets: onGround is derived once (resting on a block)
        level.with_player(id, |level, p| {
            let bb = p.base().bounding_box().move_(0.0, -1.0E-3, 0.0);
            let free = level.no_collision_aabb(&bb);
            p.base_mut().set_on_ground_raw(!free);
        });
        self.actors.known.insert(id);
        self.actors.refs.push((name.clone(), id));
        self.actors.players.push(name.clone());
        self.actors.rx.insert(name, rx);
    }

    /// `ServerboundUseItemOnPacket` minus the packet listener: reach and hit-location checks, then `ServerPlayerGameMode.useItemOn`.
    fn use_on(&mut self, a: &[String]) {
        let pos = BlockPos::new(
            a[2].parse().unwrap(),
            a[3].parse().unwrap(),
            a[4].parse().unwrap(),
        );
        let face = Direction::by_name(&a[5]).unwrap_or_else(|| panic!("face {}", a[5]));
        let cursor = a.len() > 9;
        let hit = if cursor {
            let n = a.len();
            Vec3::new(
                f64::from(pos.x()) + num(&a[n - 3]),
                f64::from(pos.y()) + num(&a[n - 2]),
                f64::from(pos.z()) + num(&a[n - 1]),
            )
        } else {
            let c = Vec3::at_center_of(&pos);
            Vec3::new(
                c.x + f64::from(face.get_step_x()) * 0.5,
                c.y + f64::from(face.get_step_y()) * 0.5,
                c.z + f64::from(face.get_step_z()) * 0.5,
            )
        };
        let hand_idx = a.len() > 6 && (a[6] == "main" || a[6] == "off");
        let hand = if hand_idx {
            hand(a, 6)
        } else {
            InteractionHand::MainHand
        };
        let result = BlockHitResult {
            location: hit,
            direction: face,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };
        self.with_player_named(&a[1], |level, p| {
            if !p.is_within_block_interaction_range(pos, 1.0) {
                return;
            }
            let d = hit.subtract(Vec3::at_center_of(&pos));
            if !(d.x.abs() < 1.0000001 && d.y.abs() < 1.0000001 && d.z.abs() < 1.0000001) {
                return;
            }
            p.use_item_on(level, hand, &result);
        });
    }

    /// Twin of the driver's `dig`: client-like progress accumulation, STOP_DESTROY_BLOCK when the client would send it.
    fn dig(&mut self, a: &[String]) {
        use mc_protocol::packets::play::ServerboundPlayerActionAction as Action;
        let pos = BlockPos::new(
            a[2].parse().unwrap(),
            a[3].parse().unwrap(),
            a[4].parse().unwrap(),
        );
        let limit: i32 = a.get(5).map_or(-1, |l| l.parse().unwrap());
        let max_y = self.level().get_max_y();
        let name = a[1].clone();
        let st = self.level().get_block_state(pos);
        let (creative, instant) = self.with_player_named(&name, |level, p| {
            p.handle_block_break_action(
                level,
                pos,
                Action::StartDestroyBlock,
                Direction::Up,
                max_y,
                false,
            );
            (p.is_creative(), p.get_destroy_progress(st) >= 1.0)
        });
        let (mut progress, mut ticks, mut stopped) = (0f32, 0, creative);
        if !creative && !st.is_air() && instant {
            stopped = true;
        }
        while !stopped {
            ticks += 1;
            let st = self.level().get_block_state(pos);
            let step = self.with_player_named(&name, |_, p| p.get_destroy_progress(st));
            progress += step;
            if if limit >= 0 {
                ticks >= limit
            } else {
                progress >= 1.0
            } {
                self.with_player_named(&name, |level, p| {
                    p.handle_block_break_action(
                        level,
                        pos,
                        Action::StopDestroyBlock,
                        Direction::Up,
                        max_y,
                        false,
                    )
                });
                stopped = true;
                self.tick_once();
                break;
            }
            self.tick_once();
            assert!(ticks < 100_000, "dig never finishes");
        }
        let broken = self.level().get_block_state(pos).is_air();
        self.out
            .push(format!("DIG {name} ticks={ticks} broken={broken}"));
    }

    fn explode(&mut self, a: &[String]) {
        use mc_game::world::level::explosion::ExplosionInteraction as I;
        let mut inter = I::None;
        let mut fire = false;
        for t in &a[5..] {
            match t.as_str() {
                "fire" => fire = true,
                "none" => inter = I::None,
                "block" => inter = I::Block,
                "mob" => inter = I::Mob,
                "tnt" => inter = I::Tnt,
                "trigger" => inter = I::Trigger,
                other => panic!("explode option {other} is not supported by the Rust replay"),
            }
        }
        let params = Explode::new(
            None,
            num(&a[1]),
            num(&a[2]),
            num(&a[3]),
            num(&a[4]) as f32,
            inter,
        )
        .with_fire(fire);
        self.level().explode(params, &mut []);
    }

    /// The player half of a tick, after `ServerLevel::tick` (which ticked the player entity): `ServerGamePacketListenerImpl.tick` -> `doTick`.
    pub(super) fn tick_players(&mut self) {
        for name in self.actors.players.clone() {
            self.with_player_named(&name, |level, p| p.do_tick(level));
        }
    }

    // ------------------------------------------------------------------------------------------------ dumps

    pub(super) fn dump_actor(&mut self, kind: &str, arg: Option<&str>) -> bool {
        match kind {
            "entities" => {
                self.name_new_entities();
                let mut all: Vec<(String, EntityId)> = self.actors.refs.clone();
                all.retain(|(n, id)| {
                    !self.actors.players.contains(n)
                        && self.level().get_entity(*id).is_some_and(|e| e.is_alive())
                });
                all.sort_by_key(|(_, id)| *id);
                for (name, id) in all {
                    let line = self.describe(&name, id);
                    self.out.push(line);
                }
            }
            "entity" | "player" => {
                let name = arg.expect("entity:REF");
                let id = self.entity_id(name);
                let line = self.describe(name, id);
                self.out.push(line);
            }
            "all_players" => {
                for name in self.actors.players.clone() {
                    let id = self.entity_id(&name);
                    let line = self.describe(&name, id);
                    self.out.push(line);
                }
            }
            "inventory" => {
                let name = arg.expect("inventory:NAME");
                let id = self.entity_id(name);
                let items: Vec<String> = self
                    .level()
                    .get_entity(id)
                    .and_then(|e| {
                        e.as_any().downcast_ref::<ServerPlayer>().map(|p| {
                            p.player
                                .inventory
                                .get_non_equipment_items()
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| !s.is_empty())
                                .map(|(i, s)| format!("I {name} {i} {}", item_name(s)))
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                self.out.extend(items);
            }
            "packets" => {
                let arg = arg.expect("packets:NAME[:type,..]");
                let (name, only) = arg
                    .split_once(':')
                    .map_or((arg, None), |(n, f)| (n, Some(f)));
                let only: Option<Vec<&str>> = only.map(|f| f.split(',').collect());
                let rx = self
                    .actors
                    .rx
                    .get_mut(name)
                    .unwrap_or_else(|| panic!("no player {name}"));
                while let Ok(msg) = rx.try_recv() {
                    let (Outbound::Payload(p) | Outbound::Write(p)) = msg else {
                        continue;
                    };
                    let mut body: &[u8] = &p;
                    let id = read_varint(&mut body).expect("packet id");
                    let ty = mc_protocol::packet::packet_name(
                        mc_protocol::packet::ConnectionState::Play,
                        mc_protocol::packet::PacketFlow::Clientbound,
                        id,
                    )
                    .unwrap_or("?");
                    if only
                        .as_ref()
                        .is_none_or(|o| o.contains(&ty.trim_start_matches("minecraft:")))
                    {
                        let hex: String = p.iter().map(|b| format!("{b:02x}")).collect();
                        self.out.push(format!("PKT {name} {ty} {hex}"));
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// The `P`/`E` line of the driver's `describe` for the fields the Rust entities model.
    fn describe(&mut self, name: &str, id: EntityId) -> String {
        let players = self.actors.players.clone();
        let level = self.level();
        let e = level
            .get_entity(id)
            .unwrap_or_else(|| panic!("entity {name} is gone"));
        let b = e.base();
        let is_player = players.iter().any(|n| n == name);
        let mut s = format!(
            "{} {name} {}",
            if is_player { "P" } else { "E" },
            e.get_type().name()
        );
        let (p, d) = (b.position(), b.delta_movement());
        s += &format!(
            " pos={} delta={} rot={},{} og={} fall={} fire={} air={} alive={} pose={}",
            vec3(p.x, p.y, p.z),
            vec3(d.x, d.y, d.z),
            flt(b.y_rot()),
            flt(b.x_rot()),
            i32::from(b.on_ground()),
            dbl(b.fall_distance),
            b.remaining_fire_ticks,
            b.get_air_supply(),
            i32::from(e.is_alive()),
            pose_name(b.get_pose())
        );
        if let Some(l) = e.living() {
            let mut fx: Vec<String> = l
                .active_effects
                .iter()
                .map(|(k, v)| format!("{}/{}/{}", k.name(), v.amplifier, v.duration))
                .collect();
            fx.sort();
            s += &format!(
                " hp={} abs={} hurt={}/{} death={} fx={}",
                flt(l.get_health()),
                flt(e.get_absorption_amount()),
                l.hurt_time,
                l.damage_cooldown_time,
                l.death_time,
                if fx.is_empty() {
                    "-".into()
                } else {
                    fx.join(";")
                }
            );
        }
        if let Some(pl) = e.as_any().downcast_ref::<ServerPlayer>() {
            s += &format!(
                " gm={} food={} sat={} exh={} xp={}/{}/{} sel={} main={}",
                pl.game_mode.game_mode_for_player.name().to_lowercase(),
                pl.player.food_data.get_food_level(),
                flt(pl.player.food_data.get_saturation_level()),
                flt(pl.player.food_data.exhaustion_level),
                pl.player.experience_level,
                pl.player.total_experience,
                flt(pl.player.experience_progress),
                pl.player.inventory.get_selected_slot(),
                item_name(pl.player.inventory.get_selected_item())
            );
        } else if let Some(item) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::item::ItemEntity>()
        {
            s += &format!(
                " item={} age={} pickup={} health={} owner={} thrower={}",
                item_name(&item.get_item()),
                item.age,
                item.pickup_delay,
                item.health,
                item.target.map_or("-".into(), |u| u.to_string()),
                item.thrower.map_or("-".into(), |u| u.to_string())
            );
        } else if let Some(t) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::item::primed_tnt::PrimedTnt>()
        {
            s += &format!(" fuse={}", t.get_fuse());
        }
        s
    }
}

fn hand(a: &[String], i: usize) -> InteractionHand {
    if a.get(i).map(String::as_str) == Some("off") {
        InteractionHand::OffHand
    } else {
        InteractionHand::MainHand
    }
}
