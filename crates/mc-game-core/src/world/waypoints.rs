//! `net.minecraft.world.waypoints`: the locator bar's waypoints. `Waypoint.Icon`, `WaypointTransmitter` and its three
//! connection kinds (`EntityAzimuthConnection`, `EntityBlockConnection`, `EntityChunkConnection`).
//!
//! Vanilla's connections hold the transmitting `LivingEntity` and the receiving `ServerPlayer`; entities live in the
//! level's storage here (and are taken out of it while they tick), so a connection keeps only ids plus what it needs
//! after the entity is gone (the transmitter's UUID) and reads the live state through [`WaypointContext`].

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_dfu::extra::RGB_COLOR_CODEC;
use mc_dfu::{Codec, MapCodec};
use mc_protocol::codec::RgbColor;
use mc_protocol::packets::play::{
    ClientboundTrackedWaypointPacket, TrackedWaypoint, TrackedWaypointData, WaypointIcon,
    WaypointOperation,
};
use uuid::Uuid;

/// `WaypointTransmitter.REALLY_FAR_DISTANCE`
pub const REALLY_FAR_DISTANCE: i32 = 332;

/// `ResourceKey<WaypointStyleAsset>` ids are plain identifiers here: `WaypointStyleAssets.DEFAULT`.
pub fn default_style() -> Identifier {
    Identifier::with_default_namespace("default").expect("valid")
}

/// `TeamColor.BLACK` is drawn as this dark grey (`-13619152`) instead of its pure black.
pub const TEAM_BLACK_ICON_COLOR: i32 = -13619152;

/// `Waypoint.Icon`: style asset and optional RGB color.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub style: Identifier,
    pub color: Option<i32>,
}

impl Default for Icon {
    fn default() -> Self {
        Self {
            style: default_style(),
            color: None,
        }
    }
}

impl Icon {
    /// `hasData()`
    pub fn has_data(&self) -> bool {
        self.style != default_style() || self.color.is_some()
    }

    /// `cloneAndAssignStyle(livingEntity)`: `team_color` is the RGB of the entity's team color (`BLACK` mapped to
    /// [`TEAM_BLACK_ICON_COLOR`]), used when the icon has no color of its own.
    pub fn clone_and_assign_style(&self, team_color: Option<i32>) -> Icon {
        Icon {
            style: self.style.clone(),
            color: self.color.or(team_color),
        }
    }

    /// `copyFrom(other)`
    pub fn copy_from(&mut self, other: &Icon) {
        self.color = other.color;
        self.style = other.style.clone();
    }

    /// `Waypoint.Icon.STREAM_CODEC`'s wire value.
    pub fn to_wire(&self) -> WaypointIcon {
        WaypointIcon {
            style: self.style.clone(),
            color: self.color.map(RgbColor),
        }
    }

    /// `Waypoint.Icon.CODEC`: `{style, color?}` (saved as `locator_bar_icon`).
    pub fn codec() -> Codec<Icon> {
        struct IconFields {
            style: Identifier,
            color: Option<i32>,
        }
        let map: MapCodec<IconFields> = mc_dfu::record!(IconFields {
            style: mc_dfu::extra::IDENTIFIER.field_of("style"),
            color: RGB_COLOR_CODEC.optional_field_of("color"),
        });
        map.xmap(
            |f| Icon {
                style: f.style,
                color: f.color,
            },
            |i: &Icon| IconFields {
                style: i.style.clone(),
                color: i.color,
            },
        )
        .codec()
    }
}

/// Everything a connection reads off the transmitting entity (`LivingEntity`).
#[derive(Clone, Debug)]
pub struct TransmitterView {
    pub id: i32,
    pub uuid: Uuid,
    pub position: Vec3,
    pub block_position: BlockPos,
    pub chunk_position: ChunkPos,
    pub first_tick: bool,
    pub spectator: bool,
    /// `getAttributeValue(WAYPOINT_TRANSMIT_RANGE)`
    pub transmit_range: f64,
    /// `locatorBarIcon`
    pub icon: Icon,
    /// The RGB of the team color (`BLACK` mapped to [`TEAM_BLACK_ICON_COLOR`]), if the entity's team has one.
    pub team_color: Option<i32>,
}

/// Everything a connection reads off the receiving `ServerPlayer`.
#[derive(Clone, Debug)]
pub struct ReceiverView {
    pub id: i32,
    pub position: Vec3,
    pub spectator: bool,
    /// `getAttributeValue(WAYPOINT_RECEIVE_RANGE)`
    pub receive_range: f64,
}

/// The level-side services the waypoint code needs (implemented by `ServerLevel`).
pub trait WaypointContext {
    /// The live transmitter `id` (`None` once it left the level).
    fn transmitter(&self, id: i32) -> Option<TransmitterView>;
    fn receiver(&self, id: i32) -> Option<ReceiverView>;
    /// `receiver.getChunkTrackingView().isInViewDistance(chunk.x, chunk.z)`
    fn is_chunk_visible(&self, receiver: i32, chunk: ChunkPos) -> bool;
    /// `source.hasIndirectPassenger(receiver)`
    fn has_indirect_passenger(&self, source: i32, receiver: i32) -> bool;
    /// `receiver.connection.send(packet)`
    fn send(&self, receiver: i32, packet: ClientboundTrackedWaypointPacket);
    /// `level.getGameRules().get(GameRules.LOCATOR_BAR)`
    fn locator_bar_enabled(&self) -> bool;
}

/// `Entity.distanceTo(entity)`
fn distance_to(a: Vec3, b: Vec3) -> f32 {
    let xd = (a.x - b.x) as f32;
    let yd = (a.y - b.y) as f32;
    let zd = (a.z - b.z) as f32;
    mth::sqrt(xd * xd + yd * yd + zd * zd)
}

/// `WaypointTransmitter.doesSourceIgnoreReceiver(source, receiver)`
pub fn does_source_ignore_receiver(
    ctx: &dyn WaypointContext,
    source: &TransmitterView,
    receiver: &ReceiverView,
) -> bool {
    if receiver.spectator {
        false
    } else if !source.spectator && !ctx.has_indirect_passenger(source.id, receiver.id) {
        let broadcast_range = source.transmit_range.min(receiver.receive_range);
        f64::from(distance_to(source.position, receiver.position)) >= broadcast_range
    } else {
        true
    }
}

/// `WaypointTransmitter.isReallyFar(source, receiver)`
fn is_really_far(source: &TransmitterView, receiver: &ReceiverView) -> bool {
    distance_to(source.position, receiver.position) > 332.0f32
}

/// `Vec3.rotateClockwise90` then `(float)Mth.atan2(z, x)`: the azimuth of `receiver` seen from `source`.
fn azimuth(source: Vec3, receiver: Vec3) -> f32 {
    let d = receiver.subtract(source);
    let direction = Vec3::new(-d.z, d.y, d.x);
    mth::atan2(direction.z, direction.x) as f32
}

/// `WaypointTransmitter.Connection` (the three vanilla implementations).
#[derive(Clone, Debug)]
pub enum Connection {
    /// `EntityAzimuthConnection`
    Azimuth {
        source: i32,
        source_uuid: Uuid,
        icon: Icon,
        receiver: i32,
        last_angle: f32,
    },
    /// `EntityBlockConnection`
    Block {
        source: i32,
        source_uuid: Uuid,
        icon: Icon,
        receiver: i32,
        last_position: BlockPos,
    },
    /// `EntityChunkConnection`
    Chunk {
        source: i32,
        source_uuid: Uuid,
        icon: Icon,
        receiver: i32,
        last_position: ChunkPos,
    },
}

fn packet(
    operation: WaypointOperation,
    uuid: Uuid,
    icon: &Icon,
    data: TrackedWaypointData,
) -> ClientboundTrackedWaypointPacket {
    ClientboundTrackedWaypointPacket {
        operation,
        waypoint: TrackedWaypoint {
            identifier: mc_protocol::codec::Either::Left(uuid),
            icon: icon.to_wire(),
            data,
        },
    }
}

/// `ClientboundTrackedWaypointPacket.removeWaypoint(uuid)`
pub fn remove_packet(uuid: Uuid) -> ClientboundTrackedWaypointPacket {
    ClientboundTrackedWaypointPacket {
        operation: WaypointOperation::Untrack,
        waypoint: TrackedWaypoint::empty(uuid),
    }
}

impl Connection {
    /// `LivingEntity.makeWaypointConnectionWith(player)`
    pub fn make(
        ctx: &dyn WaypointContext,
        source: &TransmitterView,
        receiver: &ReceiverView,
    ) -> Option<Connection> {
        if source.first_tick
            || source.id == receiver.id
            || does_source_ignore_receiver(ctx, source, receiver)
        {
            return None;
        }
        let icon = source.icon.clone_and_assign_style(source.team_color);
        Some(if is_really_far(source, receiver) {
            Connection::Azimuth {
                source: source.id,
                source_uuid: source.uuid,
                icon,
                receiver: receiver.id,
                last_angle: azimuth(source.position, receiver.position),
            }
        } else if !ctx.is_chunk_visible(receiver.id, source.chunk_position) {
            Connection::Chunk {
                source: source.id,
                source_uuid: source.uuid,
                icon,
                receiver: receiver.id,
                last_position: source.chunk_position,
            }
        } else {
            Connection::Block {
                source: source.id,
                source_uuid: source.uuid,
                icon,
                receiver: receiver.id,
                last_position: source.block_position,
            }
        })
    }

    fn receiver(&self) -> i32 {
        match self {
            Connection::Azimuth { receiver, .. }
            | Connection::Block { receiver, .. }
            | Connection::Chunk { receiver, .. } => *receiver,
        }
    }

    fn source(&self) -> i32 {
        match self {
            Connection::Azimuth { source, .. }
            | Connection::Block { source, .. }
            | Connection::Chunk { source, .. } => *source,
        }
    }

    /// `connect()`
    pub fn connect(&self, ctx: &dyn WaypointContext) {
        let p = match self {
            Connection::Azimuth {
                source_uuid,
                icon,
                last_angle,
                ..
            } => packet(
                WaypointOperation::Track,
                *source_uuid,
                icon,
                TrackedWaypointData::Azimuth(*last_angle),
            ),
            Connection::Block {
                source_uuid,
                icon,
                last_position,
                ..
            } => packet(
                WaypointOperation::Track,
                *source_uuid,
                icon,
                TrackedWaypointData::Vec3i(vec3i(*last_position)),
            ),
            Connection::Chunk {
                source_uuid,
                icon,
                last_position,
                ..
            } => packet(
                WaypointOperation::Track,
                *source_uuid,
                icon,
                TrackedWaypointData::Chunk(*last_position),
            ),
        };
        ctx.send(self.receiver(), p);
    }

    /// `disconnect()`
    pub fn disconnect(&self, ctx: &dyn WaypointContext) {
        let uuid = match self {
            Connection::Azimuth { source_uuid, .. }
            | Connection::Block { source_uuid, .. }
            | Connection::Chunk { source_uuid, .. } => *source_uuid,
        };
        ctx.send(self.receiver(), remove_packet(uuid));
    }

    /// `update()`
    pub fn update(&mut self, ctx: &dyn WaypointContext) {
        let receiver_id = self.receiver();
        let Some(source) = ctx.transmitter(self.source()) else {
            return;
        };
        match self {
            Connection::Azimuth {
                source_uuid,
                icon,
                last_angle,
                ..
            } => {
                let Some(receiver) = ctx.receiver(receiver_id) else {
                    return;
                };
                let current_angle = azimuth(source.position, receiver.position);
                if (current_angle - *last_angle).abs() > 0.008726646f32 {
                    ctx.send(
                        receiver_id,
                        packet(
                            WaypointOperation::Update,
                            *source_uuid,
                            icon,
                            TrackedWaypointData::Azimuth(current_angle),
                        ),
                    );
                    *last_angle = current_angle;
                }
            }
            Connection::Block {
                source_uuid,
                icon,
                last_position,
                ..
            } => {
                let current = source.block_position;
                if current.dist_manhattan(last_position) > 0 {
                    ctx.send(
                        receiver_id,
                        packet(
                            WaypointOperation::Update,
                            *source_uuid,
                            icon,
                            TrackedWaypointData::Vec3i(vec3i(current)),
                        ),
                    );
                    *last_position = current;
                }
            }
            Connection::Chunk {
                source_uuid,
                icon,
                last_position,
                ..
            } => {
                let current = source.chunk_position;
                if current.get_chessboard_distance(last_position) > 0 {
                    ctx.send(
                        receiver_id,
                        packet(
                            WaypointOperation::Update,
                            *source_uuid,
                            icon,
                            TrackedWaypointData::Chunk(current),
                        ),
                    );
                    *last_position = current;
                }
            }
        }
    }

    /// `isBroken()`
    pub fn is_broken(&self, ctx: &dyn WaypointContext) -> bool {
        let (Some(source), Some(receiver)) = (
            ctx.transmitter(self.source()),
            ctx.receiver(self.receiver()),
        ) else {
            return true;
        };
        match self {
            Connection::Azimuth { .. } => {
                does_source_ignore_receiver(ctx, &source, &receiver)
                    || ctx.is_chunk_visible(receiver.id, source.chunk_position)
                    || !is_really_far(&source, &receiver)
            }
            Connection::Block { last_position, .. } => {
                source.block_position.dist_manhattan(last_position) > 1
                    || does_source_ignore_receiver(ctx, &source, &receiver)
            }
            Connection::Chunk { last_position, .. } => {
                // `!ChunkConnection.isBroken() && !ignore ? isChunkVisible(lastPosition) : true`
                !(source.chunk_position.get_chessboard_distance(last_position) <= 1
                    && !does_source_ignore_receiver(ctx, &source, &receiver)
                    && !ctx.is_chunk_visible(receiver.id, *last_position))
            }
        }
    }
}

fn vec3i(pos: BlockPos) -> mc_core::pos::vec3i::Vec3i {
    mc_core::pos::vec3i::Vec3i::new(pos.x(), pos.y(), pos.z())
}
