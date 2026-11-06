//! `TheEndGatewayBlockEntity`: `Age`, `exit_portal` and `ExactTeleport`, sent to clients as update tag/packet, the
//! portal ticker (spawn animation, cooldown), the exit position search and the exit gateway creation.

use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
};
use crate::commands::arguments::coordinates::block_pos_argument::is_in_spawnable_bounds;
use crate::server::level::ServerLevel;
use crate::server::world_dimensions::end_key;
use crate::world::level::block::{BlockStateExt, behaviour};
use crate::world::level::levelgen::feature::Feature;
use crate::world::level::levelgen::feature::end_gateway_feature::EndGatewayFeature;
use crate::world::level::{BlockGetter, LevelReader};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::components::block_pos_codec;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

pub struct TheEndGatewayBlockEntity {
    base: BlockEntityData,
    pub age: i64,
    pub teleport_cooldown: i32,
    pub exit_portal: Option<BlockPos>,
    pub exact_teleport: bool,
}

impl TheEndGatewayBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::END_GATEWAY, pos, state),
            age: 0,
            teleport_cooldown: 0,
            exit_portal: None,
            exact_teleport: false,
        }
    }

    const SPAWN_TIME: i64 = 200;
    const COOLDOWN_TIME: i32 = 40;
    const ATTENTION_INTERVAL: i64 = 2400;
    const GATEWAY_HEIGHT_ABOVE_SURFACE: i32 = 10;

    /// `isSpawning()`.
    pub fn is_spawning(&self) -> bool {
        self.age < Self::SPAWN_TIME
    }

    /// `isCoolingDown()`.
    pub fn is_cooling_down(&self) -> bool {
        self.teleport_cooldown > 0
    }

    /// `triggerCooldown(level, pos, state, entity)` for the entity of the block at `pos` (taken out of the chunk by the caller).
    pub fn trigger_cooldown(&mut self, level: &mut ServerLevel) {
        self.teleport_cooldown = Self::COOLDOWN_TIME;
        let pos = self.get_block_pos();
        level.block_event(pos, self.get_block_state().block(), 1, 0);
        level.block_entity_changed(pos);
    }

    /// `getPortalPosition(currentLevel, portalEntryPos)`.
    pub fn get_portal_position(
        &mut self,
        level: &mut ServerLevel,
        portal_entry_pos: BlockPos,
    ) -> Option<Vec3> {
        if self.exit_portal.is_none() && level.dimension == end_key() {
            let exit_portal_pos = find_or_create_valid_teleport_pos(level, portal_entry_pos)
                .above_n(Self::GATEWAY_HEIGHT_ABOVE_SURFACE);
            spawn_gateway_portal(
                level,
                exit_portal_pos,
                &EndGatewayFeature {
                    exit: Some(portal_entry_pos),
                    exact: false,
                },
            );
            self.set_exit_position(exit_portal_pos, self.exact_teleport);
        }
        let exit = self.exit_portal?;
        let pos = if self.exact_teleport {
            exit
        } else {
            find_exit_position(level, exit)
        };
        Some(Vec3::at_bottom_center_of(&pos))
    }

    /// `setExitPosition(exactPosition, exact)`.
    pub fn set_exit_position(&mut self, exact_position: BlockPos, exact: bool) {
        self.exact_teleport = exact;
        self.exit_portal = Some(exact_position);
    }
}

impl BlockEntity for TheEndGatewayBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.age = tag.get_long_or("Age", 0);
        self.exit_portal = read_codec(tag, "exit_portal", &block_pos_codec())
            .filter(|p| is_in_spawnable_bounds(*p));
        self.exact_teleport = tag.get_boolean_or("ExactTeleport", false);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_long("Age", self.age);
        if let Some(exit) = &self.exit_portal
            && let Ok(encoded) = mc_dfu::encode_nbt(&block_pos_codec(), exit).into_result()
        {
            tag.put("exit_portal", encoded);
        }
        if self.exact_teleport {
            tag.put_boolean("ExactTeleport", true);
        }
    }

    /// `getUpdateTag`: `saveCustomOnly`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `portalTick`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        let spawning = self.is_spawning();
        let cooling_down = self.is_cooling_down();
        self.age += 1;
        if cooling_down {
            self.teleport_cooldown -= 1;
        } else if self.age % Self::ATTENTION_INTERVAL == 0 {
            self.trigger_cooldown(level);
        }
        if spawning != self.is_spawning() || cooling_down != self.is_cooling_down() {
            level.block_entity_changed(self.get_block_pos());
        }
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.get_block_pos(),
            r#type: self.get_type(),
            tag: self.get_update_tag(),
        })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static END_GATEWAY: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::END_GATEWAY,
    create: |pos, state| Box::new(TheEndGatewayBlockEntity::new(pos, state)),
};

/// `findExitPosition(level, exitPortal)`.
fn find_exit_position(level: &ServerLevel, exit_portal: BlockPos) -> BlockPos {
    find_tallest_block(level, exit_portal.offset(0, 2, 0), 5, false).above()
}

/// `findOrCreateValidTeleportPos(level, endGatewayPos)`.
fn find_or_create_valid_teleport_pos(
    level: &mut ServerLevel,
    end_gateway_pos: BlockPos,
) -> BlockPos {
    let tentative = find_exit_portal_xz_pos_tentative(level, end_gateway_pos);
    let mut exit_portal_pos = find_valid_spawn_in_chunk(
        level,
        mc_core::math::mth::floor_f64(tentative.x / 16.0),
        mc_core::math::mth::floor_f64(tentative.z / 16.0),
    );
    if exit_portal_pos.is_none() {
        let new_pos = BlockPos::containing(tentative.x + 0.5, 75.0, tentative.z + 0.5);
        // `EndFeatures.END_ISLAND.place(level, generator, RandomSource.create(newExitPortalPos.asLong()), newExitPortalPos)`
        let env = level.gen_env().clone();
        if let Some(island) = env.registries.feature(
            &mc_core::ident::Identifier::with_default_namespace("end_island").expect("valid id"),
        ) {
            let mut random = LegacyRandomSource::new(new_pos.as_long());
            island.value().place(level, &env, &mut random, new_pos);
        }
        exit_portal_pos = Some(new_pos);
    }
    find_tallest_block(level, exit_portal_pos.expect("set above"), 16, true)
}

fn chunk_of(level: &mut ServerLevel, pos: Vec3) -> (i32, i32) {
    let (cx, cz) = (
        mc_core::math::mth::floor_f64(pos.x / 16.0),
        mc_core::math::mth::floor_f64(pos.z / 16.0),
    );
    level.chunk_blocking_mut(cx, cz);
    (cx, cz)
}

fn is_chunk_empty(level: &mut ServerLevel, xz_pos: Vec3) -> bool {
    let (cx, cz) = chunk_of(level, xz_pos);
    level
        .chunk_blocking_mut(cx, cz)
        .data
        .access
        .get_highest_filled_section_index()
        == -1
}

/// `findExitPortalXZPosTentative(level, endGatewayPos)`.
fn find_exit_portal_xz_pos_tentative(level: &mut ServerLevel, end_gateway_pos: BlockPos) -> Vec3 {
    let direction = Vec3::new(
        f64::from(end_gateway_pos.x()),
        0.0,
        f64::from(end_gateway_pos.z()),
    )
    .normalize();
    let mut tentative = direction.scale(1024.0);
    let mut chunk_limit = 16;
    while !is_chunk_empty(level, tentative) && {
        let go = chunk_limit > 0;
        chunk_limit -= 1;
        go
    } {
        tentative = tentative.add(direction.scale(-16.0));
    }
    let mut chunk_limit = 16;
    while is_chunk_empty(level, tentative) && {
        let go = chunk_limit > 0;
        chunk_limit -= 1;
        go
    } {
        tentative = tentative.add(direction.scale(16.0));
    }
    tentative
}

/// `findTallestBlock(level, around, dist, allowBedrock)`.
fn find_tallest_block(
    level: &dyn BlockGetter,
    around: BlockPos,
    dist: i32,
    allow_bedrock: bool,
) -> BlockPos {
    let mut tallest: Option<BlockPos> = None;
    for xd in -dist..=dist {
        for zd in -dist..=dist {
            if xd != 0 || zd != 0 || allow_bedrock {
                let mut y = level.get_max_y();
                while y > tallest.map_or(level.get_min_y(), |t| t.y()) {
                    let pos = BlockPos::new(around.x() + xd, y, around.z() + zd);
                    let state = level.get_block_state(pos);
                    if behaviour(state.block()).is_collision_shape_full_block(state, level, pos)
                        && (allow_bedrock || state.block() != mc_data::block::Block::BEDROCK)
                    {
                        tallest = Some(pos);
                        break;
                    }
                    y -= 1;
                }
            }
        }
    }
    tallest.unwrap_or(around)
}

/// `findValidSpawnInChunk(chunk)`.
fn find_valid_spawn_in_chunk(
    level: &mut ServerLevel,
    chunk_x: i32,
    chunk_z: i32,
) -> Option<BlockPos> {
    let max_y = level
        .chunk_blocking_mut(chunk_x, chunk_z)
        .data
        .access
        .get_highest_section_position()
        + 16
        - 1;
    let start = BlockPos::new(chunk_x * 16, 30, chunk_z * 16);
    let end = BlockPos::new(chunk_x * 16 + 15, max_y, chunk_z * 16 + 15);
    let full = |level: &ServerLevel, pos: BlockPos| {
        let state = level.get_block_state(pos);
        behaviour(state.block()).is_collision_shape_full_block(state, level, pos)
    };
    let mut closest: Option<BlockPos> = None;
    let mut closest_dist = 0.0;
    for pos in BlockPos::between_closed(&start, &end) {
        if level.get_block_state(pos).block() == mc_data::block::Block::END_STONE
            && !full(level, pos.above())
            && !full(level, pos.above_n(2))
        {
            let dist = pos.dist_to_center_sqr_xyz(0.0, 0.0, 0.0);
            if closest.is_none() || dist < closest_dist {
                closest = Some(pos);
                closest_dist = dist;
            }
        }
    }
    closest
}

/// `spawnGatewayPortal(level, portalPos, feature)`: `feature.place(level, generator, RandomSource.create(), portalPos)`.
fn spawn_gateway_portal(
    level: &mut ServerLevel,
    portal_pos: BlockPos,
    feature: &EndGatewayFeature,
) {
    let env = level.gen_env().clone();
    let mut random = LegacyRandomSource::new(mc_core::random::generate_unique_seed());
    feature.place(level, &env, &mut random, portal_pos);
}
