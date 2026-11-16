//! Entities and players of the block scenario replay (`spawn`, `mobspawn`, `spawn_item`, `entity_seed`, `kill`, `nbt`, `hurt`, `effect`, `input` (sneak: the shift key), `op`, `pick_block`, `tp`, `snap`, `velocity`, `set`,
//! `spawn_player`, `sign_update`, `dig`, `explode`, `pcommand` (sprinting, stop_sleeping), `dump entities|xentities|mobs|entity:REF|xentity:REF|mob:REF|edata:REF|player:NAME|packets:NAME`). The mob verbs (`target love age breed tame leash mob_attack nav path_query nav_stop`, dumps `goals brain path attrs nbt`) are shared with the entity replay: `mob_verbs.rs`. The player actions (`give select use use_on release interact drop swing swap dispense`
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
use crate::common::spawn_verbs;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::LegacyRandomSource;
use mc_data::EntityType;
use mc_data::enums::GameType;
use mc_game::server::level::level_explode::Explode;
use mc_game::server::level::{ServerLevel, TicketType};
use mc_game::server::network::common_listener::CommonListenerCookie;
use mc_game::server::network::game_listener::{
    CLIENT_LOADED_TIMEOUT_TIME, ListenerSettings, ServerGamePacketListenerImpl,
};
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::server::players::join::{ListenerParams, place_new_player};
use mc_game::server::players::server_join_host::ServerJoinHost;
use mc_game::world::block_entity::sign_block_entity::{SignTextSlot, update_sign_text_from_packet};
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::projectile::eye_of_ender::EyeOfEnder;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::InteractionHand;
use mc_game::world::level::BlockGetter;
use mc_game::world::level::server_level_accessor::ServerLevelAccessor;
use mc_protocol::codec::{Decode, Encode, read_varint};
use mc_protocol::packets::play::{
    AttributeModifierOperation, ClientboundSetTimePacket, ClientboundUpdateAttributesPacket,
    PositionAndRotation, ServerboundAcceptTeleportationPacket, ServerboundChunkBatchReceivedPacket,
    ServerboundClientTickEndPacket, ServerboundContainerButtonClickPacket,
    ServerboundContainerClosePacket, ServerboundDebugSubscriptionRequestPacket,
    ServerboundMovePlayerPosPacket, ServerboundMovePlayerPosRotPacket,
    ServerboundMoveVehiclePacket, ServerboundPaddleBoatPacket, ServerboundPickItemFromBlockPacket,
    ServerboundPlayPacket as P, ServerboundSetCarriedItemPacket, ServerboundTeleportToEntityPacket,
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
    /// The UUID of each ref as last seen alive in any level: an entity that left its level (a portal trip replaces it by a copy with the
    /// same UUID) still names that UUID, like the driver's `refs` that keep the removed object.
    ref_uuids: std::collections::BTreeMap<EntityId, uuid::Uuid>,
    pub players: Vec<String>,
    /// The entries of players that left and joined again: the driver keeps the old entry of its player list next to the new one
    /// (`dump players` prints both). Position in `players` at the time, name and the dimension the old player was in.
    stale_players: Vec<(usize, String, Identifier)>,
    pub(super) known: HashSet<EntityId>,
    /// The UUID of every reference the persistence verbs saw alive (an unloaded entity is gone from the levels): `rejoin`/`reload` rebind by it.
    pub(super) persist_uuids: HashMap<EntityId, uuid::Uuid>,
    auto: HashMap<String, i32>,
    rx: HashMap<String, UnboundedReceiver<Outbound>>,
    /// The driver's `seq(player)` counters.
    sequences: crate::common::player_actions::Sequences,
    /// What the players had before the server removed them.
    departed: crate::common::player_actions::Departed,
    /// `track_movement` players with the position at the start of the previous tick (`None` before the first tick).
    tracked_movement: HashMap<EntityId, Option<Vec3>>,
    /// Keeps the inbound queue of every replayed listener open.
    _inbound: Vec<std::sync::mpsc::Sender<mc_game::server::network::InboundPacket>>,
    /// The server a scenario without `command_env` runs the join teleport's accept against (and the guard that keeps its directory).
    accept_server: Option<(mc_game::server::MinecraftServer, Box<dyn std::any::Any>)>,
    /// `mobspawn ... group=KEY`: the group data `finalizeSpawn` returned for the previous mob of that key.
    groups: HashMap<String, Box<dyn mc_game::world::entity::spawn_group_data::SpawnGroupData>>,
    pub registries_loaded: bool,
    /// The directory of the advancement files of players spawned without a command environment.
    advancements_dir: Option<tempfile::TempDir>,
    /// `datapack NAME`: the registries, recipes and advancements of the server that loaded the packs (otherwise the default pack's).
    pack: Option<ServerData>,
}

/// What a scenario with data packs reads from the server's reloaded resources instead of the default pack's.
struct ServerData {
    registries: mc_game::registry::RegistryAccess,
    recipes: mc_game::world::item::crafting::RecipeManager,
    advancements: mc_game::advancements::server_advancement_manager::ServerAdvancementManager,
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
thread_local! {
    /// `login_detail`: a login packet dumps its levels list, distances and spawn info besides the three rule flags.
    static LOGIN_DETAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A scenario starts with the plain login dump.
pub(super) fn reset_login_detail() {
    LOGIN_DETAIL.set(false);
}

/// `VanillaLevel.record`: the advancement update carries timestamps of obtained criteria; the attribute snapshots are dumped in wire
/// order as text; the recipe property sets sorted by key and item; anything else is its hex wire form.
fn packet_text(ty: &str, wire: &[u8]) -> String {
    match ty {
        "minecraft:update_advancements" => "-".into(),
        // the heightmap order of the packet varies per JVM run: the chunk and the size of the wire form stand for it
        "minecraft:level_chunk_with_light" => {
            let mut body = wire;
            read_varint(&mut body).expect("packet id");
            let coord = |b: &[u8]| i32::from_be_bytes(b[..4].try_into().expect("chunk coordinate"));
            format!(
                "chunk={},{}/len={}",
                coord(body),
                coord(&body[4..]),
                wire.len()
            )
        }
        // the three rule flags (the replay builds the rest of the packet from nothing)
        "minecraft:login" => {
            let mut body = wire;
            read_varint(&mut body).expect("packet id");
            let p = mc_protocol::packets::play::ClientboundLoginPacket::decode(&mut body)
                .expect("login");
            let flags = format!(
                "reduced_debug_info={} show_death_screen={} limited_crafting={}",
                p.reduced_debug_info, p.show_death_screen, p.do_limited_crafting
            );
            if !LOGIN_DETAIL.get() {
                return flags;
            }
            let mut levels: Vec<String> = p.levels.iter().map(|l| l.to_string()).collect();
            levels.sort();
            let info = &p.common_player_spawn_info;
            format!(
                "{flags} levels={} max_players={} chunk_radius={} simulation_distance={} dimension={} sea_level={} flat={} hardcore={}",
                levels.join(","),
                p.max_players,
                p.chunk_radius,
                p.simulation_distance,
                info.dimension,
                info.sea_level,
                info.is_flat,
                p.hardcore
            )
        }
        "minecraft:update_attributes" => {
            let mut body = wire;
            read_varint(&mut body).expect("packet id");
            let p =
                ClientboundUpdateAttributesPacket::decode(&mut body).expect("update_attributes");
            let entries: Vec<String> = p
                .attributes
                .iter()
                .map(|s| {
                    let mods: Vec<String> = s
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
                    format!("{}={}[{}]", s.attribute.name(), dbl(s.base), mods.join(","))
                })
                .collect();
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

/// `VanillaLevel.advancementDetail`: the content of an `update_advancements` packet with the obtained times reduced to done (`T`) or
/// not (`-`); `removed`, `added` and `progress` in wire order, `added` with the first 12 hex digits of the SHA-1 of the encoded advancement.
fn advancement_detail(wire: &[u8]) -> String {
    use mc_protocol::packets::play::ClientboundUpdateAdvancementsPacket;
    use sha1::{Digest, Sha1};
    let mut body = wire;
    read_varint(&mut body).expect("packet id");
    let p = ClientboundUpdateAdvancementsPacket::decode(&mut body).expect("update_advancements");
    let removed = if p.removed.is_empty() {
        "-".to_owned()
    } else {
        p.removed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let added: Vec<String> = p
        .added
        .iter()
        .map(|a| {
            let mut bytes = Vec::new();
            a.advancement
                .encode(&mut bytes)
                .expect("advancement encodes");
            let digest: String = Sha1::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!(
                "{}@{},{}#{}",
                a.advancement.id,
                flt(a.x),
                flt(a.y),
                &digest[..12]
            )
        })
        .collect();
    let added = if added.is_empty() {
        "-".to_owned()
    } else {
        added.join(";")
    };
    let progress = if p.progress.is_empty() {
        "-".to_owned()
    } else {
        p.progress
            .iter()
            .map(|(id, progress)| {
                let criteria: Vec<String> = progress
                    .criteria
                    .iter()
                    .map(|(name, c)| {
                        format!("{name}={}", if c.obtained.is_some() { "T" } else { "-" })
                    })
                    .collect();
                format!("{id}{{{}}}", criteria.join(","))
            })
            .collect::<Vec<_>>()
            .join(";")
    };
    format!(
        "reset={} show={} removed={removed} added={added} progress={progress}",
        p.should_reset, p.show_advancements
    )
}

/// The obtained times of a saved advancement file (`"yyyy-MM-dd HH:mm:ss Z"` strings) replaced by `"T"`.
fn normalise_obtained_times(line: &str) -> String {
    fn is_time(s: &str) -> bool {
        let b = s.as_bytes();
        b.len() == 25
            && b.iter().enumerate().all(|(i, c)| match i {
                4 | 7 => *c == b'-',
                10 | 19 => *c == b' ',
                13 | 16 => *c == b':',
                20 => matches!(*c, b'+' | b'-'),
                _ => c.is_ascii_digit(),
            })
    }
    line.split('"')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 && is_time(part) {
                "T"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("\"")
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

/// `LevelDebugSynchronizers` keeps its synchronizers in a `HashMap` keyed by identity in vanilla, so the debug packets of one tick
/// are sent in an order that differs per JVM run: the oracle driver (`EntityOracleDriver.debugOrder`) and this replay stably sort every
/// run of consecutive `debug/*` packets by subscription id.
fn debug_order(sent: Vec<(&'static str, Vec<u8>)>) -> Vec<(&'static str, Vec<u8>)> {
    type Sent = (&'static str, Vec<u8>);
    fn subscription(ty: &str, wire: &[u8]) -> i32 {
        let mut body = wire;
        read_varint(&mut body).expect("packet id");
        match ty {
            "minecraft:debug/block_value" | "minecraft:debug/chunk_value" => body = &body[8..],
            "minecraft:debug/entity_value" => {
                read_varint(&mut body).expect("entity id");
            }
            _ => {}
        }
        read_varint(&mut body).expect("subscription id")
    }
    fn flush(run: &mut Vec<Sent>, result: &mut Vec<Sent>) {
        run.sort_by_key(|(ty, wire)| subscription(ty, wire));
        result.append(run);
    }
    let mut result = Vec::with_capacity(sent.len());
    let mut run = Vec::new();
    for packet in sent {
        if packet.0.starts_with("minecraft:debug/") {
            run.push(packet);
        } else {
            flush(&mut run, &mut result);
            result.push(packet);
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

/// The registries of the oracle's server (what the entities of chunk generation read before `load_registries` runs).
pub(super) fn real_registries() -> mc_game::registry::RegistryAccess {
    ACCESS.0.clone()
}

impl Actors {
    /// `refs.put(name, entity)` of the driver: a name that is used again names the new entity.
    fn set_ref(&mut self, name: &str, id: EntityId) {
        self.refs.retain(|(n, _)| n != name);
        self.refs.push((name.to_string(), id));
    }

    /// The scenario's data packs are loaded: use the server's registries, recipes and advancement tree.
    pub(super) fn use_server_data(&mut self, server: &mc_game::server::MinecraftServer) {
        let registries = server.registries.composite_access().clone();
        let recipes = server
            .resources
            .managers
            .listener::<mc_game::world::item::crafting::RecipeManager>()
            .expect("recipe manager")
            .clone();
        let advancements =
            mc_game::advancements::server_advancement_manager::ServerAdvancementManager::from_registries(
                &registries,
            );
        self.pack = Some(ServerData {
            registries,
            recipes,
            advancements,
        });
    }

    /// The server reloaded its data packs: a scenario that uses their data takes the new registries, recipes and advancements.
    pub(super) fn refresh_server_data(&mut self, server: &mc_game::server::MinecraftServer) {
        if self.pack.is_some() {
            self.use_server_data(server);
            self.registries_loaded = false;
        }
    }

    fn registries(&self) -> &mc_game::registry::RegistryAccess {
        self.pack.as_ref().map_or(&ACCESS.0, |p| &p.registries)
    }

    fn recipes(&self) -> &mc_game::world::item::crafting::RecipeManager {
        self.pack.as_ref().map_or(&ACCESS.1, |p| &p.recipes)
    }

    fn advancements(
        &self,
    ) -> &mc_game::advancements::server_advancement_manager::ServerAdvancementManager {
        self.pack
            .as_ref()
            .map_or(&*ADVANCEMENTS, |p| &p.advancements)
    }
}

impl Scenario {
    /// Entity creation needs the data pack registries (damage types, enchantments, ...): loaded once per test binary.
    pub(super) fn load_registries(&mut self) {
        if !self.actors.registries_loaded {
            self.actors.registries_loaded = true;
            let registries = self.actors.registries().clone();
            let recipes = self.actors.recipes().clone();
            self.for_each_level(|level| {
                level.registries = registries.clone();
                level.shared.set_recipe_manager(recipes.clone());
            });
            // item behaviour that reads registries without a level (`Item.getUseDuration` of a goat horn)
            mc_game::world::item::set_item_registries(registries);
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

    /// The damage source of `hurt` and `crystal_destroyed`: `TYPE` with the options `by=REF direct=REF at=X,Y,Z`.
    fn damage_source(
        &mut self,
        type_name: &str,
        options: &[String],
    ) -> mc_game::world::damagesource::DamageSource {
        let (mut by, mut direct, mut at) = (None, None, None);
        for t in options {
            match t.split_once('=') {
                Some(("by", r)) => by = Some(self.entity_id(r)),
                Some(("direct", r)) => direct = Some(self.entity_id(r)),
                Some(("at", r)) => {
                    let c: Vec<f64> = r.split(',').map(num).collect();
                    at = Some(Vec3::new(c[0], c[1], c[2]));
                }
                _ => panic!("damage source option {t} is not replayed (by= direct= at=)"),
            }
        }
        let direct = direct.or(by);
        let key = Identifier::parse(&if type_name.contains(':') {
            type_name.to_string()
        } else {
            format!("minecraft:{type_name}")
        })
        .unwrap();
        let level = self.level();
        let data = level
            .registries
            .get::<mc_game::registry::typed::damage_type::DamageType>("minecraft:damage_type", &key)
            .unwrap_or_else(|| panic!("unknown damage type {key}"))
            .clone();
        mc_game::world::damagesource::DamageSource::new(
            key,
            std::sync::Arc::new(data),
            direct.and_then(|i| level.get_entity(i)),
            by.and_then(|i| level.get_entity(i)),
            at,
        )
    }

    /// `hurt REF DAMAGETYPE AMOUNT [by=REF] [direct=REF] [at=X,Y,Z]`: `entity.hurtServer(level, source, amount)`, prints `HURT ..`.
    fn hurt(&mut self, a: &[String]) {
        let source = self.damage_source(&a[2], &a[4..]);
        let amount = num(&a[3]) as f32;
        let id = self.entity_id(&a[1]);
        let r = self
            .level()
            .with_entity(id, |level, e| e.hurt_server(level, &source, amount))
            .unwrap_or_else(|| panic!("entity {} is gone", a[1]));
        self.out
            .push(format!("HURT {} {} {} -> {r}", a[1], a[2], a[3]));
    }

    /// `crystal_destroyed DRAGON CRYSTAL DAMAGETYPE [by=REF] [direct=REF] [at=X,Y,Z]`: `EnderDragon.onCrystalDestroyed` of the
    /// dragon for the crystal, as the dragon fight calls it.
    fn crystal_destroyed(&mut self, a: &[String]) {
        let source = self.damage_source(&a[3], &a[4..]);
        let (dragon, crystal) = (self.entity_id(&a[1]), self.entity_id(&a[2]));
        self.level()
            .with_entity(crystal, |level, crystal| {
                let pos = crystal.base().block_position();
                level.with_entity(dragon, |level, dragon| {
                    dragon
                        .as_any_mut()
                        .downcast_mut::<mc_game::world::entity::boss::ender_dragon::EnderDragon>()
                        .expect("crystal_destroyed needs an ender dragon")
                        .on_crystal_destroyed(level, &*crystal, pos, &source)
                });
            })
            .unwrap_or_else(|| panic!("entity {} is gone", a[2]));
    }

    /// Entities created by game logic get stable names `type_path#n` in creation (id) order (twin of `nameNewEntities`).
    pub(super) fn name_new_entities(&mut self) {
        if self.level.is_none() {
            return;
        }
        // after an unload a reference follows the entity of its UUID that is loaded again, before any entity is named
        if !self.actors.persist_uuids.is_empty() {
            self.rebind_refs();
        }
        let level = self.level.as_ref().expect("world");
        // the entities of every level, by id (the ids come from one sequence)
        let known = &self.actors.known;
        let mut fresh: Vec<(EntityId, String)> = Vec::new();
        let mut scan = |level: &ServerLevel| {
            for id in level.entity_manager.all_visible_ids() {
                if !known.contains(&id) {
                    let path = level
                        .get_entity(id)
                        .map(|e| {
                            e.get_type()
                                .name()
                                .trim_start_matches("minecraft:")
                                .to_string()
                        })
                        .unwrap_or_default();
                    fresh.push((id, path));
                }
            }
        };
        scan(level);
        if self.multi() {
            mc_game::commands::server_access::with_server(|server| {
                server.all_levels().for_each(&mut scan)
            });
        }
        fresh.sort_unstable_by_key(|(id, _)| *id);
        fresh.dedup_by_key(|(id, _)| *id);
        for (id, path) in fresh {
            if self.actors.known.insert(id) && !self.actors.refs.iter().any(|(_, i)| *i == id) {
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
                self.actors.set_ref(&a[1], id);
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
                    let handle = p.handle.as_ref().expect("player list entry");
                    let owners = LevelBasedPermissionSet::for_level(PermissionLevel::Owners);
                    handle.shared.lock().permissions = owners;
                    // `sendPlayerPermissionLevel`: the op-level entity event, then the command tree
                    handle.connection.send(
                        &mc_protocol::packets::play::ClientboundEntityEventPacket {
                            entity_id: handle.id,
                            event_id:
                                mc_game::server::players::player_list::permission_level_event_id(
                                    owners,
                                ),
                        },
                    );
                    if mc_game::commands::server_access::has_server() {
                        mc_game::commands::server_access::with_server(|server| {
                            mc_game::commands::send_commands(server, handle, &owners.into());
                        });
                    }
                    let ops = &mut level.server_state.spawn_protection;
                    ops.ops_empty = false;
                    if !ops.op_players.contains(&p.uuid()) {
                        ops.op_players.push(p.uuid());
                    }
                });
                // the installed server's operator list too: `/op`, `/deop` and the permissions of `cmd_as` read it (`op_entry` alone sends no packet)
                if mc_game::commands::server_access::has_server() {
                    let id = mc_game::server::players::NameAndId::create_offline(&a[1]);
                    mc_game::commands::server_access::with_server(|server| {
                        server.player_list.op_entry(id, None, None)
                    });
                }
            }
            // `chunk_batch_received PLAYER RATE`: `ServerboundChunkBatchReceivedPacket`
            "chunk_batch_received" => deliver(self, &a[1], |_, _| {
                P::ChunkBatchReceived(ServerboundChunkBatchReceivedPacket {
                    desired_chunks_per_tick: a[2].parse().expect("chunks per tick"),
                })
            }),
            // `debug_subscribe PLAYER NAME,..` (`-` = none): `ServerboundDebugSubscriptionRequestPacket`
            "debug_subscribe" => deliver(self, &a[1], |_, _| {
                P::DebugSubscriptionRequest(ServerboundDebugSubscriptionRequestPacket {
                    subscriptions: a[2]
                        .split(',')
                        .filter(|name| *name != "-")
                        .map(|name| {
                            mc_data::DebugSubscription::by_name(name)
                                .unwrap_or_else(|| panic!("unknown debug subscription {name}"))
                        })
                        .collect(),
                })
            }),
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
            // `teleport_to_entity PLAYER REF`: `ServerboundTeleportToEntityPacket` (a spectator teleports to the entity)
            "teleport_to_entity" => {
                let target = self.entity_id(&a[2]);
                deliver(self, &a[1], |level, _| {
                    P::TeleportToEntity(ServerboundTeleportToEntityPacket {
                        uuid: level.get_entity(target).expect("teleport target").uuid(),
                    })
                })
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
                self.level().with_entity(id, |level, e| {
                    let mut tag = mc_nbt::CompoundTag::new();
                    e.save_without_id(&mut tag, &|_| None);
                    tag.merge(&extra);
                    e.load(level, &tag)
                        .unwrap_or_else(|er| panic!("entity load: {er}"));
                    mc_game::world::entity::mob::leashable::finish_leash_load(e, level);
                    e.read_level_dependent_data(level);
                    e.resolve_registry_refs(&level.registries);
                    // vanilla's `readAdditionalSaveData` rejoins the raid of a raider right here
                    if e.raider().is_some() {
                        e.after_loaded_to_level(level);
                    }
                });
            }
            "crystal_destroyed" => {
                self.world();
                self.load_registries();
                self.crystal_destroyed(a);
            }
            "hurt" => {
                self.world();
                self.load_registries();
                self.hurt(a);
            }
            "kill" => {
                self.world();
                let id = self.entity_id(&a[1]);
                // the level-aware removal: a container cart drops its contents when it is discarded
                self.level().with_entity(id, |level, e| {
                    level.remove_held_entity(e, mc_game::world::entity::RemovalReason::Discarded)
                });
            }
            "signal" => {
                self.world();
                let id = self.entity_id(&a[1]);
                let target = Vec3::new(num(&a[2]), num(&a[3]), num(&a[4]));
                self.level().with_entity(id, |_, e| {
                    e.as_any_mut()
                        .downcast_mut::<EyeOfEnder>()
                        .expect("signal targets an eye of ender")
                        .signal_to(target)
                });
            }
            // tp_dim REF DIM X Y Z [yaw pitch]: `Entity.teleport(TeleportTransition(level DIM, X Y Z, no motion, yaw pitch, no relatives))`;
            // a non-player becomes a new entity in DIM and the name refers to that one from now on
            "tp_dim" => {
                use mc_game::server::players::dimension_change::teleport_entity;
                use mc_game::server::players::teleport_transition::{
                    PostTeleportTransition, TeleportTransition,
                };
                self.world();
                self.load_registries();
                let id = self.entity_id(&a[1]);
                let from = self.level().dimension.clone();
                let (yaw, pitch) = {
                    let e = self
                        .level()
                        .get_entity(id)
                        .unwrap_or_else(|| panic!("entity {} is gone", a[1]));
                    (e.base().y_rot(), e.base().x_rot())
                };
                let t = TeleportTransition::new(
                    super::dimension_key(&a[2]),
                    Vec3::new(num(&a[3]), num(&a[4]), num(&a[5])),
                    Vec3::ZERO,
                    a.get(6).map_or(yaw, |v| num(v) as f32),
                    a.get(7).map_or(pitch, |v| num(v) as f32),
                    PostTeleportTransition::DO_NOTHING,
                );
                let level = self.level.as_mut().expect("world");
                let moved = mc_game::commands::server_access::with_server(|server| {
                    server.with_lent_level(level, |server| teleport_entity(server, &from, id, &t))
                });
                if let Some(new_id) = moved
                    && new_id != id
                {
                    for (name, entity) in &mut self.actors.refs {
                        if *name == a[1] {
                            *entity = new_id;
                        }
                    }
                    self.actors.known.insert(new_id);
                }
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
            // partref NAME DRAGON INDEX   NAME = the part INDEX of the ender dragon DRAGON, usable as a REF
            "partref" => {
                self.world();
                let dragon = self.entity_id(&a[2]);
                let index: usize = a[3].parse().expect("part index");
                let part = self
                    .level()
                    .get_entity_or_removed(dragon)
                    .and_then(|d| d.sub_entity_ids().get(index).copied())
                    .unwrap_or_else(|| panic!("{} has no part {index}", a[2]));
                self.actors.refs.push((a[1].clone(), part));
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
            "accept_teleport" => self.accept_teleport(&a[1]),
            // paddle PLAYER LEFT RIGHT   ServerboundPaddleBoatPacket (true|false)
            "paddle" => deliver(self, &a[1], |_, _| {
                P::PaddleBoat(ServerboundPaddleBoatPacket {
                    left: a[2] == "true",
                    right: a[3] == "true",
                })
            }),
            // respawn PLAYER   ServerboundClientCommandPacket(PERFORM_RESPAWN) of a dead player
            "login_detail" => LOGIN_DETAIL.set(true),
            "respawn" => {
                self.world();
                self.load_registries();
                self.respawn(&a[1]);
            }
            // player_loaded PLAYER   ServerboundPlayerLoadedPacket
            "player_loaded" => {
                self.world();
                deliver(self, &a[1], |_, _| {
                    P::PlayerLoaded(mc_protocol::packets::play::ServerboundPlayerLoadedPacket)
                });
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
                    "portal_cooldown" => e.base_mut().portal_cooldown = v.parse().unwrap(),
                    "seen_credits" => {
                        e.as_any_mut()
                            .downcast_mut::<ServerPlayer>()
                            .expect("player")
                            .seen_credits = v == "true"
                    }
                    "invulnerable" => e.base_mut().permanently_invulnerable = v == "true",
                    "no_gravity" => e.base_mut().set_no_gravity(v == "true"),
                    "tag" => {
                        e.base_mut().add_tag(&v);
                    }
                    "health" => e
                        .living_mut()
                        .expect("living entity")
                        .set_health(v.parse().unwrap()),
                    "absorption" => e.set_absorption_amount(v.parse().unwrap()),
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
            "advancement_tab" => deliver(self, &a[1], |_, _| {
                P::SeenAdvancements(
                    mc_protocol::packets::play::ServerboundSeenAdvancementsPacket::OpenedTab(
                        Identifier::parse(&a[2]).expect("advancement id"),
                    ),
                )
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
            _ => {
                let line = self.line.clone();
                return crate::common::mob_verbs::command(self, a)
                    || spawn_verbs::command(self, a)
                    || crate::common::command_verbs::command(self, a, &line)
                    || crate::common::player_actions::command(self, a);
            }
        }
        true
    }

    /// [`Scenario::with_player_named`] for a player that may be gone (kicked or left): `None` then.
    fn try_with_player<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
    ) -> Option<R> {
        self.focus_ref(name);
        let id = self.entity_id(name);
        let mut f = Some(f);
        let found = self
            .level()
            .with_player(id, |level, p| (f.take().expect("once"))(level, p));
        if found.is_some() || !mc_game::commands::server_access::has_server() {
            return found;
        }
        // a player the end credits removed from its level is still connected and ticks: the server holds it
        let level = self.level.as_mut().expect("world");
        mc_game::commands::server_access::with_server(|server| {
            let p = server
                .detached_players
                .values_mut()
                .find(|p| p.base_id() == id)?;
            Some((f.take()?)(level, p))
        })
    }

    fn with_player_named<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
    ) -> R {
        self.try_with_player(name, f)
            .unwrap_or_else(|| panic!("no player {name}"))
    }

    fn spawn(&mut self, a: &[String]) {
        self.spawn_with(a, mc_data::enums::EntitySpawnReason::Command, false, None);
    }

    /// `mobspawn REF TYPE X Y Z [yaw pitch] [reason=NAME] [group=KEY] [nofinalize] [SNBT]`: the entity is created with that spawn
    /// reason and, unless `nofinalize`, `Mob.finalizeSpawn` runs before it joins the level (`/summon`); `group=KEY` hands it the
    /// group data of the previous mob with that key.
    fn mob_spawn(&mut self, a: &[String]) {
        let mut reason = mc_data::enums::EntitySpawnReason::Command;
        let mut finalize = true;
        let mut group = None;
        let mut rest = Vec::new();
        for t in a {
            if let Some(r) = t.strip_prefix("reason=") {
                reason = crate::common::entity_scenario::spawn_reason(r)
                    .unwrap_or_else(|| panic!("spawn reason {r}"));
            } else if let Some(k) = t.strip_prefix("group=") {
                group = Some(k.to_string());
            } else if t == "nofinalize" {
                finalize = false;
            } else {
                rest.push(t.clone());
            }
        }
        self.spawn_with(&rest, reason, finalize, group);
    }

    fn spawn_with(
        &mut self,
        a: &[String],
        reason: mc_data::enums::EntitySpawnReason,
        finalize: bool,
        group: Option<String>,
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
        let mut data = group.as_ref().and_then(|k| self.actors.groups.remove(k));
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
            e.load(level, &tag)
                .unwrap_or_else(|er| panic!("entity load: {er}"));
            e.resolve_registry_refs(&level.registries);
        }
        if finalize {
            for hook in mc_game::world::entity::scenario_dump::SCENARIO_FINALIZE_SPAWN.iter() {
                data = hook(level, e.as_mut(), reason, data);
            }
        }
        let id = e.id();
        level.add_fresh_entity_with_passengers(vec![e]);
        if let (Some(k), Some(data)) = (group, data) {
            self.actors.groups.insert(k, data);
        }
        self.actors.known.insert(id);
        self.actors.set_ref(&a[1], id);
    }

    /// `ServerboundAcceptTeleportationPacket` for the teleport the server waits on (nothing when none is pending). A scenario without
    /// `command_env` has no server for the listener's handlers: the accept then runs against one of its own, kept until the scenario ends.
    fn accept_teleport(&mut self, name: &str) {
        let pending = self.with_player_named(name, |_, p| {
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
        let Some(packet) = pending else { return };
        if mc_game::commands::server_access::has_server() {
            deliver(self, name, |_, _| P::AcceptTeleportation(packet));
            return;
        }
        let (mut server, keep) = self
            .actors
            .accept_server
            .take()
            .unwrap_or_else(crate::command_common::test_server);
        mc_game::commands::server_access::enter_server(&mut server, || {
            deliver(self, name, |_, _| P::AcceptTeleportation(packet))
        });
        self.actors.accept_server = Some((server, keep));
    }

    fn spawn_player(&mut self, a: &[String]) {
        // the player joins the current level (`VanillaLevel.connect` puts it in `v.level`)
        let spawn_at = self.at;
        let name = a[1].clone();
        let flags = &a[6..];
        let sim = flags.iter().any(|f| f == "sim");
        let joinburst = flags.iter().any(|f| f == "joinburst");
        let load = flags.iter().any(|f| f == "load");
        let n = a.len() - usize::from(sim) - usize::from(joinburst) - usize::from(load);
        // a player that left and joins again replaces its old entry
        if self.actors.players.contains(&name) {
            let old = self.entity_id(&name);
            if self.level().with_player(old, |_, _| ()).is_none() {
                let at = self.actors.players.iter().position(|n| *n == name);
                let dimension = self.level().dimension.clone();
                self.actors
                    .stale_players
                    .extend(at.map(|at| (at, name.clone(), dimension)));
                self.actors.players.retain(|n| *n != name);
                self.actors.refs.retain(|(n, _)| *n != name);
            }
        }
        let profile = mc_game::server::network::login_listener::create_offline_profile(&name);
        // a replay without a command environment has no player list: the progress file lives in a directory of its own
        let own_advancements = if mc_game::commands::server_access::has_server() {
            None
        } else {
            let save_path = self
                .actors
                .advancements_dir
                .get_or_insert_with(|| tempfile::tempdir().expect("advancement directory"))
                .path()
                .join(format!("{}.json", profile.id));
            Some(mc_game::advancements::SharedAdvancements::new(
                mc_game::advancements::player_advancements::PlayerAdvancements::new(
                    self.actors.advancements(),
                    Some(save_path),
                ),
            ))
        };
        let level = self.level();
        let id = level.next_entity_id();
        let mut p = ServerPlayer::new(
            id,
            level.dimension.clone(),
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
        // `load`: `PrepareSpawnTask.Ready.spawn` loads the player's saved data before the join
        if load {
            let saved = mc_game::commands::server_access::with_server(|server| {
                server
                    .player_list
                    .load_player_data(&mc_game::server::players::NameAndId::new(
                        profile.id,
                        profile.name.clone(),
                    ))
            });
            if let Some(tag) = saved {
                p.load(level, &tag).expect("the saved player data loads");
            }
        }
        if let Some(own) = own_advancements {
            p.advancements = own;
        }
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
        let (inbound_tx, inbound_rx) = std::sync::mpsc::channel();
        let cookie = CommonListenerCookie {
            profile: profile.clone(),
            latency: 0,
            client_information: Default::default(),
            transferred: false,
        };
        self.focus(spawn_at);
        // `VanillaLevel.connect` waits for the spawn chunks' entities (`waitForEntities`) before the player joins; the wait polls the
        // chunk source, whose task loop runs the distance manager updates. The players that joined earlier are in its trackers before
        // this one is added: the iteration order of the natural spawn map, hence the shuffled spawning chunks, depends on it.
        let has_server = mc_game::commands::server_access::has_server();
        let multi = self.multi();
        let level = self.level();
        let spawn_chunk = ChunkPos::new(
            (num(&a[3]).floor() as i32) >> 4,
            (num(&a[5]).floor() as i32) >> 4,
        );
        // a spawn far from the preloaded chunks of a one-level replay keeps its chunks as they were: its fixtures show a structure
        // mob there that has not ticked, which a wait that promotes the chunks further would change
        let waits = has_server
            && (multi
                || (-3..=3).all(|dx| {
                    (-3..=3).all(|dz| {
                        level.are_entities_loaded(
                            ChunkPos::new(spawn_chunk.x + dx, spawn_chunk.z + dz).pack(),
                        )
                    })
                }));
        if waits {
            // `PrepareSpawnTask.Ready.keepAlive`: the ticket that keeps the chunks around a spawn far from the origin loading
            level.add_ticket_with_radius(TicketType::PLAYER_SPAWN, spawn_chunk, 3);
        }
        level.run_distance_manager_updates();
        if waits {
            // `ServerLevel.waitForEntities` blocks the server thread, which polls the chunk source of every level meanwhile
            let spawn = spawn_chunk;
            let dimension = level.dimension.clone();
            mc_core::random::without_advancing_oracle_uniquifier(|| {
                mc_game::commands::server_access::with_server(|server| {
                    server.with_lent_level(level, |server| {
                        server.managed_block(|server| {
                            server
                                .with_level(&dimension, |_, level| {
                                    level.process_pending_entity_loads();
                                    (-3..=3).all(|dx| {
                                        (-3..=3).all(|dz| {
                                            level.are_entities_loaded(
                                                ChunkPos::new(spawn.x + dx, spawn.z + dz).pack(),
                                            )
                                        })
                                    })
                                })
                                .unwrap_or(true)
                        })
                    })
                })
            });
        }
        if has_server {
            // `PlayerList.placeNewPlayer` itself: the login packets, the tab list, the join message, the chunk map and the entity
            // tracker, the boss bars and the inventory menu
            let params = ListenerParams {
                cookie,
                now_ms: 0,
                command_spam_threshold_seconds: 10,
                chat_spam_threshold_seconds: 10,
                memory_connection: false,
                remote_address: None,
                inbound: inbound_rx,
                enforce_secure_profile: std::sync::Arc::new(|| false),
            };
            let dimension = level.dimension.clone();
            let handle = mc_game::commands::server_access::with_server(|server| {
                server.with_lent_level(level, |server| {
                    server.with_level(&dimension, |server, level| {
                        let mut host = ServerJoinHost { server, level };
                        place_new_player(&mut host, p, sender, params)
                    })
                })
            })
            .expect("the level the player joins");
            // the level's roster is not the server player list's one in a replay
            level.server_state.roster.add(handle);
            level.with_player(id, |_, p| {
                let listener = p.listener.as_mut().expect("player listener");
                for _ in 0..CLIENT_LOADED_TIMEOUT_TIME {
                    listener.tick_client_load_timeout();
                }
            });
        } else {
            // no server (a replay without `command_env`): the join is assembled by hand around the level
            p.connection = Some(sender.clone());
            let handle = mc_game::server::players::PlayerHandle::new(
                id,
                profile,
                sender.clone(),
                "<unknown>".to_owned(),
                level.dimension.clone(),
            );
            level.server_state.roster.add(handle.clone());
            let mut listener = ServerGamePacketListenerImpl::new(
                sender,
                inbound_rx,
                &cookie,
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
            let joined =
                mc_game::server::players::player_list::join_message(p.get_display_name(), None);
            p.handle = Some(handle.clone());
            p.sync_handle();
            p.connection_teleport(num(&a[3]), num(&a[4]), num(&a[5]), yaw, pitch);
            // `PlayerList.placeNewPlayer` broadcasts the join message before the new player joins the list: only the players
            // already in the level get it
            for other in self.actors.players.clone() {
                // a player that left is no longer on the list
                self.try_with_player(&other, |_, o| {
                    o.send(&mc_protocol::packets::play::ClientboundSystemChatPacket {
                        content: joined.clone(),
                        overlay: false,
                    });
                    // `broadcastAll(createPlayerInitializing(List.of(player)))`: the tab list entry reaches every player
                    o.send(
                        &mc_game::server::players::player_handle::create_player_initializing([
                            &handle,
                        ]),
                    )
                });
            }
            let level = self.level();
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
        }
        // `PrepareSpawnTask.Ready.spawn`: the pearls the player saved are spawned after the join
        if load {
            let level = self.level.as_mut().expect("world");
            mc_game::commands::server_access::with_server(|server| {
                mc_game::server::players::ender_pearl_ticket::load_and_spawn_ender_pearls(
                    server, level, id,
                )
            });
        }
        self.actors.known.insert(id);
        self.actors.refs.push((name.clone(), id));
        self.actors.players.push(name.clone());
        self.actors._inbound.push(inbound_tx);
        // `VanillaLevel.connect`: the listener has recorded the first position, then the client accepts the join teleport. Its
        // `handlePlayerPositionChange` moves the player once (which refreshes the fluid state and wraps the rotation)
        self.with_player_named(&name, |_, p| {
            let at = p.position();
            p.listener
                .as_mut()
                .expect("player listener")
                .reset_position(at);
        });
        self.accept_teleport(&name);
        // a non-simulated player has no movement packets: onGround is derived once (resting on a block)
        let level = self.level();
        if !sim {
            level.with_player(id, |level, p| {
                let bb = p.base().bounding_box().move_(0.0, -1.0E-3, 0.0);
                let free = level.no_collision_aabb(&bb);
                p.base_mut().set_on_ground_raw(!free);
            });
        }
        // the accept flags a hovering client (kick after 80 ticks); a real client keeps sending moves
        self.with_player_named(&name, |_, p| {
            p.listener
                .as_mut()
                .expect("player listener")
                .set_client_is_floating(false);
        });
        // the join burst is only compared with `joinburst`: otherwise `packets:NAME` dumps start after the join
        let mut rx = rx;
        while !joinburst && rx.try_recv().is_ok() {}
        self.actors.rx.insert(name, rx);
    }

    /// `handleClientCommand(PERFORM_RESPAWN)` -> `PlayerList.respawn(player, false)` -> the client reports the new level loaded. The
    /// new player keeps the id, the listener and the connection of the dead one; a living player ignores the request.
    fn respawn(&mut self, name: &str) {
        use mc_protocol::packets::play::{
            ServerboundClientCommandAction, ServerboundClientCommandPacket,
        };
        // the client's PERFORM_RESPAWN command: the listener queues it and the server's packet processing runs `PlayerList.respawn`
        let packet = P::ClientCommand(ServerboundClientCommandPacket {
            action: ServerboundClientCommandAction::PerformRespawn,
        });
        // the player waits for its respawn in the level it died in, which may not be the one held
        self.focus_ref(name);
        let id = self.entity_id(name);
        // a player that watched the credits left its level: the server holds it apart until this request
        let detached = mc_game::commands::server_access::with_server(|server| {
            server.detached_players.values().any(|p| p.base_id() == id)
        });
        if detached {
            let level = self.level.as_mut().expect("world");
            mc_game::commands::server_access::with_server(|server| {
                server.with_lent_level(level, |server| {
                    let uuid = server
                        .player_list
                        .players()
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.uuid)
                        .expect("the detached player is in the player list");
                    mc_game::server::network::game_packets::with_player(server, uuid, |ctx| {
                        ctx.deliver(packet)
                    })
                })
            });
        } else {
            deliver(self, name, |_, _| packet);
        }
        let level = self.level.as_mut().expect("world");
        mc_game::commands::server_access::with_server(|server| {
            server.with_lent_level(level, |server| {
                mc_game::server::network::game_packets::process_queued_packets(server)
            })
        });
        self.focus_ref(name);
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
            self.level().explode(params);
            return;
        };
        // the source entity is out of the level's storage while it explodes, like a primed TNT's own tick
        let id = self.entity_id(&source);
        self.level().with_entity(id, |level, e| {
            let params = Explode::new(Some(e), x, y, z, radius, inter).with_fire(fire);
            level.explode(params);
        });
    }

    /// `MinecraftServer.tickChildren`: every 20th server tick (the tick counter is the replayed tick number, as in the oracle, whose
    /// server ticks only through `VanillaLevel.tick`) all players get the game time, before the level ticks.
    pub(super) fn force_game_time_synchronization(&mut self) {
        if (self.ticks + 1) % 20 != 0 {
            return;
        }
        let keep = self.at;
        for name in self.actors.players.clone() {
            // a player the listener disconnected is gone from the level
            self.try_with_player(&name, |level, p| {
                p.send(&ClientboundSetTimePacket {
                    game_time: level.game_time(),
                    clock_updates: Vec::new(),
                });
            });
        }
        self.focus(keep);
    }

    /// `VanillaLevel.tick`: before the server ticks, a simulated player applies its keyboard input (`SimPlayer.applyInput`).
    pub(super) fn apply_player_inputs(&mut self) {
        let keep = self.at;
        for name in self.actors.players.clone() {
            let id = self.entity_id(&name);
            let mut tracked = self.actors.tracked_movement.get(&id).copied();
            self.try_with_player(&name, |_, p| {
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
        self.focus(keep);
    }

    /// `Connection.handleDisconnection` of every player whose connection the server closed outside a packet handler or the tick (a console
    /// command's `kick`): a replay has no socket, so the closed connection is noticed here.
    pub(super) fn handle_disconnections(&mut self) {
        let keep = self.at;
        for name in self.actors.players.clone() {
            self.try_with_player(&name, |level, p| {
                mc_game::commands::server_access::with_server(|server| {
                    mc_game::server::network::game_packets::PlayCtx {
                        server,
                        level,
                        player: p,
                    }
                    .handle_disconnection_without_socket()
                });
            });
        }
        self.focus(keep);
    }

    /// The player half of a tick, after `ServerLevel::tick` (which ticked the player entity): `ServerGamePacketListenerImpl.tick` -> `doTick`.
    pub(super) fn tick_players(&mut self) {
        let keep = self.at;
        for name in self.actors.players.clone() {
            // `tickPlayer`: `resetPosition`, `xo/yo/zo`, `doTick`, then `absSnapTo(firstGood)`: a client-authoritative player
            // keeps the position it had, whatever the server-side tick did to it
            // a player the listener disconnected is gone from the level
            let mut moved = false;
            // `Some(started)`: `doTick()` ran up to the portal step; `None`: a replay without a server ticks the player whole
            let mut started = None;
            self.try_with_player(&name, |level, p| {
                if let Some(listener) = p.listener.as_mut() {
                    listener.send_block_changed_ack();
                    listener.tick_throttlers();
                }
                let pos = p.position();
                // `resetPosition()`: the positions the client's packets of this tick are checked against
                if let Some(listener) = p.listener.as_mut() {
                    listener.reset_position(pos);
                }
                p.player.living.entity.set_old_pos(pos);
                // the portal step of the player's `baseTick` can move it to another level: its tick continues there
                if mc_game::commands::server_access::has_server() {
                    let began = p.do_tick_to_portal(level);
                    started = Some(began);
                    if began {
                        moved = mc_game::commands::server_access::with_server(|server| {
                            mc_game::server::players::dimension_change::handle_player_portal(
                                server, level, p,
                            )
                        });
                    }
                }
                if !moved {
                    Self::tick_player_body(level, p, pos, started);
                }
            });
            if moved {
                // the player lives in another level now
                self.focus_ref(&name);
                self.try_with_player(&name, |level, p| {
                    let pos = p.position();
                    Self::tick_player_body(level, p, pos, Some(true));
                });
            }
            // the removal the end credits asked for runs once the player is back in its level
            if mc_game::commands::server_access::has_server() {
                self.focus_ref(&name);
                let id = self.entity_id(&name);
                let level = self.level.as_mut().expect("world");
                if let Some(uuid) = level.get_entity(id).map(|e| e.uuid()) {
                    mc_game::commands::server_access::with_server(|server| {
                        server.with_lent_level(level, |server| {
                            mc_game::server::players::end_credits::finish_player_step(server, uuid)
                        })
                    });
                }
            }
        }
        self.focus(keep);
    }

    /// `tickPlayer()` from `doTick()`'s portal step on. `pos` is the position the tick started at.
    fn tick_player_body(
        level: &mut ServerLevel,
        p: &mut ServerPlayer,
        pos: mc_core::phys::Vec3,
        started: Option<bool>,
    ) {
        match started {
            Some(started) => p.do_tick_after_portal(level, started),
            None => p.do_tick(level),
        }
        let (y_rot, x_rot) = (p.get_y_rot(), p.get_x_rot());
        // a simulated player keeps the position its own physics produced (`SimPlayer.absSnapTo`)
        if !p.simulate_local_player {
            p.abs_snap_to(pos.x, pos.y, pos.z, y_rot, x_rot);
        }
        // the rest of `tickPlayer`: the tick count, the floating kicks, the vehicle a `move_vehicle` packet is checked against
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| {
                let mut ctx = mc_game::server::network::game_packets::PlayCtx {
                    server,
                    level,
                    player: p,
                };
                if ctx.finish_tick_player() {
                    ctx.handle_disconnection_without_socket();
                }
            });
        }
    }

    /// `MinecraftServer.tickChildren` "send chunks", after the players ticked: every player's `chunkSender.sendNextChunks(player)`. The
    /// oracle's connection never acknowledges a batch, so a player gets one batch of chunks and tracks entities in those only.
    pub(super) fn send_chunks(&mut self) {
        let keep = self.at;
        for name in self.actors.players.clone() {
            self.try_with_player(&name, |level, p| {
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
                        player: Entity::id(&*p),
                    },
                );
            });
        }
        self.focus(keep);
    }

    // ------------------------------------------------------------------------------------------------ dumps

    pub(super) fn dump_actor(&mut self, kind: &str, arg: Option<&str>) -> bool {
        match kind {
            "entities" | "xentities" => {
                self.name_new_entities();
                for (name, id) in self.live_refs(false) {
                    let line = self.describe(&name, id, kind == "xentities");
                    self.out.push(line);
                }
            }
            // `G type pos rot saved-fields` of every live entity but players, items, falling blocks and orbs, sorted (the driver's `placed`)
            "placed" => {
                const PLACED_KEYS: [&str; 11] = [
                    "Age",
                    "CatType",
                    "Color",
                    "Health",
                    "PersistenceRequired",
                    "RabbitType",
                    "Sheared",
                    "Type",
                    "Variant",
                    "VillagerData",
                    "variant",
                ];
                let level = self.level();
                let mut lines: Vec<String> = level
                    .entity_manager
                    .get_all()
                    .filter(|e| {
                        e.is_alive()
                            && e.as_any().downcast_ref::<ServerPlayer>().is_none()
                            && !matches!(
                                e.get_type(),
                                EntityType::ITEM
                                    | EntityType::FALLING_BLOCK
                                    | EntityType::EXPERIENCE_ORB
                            )
                    })
                    .map(|e| {
                        use mc_core::math::java::java_float_to_string as jf;
                        let (b, p) = (e.base(), e.base().position());
                        let mut line = format!(
                            "G {} pos={},{},{} rot={}",
                            e.get_type().name(),
                            mc_core::math::java::java_double_to_string(p.x),
                            mc_core::math::java::java_double_to_string(p.y),
                            mc_core::math::java::java_double_to_string(p.z),
                            if arg == Some("norot") {
                                "-".to_string()
                            } else {
                                format!("{},{}", jf(b.y_rot()), jf(b.x_rot()))
                            },
                        );
                        let mut tag = mc_nbt::CompoundTag::new();
                        e.save_without_id(&mut tag, &|i| level.get_entity(i));
                        for key in PLACED_KEYS {
                            if let Some(value) = tag.get(key) {
                                line.push_str(&format!(
                                    " {key}={}",
                                    crate::common::entity_scenario::canon(value)
                                ));
                            }
                        }
                        if arg == Some("attack") {
                            let base = tag
                                .get_list("attributes")
                                .into_iter()
                                .flat_map(|l| l.iter())
                                .filter_map(|t| t.as_compound())
                                .find(|c| c.get_string("id") == Some("minecraft:attack_damage"))
                                .and_then(|c| c.get("base"));
                            if let Some(base) = base {
                                line.push_str(&format!(
                                    " attack={}",
                                    crate::common::entity_scenario::canon(base)
                                ));
                            }
                        }
                        line
                    })
                    .collect();
                lines.sort();
                self.out.extend(lines);
            }
            // every live Mob as a rich `E` line (the entity replay's `mobs`)
            "mobs" => {
                self.name_new_entities();
                for (name, id) in self.live_refs(true) {
                    let line = self.describe(&name, id, true);
                    self.out.push(line);
                }
            }
            // `LEVEL DIM gametime= forced= players= border=` per level of the replay
            "levels" => {
                let dims = if self.multi() {
                    self.dims.clone()
                } else {
                    vec![self.at]
                };
                for dimension in dims {
                    self.focus(dimension);
                    let level = self.level();
                    let (forced, loaded, players) = (
                        level.chunk_source().get_force_loaded_chunks().len(),
                        level.chunk_source().get_loaded_chunks_count(),
                        level.players().len(),
                    );
                    let line = format!(
                        "LEVEL minecraft:{dimension} gametime={} forced={forced} loaded={loaded} players={players} border={}",
                        level.game_time(),
                        mc_core::math::java::java_double_to_string(
                            level.get_world_border().get_size()
                        ),
                    );
                    self.out.push(line);
                }
            }
            // `PL NAME dim=DIM` per player of the replay
            "players" => {
                let mut lines = Vec::new();
                for name in self.actors.players.clone() {
                    let id = self.entity_id(&name);
                    let located = self.dimension_of(id);
                    if let Some(d) = located {
                        self.focus(d);
                    }
                    // a player that left reports the level it left from
                    let dimension = match self.actors.departed.get(&name) {
                        Some(left) if located.is_none() && self.multi() => left.dimension.clone(),
                        _ => self.level().dimension.clone(),
                    };
                    lines.push(format!("PL {name} dim={dimension}"));
                }
                for (at, name, dimension) in &self.actors.stale_players {
                    lines.insert((*at).min(lines.len()), format!("PL {name} dim={dimension}"));
                }
                self.out.extend(lines);
            }
            "entity" | "player" | "xentity" | "mob" => {
                let name = arg.expect("entity:REF");
                let id = self.entity_id(name);
                let line = self.describe(name, id, matches!(kind, "xentity" | "mob"));
                self.out.push(line);
            }
            // RNG level|REF STATE: the 48-bit state of `Level.random` or of the entity's own random
            "rng" => {
                let name = arg.expect("rng:level|REF");
                let state = if name == "level" {
                    self.level().tick_state.random.state()
                } else {
                    let id = self.entity_id(name);
                    self.level()
                        .get_entity_or_removed(id)
                        .unwrap_or_else(|| panic!("entity {name} is gone"))
                        .base()
                        .random
                        .state()
                };
                self.out.push(format!("RNG {name} {state}"));
            }
            // SPAWNCHUNKS n cx,cz ..: the chunks `tickChunks` shuffles, sorted
            "spawnchunks" => {
                let mut keys: Vec<(i32, i32)> = self
                    .level()
                    .chunk_source_mut()
                    .chunk_map
                    .collect_spawning_chunks()
                    .into_iter()
                    .map(|k| {
                        let p = mc_core::pos::ChunkPos::unpack(k);
                        (p.x, p.z)
                    })
                    .collect();
                keys.sort_unstable();
                let chunks: String = keys.iter().map(|(x, z)| format!(" {x},{z}")).collect();
                self.out.push(format!("SPAWNCHUNKS {}{chunks}", keys.len()));
            }
            // PART REF index name id=+N pos=.. old=.. size=w,h bb=..: the parts of the ender dragon REF
            "parts" => {
                use mc_core::math::java::{
                    java_double_to_string as jd, java_float_to_string as jf,
                };
                let name = arg.expect("parts:REF");
                let id = self.entity_id(name);
                let level = self.level();
                let dragon = level
                    .get_entity_or_removed(id)
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                let part_ids = dragon.sub_entity_ids().to_vec();
                let mut lines = Vec::new();
                for (index, part_id) in part_ids.iter().enumerate() {
                    let part = level
                        .dragon_part(*part_id)
                        .unwrap_or_else(|| panic!("{name} has no part {index}"));
                    let base = part.base();
                    let (p, bb) = (base.position(), base.bounding_box());
                    let line = format!(
                        "PART {name} {index} {} id=+{} pos={},{},{} old={},{},{} size={},{} bb={},{},{},{},{},{}",
                        part.as_any()
                            .downcast_ref::<mc_game::world::entity::boss::ender_dragon::EnderDragonPart>()
                            .map_or("?", |p| p.name),
                        part_id - id,
                        jd(p.x),
                        jd(p.y),
                        jd(p.z),
                        jd(base.xo),
                        jd(base.yo),
                        jd(base.zo),
                        jf(base.dimensions.width),
                        jf(base.dimensions.height),
                        jd(bb.min_x),
                        jd(bb.min_y),
                        jd(bb.min_z),
                        jd(bb.max_x),
                        jd(bb.max_y),
                        jd(bb.max_z),
                    );
                    lines.push(line);
                }
                self.out.extend(lines);
            }
            // COUNTER N: the last entity id handed out
            "counter" => {
                let n = self
                    .level()
                    .entity_id_counter
                    .as_ref()
                    .expect("per-level counter")
                    .load(std::sync::atomic::Ordering::Relaxed);
                self.out.push(format!("COUNTER {n}"));
            }
            // ED REF id:serializer:hex ..
            "edata" => {
                let name = arg.expect("edata:REF");
                let id = self.entity_id(name);
                let level = self.level();
                let e = level
                    .get_entity_or_removed(id)
                    .unwrap_or_else(|| panic!("entity {name} is gone"));
                let line = super::super::entity_describe::entity_data_line(name, e);
                self.out.push(line);
            }
            "advancements" => {
                let name = arg.expect("advancements:PLAYER");
                let mut lines: Vec<(String, String)> = Vec::new();
                let holders = self.actors.advancements().get_all_advancements().to_vec();
                self.with_player_named(name, |_, p| {
                    let progress = p.advancements.lock();
                    for holder in &holders {
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
            // TRADERDATA delay=N chance=N
            "trader_data" => {
                let line = {
                    let data = self.level().shared.wandering_trader.lock();
                    format!(
                        "TRADERDATA delay={} chance={}",
                        data.spawn_delay(),
                        data.spawn_chance()
                    )
                };
                self.out.push(line);
            }
            "all_players" => {
                for name in self.actors.players.clone() {
                    let id = self.entity_id(&name);
                    let line = self.describe(&name, id, false);
                    self.out.push(line);
                }
            }
            // ADVFILE PLAYER LINE: `PlayerAdvancements.save()` and the file it wrote, the obtained times replaced by `T`
            "advfile" => {
                let name = arg.expect("advfile:PLAYER");
                let path = self.with_player_named(name, |_, p| {
                    let advancements = p.advancements.lock();
                    advancements.save();
                    advancements.save_path().expect("save path").to_owned()
                });
                let text = std::fs::read_to_string(&path).expect("saved advancement file");
                for line in text.lines() {
                    self.out
                        .push(format!("ADVFILE {name} {}", normalise_obtained_times(line)));
                }
            }
            "packets" | "advpackets" | "fightpackets" => {
                let arg = arg.expect("packets:NAME[:type,..]");
                let (name, only) = arg
                    .split_once(':')
                    .map_or((arg, None), |(n, f)| (n, Some(f)));
                // `fightpackets:NAME[:type,..]`: only the boss bar and level event packets (and the listed types), the boss bar uuid zeroed
                let fight_packets = kind == "fightpackets";
                let fight_types = format!(
                    "boss_event,level_event{}",
                    only.map_or(String::new(), |f| format!(",{f}"))
                );
                let only = if fight_packets {
                    Some(fight_types.as_str())
                } else {
                    only
                };
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
                for (ty, mut p) in debug_order(chunk_order(sent)) {
                    if only
                        .as_ref()
                        .is_none_or(|o| o.contains(&ty.trim_start_matches("minecraft:")) != except)
                    {
                        let hex = if kind == "advpackets" && ty == "minecraft:update_advancements" {
                            advancement_detail(&p)
                        } else if fight_packets && ty == "minecraft:boss_event" {
                            // the bar's uuid comes from the level's random
                            p[1..17].fill(0);
                            packet_text(ty, &p)
                        } else {
                            packet_text(ty, &p)
                        };
                        self.out.push(format!("PKT {name} {ty} {hex}"));
                    }
                }
            }
            _ => {
                return crate::common::mob_verbs::dump(self, kind, arg)
                    || spawn_verbs::dump(self, kind, arg)
                    || crate::common::player_actions::dump(self, kind, arg);
            }
        }
        true
    }

    /// The scenario names of the entities and the UUID table the dump context needs (the driver's `refs` / `uuidName`: a known entity
    /// prints as its scenario name, a removed one too, `refs` keeps it).
    pub(super) fn names_and_uuids(
        &self,
    ) -> (
        std::collections::BTreeMap<EntityId, String>,
        std::collections::BTreeMap<uuid::Uuid, EntityId>,
    ) {
        let level = self.level.as_ref().expect("world");
        let names = self
            .actors
            .refs
            .iter()
            .map(|(n, id)| (*id, n.clone()))
            .collect();
        // `uuidName` returns the first reference with the UUID (an entity the level refused for its UUID shares it)
        let mut uuids = std::collections::BTreeMap::new();
        for (_, id) in &self.actors.refs {
            // the driver names the entities of every level, not only the one it dumps
            let uuid = level
                .get_entity_or_removed(*id)
                .map(|e| e.uuid())
                .or_else(|| {
                    if !mc_game::commands::server_access::has_server() {
                        return None;
                    }
                    mc_game::commands::server_access::with_server(|server| {
                        server
                            .all_levels()
                            .find_map(|l| l.get_entity_or_removed(*id).map(|e| e.uuid()))
                    })
                })
                .or_else(|| self.actors.ref_uuids.get(id).copied())
                // an entity that was unloaded keeps the UUID it had
                .or_else(|| self.actors.persist_uuids.get(id).copied());
            if let Some(uuid) = uuid {
                uuids.entry(uuid).or_insert(*id);
            }
        }
        (names, uuids)
    }

    /// Remembers the UUID of every ref that is alive in some level (see [`Actors::ref_uuids`]); called before a tick moves entities away.
    pub(super) fn remember_ref_uuids(&mut self) {
        let focus = self.at;
        let ids: Vec<EntityId> = self.actors.refs.iter().map(|(_, id)| *id).collect();
        for id in ids {
            if self.actors.ref_uuids.contains_key(&id) {
                continue;
            }
            if let Some(dimension) = self.dimension_of(id) {
                self.focus(dimension);
            }
            if let Some(uuid) = self.level().get_entity(id).map(|e| e.uuid()) {
                self.actors.ref_uuids.insert(id, uuid);
            }
        }
        self.focus(focus);
    }

    /// The `P`/`E` line of the driver's `describe` (`rich`: with the fields of the registered classes, `xentity:REF`).
    fn describe(&mut self, name: &str, id: EntityId, rich: bool) -> String {
        if let Some(d) = self.dimension_of(id) {
            self.focus(d);
        }
        let (names, uuids) = self.names_and_uuids();
        let level = self.level();
        let ctx = mc_game::world::entity::scenario_dump::ScenarioDumpCtx {
            names: &names,
            uuids: &uuids,
        };
        // a player the end credits removed from its level: the server keeps it until it asks to respawn (the level keeps a
        // removed stand-in)
        let detached = mc_game::commands::server_access::has_server().then(|| {
            mc_game::commands::server_access::with_server(|server| {
                server
                    .detached_players
                    .values()
                    .find(|p| p.base_id() == id)
                    .map(|p| {
                        super::super::entity_describe::describe(
                            name,
                            &**p as &dyn Entity,
                            level,
                            &ctx,
                            rich,
                        )
                    })
            })
        });
        if let Some(line) = detached.flatten() {
            return line;
        }
        let e = level
            .get_entity_or_removed(id)
            .unwrap_or_else(|| panic!("entity {name} is gone"));
        super::super::entity_describe::describe(name, e, level, &ctx, rich)
    }

    /// The refs of the live entities that are no players (only Mobs with `mobs_only`), by id; each is looked up in its own level.
    fn live_refs(&mut self, mobs_only: bool) -> Vec<(String, EntityId)> {
        let mut all: Vec<(String, EntityId)> = self.actors.refs.clone();
        all.retain(|(n, id)| {
            if self.actors.players.contains(n) {
                return false;
            }
            if let Some(d) = self.dimension_of(*id) {
                self.focus(d);
            }
            self.level().get_entity_or_removed(*id).is_some_and(|e| {
                e.is_alive() && (!mobs_only || crate::common::entity_scenario::is_mob(e))
            })
        });
        all.sort_by_key(|(_, id)| *id);
        all
    }
}

impl spawn_verbs::SpawnHost for Scenario {
    const TICKING: bool = true;

    fn entity_names(&mut self) -> std::collections::BTreeMap<EntityId, String> {
        self.name_new_entities();
        self.actors
            .refs
            .iter()
            .map(|(n, id)| (*id, n.clone()))
            .collect()
    }

    fn all_entity_ids(&mut self) -> std::collections::BTreeSet<EntityId> {
        self.name_new_entities();
        self.actors.known.iter().copied().collect()
    }

    fn fresh_entity_lines(&mut self, before: &std::collections::BTreeSet<EntityId>) -> Vec<String> {
        self.name_new_entities();
        let mut fresh: Vec<(String, EntityId)> = self
            .actors
            .refs
            .iter()
            .filter(|(_, id)| !before.contains(id))
            .cloned()
            .collect();
        fresh.sort_by_key(|(_, id)| *id);
        fresh
            .into_iter()
            .map(|(name, id)| self.describe(&name, id, true))
            .collect()
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
    fn command_done(&mut self, command: &str, succeeded: bool) {
        self.after_command(command, succeeded);
    }
    fn departed(&mut self) -> Option<&mut crate::common::player_actions::Departed> {
        Some(&mut self.actors.departed)
    }
}

impl crate::common::mob_verbs::MobHost for Scenario {
    fn fail(&self, msg: &str) -> ! {
        panic!("scenario line {}: `{}`: {msg}", self.line_no, self.line)
    }
    fn dump_names(
        &self,
    ) -> (
        std::collections::BTreeMap<EntityId, String>,
        std::collections::BTreeMap<uuid::Uuid, EntityId>,
    ) {
        self.names_and_uuids()
    }
}
