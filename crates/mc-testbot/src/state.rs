//! Tracked client state and the packet handlers that update it (the part of `ClientPacketListener`
//! the bot needs). Handlers push the packets the real client would answer with into `out`.
use std::collections::HashMap;
use std::time::Instant;

use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::block::BlockState;
use mc_protocol::packets::play::*;

use crate::inventory::Menu;
use crate::join::{Configured, SyncedRegistry};
use crate::physics::{MoveOut, PlayerBody};
use crate::tracking::*;
use crate::world::ClientWorld;
use mc_protocol::packets::play::{ClientboundPlayPacket, ServerboundPlayPacket};

pub type Out = Vec<ServerboundPlayPacket>;

fn ident(s: &str) -> Identifier {
    Identifier::parse(s).expect("static identifier")
}

/// `ChunkBatchSizeCalculator`: the chunk rate the client asks the server for.
#[derive(Debug)]
pub struct ChunkBatchSizeCalculator {
    aggregated_nanos_per_chunk: f64,
    old_samples_weight: i32,
    batch_start: Instant,
}

impl Default for ChunkBatchSizeCalculator {
    fn default() -> Self {
        Self {
            aggregated_nanos_per_chunk: 2_000_000.0,
            old_samples_weight: 1,
            batch_start: Instant::now(),
        }
    }
}

impl ChunkBatchSizeCalculator {
    pub fn on_batch_start(&mut self) {
        self.batch_start = Instant::now();
    }

    pub fn on_batch_finished(&mut self, batch_size: i32) {
        if batch_size > 0 {
            let duration = self.batch_start.elapsed().as_nanos() as f64;
            let per_chunk = duration / batch_size as f64;
            let clamped = per_chunk.clamp(
                self.aggregated_nanos_per_chunk / 3.0,
                self.aggregated_nanos_per_chunk * 3.0,
            );
            self.aggregated_nanos_per_chunk =
                (self.aggregated_nanos_per_chunk * self.old_samples_weight as f64 + clamped)
                    / (self.old_samples_weight + 1) as f64;
            self.old_samples_weight = (self.old_samples_weight + 1).min(49);
        }
    }

    pub fn desired_chunks_per_tick(&self) -> f32 {
        (7_000_000.0 / self.aggregated_nanos_per_chunk) as f32
    }
}

/// One observed block change (`block_update` / `section_blocks_update`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockUpdate {
    pub tick: u64,
    pub pos: BlockPos,
    pub state: BlockState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatLine {
    pub tick: u64,
    /// Packet kind: `player_chat`, `system_chat`, `disguised_chat`.
    pub kind: &'static str,
    /// Plain text (translations resolved with en_us).
    pub text: String,
    /// The message component as JSON (empty for a `player_chat` without unsigned content).
    pub json: String,
    /// True for system messages shown on the action bar.
    pub overlay: bool,
    /// Sender display name for chat messages.
    pub sender: Option<String>,
}

/// Walk target steered by the tick loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkGoal {
    pub x: f64,
    pub z: f64,
    pub radius: f64,
    pub sprint: bool,
    /// Give up (leaving `goal_failed` set) after this many ticks.
    pub ticks_left: u32,
}

#[derive(Debug, Default)]
pub struct Control {
    /// Keys held by the script.
    pub input: Input,
    pub goal: Option<WalkGoal>,
    pub goal_failed: bool,
    /// Ticks remaining to hold the jump key (from `Bot::jump`).
    pub jump_ticks: u32,
    pub last_sent_input: Input,
    pub last_sent_sprint: bool,
}

/// Block currently being broken (`MultiPlayerGameMode.destroyBlockPos` & co).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dig {
    pub pos: BlockPos,
    pub face: Direction,
    pub progress: f32,
}

/// What the held item contributes to mining (`Player.getDestroySpeed` / `hasCorrectToolForDrops`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandTool {
    pub speed: f32,
    pub correct_for_drops: bool,
}

#[derive(Debug, Default)]
pub struct LevelLoad {
    pub chunks_load_started: bool,
    pub player_loaded_sent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    /// Connected, `login` not yet received.
    Joining,
    Playing,
    /// Connection ended: the reason shown to the player (kick message / error text).
    Disconnected(String),
}

#[derive(Debug)]
pub struct State {
    pub phase: Phase,
    pub configured: Configured,
    pub entity_id: i32,
    pub game_mode: GameType,
    pub previous_game_mode: Option<GameType>,
    pub hardcore: bool,
    pub dimension: Identifier,
    pub sea_level: i32,
    pub world: ClientWorld,
    pub body: PlayerBody,
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub xp_progress: f32,
    pub xp_level: i32,
    pub xp_total: i32,
    pub dead: bool,
    pub immediate_respawn: bool,
    pub tick: u64,
    pub game_time: i64,
    pub clocks: HashMap<i32, ClockNetworkState>,
    pub rain_level: f32,
    pub thunder_level: f32,
    pub difficulty: Option<Difficulty>,
    pub chunk_center: (i32, i32),
    pub chunks_received: u64,
    pub batch: ChunkBatchSizeCalculator,
    pub level_load: LevelLoad,
    pub block_updates: Vec<BlockUpdate>,
    pub acked_sequence: i32,
    pub next_sequence: i32,
    pub last_teleport_id: Option<i32>,
    pub teleports: u32,
    pub control: Control,
    pub digging: Option<Dig>,
    pub dig_delay: u32,
    /// Blocks whose breaking the client completed (STOP sent or creative break).
    pub dug: Vec<BlockPos>,
    pub effects: Vec<(mc_data::MobEffect, i32)>,
    /// Root literals of the command tree.
    pub commands: Vec<String>,
    pub entities: HashMap<i32, TrackedEntity>,
    pub social: Social,
    /// The player's own inventory menu (container id 0).
    pub inventory: Menu,
    /// The currently open container menu, if any.
    pub open_menu: Option<Menu>,
    pub selected_slot: usize,
    /// Signed chat messages seen since the last `chat_ack`.
    pub pending_chat_acks: i32,
    pub chat: Vec<ChatLine>,
    pub custom_payloads: Vec<(Identifier, Vec<u8>)>,
    pub transfer: Option<(String, i32)>,
    pub reconfigure_requested: bool,
    /// Non-fatal protocol problems noticed while handling packets (chunk decode errors, …).
    pub errors: Vec<String>,
    /// See [`Observed`].
    pub events: Vec<Observed>,
    /// Own attribute snapshots from `update_attributes` (base, modifiers); see [`State::attribute_value`].
    pub attributes: HashMap<mc_data::Attribute, (f64, Vec<AttributeModifier>)>,
    /// Last `merchant_offers` packet (villager / wandering trader trading screen).
    pub merchant_offers: Option<ClientboundMerchantOffersPacket>,
}

/// Server events a scenario can wait for / compare (the log only grows; take `events.len()` as a mark before an action).
#[derive(Debug, Clone, PartialEq)]
pub enum Observed {
    /// `player_combat_kill`: the player died; `message` is the plain death message.
    Death {
        tick: u64,
        player_id: i32,
        message: String,
    },
    /// `damage_event` (entity took damage; ids `-1` = none, `source_type` = raw damage_type registry id).
    Damage {
        tick: u64,
        entity_id: i32,
        source_type: i32,
        cause_id: i32,
        direct_id: i32,
    },
    /// `explode`.
    Explosion {
        tick: u64,
        center: Vec3,
        radius: f32,
        block_count: i32,
        knockback: Option<Vec3>,
    },
    /// `take_item_entity` (pickup animation: `collector_id` picked `amount` of entity `item_id`).
    TakeItem {
        tick: u64,
        item_id: i32,
        collector_id: i32,
        amount: i32,
    },
    /// `entity_event` (status byte), e.g. 2 hurt, 3 death, 9 use item finished.
    EntityEvent {
        tick: u64,
        entity_id: i32,
        event_id: i8,
    },
    /// `hurt_animation`.
    HurtAnimation { tick: u64, entity_id: i32 },
    /// `respawn` (after the death screen).
    Respawn { tick: u64 },
}

/// `(min_y, height)` of a dimension type from the synced registry (or the vanilla data pack for entries sent
/// without data because the client knows them from the known pack).
fn dimension_height(registries: &[SyncedRegistry], dimension_type: i32) -> (i32, i32) {
    let entry = registries
        .iter()
        .find(|r| r.id == ident("minecraft:dimension_type"))
        .and_then(|r| {
            usize::try_from(dimension_type)
                .ok()
                .and_then(|i| r.entries.get(i))
        });
    if let Some((name, data)) = entry {
        if let Some(mc_nbt::Tag::Compound(c)) = data {
            return (c.get_int_or("min_y", 0), c.get_int_or("height", 256));
        }
        let path = format!(
            "data/{}/dimension_type/{}.json",
            name.namespace(),
            name.path()
        );
        if let Some(json) = mc_data::datapack::read_string(&path)
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            let get = |k: &str, d: i64| json[k].as_i64().unwrap_or(d) as i32;
            return (get("min_y", 0), get("height", 256));
        }
    }
    (0, 256)
}

fn biome_count(registries: &[SyncedRegistry]) -> usize {
    registries
        .iter()
        .find(|r| r.id == ident("minecraft:worldgen/biome"))
        .map_or(64, |r| r.entries.len())
}

impl State {
    pub fn new(configured: Configured) -> Self {
        let world = ClientWorld::new(0, 256, biome_count(&configured.registries));
        State {
            phase: Phase::Joining,
            configured,
            entity_id: -1,
            game_mode: GameType::Survival,
            previous_game_mode: None,
            hardcore: false,
            dimension: ident("minecraft:overworld"),
            sea_level: 63,
            world,
            body: PlayerBody::new(Vec3::ZERO, 0.0, 0.0),
            health: 20.0,
            food: 20,
            saturation: 5.0,
            xp_progress: 0.0,
            xp_level: 0,
            xp_total: 0,
            dead: false,
            immediate_respawn: false,
            tick: 0,
            game_time: 0,
            clocks: HashMap::new(),
            rain_level: 0.0,
            thunder_level: 0.0,
            difficulty: None,
            chunk_center: (0, 0),
            chunks_received: 0,
            batch: ChunkBatchSizeCalculator::default(),
            level_load: LevelLoad::default(),
            block_updates: Vec::new(),
            acked_sequence: -1,
            next_sequence: 0,
            last_teleport_id: None,
            teleports: 0,
            control: Control::default(),
            digging: None,
            dig_delay: 0,
            dug: Vec::new(),
            effects: Vec::new(),
            commands: Vec::new(),
            entities: HashMap::new(),
            social: Social::default(),
            inventory: Menu::player_inventory(),
            open_menu: None,
            selected_slot: 0,
            pending_chat_acks: 0,
            chat: Vec::new(),
            custom_payloads: Vec::new(),
            transfer: None,
            reconfigure_requested: false,
            errors: Vec::new(),
            events: Vec::new(),
            attributes: HashMap::new(),
            merchant_offers: None,
        }
    }

    /// Own air supply from the synched entity data (accessor 1 of `Entity`), if the server sent it.
    pub fn air(&self) -> Option<i32> {
        match self.entities.get(&self.entity_id)?.data.get(&1)? {
            EntityDataValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn joined(&self) -> bool {
        self.phase == Phase::Playing
    }

    pub fn disconnected(&self) -> Option<&str> {
        match &self.phase {
            Phase::Disconnected(r) => Some(r),
            _ => None,
        }
    }

    pub fn disconnect(&mut self, reason: impl Into<String>) {
        if !matches!(self.phase, Phase::Disconnected(_)) {
            self.phase = Phase::Disconnected(reason.into());
        }
    }

    /// Next block-interaction sequence number (`BlockStatePredictionHandler.startPredicting`).
    pub fn next_sequence(&mut self) -> i32 {
        self.next_sequence += 1;
        self.next_sequence
    }

    /// Registry entry name by protocol id.
    pub fn registry_entry(&self, registry: &str, id: i32) -> Option<&Identifier> {
        let reg = ident(registry);
        self.configured
            .registries
            .iter()
            .find(|r| r.id == reg)?
            .entries
            .get(usize::try_from(id).ok()?)
            .map(|(n, _)| n)
    }

    /// Ticks of a world clock by registry name (`minecraft:overworld`), from `set_time`.
    pub fn clock_ticks(&self, name: &str) -> Option<i64> {
        let id = self
            .configured
            .registries
            .iter()
            .find(|r| r.id == ident("minecraft:world_clock"))?
            .entries
            .iter()
            .position(|(n, _)| *n == ident(name))?;
        self.clocks.get(&(id as i32)).map(|c| c.total_ticks)
    }

    fn reset_level(&mut self, info: &CommonPlayerSpawnInfo) {
        let (min_y, height) = dimension_height(&self.configured.registries, info.dimension_type);
        self.world = ClientWorld::new(min_y, height, biome_count(&self.configured.registries));
        self.dimension = info.dimension.clone();
        self.sea_level = info.sea_level;
        self.game_mode = info.game_type;
        self.previous_game_mode = info.previous_game_type;
        self.level_load = LevelLoad::default();
        self.chunk_center = (0, 0);
        self.body.abilities.instabuild = info.game_type == GameType::Creative;
        self.body.abilities.may_fly =
            matches!(info.game_type, GameType::Creative | GameType::Spectator);
        self.control.goal = None;
        self.control.input = Input::default();
        self.entities.clear();
    }

    /// Update everything that follows from the packet; replies go to `out`.
    pub fn on_packet(&mut self, packet: ClientboundPlayPacket, out: &mut Out) {
        use ClientboundPlayPacket as P;
        match packet {
            P::KeepAlive(p) => out.push(ServerboundPlayPacket::KeepAlive(
                ServerboundKeepAlivePacket { id: p.id },
            )),
            P::Ping(p) => out.push(ServerboundPlayPacket::Pong(ServerboundPongPacket {
                id: p.id,
            })),
            P::Disconnect(p) => self.disconnect(p.reason.get_string()),
            P::Transfer(p) => {
                self.transfer = Some((p.host.clone(), p.port));
                self.disconnect(format!("transfer to {}:{}", p.host, p.port));
            }
            P::StartConfiguration(_) => self.reconfigure_requested = true,
            P::CustomPayload(p) => self
                .custom_payloads
                .push((p.payload.id.clone(), p.payload.data.clone())),
            P::Login(p) => {
                self.entity_id = p.player_id;
                self.hardcore = p.hardcore;
                self.reset_level(&p.common_player_spawn_info);
                self.phase = Phase::Playing;
            }
            P::Respawn(p) => {
                self.reset_level(&p.common_player_spawn_info);
                if p.data_to_keep & 1 == 0 {
                    // attributes are not kept: abilities/effects reset by the server's following packets
                    self.body.vel = Vec3::ZERO;
                }
                self.dead = false;
                self.health = 20.0;
                self.events.push(Observed::Respawn { tick: self.tick });
            }
            P::PlayerPosition(p) => self.on_player_position(p, out),
            P::PlayerRotation(p) => {
                let b = &mut self.body;
                b.yaw = if p.relative_y {
                    b.yaw + p.y_rot
                } else {
                    p.y_rot
                };
                b.pitch = if p.relative_x {
                    b.pitch + p.x_rot
                } else {
                    p.x_rot
                };
            }
            P::PlayerAbilities(p) => {
                let a = &mut self.body.abilities;
                a.invulnerable = p.invulnerable;
                a.flying = p.is_flying;
                a.may_fly = p.can_fly;
                a.instabuild = p.instabuild;
                a.flying_speed = p.flying_speed;
                a.walking_speed = p.walking_speed;
            }
            P::ChunkBatchStart(_) => self.batch.on_batch_start(),
            P::ChunkBatchFinished(p) => {
                self.batch.on_batch_finished(p.batch_size);
                out.push(ServerboundPlayPacket::ChunkBatchReceived(
                    ServerboundChunkBatchReceivedPacket {
                        desired_chunks_per_tick: self.batch.desired_chunks_per_tick(),
                    },
                ));
            }
            P::LevelChunkWithLight(p) => {
                match self.world.load_chunk(p.x, p.z, &p.chunk_data.buffer) {
                    Ok(()) => self.chunks_received += 1,
                    Err(e) => self.errors.push(format!("chunk {},{}: {e}", p.x, p.z)),
                }
            }
            P::ForgetLevelChunk(p) => self.world.unload_chunk(p.pos.x, p.pos.z),
            P::SetChunkCacheCenter(p) => self.chunk_center = (p.x, p.z),
            P::BlockUpdate(p) => self.set_block(p.pos, p.block_state),
            P::SectionBlocksUpdate(p) => {
                let (sx, sy, sz) = (
                    p.section_pos.x() << 4,
                    p.section_pos.y() << 4,
                    p.section_pos.z() << 4,
                );
                for (packed, state) in p.changes {
                    let pos = BlockPos::new(
                        sx + (packed >> 8 & 15) as i32,
                        sy + (packed & 15) as i32,
                        sz + (packed >> 4 & 15) as i32,
                    );
                    self.set_block(pos, state);
                }
            }
            P::BlockChangedAck(p) => self.acked_sequence = self.acked_sequence.max(p.sequence),
            P::GameEvent(p) => self.on_game_event(p),
            P::SetTime(p) => {
                self.game_time = p.game_time;
                for (id, c) in p.clock_updates {
                    self.clocks.insert(id, c);
                }
            }
            _ => self.on_other(packet, out),
        }
    }

    fn set_block(&mut self, pos: BlockPos, state: BlockState) {
        self.world
            .set_block_state_id(pos.x(), pos.y(), pos.z(), state.id() as u16);
        self.block_updates.push(BlockUpdate {
            tick: self.tick,
            pos,
            state,
        });
        if self.block_updates.len() > 20_000 {
            self.block_updates.drain(..10_000);
        }
    }

    fn on_game_event(&mut self, p: ClientboundGameEventPacket) {
        match p.event {
            ClientboundGameEventPacket::START_RAINING => self.rain_level = 1.0,
            ClientboundGameEventPacket::STOP_RAINING => self.rain_level = 0.0,
            ClientboundGameEventPacket::CHANGE_GAME_MODE => {
                self.game_mode = GameType::by_id(p.param as i32);
                self.body.abilities.instabuild = self.game_mode == GameType::Creative;
            }
            ClientboundGameEventPacket::RAIN_LEVEL_CHANGE => self.rain_level = p.param,
            ClientboundGameEventPacket::THUNDER_LEVEL_CHANGE => self.thunder_level = p.param,
            ClientboundGameEventPacket::IMMEDIATE_RESPAWN => {
                self.immediate_respawn = p.param == 1.0
            }
            ClientboundGameEventPacket::LEVEL_CHUNKS_LOAD_START => {
                self.level_load.chunks_load_started = true
            }
            _ => {}
        }
    }

    /// `ClientPacketListener.handleMovePlayer` (teleport from the server).
    fn on_player_position(&mut self, p: ClientboundPlayerPositionPacket, out: &mut Out) {
        let b = &self.body;
        let (cur_pos, cur_vel) = (b.pos, b.vel);
        let rel = |r: Relative| p.relatives.contains(r);
        let pos = Vec3::new(
            if rel(Relative::X) {
                cur_pos.x + p.change.position.x
            } else {
                p.change.position.x
            },
            if rel(Relative::Y) {
                cur_pos.y + p.change.position.y
            } else {
                p.change.position.y
            },
            if rel(Relative::Z) {
                cur_pos.z + p.change.position.z
            } else {
                p.change.position.z
            },
        );
        let yaw = if rel(Relative::YRot) {
            b.yaw + p.change.y_rot
        } else {
            p.change.y_rot
        };
        let pitch = if rel(Relative::XRot) {
            b.pitch + p.change.x_rot
        } else {
            p.change.x_rot
        };
        let mut vel = cur_vel;
        if rel(Relative::RotateDelta) {
            let (dy, dx) = (p.change.y_rot - b.yaw, p.change.x_rot - b.pitch);
            vel = vel
                .x_rot(-dx * (std::f32::consts::PI / 180.0))
                .y_rot(-dy * (std::f32::consts::PI / 180.0));
        }
        let vel = Vec3::new(
            if rel(Relative::DeltaX) {
                vel.x + p.change.delta_movement.x
            } else {
                p.change.delta_movement.x
            },
            if rel(Relative::DeltaY) {
                vel.y + p.change.delta_movement.y
            } else {
                p.change.delta_movement.y
            },
            if rel(Relative::DeltaZ) {
                vel.z + p.change.delta_movement.z
            } else {
                p.change.delta_movement.z
            },
        );
        // `setPos` + rotation; the position/rotation "last sent" bookkeeping of LocalPlayer is left untouched
        self.body.pos = pos;
        self.body.vel = vel;
        self.body.yaw = yaw;
        self.body.pitch = pitch;
        self.last_teleport_id = Some(p.id);
        self.teleports += 1;
        out.push(ServerboundPlayPacket::AcceptTeleportation(
            ServerboundAcceptTeleportationPacket {
                id: p.id,
                x: pos.x,
                y: pos.y,
                z: pos.z,
                y_rot: yaw,
                x_rot: pitch,
            },
        ));
    }

    /// `AttributeInstance.getValue` of an own attribute known from `update_attributes` (`default` when never sent).
    pub fn attribute_value(&self, attribute: mc_data::Attribute, default: f64) -> f64 {
        let Some((base, mods)) = self.attributes.get(&attribute) else {
            return default;
        };
        let sum = |op: AttributeModifierOperation| {
            mods.iter()
                .filter(move |m| m.operation == op)
                .map(|m| m.amount)
        };
        let base_total = base + sum(AttributeModifierOperation::AddValue).sum::<f64>();
        let mut value =
            base_total * (1.0 + sum(AttributeModifierOperation::AddMultipliedBase).sum::<f64>());
        for m in sum(AttributeModifierOperation::AddMultipliedTotal) {
            value *= 1.0 + m;
        }
        value
    }

    fn effect_amp(&self, effect: mc_data::MobEffect) -> Option<i32> {
        self.effects
            .iter()
            .find(|(e, _)| *e == effect)
            .map(|(_, amp)| *amp - 1)
    }

    /// `Player.getDestroySpeed` for the held stack (attributes and effects as the server told this client).
    pub fn destroy_speed(&self, block: mc_data::Block) -> f32 {
        use mc_data::{Attribute, MobEffect};
        let mut speed = self.tool_for(block).speed;
        if speed > 1.0 {
            speed += self.attribute_value(Attribute::MINING_EFFICIENCY, 0.0) as f32;
        }
        let dig = [
            self.effect_amp(MobEffect::HASTE),
            self.effect_amp(MobEffect::CONDUIT_POWER),
        ]
        .into_iter()
        .flatten()
        .max();
        if let Some(amp) = dig {
            speed *= 1.0 + (amp + 1) as f32 * 0.2;
        }
        if let Some(amp) = self.effect_amp(MobEffect::MINING_FATIGUE) {
            speed *= 0.3f64.powi(amp + 1) as f32;
        }
        speed *= self.attribute_value(Attribute::BLOCK_BREAK_SPEED, 1.0) as f32;
        let eye = self.body.eye_position();
        let in_water = self
            .world
            .block_state(
                eye.x.floor() as i32,
                eye.y.floor() as i32,
                eye.z.floor() as i32,
            )
            .is_some_and(|s| s.block() == mc_data::Block::WATER);
        if in_water {
            speed *= self.attribute_value(Attribute::SUBMERGED_MINING_SPEED, 0.2) as f32;
        }
        if !self.body.on_ground {
            speed /= 5.0;
        }
        speed
    }

    /// `BlockBehaviour.getDestroyProgress` per tick for the block at `pos` (`Player.getDestroySpeed / hardness / 30|100`).
    pub fn destroy_progress(&self, pos: BlockPos) -> f32 {
        let Some(state) = self.world.block_state_at(pos) else {
            return 0.0;
        };
        let hardness = state.data().destroy_speed;
        if hardness == -1.0 {
            return 0.0;
        }
        let correct = !state.requires_correct_tool_for_drops()
            || self.tool_for(state.block()).correct_for_drops;
        self.destroy_speed(state.block()) / hardness / if correct { 30.0 } else { 100.0 }
    }

    /// Mining speed / drop correctness of the held stack against `block` (`Tool` rules, else bare hand).
    pub fn tool_for(&self, block: mc_data::Block) -> HandTool {
        use mc_protocol::components::{RegistrySet, TOOL};
        let mut out = HandTool {
            speed: 1.0,
            correct_for_drops: false,
        };
        let Some(t) = self.held_item().get(&TOOL) else {
            return out;
        };
        out.speed = t.default_mining_speed;
        let matches = |set: &RegistrySet<mc_data::Block>| match set {
            RegistrySet::Tag(tag) => block.is_in_tag(&tag.to_string()),
            RegistrySet::Entries(v) => v.contains(&block),
        };
        if let Some(rule) = t
            .rules
            .iter()
            .find(|r| r.correct_for_drops.is_some() && matches(&r.blocks))
        {
            out.correct_for_drops = rule.correct_for_drops.unwrap_or(false);
        }
        if let Some(rule) = t
            .rules
            .iter()
            .find(|r| r.speed.is_some() && matches(&r.blocks))
        {
            out.speed = rule.speed.unwrap_or(1.0);
        }
        out
    }

    fn destroy_block_locally(&mut self, pos: BlockPos) {
        let air = mc_data::Block::AIR.default_state();
        self.world
            .set_block_state_id(pos.x(), pos.y(), pos.z(), air.id() as u16);
        self.dug.push(pos);
    }

    /// `MultiPlayerGameMode.startDestroyBlock`; returns the packets.
    pub fn start_dig(&mut self, pos: BlockPos, face: Direction, out: &mut Out) {
        if self.body.abilities.instabuild {
            let sequence = self.next_sequence();
            out.push(ServerboundPlayPacket::PlayerAction(
                ServerboundPlayerActionPacket {
                    action: ServerboundPlayerActionAction::StartDestroyBlock,
                    pos,
                    direction: face,
                    sequence,
                },
            ));
            self.destroy_block_locally(pos);
            self.dig_delay = 5;
            return;
        }
        if let Some(old) = self.digging.take() {
            let sequence = self.next_sequence();
            out.push(ServerboundPlayPacket::PlayerAction(
                ServerboundPlayerActionPacket {
                    action: ServerboundPlayerActionAction::AbortDestroyBlock,
                    pos: old.pos,
                    direction: old.face,
                    sequence,
                },
            ));
        }
        let sequence = self.next_sequence();
        out.push(ServerboundPlayPacket::PlayerAction(
            ServerboundPlayerActionPacket {
                action: ServerboundPlayerActionAction::StartDestroyBlock,
                pos,
                direction: face,
                sequence,
            },
        ));
        let solid = self
            .world
            .block_state_at(pos)
            .is_some_and(|s| !s.block().is_air());
        if solid && self.destroy_progress(pos) >= 1.0 {
            self.destroy_block_locally(pos);
        } else {
            self.digging = Some(Dig {
                pos,
                face,
                progress: 0.0,
            });
        }
    }

    /// Stop breaking without finishing (`stopDestroyBlock`).
    pub fn abort_dig(&mut self, out: &mut Out) {
        if let Some(d) = self.digging.take() {
            let sequence = self.next_sequence();
            out.push(ServerboundPlayPacket::PlayerAction(
                ServerboundPlayerActionPacket {
                    action: ServerboundPlayerActionAction::AbortDestroyBlock,
                    pos: d.pos,
                    direction: d.face,
                    sequence,
                },
            ));
        }
    }

    /// `continueDestroyBlock` for one tick.
    fn tick_digging(&mut self, out: &mut Out) {
        if self.dig_delay > 0 {
            self.dig_delay -= 1;
            return;
        }
        let Some(mut d) = self.digging else { return };
        if self
            .world
            .block_state_at(d.pos)
            .is_none_or(|s| s.block().is_air())
        {
            self.digging = None;
            return;
        }
        d.progress += self.destroy_progress(d.pos);
        if d.progress >= 1.0 {
            self.digging = None;
            let sequence = self.next_sequence();
            out.push(ServerboundPlayPacket::PlayerAction(
                ServerboundPlayerActionPacket {
                    action: ServerboundPlayerActionAction::StopDestroyBlock,
                    pos: d.pos,
                    direction: d.face,
                    sequence,
                },
            ));
            self.destroy_block_locally(d.pos);
            self.dig_delay = 5;
        } else {
            self.digging = Some(d);
        }
    }

    /// Input for this tick: keys held by the script plus what the walk goal wants.
    fn steer(&mut self) -> Input {
        let mut input = self.control.input;
        if self.control.jump_ticks > 0 {
            input.jump = true;
            self.control.jump_ticks -= 1;
        }
        if let Some(g) = &mut self.control.goal {
            let (dx, dz) = (g.x - self.body.pos.x, g.z - self.body.pos.z);
            if dx.hypot(dz) <= g.radius {
                self.control.goal = None;
            } else if g.ticks_left == 0 {
                self.control.goal = None;
                self.control.goal_failed = true;
            } else {
                g.ticks_left -= 1;
                self.body.yaw = mc_core::math::mth::wrap_degrees_f32(
                    (dz.atan2(dx) * 57.29577951308232) as f32 - 90.0,
                );
                input.forward = true;
                input.sprint |= g.sprint;
                // auto-jump: blocked while standing
                if self.body.horizontal_collision && self.body.on_ground {
                    input.jump = true;
                }
            }
        }
        input
    }

    /// One client tick (`LocalPlayer.tick` + the tick-end packet).
    pub fn client_tick(&mut self, out: &mut Out) {
        if !self.joined() {
            return;
        }
        self.tick += 1;
        let (bx, bz) = (self.body.block_x(), self.body.block_z());
        if self.level_load.chunks_load_started
            && !self.level_load.player_loaded_sent
            && self.teleports > 0
            && self.world.has_chunk_at_block(bx, bz)
        {
            self.level_load.player_loaded_sent = true;
            out.push(ServerboundPlayPacket::PlayerLoaded(
                ServerboundPlayerLoadedPacket,
            ));
        }
        // LocalPlayer only simulates inside loaded chunks
        if self.world.has_chunk_at_block(bx, bz) && !self.dead {
            let input = self.steer();
            self.body.food_level = self.food;
            self.body.tick(&self.world, input);
            if input != self.control.last_sent_input {
                self.control.last_sent_input = input;
                out.push(ServerboundPlayPacket::PlayerInput(
                    ServerboundPlayerInputPacket { input },
                ));
            }
            if self.body.sprinting != self.control.last_sent_sprint {
                self.control.last_sent_sprint = self.body.sprinting;
                out.push(ServerboundPlayPacket::PlayerCommand(
                    ServerboundPlayerCommandPacket {
                        id: self.entity_id,
                        action: if self.body.sprinting {
                            ServerboundPlayerCommandAction::StartSprinting
                        } else {
                            ServerboundPlayerCommandAction::StopSprinting
                        },
                        data: 0,
                    },
                ));
            }
            self.tick_digging(out);
            if let Some(m) = self.body.position_packet() {
                out.push(match m {
                    MoveOut::PosRot {
                        x,
                        y,
                        z,
                        yaw,
                        pitch,
                        on_ground,
                        h_collision,
                    } => {
                        ServerboundPlayPacket::MovePlayerPosRot(ServerboundMovePlayerPosRotPacket {
                            x,
                            y,
                            z,
                            y_rot: yaw,
                            x_rot: pitch,
                            on_ground,
                            horizontal_collision: h_collision,
                        })
                    }
                    MoveOut::Pos {
                        x,
                        y,
                        z,
                        on_ground,
                        h_collision,
                    } => ServerboundPlayPacket::MovePlayerPos(ServerboundMovePlayerPosPacket {
                        x,
                        y,
                        z,
                        on_ground,
                        horizontal_collision: h_collision,
                    }),
                    MoveOut::Rot {
                        yaw,
                        pitch,
                        on_ground,
                        h_collision,
                    } => ServerboundPlayPacket::MovePlayerRot(ServerboundMovePlayerRotPacket {
                        y_rot: yaw,
                        x_rot: pitch,
                        on_ground,
                        horizontal_collision: h_collision,
                    }),
                    MoveOut::Status {
                        on_ground,
                        h_collision,
                    } => ServerboundPlayPacket::MovePlayerStatusOnly(
                        ServerboundMovePlayerStatusOnlyPacket {
                            on_ground,
                            horizontal_collision: h_collision,
                        },
                    ),
                });
            }
        }
        out.push(ServerboundPlayPacket::ClientTickEnd(
            ServerboundClientTickEndPacket,
        ));
    }
}

impl State {
    /// After a click on the open menu: mirror its player-inventory tail into the inventory menu (and the cursor).
    pub fn mirror_menu_after_click(&mut self) {
        if let Some(open) = &self.open_menu {
            for i in 0..36 {
                if let (Some(a), Some(b)) = (
                    self.inventory.slot_of_inventory(i),
                    open.slot_of_inventory(i),
                ) {
                    self.inventory.slots[a] = open.slots[b].clone();
                }
            }
            self.inventory.cursor = open.cursor.clone();
        }
    }
}
