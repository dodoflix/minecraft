//! `net.minecraft.world.level.portal.TeleportTransition` and its `PostTeleportTransition` callbacks.
//!
//! Levels are owned by the server in this port, so `newLevel` is the destination's [`DimensionKey`]; everything that
//! needs the level itself takes it as a parameter (see [`super::dimension_change`] for the teleport flow).

use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::TicketType;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::DimensionKey;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_protocol::packets::play::{ClientboundLevelEventPacket, Relatives};
use std::fmt;
use std::sync::Arc;

type PostFn = dyn Fn(&mut ServerLevel, &mut dyn Entity) + Send + Sync;

/// `TeleportTransition.PostTeleportTransition`: runs on the teleported entity (the new entity for a cross-dimension
/// move of a non-player) once it is in the destination level.
#[derive(Clone)]
pub enum PostTeleportTransition {
    /// `DO_NOTHING`.
    DoNothing,
    /// `PLAY_PORTAL_SOUND`: `ClientboundLevelEvent(1032 = SOUND_PORTAL_TRAVEL)` to a player.
    PlayPortalSound,
    /// `PLACE_PORTAL_TICKET`: `entity.placePortalTicket(BlockPos.containing(entity.position()))`.
    PlacePortalTicket,
    /// `entity -> entity.placePortalTicket(pos)`.
    PlacePortalTicketAt(BlockPos),
    /// `first.then(second)`.
    Then(Arc<(PostTeleportTransition, PostTeleportTransition)>),
    /// Any other lambda.
    Custom(Arc<PostFn>),
}

impl fmt::Debug for PostTeleportTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DoNothing => f.write_str("DoNothing"),
            Self::PlayPortalSound => f.write_str("PlayPortalSound"),
            Self::PlacePortalTicket => f.write_str("PlacePortalTicket"),
            Self::PlacePortalTicketAt(pos) => write!(f, "PlacePortalTicketAt({pos:?})"),
            Self::Then(pair) => write!(f, "Then({:?}, {:?})", pair.0, pair.1),
            Self::Custom(_) => f.write_str("Custom(..)"),
        }
    }
}

impl PostTeleportTransition {
    pub const DO_NOTHING: Self = Self::DoNothing;
    pub const PLAY_PORTAL_SOUND: Self = Self::PlayPortalSound;
    pub const PLACE_PORTAL_TICKET: Self = Self::PlacePortalTicket;

    pub fn place_portal_ticket_at(pos: BlockPos) -> Self {
        Self::PlacePortalTicketAt(pos)
    }

    pub fn custom(f: impl Fn(&mut ServerLevel, &mut dyn Entity) + Send + Sync + 'static) -> Self {
        Self::Custom(Arc::new(f))
    }

    /// `PostTeleportTransition.then`.
    pub fn then(self, next: Self) -> Self {
        Self::Then(Arc::new((self, next)))
    }

    /// `onTransition(entity)`; `level` is the level the entity is in now.
    pub fn on_transition(&self, level: &mut ServerLevel, entity: &mut dyn Entity) {
        match self {
            Self::DoNothing => {}
            Self::PlayPortalSound => {
                if let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() {
                    player.send(&ClientboundLevelEventPacket { r#type: 1032, pos: BlockPos::ZERO, data: 0, global_event: false });
                }
            }
            Self::PlacePortalTicket => place_portal_ticket(level, BlockPos::containing_position(&entity.base().position())),
            Self::PlacePortalTicketAt(pos) => place_portal_ticket(level, *pos),
            Self::Then(pair) => {
                pair.0.on_transition(level, entity);
                pair.1.on_transition(level, entity);
            }
            Self::Custom(f) => f(level, entity),
        }
    }
}

/// `Entity.placePortalTicket(pos)`: `addTicketWithRadius(PORTAL, ChunkPos.containing(pos), 3)`.
pub fn place_portal_ticket(level: &mut ServerLevel, pos: BlockPos) {
    level.add_ticket_with_radius(TicketType::PORTAL, ChunkPos::new(pos.x() >> 4, pos.z() >> 4), 3);
}

/// `TeleportTransition`.
#[derive(Clone, Debug)]
pub struct TeleportTransition {
    /// `newLevel().dimension()`.
    pub level: DimensionKey,
    pub position: Vec3,
    pub delta_movement: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub missing_respawn_block: bool,
    pub as_passenger: bool,
    pub relatives: Relatives,
    pub post_teleport_transition: PostTeleportTransition,
}

impl TeleportTransition {
    /// The 6-argument constructor (`relatives = Set.of()`).
    pub fn new(level: DimensionKey, position: Vec3, delta_movement: Vec3, y_rot: f32, x_rot: f32, post_teleport_transition: PostTeleportTransition) -> Self {
        Self {
            level,
            position,
            delta_movement,
            y_rot,
            x_rot,
            missing_respawn_block: false,
            as_passenger: false,
            relatives: Relatives::NONE,
            post_teleport_transition,
        }
    }

    /// The 7-argument constructor.
    pub fn with_relatives(mut self, relatives: Relatives) -> Self {
        self.relatives = relatives;
        self
    }

    /// `createDefault(player, post)`: the world spawn of the respawn dimension. `level` is the player's current level
    /// (taken out of `server`).
    pub fn create_default(server: &mut MinecraftServer, level: &mut ServerLevel, post_teleport_transition: PostTeleportTransition) -> Self {
        Self::at_shared_spawn(server, level, false, post_teleport_transition)
    }

    /// `missingRespawnBlock(player, post)`.
    pub fn missing_respawn_block(server: &mut MinecraftServer, level: &mut ServerLevel, post_teleport_transition: PostTeleportTransition) -> Self {
        Self::at_shared_spawn(server, level, true, post_teleport_transition)
    }

    fn at_shared_spawn(server: &mut MinecraftServer, level: &mut ServerLevel, missing_respawn_block: bool, post_teleport_transition: PostTeleportTransition) -> Self {
        let dimension = server.find_respawn_dimension();
        let respawn = server.get_respawn_data();
        let position = super::respawn::find_adjusted_shared_spawn_pos(server, level, &dimension, respawn.pos);
        Self {
            level: dimension,
            position,
            delta_movement: Vec3::ZERO,
            y_rot: respawn.yaw,
            x_rot: respawn.pitch,
            missing_respawn_block,
            as_passenger: false,
            relatives: Relatives::NONE,
            post_teleport_transition,
        }
    }

    /// `withRotation(yRot, xRot)`.
    pub fn with_rotation(&self, y_rot: f32, x_rot: f32) -> Self {
        Self { y_rot, x_rot, ..self.clone() }
    }

    /// `withPosition(position)`.
    pub fn with_position(&self, position: Vec3) -> Self {
        Self { position, ..self.clone() }
    }

    /// `transitionAsPassenger()`.
    pub fn transition_as_passenger(&self) -> Self {
        Self { as_passenger: true, ..self.clone() }
    }

    /// `PositionMoveRotation.of(transition)`.
    pub fn position_move_rotation(&self) -> mc_protocol::packets::play::PositionMoveRotation {
        mc_protocol::packets::play::PositionMoveRotation { position: self.position, delta_movement: self.delta_movement, y_rot: self.y_rot, x_rot: self.x_rot }
    }
}

/// `Vec3.atBottomCenterOf(entity.adjustSpawnLocation(newLevel, suggestion))`: the `ServerPlayer` override runs the
/// `PlayerSpawnFinder`, every other entity uses the world spawn column's top. `current_level` is the entity's level
/// (taken out of `server`); `new_dimension` the destination.
pub fn adjusted_spawn_pos(server: &mut MinecraftServer, current_level: &mut ServerLevel, new_dimension: &DimensionKey, entity: &dyn Entity, suggestion: BlockPos) -> Vec3 {
    if entity.is_player() {
        return super::respawn::find_adjusted_shared_spawn_pos(server, current_level, new_dimension, suggestion);
    }
    let column = |level: &mut ServerLevel| {
        use mc_world::heightmap::HeightmapType;
        let spawn_block = level.level_data.get_respawn_data().pos;
        level.chunk_blocking_mut(spawn_block.x() >> 4, spawn_block.z() >> 4);
        let height = crate::world::level::LevelReader::get_height_at(&*level, HeightmapType::MotionBlockingNoLeaves, spawn_block.x(), spawn_block.z()) + 1;
        Vec3::at_bottom_center_of(&BlockPos::new(spawn_block.x(), height, spawn_block.z()))
    };
    if *new_dimension == current_level.dimension {
        column(current_level)
    } else {
        server.with_level(new_dimension, |_, level| column(level)).unwrap_or(Vec3::at_bottom_center_of(&suggestion))
    }
}
