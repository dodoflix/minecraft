//! Entities and players of the block scenario replay (`spawn`, `spawn_item`, `entity_seed`, `kill`, `nbt`, `hurt`, `input` (sneak: the shift key), `op`, `pick_block`, `tp`, `snap`, `velocity`, `set`,
//! `spawn_player`, `give`, `select`, `use_on`, `use`, `sign_update`, `dig`, `explode`, `pcommand` (sprinting, stop_sleeping), `dump entities|entity:REF|player:NAME|inventory:NAME|packets:NAME`).
//! Included by `block_scenario.rs` (private state of `Scenario`). Differences from the Java driver, all caused by the Rust level having no
//! server: players act through `ServerPlayer::{use_item_on, use_item, handle_block_break_action}` and the replay re-does what the packet
//! handlers around them add (the listener's sequence number and block change ack, the swing and block updates sent back), but no
//! serverbound packet is decoded and the build-limit messages are not replayed. Entity random seeds come from `entity_seed REF N` (Java draws them from a per-thread counter that Rust cannot
//! reproduce: always call `entity_seed` right after `spawn`/`spawn_item` when the entity's randomness matters, and give `spawn_item` an
//! explicit velocity), `explode` takes `source=REF`, `attack/swing/drop/respawn/effect/attribute/move_packet` are not replayed.
use super::Scenario;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::LegacyRandomSource;
use mc_data::enums::GameType;
use mc_data::{EntityType, Item};
use mc_game::server::level::chunk_send_sink::ChunkSendSink;
use mc_game::server::level::level_explode::Explode;
use mc_game::server::level::{PlayerTrackingInfo, ServerLevel};
use mc_game::server::network::common_listener::CommonListenerCookie;
use mc_game::server::network::game_listener::{
    CLIENT_LOADED_TIMEOUT_TIME, ListenerSettings, ServerGamePacketListenerImpl,
};
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::world::block_entity::sign_block_entity::{SignTextSlot, update_sign_text_from_packet};
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::player::menu_ctx::PlayerCtx;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::inventory::{PlayerMenus, handle_container_button_click};
use mc_game::world::item::{InteractionHand, InteractionResult, ItemStack, SwingSource};
use mc_game::world::level::BlockGetter;
use mc_game::world::level::server_level_accessor::ServerLevelAccessor;
use mc_protocol::codec::{Decode, read_varint};
use mc_protocol::packets::play::{
    AttributeModifierOperation, ClientboundBlockUpdatePacket, ClientboundSetTimePacket,
    ClientboundUpdateAttributesPacket,
};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// The chunk sink of a player that acknowledges every chunk at once.
struct AcknowledgedChunks;

impl ChunkSendSink for AcknowledgedChunks {
    fn mark_chunk_pending_to_send(&mut self, _pos: ChunkPos) {}
    fn drop_chunk(&mut self, _pos: ChunkPos, _alive: bool) {}
    fn is_pending(&self, _pos: i64) -> bool {
        false
    }
}

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
    /// The driver's `seq(player)`: the sequence number of the player's next `use_on` packet.
    sequence: HashMap<String, i32>,
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
/// Java (identity hash order), so they are dumped sorted by attribute and modifier id; anything else is its hex wire form.
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

    /// The driver's `stack(spec, count)`: an item id, or an item with components (`minecraft:iron_pickaxe[enchantments={...}]`).
    fn item_stack(&self, spec: &str, count: i32) -> ItemStack {
        if spec.contains('[') {
            use mc_game::commands::CommandBuildContext;
            use mc_game::commands::command_build_context::StaticRegistries;
            let context = CommandBuildContext::simple(
                std::sync::Arc::new(StaticRegistries),
                mc_world::level_storage::FeatureFlagSet::ALL,
            );
            let mut reader = mc_brigadier::StringReader::new(spec);
            return mc_game::commands::arguments::item::item_parser::ItemParser::new(&context)
                .parse(&mut reader)
                .and_then(|input| input.create_item_stack(count))
                .unwrap_or_else(|e| panic!("stack {spec}: {e:?}"));
        }
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
                reseed_for(&a[1]);
                self.spawn(a);
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
            "op" => {
                self.world();
                // `PlayerList.op`: the operator permissions of the server (level 4)
                self.with_player_named(&a[1], |_, p| {
                    use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
                    p.handle
                        .as_ref()
                        .expect("player list entry")
                        .shared
                        .lock()
                        .permissions = LevelBasedPermissionSet::for_level(PermissionLevel::Owners);
                });
            }
            "pick_block" => {
                self.world();
                // ServerboundPickItemFromBlockPacket: `pick_block PLAYER X Y Z [true|false]` (include block data)
                let packet = mc_protocol::packets::play::ServerboundPickItemFromBlockPacket {
                    pos: BlockPos::new(
                        a[2].parse().unwrap(),
                        a[3].parse().unwrap(),
                        a[4].parse().unwrap(),
                    ),
                    include_data: a.get(5).is_some_and(|v| v == "true"),
                };
                self.with_player_named(&a[1], |level, p| {
                    mc_game::world::item::pick_item_from_block(p, level, &packet)
                });
            }
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
            "pcommand" => {
                self.world();
                // `handlePlayerCommand` of the actions the scenarios use (the packet handler itself needs a connection)
                self.with_player_named(&a[1], |level, p| {
                    match a[2].to_ascii_lowercase().as_str() {
                        "start_sprinting" => p.set_sprinting(true),
                        "stop_sprinting" => p.set_sprinting(false),
                        "stop_sleeping" => {
                            if p.is_sleeping() {
                                p.stop_sleep_in_bed(level, false, true);
                            }
                        }
                        other => panic!("pcommand {other} is not supported by the Rust replay"),
                    }
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
            "menu_button" => {
                self.world();
                // `handleContainerButtonClick` without the packet: the menu's own handler on the player's open menu
                let button: i32 = a[2].parse().unwrap();
                self.with_player_named(&a[1], |level, p| {
                    let container_id = p.player.menus.current_container_id();
                    let mut ctx = PlayerCtx { player: p, level };
                    PlayerMenus::with_current(
                        &mut ctx,
                        |c| &mut c.player.player.menus,
                        |menu, c| handle_container_button_click(menu, c, container_id, button),
                    );
                });
            }
            "close_menu" => {
                self.world();
                self.with_player_named(&a[1], |level, p| p.do_close_container(level));
            }
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
        let (yaw, pitch) = if a.len() > 7 {
            (num(&a[6]) as f32, num(&a[7]) as f32)
        } else {
            (0.0, 0.0)
        };
        p.snap_to(num(&a[3]), num(&a[4]), num(&a[5]), yaw, pitch);
        p.base_mut().first_tick = true;
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
                memory_connection: true,
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
        let tracking = PlayerTrackingInfo {
            entity_id: id,
            uuid: p.uuid(),
            connection: sender,
            sink: Box::new(AcknowledgedChunks),
            position: p.position(),
            requested_view_distance: p.requested_view_distance,
            spectator: p.is_spectator(),
            alive: true,
        };
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
        level.add_player(Box::new(p));
        level.with_player(id, |level, p| p.init_inventory_menu(level));
        // `PlayerList.placeNewPlayer` registers the player with the chunk map: the spawning chunks it creates are shuffled
        // with the level random on every tick (`ServerChunkCache.tickChunks`), which every later spawner draw depends on.
        level.chunk_source.chunk_map.add_player(tracking);
        // a non-simulated player has no movement packets: onGround is derived once (resting on a block)
        level.with_player(id, |level, p| {
            let bb = p.base().bounding_box().move_(0.0, -1.0E-3, 0.0);
            let free = level.no_collision_aabb(&bb);
            p.base_mut().set_on_ground_raw(!free);
        });
        self.actors.known.insert(id);
        self.actors.refs.push((name.clone(), id));
        self.actors.players.push(name.clone());
        self.actors._inbound.push(inbound_tx);
        // the join burst is not compared: `packets:NAME` dumps start after the join
        let mut rx = rx;
        while rx.try_recv().is_ok() {}
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
        let sequence = self.actors.sequence.entry(a[1].clone()).or_insert(0);
        *sequence += 1;
        let sequence = *sequence;
        self.with_player_named(&a[1], |level, p| {
            let listener = p
                .listener
                .as_mut()
                .expect("replayed players have a listener");
            listener
                .ack_block_changes_up_to(sequence)
                .expect("positive sequence");
            if !p.is_within_block_interaction_range(pos, 1.0) {
                return;
            }
            let d = hit.subtract(Vec3::at_center_of(&pos));
            if !(d.x.abs() < 1.0000001 && d.y.abs() < 1.0000001 && d.z.abs() < 1.0000001) {
                return;
            }
            // `handleUseItemOn`: nothing happens while a teleport waits for the client's confirmation
            if p.listener
                .as_ref()
                .is_some_and(|l| l.is_awaiting_position_from_client())
            {
                return;
            }
            let animation = p
                .get_item_in_hand(hand)
                .get(&mc_protocol::components::INTERACT_ANIMATION)
                .copied()
                .unwrap_or(mc_game::world::entity::player::item_use::SWING_ANIMATION_DEFAULT);
            let outcome = p.use_item_on(level, hand, &result);
            // a successful use swings (the decompiled handler swings in two branches, both reach the client)
            if let InteractionResult::Success(s) = &outcome
                && s.should_swing()
            {
                for _ in 0..2 {
                    if p.swing(
                        level,
                        hand,
                        animation,
                        s.swing_source != SwingSource::Predicted,
                    ) {
                        p.reset_attack_strength_ticker();
                    }
                }
            }
            // ... and the client gets the clicked and the adjacent block back
            for at in [pos, pos.relative(face)] {
                p.send(&ClientboundBlockUpdatePacket {
                    pos: at,
                    block_state: level.get_block_state(at),
                });
            }
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
        let creative = self.with_player_named(&name, |level, p| {
            p.handle_block_break_action(
                level,
                pos,
                Action::StartDestroyBlock,
                Direction::Up,
                max_y,
                false,
            );
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

    /// The player half of a tick, after `ServerLevel::tick` (which ticked the player entity): `ServerGamePacketListenerImpl.tick` -> `doTick`.
    pub(super) fn tick_players(&mut self) {
        for name in self.actors.players.clone() {
            // `tickPlayer`: `resetPosition`, `xo/yo/zo`, `doTick`, then `absSnapTo(firstGood)`: a client-authoritative player
            // keeps the position it had, whatever the server-side tick did to it
            self.with_player_named(&name, |level, p| {
                if let Some(listener) = p.listener.as_mut() {
                    listener.send_block_changed_ack();
                }
                let pos = p.position();
                p.player.living.entity.set_old_pos(pos);
                p.do_tick(level);
                let (y_rot, x_rot) = (p.get_y_rot(), p.get_x_rot());
                p.abs_snap_to(pos.x, pos.y, pos.z, y_rot, x_rot);
            });
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
            "stats" => {
                let name = arg.expect("stats:PLAYER");
                let mut lines: Vec<(String, i32)> = self.with_player_named(name, |_, p| {
                    p.stats
                        .lock()
                        .counter
                        .iter_fast_order()
                        .into_iter()
                        .map(|(stat, n)| (mc_game::stats::stat::build_name(&stat), n))
                        .collect()
                });
                lines.sort();
                for (stat, n) in lines {
                    self.out.push(format!("STAT {name} {stat} {n}"));
                }
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
                            // `Inventory.getItem(i)`: the 36 slots, then the equipment slots (feet, legs, chest, head, offhand, body, saddle)
                            let inv = &p.player.inventory;
                            let main = inv.get_non_equipment_items().iter().enumerate();
                            let worn = (36..=42).filter_map(|i| {
                                mc_game::world::entity::player::inventory::equipment_slot_for(i)
                                    .map(|es| (i, inv.equipment_get(es)))
                            });
                            main.chain(worn)
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
                    sent.push((ty, p.to_vec()));
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
        } else if let Some(o) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::experience_orb::ExperienceOrb>()
        {
            s += &format!(
                " value={} count={} age={} health={}",
                o.get_value(),
                o.count,
                o.age,
                o.health
            );
        } else if let Some(f) =
            e.as_any()
                .downcast_ref::<mc_game::world::entity::falling_block_entity::FallingBlockEntity>()
        {
            s += &format!(
                " state={} time={} hurt={} cancel={} drop={}",
                super::state_string(f.get_block_state()),
                f.time,
                f.hurts_entities(),
                f.is_drop_cancelled(),
                f.drop_item
            );
        } else if let Some(t) = e
            .as_any()
            .downcast_ref::<mc_game::world::entity::item::primed_tnt::PrimedTnt>()
        {
            s += &format!(" fuse={}", t.get_fuse());
        } else if let Some(a) = super::super::entity_scenario::abstract_arrow(e) {
            let d = a.arrow_data();
            s += &format!(
                " ground={} crit={} pierce={} pickup={} shake={} life={} dmg={}",
                u8::from(a.is_in_ground()),
                u8::from(a.is_crit_arrow()),
                a.pierce_level(),
                d.pickup.ordinal(),
                d.shake_time,
                d.life,
                dbl(d.base_damage)
            );
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
