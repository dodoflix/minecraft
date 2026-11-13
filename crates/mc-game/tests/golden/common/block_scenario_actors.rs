//! Entities and players of the block scenario replay (`spawn`, `mobspawn`, `spawn_item`, `entity_seed`, `kill`, `nbt`, `hurt`, `effect`, `mob_attack`, `input` (sneak: the shift key), `op`, `pick_block`, `tp`, `snap`, `velocity`, `set`,
//! `leash`, `spawn_player`, `sign_update`, `dig`, `explode`, `pcommand` (sprinting, stop_sleeping), `dump entities|xentities|entity:REF|xentity:REF|edata:REF|player:NAME|packets:NAME`). The player actions (`give select use use_on release interact drop swing swap dispense`
//! and the dumps `use cooldowns stats inventory`) are shared with the entity replay: `player_actions.rs`; the `E` line of every dump is
//! `entity_describe.rs`. `move_packet`, `move_vehicle`, `paddle`, `accept_teleport` and `client_tick_end` hand the packet the Java verb builds to the player's
//! listener; `respawn` replays the player list's respawn and then delivers the client's loaded report.
//! Included by `block_scenario.rs` (private state of `Scenario`). Differences from the Java driver, all caused by the Rust level having no
//! server: players act through `ServerPlayer::{use_item_on, use_item, handle_block_break_action}` and the replay re-does what the packet
//! handlers around them add (the listener's sequence number and block change ack, the swing and block updates sent back), but no
//! serverbound packet of those verbs is decoded and the build-limit messages are not replayed. Entity random seeds come from `entity_seed REF N` (Java draws them from a per-thread counter that Rust cannot
//! reproduce: always call `entity_seed` right after `spawn`/`spawn_item` when the entity's randomness matters, and give `spawn_item` an
//! explicit velocity), `explode` takes `source=REF`. Players get their chunks through the listener's chunk sender, like the oracle's
//! connection (no batch is acknowledged: a player tracks entities only in the first batch of chunks).
use super::Scenario;
use crate::common::menu_actions::deliver;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::LegacyRandomSource;
use mc_data::EntityType;
use mc_data::enums::GameType;
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_explode::Explode;
use mc_game::server::network::common_listener::CommonListenerCookie;
use mc_game::server::network::game_listener::{
    CLIENT_LOADED_TIMEOUT_TIME, ListenerSettings, ServerGamePacketListenerImpl,
};
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::world::block_entity::sign_block_entity::{SignTextSlot, update_sign_text_from_packet};
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::mob::leashable;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::InteractionHand;
use mc_game::world::level::BlockGetter;
use mc_game::world::level::server_level_accessor::ServerLevelAccessor;
use mc_protocol::codec::{Decode, Encode, read_varint};
use mc_protocol::packets::play::{
    AttributeModifierOperation, ClientboundSetTimePacket, ClientboundUpdateAttributesPacket,
    PositionAndRotation, ServerboundAcceptTeleportationPacket, ServerboundClientTickEndPacket,
    ServerboundContainerButtonClickPacket, ServerboundContainerClosePacket,
    ServerboundMovePlayerPosPacket, ServerboundMovePlayerPosRotPacket,
    ServerboundMoveVehiclePacket, ServerboundPaddleBoatPacket, ServerboundPickItemFromBlockPacket,
    ServerboundPlayPacket as P, ServerboundSetCarriedItemPacket,
};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// `VanillaLevel.reseed(hash(name))`: the entity random seeds of what a spawn command creates.
fn reseed_for(name: &str) {
    mc_core::random::reseed_uniquifier(crate::common::entity_scenario::java_hash(name));
}

/// What the actors need next to the level.
#[derive(Default)]
pub struct Actors {
    /// Named entities (spawned by the script or auto-named `type_path#n`), in naming order.
    pub refs: Vec<(String, EntityId)>,
    pub players: Vec<String>,
    known: HashSet<EntityId>,
    auto: HashMap<String, i32>,
    rx: HashMap<String, UnboundedReceiver<Outbound>>,
    /// The driver's `seq(player)` counters.
    sequences: crate::common::player_actions::Sequences,
    /// `track_movement` players with the position at the start of the previous tick (`None` before the first tick).
    tracked_movement: HashMap<EntityId, Option<Vec3>>,
    /// Keeps the inbound queue of every replayed listener open.
    _inbound: Vec<std::sync::mpsc::Sender<mc_game::server::network::InboundPacket>>,
    pub registries_loaded: bool,
}

static ACCESS: std::sync::LazyLock<(
    mc_game::registry::RegistryAccess,
    mc_game::world::item::crafting::RecipeManager,
)> = std::sync::LazyLock::new(|| {
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
    let recipes = stem
        .data_pack_resources
        .listener::<mc_game::world::item::crafting::RecipeManager>()
        .expect("recipe manager")
        .clone();
    (stem.registries.composite_access().clone(), recipes)
});

/// The vanilla advancement tree of the default data pack; every replayed player gets progress over it (`PlayerAdvancements`).
static ADVANCEMENTS: std::sync::LazyLock<
    mc_game::advancements::server_advancement_manager::ServerAdvancementManager,
> = std::sync::LazyLock::new(|| {
    mc_game::advancements::server_advancement_manager::ServerAdvancementManager::from_registries(
        &ACCESS.0,
    )
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
/// `VanillaLevel.record`: the advancement update carries timestamps of obtained criteria; the attribute snapshots are a `HashSet` in
/// Java (identity hash order), so they are dumped sorted by attribute and modifier id; the recipe property sets sorted by key and item;
/// anything else is its hex wire form.
fn packet_text(ty: &str, wire: &[u8]) -> String {
    match ty {
        "minecraft:update_advancements" => "-".into(),
        "minecraft:update_attributes" => {
            let mut body = wire;
            read_varint(&mut body).expect("packet id");
            let p =
                ClientboundUpdateAttributesPacket::decode(&mut body).expect("update_attributes");
            let mut entries: Vec<String> = p
                .attributes
                .iter()
                .map(|s| {
                    let mut mods: Vec<String> = s
                        .modifiers
                        .iter()
                        .map(|m| {
                            let op = match m.operation {
                                AttributeModifierOperation::AddValue => "add_value",
                                AttributeModifierOperation::AddMultipliedBase => {
                                    "add_multiplied_base"
                                }
                                AttributeModifierOperation::AddMultipliedTotal => {
                                    "add_multiplied_total"
                                }
                            };
                            format!("{}/{}/{op}", m.id, dbl(m.amount))
                        })
                        .collect();
                    mods.sort();
                    format!("{}={}[{}]", s.attribute.name(), dbl(s.base), mods.join(","))
                })
                .collect();
            entries.sort();
            format!("entity={};{}", p.entity_id, entries.join(";"))
        }
        // the property sets are a `HashMap` of `Set.copyOf` sets in Java (per JVM order): canonical order by key and item id
        "minecraft:update_recipes" => {
            let mut body = wire;
            read_varint(&mut body).expect("packet id");
            let p = mc_protocol::packets::play::ClientboundUpdateRecipesPacket::decode(&mut body)
                .expect("update_recipes");
            let mut sets: Vec<(String, Vec<&str>)> = p
                .item_sets
                .iter()
                .map(|(key, set)| {
                    let mut items: Vec<&str> = set.items.iter().map(|i| i.name()).collect();
                    items.sort();
                    (key.to_string(), items)
                })
                .collect();
            sets.sort();
            let mut stonecutter = Vec::new();
            p.stonecutter_recipes
                .encode(&mut stonecutter)
                .expect("stonecutter recipes encode");
            let sets: String = sets
                .iter()
                .map(|(key, items)| format!("{key}={};", items.join(",")))
                .collect();
            let stonecutter: String = stonecutter.iter().map(|b| format!("{b:02x}")).collect();
            format!("sets={sets} stonecutter={stonecutter}")
        }
        _ => wire.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

/// `ServerChunkCache.broadcastChangedChunks` walks an identity-hashed set in vanilla, so the chunks of one tick send their block
/// changes in an order that differs per JVM run: the oracle driver (`EntityOracleDriver.chunkOrder`) and this replay stably sort every
/// run of consecutive chunk change packets by chunk.
fn chunk_order(sent: Vec<(&'static str, Vec<u8>)>) -> Vec<(&'static str, Vec<u8>)> {
    type Sent = (&'static str, Vec<u8>);
    fn flush(run: &mut Vec<((i64, i64), Sent)>, result: &mut Vec<Sent>) {
        run.sort_by_key(|(chunk, _)| *chunk);
        result.extend(run.drain(..).map(|(_, p)| p));
    }
    let mut result = Vec::with_capacity(sent.len());
    let mut run = Vec::new();
    for (ty, wire) in sent {
        if let Some(chunk) = packet_chunk(ty, &wire) {
            run.push((chunk, (ty, wire)));
        } else {
            flush(&mut run, &mut result);
            result.push((ty, wire));
        }
    }
    flush(&mut run, &mut result);
    result
}

/// The chunk (x, z) a chunk change packet belongs to, read from its wire form; `None` for other packets.
fn packet_chunk(ty: &str, wire: &[u8]) -> Option<(i64, i64)> {
    if !matches!(
        ty,
        "minecraft:block_update"
            | "minecraft:block_entity_data"
            | "minecraft:section_blocks_update"
            | "minecraft:light_update"
    ) {
        return None;
    }
    let mut body = wire;
    read_varint(&mut body).expect("packet id");
    if ty == "minecraft:light_update" {
        let x = read_varint(&mut body).expect("chunk x");
        let z = read_varint(&mut body).expect("chunk z");
        return Some((i64::from(x), i64::from(z)));
    }
    let v = i64::from_be_bytes(body[..8].try_into().expect("position"));
    Some(if ty == "minecraft:section_blocks_update" {
        (v >> 42, (v << 22) >> 42)
    } else {
        ((v >> 38) >> 4, ((v << 26) >> 38) >> 4)
    })
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

/// The worldgen biome registry of the vanilla data pack (`cmd weather` scenarios: precipitation reads the biome).
pub(super) fn worldgen_biomes() -> std::sync::Arc<mc_worldgen::biome::BiomeRegistry> {
    std::sync::Arc::new(
        mc_worldgen::biome::BiomeRegistry::load(&ACCESS.0.worldgen_registries())
            .expect("vanilla biomes load"),
    )
}

impl Scenario {
    /// Entity creation needs the data pack registries (damage types, enchantments, ...): loaded once per test binary.
    pub(super) fn load_registries(&mut self) {
        if !self.actors.registries_loaded {
            self.actors.registries_loaded = true;
            let level = self.level();
            level.registries = ACCESS.0.clone();
            level.shared.set_recipe_manager(ACCESS.1.clone());
            // item behaviour that reads registries without a level (`Item.getUseDuration` of a goat horn)
            mc_game::world::item::set_item_registries(ACCESS.0.clone());
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

    /// `hurt REF DAMAGETYPE AMOUNT [by=REF] [direct=REF] [at=X,Y,Z]`: `entity.hurtServer(level, source, amount)`, prints `HURT ..`.
    fn hurt(&mut self, a: &[String]) {
        let (mut by, mut direct, mut at) = (None, None, None);
        for t in &a[4..] {
            match t.split_once('=') {
                Some(("by", r)) => by = Some(self.entity_id(r)),
                Some(("direct", r)) => direct = Some(self.entity_id(r)),
                Some(("at", r)) => {
                    let c: Vec<f64> = r.split(',').map(num).collect();
                    at = Some(Vec3::new(c[0], c[1], c[2]));
                }
                _ => panic!("hurt option {t} is not replayed (by= direct= at=)"),
            }
        }
        let direct = direct.or(by);
        let key = Identifier::parse(&if a[2].contains(':') {
            a[2].clone()
        } else {
            format!("minecraft:{}", a[2])
        })
        .unwrap();
        let level = self.level();
        let data = level
            .registries
            .get::<mc_game::registry::typed::damage_type::DamageType>("minecraft:damage_type", &key)
            .unwrap_or_else(|| panic!("unknown damage type {key}"))
            .clone();
        let source = mc_game::world::damagesource::DamageSource::new(
            key,
            std::sync::Arc::new(data),
            direct.and_then(|i| level.get_entity(i)),
            by.and_then(|i| level.get_entity(i)),
            at,
        );
        let amount = num(&a[3]) as f32;
        let id = self.entity_id(&a[1]);
        let r = self
            .level()
            .with_entity(id, |level, e| e.hurt_server(level, &source, amount))
            .unwrap_or_else(|| panic!("entity {} is gone", a[1]));
        self.out
            .push(format!("HURT {} {} {} -> {r}", a[1], a[2], a[3]));
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
                reseed_for(&a[1]);
                self.spawn(a);
            }
            // leash REF HOLDER|-          Leashable.setLeashedTo(holder, true) / removeLeash()
            "leash" => {
                self.world();
                let id = self.entity_id(&a[1]);
                if a[2] == "-" {
                    self.level()
                        .with_entity(id, |level, e| leashable::remove_leash(e, level));
                } else {
                    let holder_id = self.entity_id(&a[2]);
                    let holder = self
                        .level()
                        .take_entity(holder_id)
                        .unwrap_or_else(|| panic!("leash holder {} is not in the level", a[2]));
                    self.level().with_entity(id, |level, e| {
                        leashable::set_leashed_to(
                            e,
                            level,
                            leashable::LeashHolder::of(&*holder),
                            true,
                        )
                    });
                    self.level().put_back_entity(holder_id, holder);
                }
            }
            "spawn_item" => {
                self.world();
                self.load_registries();
                reseed_for(&a[1]);
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
                let stack = crate::common::player_actions::item_stack(
                    &a[2],
                    a.get(9).map_or(1, |c| c.parse().unwrap()),
                );
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
            "input" => {
                self.world();
                let flags: Vec<&str> = a[2].split(',').collect();
                let has = |f: &str| flags.contains(&f);
                let input = mc_protocol::packets::play::Input {
                    forward: has("forward"),
                    backward: has("back"),
                    left: has("left"),
                    right: has("right"),
                    jump: has("jump"),
                    shift: has("sneak"),
                    sprint: has("sprint"),
                };
                // `handlePlayerInput`
                self.with_player_named(&a[1], |_, p| {
                    p.set_last_client_input(input);
                    p.set_shift_key_down(input.shift);
                });
            }
            // ride REF VEHICLE: `startRiding(vehicle, true, true)`
            "ride" => {
                self.world();
                let (rider, vehicle) = (self.entity_id(&a[1]), self.entity_id(&a[2]));
                self.level()
                    .with_entity(rider, |level, e| level.start_riding(e, vehicle, true, true));
            }
            // track_movement PLAYER: a sim player reports the movement of the previous tick as its known movement
            "track_movement" => {
                self.world();
                let id = self.entity_id(&a[1]);
                self.actors.tracked_movement.insert(id, None);
            }
            "op" => {
                self.world();
                // `PlayerList.op`: the operator permissions of the player list entry (level 4) and the server's op list, which
                // `PlayerList.isOp` and the spawn protection read
                self.with_player_named(&a[1], |level, p| {
                    use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
                    p.handle
                        .as_ref()
                        .expect("player list entry")
                        .shared
                        .lock()
                        .permissions = LevelBasedPermissionSet::for_level(PermissionLevel::Owners);
                    let ops = &mut level.server_state.spawn_protection;
                    ops.ops_empty = false;
                    if !ops.op_players.contains(&p.uuid()) {
                        ops.op_players.push(p.uuid());
                    }
                });
            }
            // `pick_block PLAYER X Y Z [true|false]` (include block data)
            "pick_block" => deliver(self, &a[1], |_, _| {
                P::PickItemFromBlock(ServerboundPickItemFromBlockPacket {
                    pos: BlockPos::new(
                        a[2].parse().unwrap(),
                        a[3].parse().unwrap(),
                        a[4].parse().unwrap(),
                    ),
                    include_data: a.get(5).is_some_and(|v| v == "true"),
                })
            }),
            "jigsaw_generate" => {
                self.world();
                self.load_registries();
                // `handleJigsawGenerate` through the registered play handlers; it needs the command environment (`command_env`)
                let packet = mc_protocol::packets::play::ServerboundJigsawGeneratePacket {
                    pos: BlockPos::new(
                        a[2].parse().unwrap(),
                        a[3].parse().unwrap(),
                        a[4].parse().unwrap(),
                    ),
                    levels: a[5].parse().unwrap(),
                    keep_jigsaws: a[6].parse().unwrap(),
                };
                self.with_player_named(&a[1], |level, player| {
                    mc_game::commands::server_access::with_server(|server| {
                        let mut ctx = mc_game::server::network::game_packets::PlayCtx {
                            server,
                            level,
                            player,
                        };
                        for h in mc_game::server::network::game_packets::hooks::PLAY_HANDLERS.iter()
                        {
                            h.handle_jigsaw_generate(&mut ctx, &packet);
                        }
                    })
                });
            }
            "nbt" => {
                self.world();
                let id = self.entity_id(&a[1]);
                let snbt = self.line[self.line.find(&a[2]).expect("snbt token")..].to_string();
                let extra = mc_nbt::snbt::parse_compound(&snbt)
                    .unwrap_or_else(|e| panic!("bad SNBT {snbt}: {e}"));
                self.level().with_entity(id, |_, e| {
                    let mut tag = mc_nbt::CompoundTag::new();
                    e.save_without_id(&mut tag, &|_| None);
                    tag.merge(&extra);
                    e.load(&tag)
                        .unwrap_or_else(|er| panic!("entity load: {er}"));
                });
            }
            "hurt" => {
                self.world();
                self.load_registries();
                self.hurt(a);
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
                    if let Some(p) = e.as_any_mut().downcast_mut::<ServerPlayer>() {
                        // the driver calls `teleportTo(level, x, y, z, Set.of(), yRot, xRot, false)`: a client teleport that the client
                        // has to confirm (`awaitingPositionFromClient`), which also wakes the player
                        if p.is_sleeping() {
                            p.stop_sleep_in_bed(level, true, true);
                        }
                        let rot = |i: usize, old: f32| a.get(i).map_or(old, |v| num(v) as f32);
                        let (y_rot, x_rot) = (rot(5, yr), rot(6, xr));
                        p.connection_teleport(num(&a[2]), num(&a[3]), num(&a[4]), y_rot, x_rot);
                        p.set_y_head_rot(y_rot);
                        if let Some(l) = p.listener.as_mut() {
                            l.reset_flying_ticks();
                        }
                        return;
                    }
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
            // effect REF EFFECT AMPLIFIER TICKS [hidden]      addEffect; prints `EFFECT REF EFFECT added=B`
            "effect" => {
                self.world();
                self.load_registries();
                let effect =
                    mc_data::MobEffect::by_name(&a[2]).unwrap_or_else(|| panic!("effect {}", a[2]));
                let hidden = a.iter().any(|t| t == "hidden");
                let instance = mc_game::world::entity::effect::MobEffectInstance::new(
                    effect,
                    a[4].parse().expect("effect ticks"),
                    a[3].parse().expect("effect amplifier"),
                    false,
                    !hidden,
                    !hidden,
                    None,
                );
                let id = self.entity_id(&a[1]);
                let added = self
                    .level()
                    .with_entity(id, |level, e| e.add_effect(level, instance, None))
                    .unwrap_or_else(|| panic!("entity {} is gone", a[1]));
                self.out
                    .push(format!("EFFECT {} {} added={added}", a[1], a[2]));
            }
            // mob_attack MOB REF        Mob.doHurtTarget(level, REF); prints `MOBATTACK mob ref -> result`
            "mob_attack" => {
                self.world();
                self.load_registries();
                let (mob, target) = (self.entity_id(&a[1]), self.entity_id(&a[2]));
                let result = self
                    .level()
                    .with_entity(mob, |level, m| {
                        level
                            .with_entity(target, |level, t| m.do_hurt_target(level, t))
                            .unwrap_or_else(|| panic!("entity {} is gone", a[2]))
                    })
                    .unwrap_or_else(|| panic!("entity {} is gone", a[1]));
                self.out
                    .push(format!("MOBATTACK {} {} -> {result}", a[1], a[2]));
            }
            "mobspawn" => {
                self.world();
                self.load_registries();
                reseed_for(&a[1]);
                self.mob_spawn(a);
            }
            // the listener's packet handlers (`deliver`): the same packets as the Java verbs
            "client_tick_end" => deliver(self, &a[1], |_, _| {
                P::ClientTickEnd(ServerboundClientTickEndPacket)
            }),
            // move_packet PLAYER X Y Z ONGROUND [yaw pitch]   ServerboundMovePlayerPacket (the server-authoritative path)
            "move_packet" => deliver(self, &a[1], |_, _| {
                let (x, y, z) = (num(&a[2]), num(&a[3]), num(&a[4]));
                let on_ground = a[5] == "true";
                if a.len() > 7 {
                    P::MovePlayerPosRot(ServerboundMovePlayerPosRotPacket {
                        x,
                        y,
                        z,
                        y_rot: num(&a[6]) as f32,
                        x_rot: num(&a[7]) as f32,
                        on_ground,
                        horizontal_collision: false,
                    })
                } else {
                    P::MovePlayerPos(ServerboundMovePlayerPosPacket {
                        x,
                        y,
                        z,
                        on_ground,
                        horizontal_collision: false,
                    })
                }
            }),
            // move_vehicle PLAYER X Y Z YROT XROT ONGROUND   ServerboundMoveVehiclePacket of the vehicle the player controls
            "move_vehicle" => deliver(self, &a[1], |_, _| {
                P::MoveVehicle(ServerboundMoveVehiclePacket {
                    moving_to: PositionAndRotation {
                        position: Vec3::new(num(&a[2]), num(&a[3]), num(&a[4])),
                        y_rot: num(&a[5]) as f32,
                        x_rot: num(&a[6]) as f32,
                    },
                    on_ground: a[7] == "true",
                })
            }),
            // accept_teleport PLAYER   ServerboundAcceptTeleportationPacket for the teleport the server waits on (nothing when none is pending)
            "accept_teleport" => {
                let pending = self.with_player_named(&a[1], |_, p| {
                    let (id, at) = p.listener.as_ref()?.pending_teleport()?;
                    Some(ServerboundAcceptTeleportationPacket {
                        id,
                        x: at.x,
                        y: at.y,
                        z: at.z,
                        y_rot: p.get_y_rot(),
                        x_rot: p.get_x_rot(),
                    })
                });
                if let Some(packet) = pending {
                    deliver(self, &a[1], |_, _| P::AcceptTeleportation(packet));
                }
            }
            // paddle PLAYER LEFT RIGHT   ServerboundPaddleBoatPacket (true|false)
            "paddle" => deliver(self, &a[1], |_, _| {
                P::PaddleBoat(ServerboundPaddleBoatPacket {
                    left: a[2] == "true",
                    right: a[3] == "true",
                })
            }),
            // respawn PLAYER   ServerboundClientCommandPacket(PERFORM_RESPAWN) of a dead player
            "respawn" => {
                self.world();
                self.load_registries();
                self.respawn(&a[1]);
            }
            "attribute" => {
                self.world();
                let id = self.entity_id(&a[1]);
                let attribute = mc_data::Attribute::by_name(&a[2])
                    .unwrap_or_else(|| panic!("attribute {}", a[2]));
                self.level().with_entity(id, |_, e| {
                    e.living_mut()
                        .expect("attribute on a living entity")
                        .attributes
                        .get_instance(attribute)
                        .unwrap_or_else(|| panic!("entity has no attribute {attribute:?}"))
                        .set_base_value(num(&a[3]));
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
                    "health" => e
                        .living_mut()
                        .expect("living entity")
                        .set_health(v.parse().unwrap()),
                    "xp_level" | "enchant_seed" => {
                        let player = e
                            .as_any_mut()
                            .downcast_mut::<ServerPlayer>()
                            .expect("player");
                        if a[2] == "xp_level" {
                            player.set_experience_levels(v.parse().unwrap());
                        } else {
                            player.player.enchantment_seed = v.parse().unwrap();
                        }
                    }
                    "food" | "saturation" | "exhaustion" => {
                        let food = &mut e
                            .as_any_mut()
                            .downcast_mut::<ServerPlayer>()
                            .expect("player")
                            .player
                            .food_data;
                        match a[2].as_str() {
                            "food" => food.set_food_level(v.parse().unwrap()),
                            "saturation" => food.set_saturation(v.parse().unwrap()),
                            _ => food.exhaustion_level = v.parse().unwrap(),
                        }
                    }
                    other => panic!("set {other} is not supported by the Rust replay"),
                });
            }
            "spawn_player" => {
                self.world();
                self.load_registries();
                reseed_for(&a[1]);
                self.spawn_player(a);
            }
            // the listener's packet handlers (`deliver`): the same packets as the Java verbs
            "menu_button" => deliver(self, &a[1], |_, p| {
                P::ContainerButtonClick(ServerboundContainerButtonClickPacket {
                    container_id: p.player.menus.current_container_id(),
                    button_id: a[2].parse().unwrap(),
                })
            }),
            "close_menu" => deliver(self, &a[1], |_, p| {
                P::ContainerClose(ServerboundContainerClosePacket {
                    container_id: p.player.menus.current_container_id(),
                })
            }),
            "select" => deliver(self, &a[1], |_, _| {
                P::SetCarriedItem(ServerboundSetCarriedItemPacket {
                    slot: a[2].parse().expect("hotbar slot"),
                })
            }),
            "brush" => {
                self.world();
                let pos = BlockPos::new(
                    a[1].parse().unwrap(),
                    a[2].parse().unwrap(),
                    a[3].parse().unwrap(),
                );
                let direction =
                    Direction::by_name(&a[5]).unwrap_or_else(|| panic!("direction {}", a[5]));
                let result = self.with_player_named(&a[4], |level, p| {
                    let brush = p.get_item_in_hand(InteractionHand::MainHand);
                    mc_game::world::block_entity::brush_block(level, pos, p, direction, &brush)
                });
                self.out
                    .push(format!("BRUSH {} {} {} {result}", a[1], a[2], a[3]));
            }
            "sign_update" => {
                self.world();
                let pos = BlockPos::new(
                    a[2].parse().unwrap(),
                    a[3].parse().unwrap(),
                    a[4].parse().unwrap(),
                );
                let slot = if a[5] == "front" {
                    SignTextSlot::Front
                } else {
                    SignTextSlot::Back
                };
                let lines: Vec<String> = (6..10)
                    .map(|i| {
                        a.get(i).map_or("", |l| {
                            l.strip_prefix('"')
                                .and_then(|l| l.strip_suffix('"'))
                                .unwrap_or(l)
                        })
                    })
                    .map(mc_text::ChatFormatting::strip_formatting)
                    .collect();
                self.with_player_named(&a[1], |level, p| {
                    update_sign_text_from_packet(level, p, pos, slot, &lines)
                });
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
            _ => return crate::common::player_actions::command(self, a),
        }
        true
    }

    /// `/recipe give|take PLAYER RECIPE|*`: `awardRecipes` / `resetRecipes` over the one recipe or every learnable recipe.
    pub(super) fn recipe_command(&mut self, a: &[String]) {
        self.load_registries();
        let level_recipes = self.level().recipe_access();
        let holders: Vec<_> = if a[4] == "*" {
            level_recipes.learnable_recipes().to_vec()
        } else {
            let id = Identifier::parse(&a[4]).unwrap_or_else(|e| panic!("recipe {}: {e:?}", a[4]));
            vec![
                level_recipes
                    .by_key(&id)
                    .unwrap_or_else(|| panic!("unknown recipe {id}"))
                    .clone(),
            ]
        };
        self.with_player_named(&a[3], |level, p| {
            if a[2] == "give" {
                p.award_recipes(level, &holders);
            } else {
                p.reset_recipes(level, &holders);
            }
        });
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
        self.spawn_with(a, mc_data::enums::EntitySpawnReason::Command, false);
    }

    /// `mobspawn REF TYPE X Y Z [yaw pitch] [reason=NAME] [nofinalize] [SNBT]`: the entity is created with that spawn reason and,
    /// unless `nofinalize`, `Mob.finalizeSpawn` runs before it joins the level (`/summon`).
    fn mob_spawn(&mut self, a: &[String]) {
        let mut reason = mc_data::enums::EntitySpawnReason::Command;
        let mut finalize = true;
        let mut rest = Vec::new();
        for t in a {
            if let Some(r) = t.strip_prefix("reason=") {
                reason = crate::common::entity_scenario::spawn_reason(r)
                    .unwrap_or_else(|| panic!("spawn reason {r}"));
            } else if t == "nofinalize" {
                finalize = false;
            } else {
                rest.push(t.clone());
            }
        }
        self.spawn_with(&rest, reason, finalize);
    }

    fn spawn_with(
        &mut self,
        a: &[String],
        reason: mc_data::enums::EntitySpawnReason,
        finalize: bool,
    ) {
        let ty = EntityType::by_name(&if a[2].contains(':') {
            a[2].clone()
        } else {
            format!("minecraft:{}", a[2])
        })
        .unwrap_or_else(|| panic!("entity type {}", a[2]));
        let request = EntitySpawnRequest {
            reason,
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
        if finalize {
            for hook in mc_game::world::entity::scenario_dump::SCENARIO_FINALIZE_SPAWN.iter() {
                hook(level, e.as_mut(), reason);
            }
        }
        let id = e.id();
        level.add_fresh_entity_with_passengers(vec![e]);
        self.actors.known.insert(id);
        self.actors.refs.push((a[1].clone(), id));
    }

    fn spawn_player(&mut self, a: &[String]) {
        let name = a[1].clone();
        let flags = &a[6..];
        let sim = flags.iter().any(|f| f == "sim");
        let joinburst = flags.iter().any(|f| f == "joinburst");
        let n = a.len() - usize::from(sim) - usize::from(joinburst);
        let level = self.level();
        let id = level.next_entity_id();
        let profile = mc_game::server::network::login_listener::create_offline_profile(&name);
        let mut p = ServerPlayer::new(
            id,
            Identifier::parse("minecraft:overworld").unwrap(),
            profile.clone(),
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
        p.advancements = mc_game::advancements::SharedAdvancements::new(
            mc_game::advancements::player_advancements::PlayerAdvancements::new(
                &ADVANCEMENTS,
                None,
            ),
        );
        let (yaw, pitch) = if n > 7 {
            (num(&a[6]) as f32, num(&a[7]) as f32)
        } else {
            (0.0, 0.0)
        };
        p.snap_to(num(&a[3]), num(&a[4]), num(&a[5]), yaw, pitch);
        p.base_mut().first_tick = true;
        p.simulate_local_player = sim;
        let (tx, rx) = unbounded_channel();
        let sender = ConnectionSender::new(tx);
        p.connection = Some(sender.clone());
        // the player list's roster: `PlayerList.broadcastSystemMessage` (advancement announcements) reaches the players
        let handle = mc_game::server::players::PlayerHandle::new(
            id,
            profile.clone(),
            sender.clone(),
            "<unknown>".to_owned(),
            level.dimension.clone(),
        );
        level.server_state.roster.add(handle.clone());
        // `PlayerList.placeNewPlayer`: the packet listener (teleports, block change acks), the join teleport and the client's
        // "loaded" report (`VanillaLevel.connect`: the driver accepts the teleport and sends `PlayerLoaded`)
        let (inbound_tx, inbound_rx) = std::sync::mpsc::channel();
        let mut listener = ServerGamePacketListenerImpl::new(
            sender.clone(),
            inbound_rx,
            &CommonListenerCookie {
                profile,
                latency: 0,
                client_information: Default::default(),
                transferred: false,
            },
            handle.chat_out.clone(),
            ListenerSettings {
                command_spam_threshold_seconds: 10,
                chat_spam_threshold_seconds: 10,
                memory_connection: false,
                enforce_secure_profile: std::sync::Arc::new(|| false),
            },
            0,
        );
        for _ in 0..CLIENT_LOADED_TIMEOUT_TIME {
            listener.tick_client_load_timeout();
        }
        p.listener = Some(listener);
        p.handle = Some(handle);
        p.connection_teleport(num(&a[3]), num(&a[4]), num(&a[5]), yaw, pitch);
        if let Some(l) = p.listener.as_mut() {
            l.confirm_teleport();
        }
        // the accepted teleport goes through `handlePlayerPositionChange`, which wraps the client's rotation
        let at = p.position();
        p.abs_snap_to(
            at.x,
            at.y,
            at.z,
            mc_core::math::mth::wrap_degrees_f32(yaw),
            mc_core::math::mth::wrap_degrees_f32(pitch),
        );
        // `PlayerList.placeNewPlayer` broadcasts the join message before the new player joins the list: only the players
        // already in the level get it
        let joined =
            mc_game::server::players::player_list::join_message(p.get_display_name(), None);
        for other in self.actors.players.clone() {
            self.with_player_named(&other, |_, o| {
                o.send(&mc_protocol::packets::play::ClientboundSystemChatPacket {
                    content: joined.clone(),
                    overlay: false,
                })
            });
        }
        let level = self.level();
        // `PlayerList.placeNewPlayer` registers the player with the chunk map and the entity tracker (`add_player` does, through
        // the listener's chunk sender: the spawning chunks it creates are shuffled with the level random on every tick, which
        // every later spawner draw depends on, and the batches it sends decide which chunks the player tracks entities in)
        level.add_player(Box::new(p));
        level.with_player(id, |level, p| {
            if joinburst {
                // `placeNewPlayer`: the held slot, the recipe properties and the recipe book come before the inventory menu
                p.send(&mc_protocol::packets::play::ClientboundSetHeldSlotPacket {
                    slot: p.player.inventory.get_selected_slot(),
                });
                let recipes = level.recipe_access();
                p.send(&mc_game::world::item::crafting::update_recipes_packet(
                    &recipes,
                ));
                mc_game::server::players::join::send_initial_recipe_book(p, &recipes);
            }
            p.init_inventory_menu(level)
        });
        // a non-simulated player has no movement packets: onGround is derived once (resting on a block)
        if !sim {
            level.with_player(id, |level, p| {
                let bb = p.base().bounding_box().move_(0.0, -1.0E-3, 0.0);
                let free = level.no_collision_aabb(&bb);
                p.base_mut().set_on_ground_raw(!free);
            });
        }
        self.actors.known.insert(id);
        self.actors.refs.push((name.clone(), id));
        self.actors.players.push(name.clone());
        self.actors._inbound.push(inbound_tx);
        // the join burst is only compared with `joinburst`: otherwise `packets:NAME` dumps start after the join
        let mut rx = rx;
        while !joinburst && rx.try_recv().is_ok() {}
        self.actors.rx.insert(name, rx);
    }

    /// `handleClientCommand(PERFORM_RESPAWN)` -> `PlayerList.respawn(player, false)` -> the client reports the new level loaded. The
    /// new player keeps the id, the listener and the connection of the dead one; a living player ignores the request.
    fn respawn(&mut self, name: &str) {
        use mc_game::server::players::respawn::{
            create_respawned_player, find_spawn_position, take_old_player,
        };
        use mc_game::server::players::teleport_transition::{
            PostTeleportTransition, TeleportTransition,
        };
        let id = self.entity_id(name);
        let level = self.level();
        if level
            .with_player(id, |_, p| p.get_health() > 0.0)
            .unwrap_or_else(|| panic!("no player {name}"))
        {
            return;
        }
        let keep_inventory = level.keep_inventory();
        let old = take_old_player(level, id, mc_game::world::entity::RemovalReason::Killed)
            .expect("player");
        let position = find_spawn_position(level, false, 10, BlockPos::new(0, -60, 0));
        let transition = TeleportTransition::new(
            old.dimension.clone(),
            position,
            Vec3::ZERO,
            0.0,
            0.0,
            PostTeleportTransition::DO_NOTHING,
        );
        let mut new = create_respawned_player(old, level, &transition, false, keep_inventory);
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
        let pos = new.position();
        if let Some(listener) = new.listener.as_mut() {
            listener.reset_position(pos);
            listener.restart_client_load_timer_after_respawn();
        }
        level.add_player(Box::new(new));
        level.with_player(id, |level, p| p.init_inventory_menu(level));
        // a real client reports the new level loaded (`ServerboundPlayerLoadedPacket`)
        deliver(self, name, |_, _| {
            P::PlayerLoaded(mc_protocol::packets::play::ServerboundPlayerLoadedPacket)
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
        // `handlePlayerAction` of a block action: the break handler, then `ackBlockChangesUpTo(sequence)`
        let sequence = self.actors.sequences.next(&name);
        let creative = self.with_player_named(&name, |level, p| {
            p.handle_block_break_action(
                level,
                pos,
                Action::StartDestroyBlock,
                Direction::Up,
                max_y,
                false,
            );
            crate::common::player_actions::acknowledge(p, sequence);
            p.is_creative()
        });
        // the client decides on the state the START packet left behind (air after an instant break: it then ticks once)
        let st = self.level().get_block_state(pos);
        let instant = self.with_player_named(&name, |_, p| p.get_destroy_progress(st) >= 1.0);
        let (mut progress, mut ticks) = (0f32, 0);
        let stopped = creative || (!st.is_air() && instant);
        if !stopped {
            loop {
                ticks += 1;
                let st = self.level().get_block_state(pos);
                let step = self.with_player_named(&name, |_, p| p.get_destroy_progress(st));
                progress += step;
                if if limit >= 0 {
                    ticks >= limit
                } else {
                    progress >= 1.0
                } {
                    let sequence = self.actors.sequences.next(&name);
                    self.with_player_named(&name, |level, p| {
                        p.handle_block_break_action(
                            level,
                            pos,
                            Action::StopDestroyBlock,
                            Direction::Up,
                            max_y,
                            false,
                        );
                        crate::common::player_actions::acknowledge(p, sequence);
                    });
                    self.tick_once();
                    break;
                }
                self.tick_once();
                assert!(ticks < 100_000, "dig never finishes");
            }
        }
        let broken = self.level().get_block_state(pos).is_air();
        self.out
            .push(format!("DIG {name} ticks={ticks} broken={broken}"));
    }

    fn explode(&mut self, a: &[String]) {
        use mc_game::world::level::explosion::ExplosionInteraction as I;
        let mut inter = I::None;
        let mut fire = false;
        let mut source = None;
        for t in &a[5..] {
            match t.as_str() {
                "fire" => fire = true,
                t if t.starts_with("source=") => source = Some(t[7..].to_owned()),
                "none" => inter = I::None,
                "block" => inter = I::Block,
                "mob" => inter = I::Mob,
                "tnt" => inter = I::Tnt,
                "trigger" => inter = I::Trigger,
                other => panic!("explode option {other} is not supported by the Rust replay"),
            }
        }
        let (x, y, z, radius) = (num(&a[1]), num(&a[2]), num(&a[3]), num(&a[4]) as f32);
        let Some(source) = source else {
            let params = Explode::new(None, x, y, z, radius, inter).with_fire(fire);
            self.level().explode(params, &mut []);
            return;
        };
        // the source entity is out of the level's storage while it explodes, like a primed TNT's own tick
        let id = self.entity_id(&source);
        self.level().with_entity(id, |level, e| {
            let params = Explode::new(Some(&*e), x, y, z, radius, inter).with_fire(fire);
            level.explode(params, &mut []);
        });
    }

    /// `MinecraftServer.tickChildren`: every 20th server tick (the tick counter is the replayed tick number, as in the oracle, whose
    /// server ticks only through `VanillaLevel.tick`) all players get the game time, before the level ticks.
    pub(super) fn force_game_time_synchronization(&mut self) {
        if (self.ticks + 1) % 20 != 0 {
            return;
        }
        for name in self.actors.players.clone() {
            self.with_player_named(&name, |level, p| {
                p.send(&ClientboundSetTimePacket {
                    game_time: level.game_time(),
                    clock_updates: Vec::new(),
                });
            });
        }
    }

    /// `VanillaLevel.tick`: before the server ticks, a simulated player applies its keyboard input (`SimPlayer.applyInput`).
    pub(super) fn apply_player_inputs(&mut self) {
        for name in self.actors.players.clone() {
            let id = self.entity_id(&name);
            let mut tracked = self.actors.tracked_movement.get(&id).copied();
            self.with_player_named(&name, |_, p| {
                if p.simulate_local_player {
                    if let Some(last) = tracked.as_mut() {
                        let pos = p.position();
                        p.last_known_client_movement = last.map_or(Vec3::ZERO, |l| pos.subtract(l));
                        *last = Some(pos);
                    }
                    p.apply_input();
                }
            });
            if let Some(t) = tracked {
                self.actors.tracked_movement.insert(id, t);
            }
        }
    }

    /// The player half of a tick, after `ServerLevel::tick` (which ticked the player entity): `ServerGamePacketListenerImpl.tick` -> `doTick`.
    pub(super) fn tick_players(&mut self) {
        for name in self.actors.players.clone() {
            // `tickPlayer`: `resetPosition`, `xo/yo/zo`, `doTick`, then `absSnapTo(firstGood)`: a client-authoritative player
            // keeps the position it had, whatever the server-side tick did to it
            self.with_player_named(&name, |level, p| {
                if let Some(listener) = p.listener.as_mut() {
                    listener.send_block_changed_ack();
                    listener.tick_throttlers();
                }
                let pos = p.position();
                p.player.living.entity.set_old_pos(pos);
                p.do_tick(level);
                let (y_rot, x_rot) = (p.get_y_rot(), p.get_x_rot());
                // a simulated player keeps the position its own physics produced (`SimPlayer.absSnapTo`)
                if !p.simulate_local_player {
                    p.abs_snap_to(pos.x, pos.y, pos.z, y_rot, x_rot);
                }
                // `tickPlayer`'s vehicle tracking (the vehicle a `move_vehicle` packet is checked against)
                if mc_game::commands::server_access::has_server() {
                    mc_game::commands::server_access::with_server(|server| {
                        mc_game::server::network::game_packets::PlayCtx {
                            server,
                            level,
                            player: p,
                        }
                        .tick_vehicle_tracking();
                    });
                }
            });
        }
    }

    /// `MinecraftServer.tickChildren` "send chunks", after the players ticked: every player's `chunkSender.sendNextChunks(player)`. The
    /// oracle's connection never acknowledges a batch, so a player gets one batch of chunks and tracks entities in those only.
    pub(super) fn send_chunks(&mut self) {
        for name in self.actors.players.clone() {
            self.with_player_named(&name, |level, p| {
                let sender = p
                    .listener
                    .as_ref()
                    .expect("player listener")
                    .chunk_sender
                    .clone();
                sender.send_next_chunks(
                    Entity::chunk_position(&*p),
                    &mut mc_game::server::network::player_chunk_sender::LevelChunkSource {
                        level: &*level,
                    },
                );
            });
        }
    }

    // ------------------------------------------------------------------------------------------------ dumps

    pub(super) fn dump_actor(&mut self, kind: &str, arg: Option<&str>) -> bool {
        match kind {
            "entities" | "xentities" => {
                self.name_new_entities();
                let mut all: Vec<(String, EntityId)> = self.actors.refs.clone();
                all.retain(|(n, id)| {
                    !self.actors.players.contains(n)
                        && self.level().get_entity(*id).is_some_and(|e| e.is_alive())
                });
                all.sort_by_key(|(_, id)| *id);
                for (name, id) in all {
                    let line = self.describe(&name, id, kind == "xentities");
                    self.out.push(line);
                }
            }
            "entity" | "player" | "xentity" | "mob" => {
                let name = arg.expect("entity:REF");
                let id = self.entity_id(name);
                let line = self.describe(name, id, matches!(kind, "xentity" | "mob"));
                self.out.push(line);
            }
            // ED REF id:serializer:hex ..
            "edata" => {
                let name = arg.expect("edata:REF");
                let id = self.entity_id(name);
                let level = self.level();
                let e = level
                    .get_entity(id)
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                let line = super::super::entity_describe::entity_data_line(name, e);
                self.out.push(line);
            }
            // BRAIN REF .. / BM / BR lines (`mc_game::world::entity::scenario_dump`, shared with the entity replay)
            "brain" => {
                use mc_game::world::entity::scenario_dump as sd;
                let name = arg.expect("brain:REF");
                let id = self.entity_id(name);
                let names: std::collections::BTreeMap<EntityId, String> = self
                    .actors
                    .refs
                    .iter()
                    .map(|(n, id)| (*id, n.clone()))
                    .collect();
                let uuids = std::collections::BTreeMap::new();
                let ctx = sd::ScenarioDumpCtx {
                    names: &names,
                    uuids: &uuids,
                };
                let level = self.level();
                let e = level
                    .get_entity(id)
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                let lines = sd::brain_lines(name, e, level, &ctx);
                self.out.extend(lines);
            }
            // N REF {canonical sorted SNBT of saveWithoutId, UUID removed}
            "nbt" => {
                let name = arg.expect("nbt:REF");
                let id = self.entity_id(name);
                let level = self.level();
                let e = level
                    .get_entity(id)
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                let mut tag = mc_nbt::CompoundTag::new();
                e.save_without_id(&mut tag, &|i| level.get_entity(i));
                tag.remove("UUID");
                let line = format!(
                    "N {name} {}",
                    super::super::entity_scenario::canon(&mc_nbt::Tag::Compound(tag))
                );
                self.out.push(line);
            }
            "advancements" => {
                let name = arg.expect("advancements:PLAYER");
                let mut lines: Vec<(String, String)> = Vec::new();
                self.with_player_named(name, |_, p| {
                    let progress = p.advancements.lock();
                    for holder in ADVANCEMENTS.get_all_advancements() {
                        let Some(progress) = progress.progress_of(holder) else {
                            continue;
                        };
                        let mut done = progress.get_completed_criteria();
                        if done.is_empty() {
                            continue;
                        }
                        done.sort();
                        lines.push((holder.id.to_string(), done.join(",")));
                    }
                });
                lines.sort();
                for (id, criteria) in lines {
                    self.out.push(format!("ADV {name} {id} {criteria}"));
                }
            }
            "offers" => {
                let name = arg.expect("offers:REF");
                let id = self.entity_id(name);
                let offers = self
                    .level()
                    .with_entity(id, |level, e| {
                        e.as_merchant_mut()
                            .expect("merchant")
                            .get_offers(level)
                            .clone()
                    })
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                self.out
                    .extend(super::super::entity_scenario::offer_lines(name, &offers));
            }
            // TRADER REF xp=N level=N|- trading=REF|-
            "trader" => {
                let name = arg.expect("trader:REF");
                let id = self.entity_id(name);
                let (xp, villager_level, trading) = self
                    .level()
                    .with_entity(id, |_, e| {
                        let m = e.as_merchant().expect("merchant");
                        (
                            m.get_villager_xp(),
                            mc_game::world::entity::npc::villager::villager_ref(e)
                                .map(|v| v.get_villager_data().level),
                            m.get_trading_player(),
                        )
                    })
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                let trading = trading.map_or("-".to_owned(), |t| {
                    self.actors
                        .refs
                        .iter()
                        .find(|(_, id)| *id == t)
                        .map_or("?".to_owned(), |(n, _)| n.clone())
                });
                self.out.push(format!(
                    "TRADER {name} xp={xp} level={} trading={trading}",
                    villager_level.map_or("-".to_owned(), |l| l.to_string())
                ));
            }
            "all_players" => {
                for name in self.actors.players.clone() {
                    let id = self.entity_id(&name);
                    let line = self.describe(&name, id, false);
                    self.out.push(line);
                }
            }
            "packets" => {
                let arg = arg.expect("packets:NAME[:type,..]");
                let (name, only) = arg
                    .split_once(':')
                    .map_or((arg, None), |(n, f)| (n, Some(f)));
                let except = only.is_some_and(|f| f.starts_with('!'));
                let only: Option<Vec<&str>> =
                    only.map(|f| f.trim_start_matches('!').split(',').collect());
                let rx = self
                    .actors
                    .rx
                    .get_mut(name)
                    .unwrap_or_else(|| panic!("no player {name}"));
                let mut sent: Vec<(&'static str, Vec<u8>)> = Vec::new();
                while let Ok(msg) = rx.try_recv() {
                    let p: Vec<u8> = match msg {
                        Outbound::Payload(p) | Outbound::Write(p) => p.to_vec(),
                        // the listener's `disconnect(reason)`: `send(ClientboundDisconnectPacket)`, then the connection closes
                        Outbound::Disconnect(reason) => mc_protocol::packet::encode_packet(
                            &mc_protocol::packets::play::ClientboundDisconnectPacket { reason },
                        )
                        .expect("disconnect reason encodes"),
                        _ => continue,
                    };
                    let mut body: &[u8] = &p;
                    let id = read_varint(&mut body).expect("packet id");
                    let ty = mc_protocol::packet::packet_name(
                        mc_protocol::packet::ConnectionState::Play,
                        mc_protocol::packet::PacketFlow::Clientbound,
                        id,
                    )
                    .unwrap_or("?");
                    sent.push((ty, p));
                }
                for (ty, p) in chunk_order(sent) {
                    if only
                        .as_ref()
                        .is_none_or(|o| o.contains(&ty.trim_start_matches("minecraft:")) != except)
                    {
                        let hex = packet_text(ty, &p);
                        self.out.push(format!("PKT {name} {ty} {hex}"));
                    }
                }
            }
            _ => return crate::common::player_actions::dump(self, kind, arg),
        }
        true
    }

    /// The `P`/`E` line of the driver's `describe` (`rich`: with the fields of the registered classes, `xentity:REF`).
    fn describe(&mut self, name: &str, id: EntityId, rich: bool) -> String {
        let refs = self.actors.refs.clone();
        let level = self.level();
        let names: std::collections::BTreeMap<EntityId, String> =
            refs.iter().map(|(n, id)| (*id, n.clone())).collect();
        // the driver's `uuidName`: a known entity prints as its scenario name
        let uuids: std::collections::BTreeMap<uuid::Uuid, EntityId> = refs
            .iter()
            .filter_map(|(_, id)| level.get_entity(*id).map(|e| (e.uuid(), *id)))
            .collect();
        let ctx = mc_game::world::entity::scenario_dump::ScenarioDumpCtx {
            names: &names,
            uuids: &uuids,
        };
        let e = level
            .get_entity(id)
            .unwrap_or_else(|| panic!("entity {name} is gone"));
        super::super::entity_describe::describe(name, e, level, &ctx, rich)
    }
}

impl crate::common::player_actions::ActionHost for Scenario {
    fn boot(&mut self) {
        self.world();
    }
    fn action_level(&mut self) -> &mut ServerLevel {
        self.level()
    }
    fn action_ref(&self, name: &str) -> EntityId {
        self.entity_id(name)
    }
    fn next_sequence(&mut self, player: &str) -> i32 {
        self.actors.sequences.next(player)
    }
    fn emit(&mut self, line: String) {
        self.out.push(line);
    }
}
