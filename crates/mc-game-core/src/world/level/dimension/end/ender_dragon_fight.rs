//! `net.minecraft.world.level.dimension.end.EnderDragonFight`.

use super::DragonRespawnStage;
use crate::impl_saved_data;
use crate::server::bossevents::{BossBarColor, BossBarOverlay};
use crate::server::level::server_boss_event::ServerBossEvent;
use crate::server::level::{FullChunkStatus, ServerLevel, TicketType};
use crate::util::codec::{self, DataResult, DecodeResult};
use crate::world::damagesource::DamageSource;
use crate::world::entity::boss::end_crystal::EndCrystal;
use crate::world::entity::boss::ender_dragon::{EnderDragon, EnderDragonPhase};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{Entity, EntityId, RemovalReason, ServerPlayer};
use crate::world::level::block::pattern::{BlockPattern, BlockPatternBuilder, BlockPatternMatch};
use crate::world::level::levelgen::WorldGenLevel as _;
use crate::world::level::levelgen::feature::end_spike::get_spikes_for_level;
use crate::world::level::saveddata::{SavedData, SavedDataType};
use crate::world::level::{BlockGetter as _, LevelAccessor as _};
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{RandomSource, create_thread_local_instance};
use mc_core::uuid_util::{uuid_from_int_array, uuid_to_int_array};
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{BlockEntityType, EntityType};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_text::Component;
use mc_world::heightmap::HeightmapType;
use std::sync::LazyLock;
use uuid::Uuid;

const MAX_TICKS_BEFORE_DRAGON_RESPAWN: i32 = 1200;
const TIME_BETWEEN_CRYSTAL_SCANS: i32 = 100;
pub const TIME_BETWEEN_PLAYER_SCANS: i32 = 20;
pub const ARENA_TICKET_LEVEL: i32 = 9;
const GATEWAY_COUNT: i32 = 20;
const GATEWAY_DISTANCE: f64 = 96.0;
pub const DRAGON_SPAWN_Y: i32 = 128;

/// `exitPortalPattern`: the bedrock fountain around the exit portal.
static EXIT_PORTAL_PATTERN: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&[
            "       ", "       ", "       ", "   #   ", "       ", "       ", "       ",
        ])
        .aisle(&[
            "       ", "       ", "       ", "   #   ", "       ", "       ", "       ",
        ])
        .aisle(&[
            "       ", "       ", "       ", "   #   ", "       ", "       ", "       ",
        ])
        .aisle(&[
            "  ###  ", " #   # ", "#     #", "#  #  #", "#     #", " #   # ", "  ###  ",
        ])
        .aisle(&[
            "       ", "  ###  ", " ##### ", " ##### ", " ##### ", "  ###  ", "       ",
        ])
        .where_('#', |block, level| block.is_block(level, Block::BEDROCK))
        .build()
});

static TYPE: LazyLock<SavedDataType<EnderDragonFight>> = LazyLock::new(|| {
    SavedDataType::new(
        Identifier::parse("minecraft:ender_dragon_fight").expect("static id"),
        EnderDragonFight::create_default,
        EnderDragonFight::decode,
        EnderDragonFight::encode,
    )
});

/// `EnderDragonFight.TYPE`.
pub fn ender_dragon_fight_type() -> &'static SavedDataType<EnderDragonFight> {
    &TYPE
}

/// `EnderDragonFight`: the saved fields and the runtime state of the End's fight.
pub struct EnderDragonFight {
    dirty: bool,
    needs_state_scanning: bool,
    dragon_killed: bool,
    has_previously_killed_dragon: bool,
    respawn_stage: Option<DragonRespawnStage>,
    respawn_time: i32,
    dragon_uuid: Option<Uuid>,
    exit_portal_location: Option<BlockPos>,
    gateways: Vec<i32>,
    /// `respawnCrystals`: the `EntityReference<EndCrystal>` list as UUIDs (the reference's entity cache is not modelled).
    respawn_crystals: Vec<Uuid>,
    origin: BlockPos,
    /// `dragonEvent`, created by `init`.
    dragon_event: Option<ServerBossEvent>,
    ticks_since_dragon_seen: i32,
    alive_crystals: i32,
    ticks_since_crystals_scanned: i32,
    ticks_since_last_player_scan: i32,
    skip_arena_loaded_check: bool,
}

impl_saved_data!(EnderDragonFight);

impl EnderDragonFight {
    /// `createDefault()`.
    pub fn create_default() -> EnderDragonFight {
        EnderDragonFight::new(
            true,
            false,
            false,
            None,
            0,
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
    }

    /// The record constructor.
    pub fn new(
        needs_state_scanning: bool,
        dragon_killed: bool,
        previously_killed: bool,
        respawn_stage: Option<DragonRespawnStage>,
        respawn_time: i32,
        dragon_uuid: Option<Uuid>,
        exit_portal_location: Option<BlockPos>,
        gateways: Vec<i32>,
        respawn_crystals: Vec<Uuid>,
    ) -> EnderDragonFight {
        EnderDragonFight {
            dirty: false,
            needs_state_scanning,
            dragon_killed,
            has_previously_killed_dragon: previously_killed,
            respawn_stage,
            respawn_time,
            dragon_uuid,
            exit_portal_location,
            gateways,
            respawn_crystals,
            origin: BlockPos::ZERO,
            dragon_event: None,
            ticks_since_dragon_seen: 0,
            alive_crystals: 0,
            ticks_since_crystals_scanned: 0,
            ticks_since_last_player_scan: 21,
            skip_arena_loaded_check: false,
        }
    }

    /// `CODEC` decode.
    #[expect(
        clippy::result_large_err,
        reason = "the saved data decoder signature is DecodeResult<T>, whose partial value is the fight itself"
    )]
    pub fn decode(tag: &Tag) -> DecodeResult<EnderDragonFight> {
        let m = codec::compound(tag)?;
        let respawn_stage = codec::optional(m, "respawn_stage", |t| {
            let name = codec::string(t)?;
            DragonRespawnStage::by_name(name).ok_or_else(|| format!("Unknown element name:{name}"))
        })?;
        let respawn_crystals = codec::optional(m, "respawn_crystals", |t| {
            codec::list(t)?.iter().map(uuid_of).collect()
        })?
        .unwrap_or_default();
        // `lenientOptionalFieldOf`: a present but invalid value reads as absent
        let dragon_uuid = m.get("dragon_uuid").and_then(|t| uuid_of(t).ok());
        let exit_portal_location = m
            .get("exit_portal_location")
            .and_then(|t| block_pos_of(t).ok());
        let gateways = m
            .get("gateways")
            .and_then(|t| codec::list(t).ok())
            .and_then(|l| {
                l.iter()
                    .map(codec::int)
                    .collect::<DataResult<Vec<i32>>>()
                    .ok()
            })
            .unwrap_or_default();
        Ok(EnderDragonFight::new(
            codec::optional(m, "needs_state_scanning", codec::boolean)?.unwrap_or(true),
            codec::optional(m, "dragon_killed", codec::boolean)?.unwrap_or(false),
            codec::optional(m, "previously_killed", codec::boolean)?.unwrap_or(false),
            respawn_stage,
            codec::optional(m, "respawn_time", codec::int)?.unwrap_or(0),
            dragon_uuid,
            exit_portal_location,
            gateways,
            respawn_crystals,
        ))
    }

    /// `CODEC` encode.
    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put_boolean("needs_state_scanning", self.needs_state_scanning);
        m.put_boolean("dragon_killed", self.dragon_killed);
        m.put_boolean("previously_killed", self.has_previously_killed_dragon);
        if let Some(stage) = self.respawn_stage {
            m.put_string("respawn_stage", stage.get_serialized_name());
        }
        m.put_int("respawn_time", self.respawn_time);
        if let Some(uuid) = self.dragon_uuid {
            m.put_int_array("dragon_uuid", uuid_to_int_array(&uuid).to_vec());
        }
        if let Some(pos) = self.exit_portal_location {
            m.put_int_array("exit_portal_location", vec![pos.x(), pos.y(), pos.z()]);
        }
        // `optionalFieldOf(name, default)` omits a value equal to its default (an empty list)
        if !self.gateways.is_empty() {
            let mut list = ListTag::new();
            for &gateway in &self.gateways {
                list.push(gateway);
            }
            m.put("gateways", list);
        }
        if !self.respawn_crystals.is_empty() {
            let mut list = ListTag::new();
            for uuid in &self.respawn_crystals {
                list.push(Tag::IntArray(uuid_to_int_array(uuid).to_vec()));
            }
            m.put("respawn_crystals", list);
        }
        Ok(Tag::Compound(m))
    }

    /// `getPodiumLocation(offset)`: `END_PODIUM_LOCATION` is the zero position, so the podium is at the offset.
    pub fn get_podium_location(offset: BlockPos) -> BlockPos {
        offset
    }

    /// `init(level, seed, origin)` without the level capture: `event_uuid` is `Mth.createInsecureUUID(level.getRandom())`.
    fn init(&mut self, event_uuid: Uuid, seed: i64, origin: BlockPos) {
        self.origin = origin;
        let mut event = ServerBossEvent::new(
            event_uuid,
            Component::translatable("entity.minecraft.ender_dragon", Vec::new()),
            BossBarColor::Pink,
            BossBarOverlay::Progress,
        );
        event.set_play_boss_music(true).set_create_world_fog(true);
        self.dragon_event = Some(event);
        if self.gateways.is_empty() {
            let mut new_gateways: Vec<i32> = (0..GATEWAY_COUNT).collect();
            crate::server::status::shuffle(
                &mut new_gateways,
                &mut create_thread_local_instance(seed),
            );
            self.gateways.extend(new_gateways);
            self.set_dirty();
        }
    }

    /// `skipArenaLoadedCheck()`.
    pub fn skip_arena_loaded_check(&mut self) {
        self.skip_arena_loaded_check = true;
    }

    /// `removeAllGateways()`.
    pub fn remove_all_gateways(&mut self) {
        self.gateways.clear();
        self.set_dirty();
    }

    /// `aliveCrystals()`.
    pub fn alive_crystals(&self) -> i32 {
        self.alive_crystals
    }

    /// `hasPreviouslyKilledDragon()`.
    pub fn has_previously_killed_dragon(&self) -> bool {
        self.has_previously_killed_dragon
    }

    /// `dragonUUID()`.
    pub fn dragon_uuid(&self) -> Option<Uuid> {
        self.dragon_uuid
    }

    pub fn is_dragon_killed(&self) -> bool {
        self.dragon_killed
    }

    pub fn respawn_stage(&self) -> Option<DragonRespawnStage> {
        self.respawn_stage
    }

    pub fn respawn_time(&self) -> i32 {
        self.respawn_time
    }

    pub fn exit_portal_location(&self) -> Option<BlockPos> {
        self.exit_portal_location
    }

    pub fn gateways(&self) -> &[i32] {
        &self.gateways
    }

    pub fn needs_state_scanning(&self) -> bool {
        self.needs_state_scanning
    }

    pub fn ticks_since_dragon_seen(&self) -> i32 {
        self.ticks_since_dragon_seen
    }

    pub fn ticks_since_last_player_scan(&self) -> i32 {
        self.ticks_since_last_player_scan
    }

    pub fn ticks_since_crystals_scanned(&self) -> i32 {
        self.ticks_since_crystals_scanned
    }

    pub fn respawn_crystal_count(&self) -> usize {
        self.respawn_crystals.len()
    }

    /// `dragonEvent`.
    pub fn dragon_event(&self) -> &ServerBossEvent {
        self.dragon_event
            .as_ref()
            .expect("EnderDragonFight.init was not called")
    }

    fn event_mut(&mut self) -> &mut ServerBossEvent {
        self.dragon_event
            .as_mut()
            .expect("EnderDragonFight.init was not called")
    }
}

/// `UUIDUtil.CODEC`: four ints (an int array or a list of ints), or the textual form.
fn uuid_of(tag: &Tag) -> DataResult<Uuid> {
    if let Some(text) = tag.as_string() {
        return Uuid::parse_str(text).map_err(|e| format!("Invalid UUID {text}: {e}"));
    }
    match codec::list(tag)?
        .iter()
        .map(codec::int)
        .collect::<DataResult<Vec<_>>>()?[..]
    {
        [a, b, c, d] => Ok(uuid_from_int_array([a, b, c, d])),
        _ => Err("Expected an int stream of length 4".to_string()),
    }
}

/// `BlockPos.CODEC`: three ints.
fn block_pos_of(tag: &Tag) -> DataResult<BlockPos> {
    match codec::list(tag)?
        .iter()
        .map(codec::int)
        .collect::<DataResult<Vec<_>>>()?[..]
    {
        [x, y, z] => Ok(BlockPos::new(x, y, z)),
        _ => Err("Expected an int stream of length 3".to_string()),
    }
}

impl ServerLevel {
    /// `getDragonFight()`: `Some` for a level that runs a fight (the End).
    pub fn dragon_fight(&self) -> Option<&EnderDragonFight> {
        self.chunk_source
            .data_storage
            .peek(ender_dragon_fight_type())
    }

    /// `getDragonFight()` for the code that changes it.
    pub fn dragon_fight_mut(&mut self) -> Option<&mut EnderDragonFight> {
        self.dragon_fight()?;
        self.chunk_source
            .data_storage
            .get(ender_dragon_fight_type())
    }

    /// `ServerLevel` constructor, for a dimension type with a fight: `dragonFight = getDataStorage().computeIfAbsent(TYPE);
    /// dragonFight.init(this, seed, BlockPos.ZERO)`.
    pub fn install_dragon_fight(&mut self) {
        let seed = self.get_seed();
        self.chunk_source
            .data_storage
            .compute_if_absent(ender_dragon_fight_type());
        let uuid = mth::create_insecure_uuid(&mut self.tick_state.random);
        fight(self).init(uuid, seed, BlockPos::ZERO);
    }
}

fn fight(level: &mut ServerLevel) -> &mut EnderDragonFight {
    level
        .dragon_fight_mut()
        .expect("this level has no dragon fight")
}

/// `getDragons()`: the living dragons in lookup order, with their UUIDs.
fn get_dragons(level: &ServerLevel) -> Vec<(EntityId, Uuid)> {
    level
        .entity_manager
        .get_all()
        .filter(|e| e.get_type() == EntityType::ENDER_DRAGON && e.is_alive())
        .map(|e| (e.id(), e.uuid()))
        .collect()
}

/// `EnderDragonFight.tick()`.
pub fn tick(level: &mut ServerLevel) {
    {
        let f = fight(level);
        let visible = !f.dragon_killed;
        f.event_mut().set_visible(visible);
        f.ticks_since_last_player_scan += 1;
    }
    if fight(level).ticks_since_last_player_scan >= TIME_BETWEEN_PLAYER_SCANS {
        update_players(level);
        fight(level).ticks_since_last_player_scan = 0;
    }

    if fight(level).dragon_event().get_players().next().is_some() {
        level.chunk_source.add_ticket_with_radius(
            TicketType::DRAGON,
            ChunkPos::new(0, 0),
            ARENA_TICKET_LEVEL,
        );
        if !is_arena_loaded(level) {
            return;
        }

        if fight(level).needs_state_scanning {
            scan_state(level);
            let f = fight(level);
            f.needs_state_scanning = false;
            f.set_dirty();
        }

        if let Some(stage) = fight(level).respawn_stage {
            let crystals = respawn_crystals(level);
            if crystals.is_empty() {
                abort_respawn_sequence(level);
                return;
            }
            let f = fight(level);
            let time = f.respawn_time;
            f.respawn_time += 1;
            stage.tick(level, &crystals, time);
            fight(level).set_dirty();
        }

        if !fight(level).dragon_killed {
            let f = fight(level);
            let find = f.dragon_uuid.is_none() || {
                f.ticks_since_dragon_seen += 1;
                f.ticks_since_dragon_seen >= MAX_TICKS_BEFORE_DRAGON_RESPAWN
            };
            if find {
                find_or_create_dragon(level);
                fight(level).ticks_since_dragon_seen = 0;
            }

            let f = fight(level);
            f.ticks_since_crystals_scanned += 1;
            if f.ticks_since_crystals_scanned >= TIME_BETWEEN_CRYSTAL_SCANS {
                update_crystal_count(level);
                fight(level).ticks_since_crystals_scanned = 0;
            }
        }
    } else {
        level.chunk_source.remove_ticket_with_radius(
            TicketType::DRAGON,
            ChunkPos::new(0, 0),
            ARENA_TICKET_LEVEL,
        );
    }
}

/// `respawnCrystals.stream().map(e -> e.getEntity(level, EndCrystal.class)).filter(Objects::nonNull)`: the ids of the ritual's
/// crystals that are still in the level.
fn respawn_crystals(level: &mut ServerLevel) -> Vec<EntityId> {
    let uuids = fight(level).respawn_crystals.clone();
    uuids
        .into_iter()
        .filter_map(|uuid| level.get_entity_by_uuid(uuid))
        .filter(|e| e.get_type() == EntityType::END_CRYSTAL)
        .map(|e| e.id())
        .collect()
}

fn scan_state(level: &mut ServerLevel) {
    log::info!("Scanning for legacy world dragon fight...");
    let active_portal_exists = has_active_exit_portal(level);
    if active_portal_exists {
        log::info!("Found that the dragon has been killed in this world already.");
        fight(level).has_previously_killed_dragon = true;
    } else {
        log::info!("Found that the dragon has not yet been killed in this world.");
        fight(level).has_previously_killed_dragon = false;
        if find_exit_portal(level).is_none() {
            spawn_exit_portal(level, false);
        }
    }

    let dragons = get_dragons(level);
    if let Some(&(id, uuid)) = dragons.first() {
        fight(level).dragon_uuid = Some(uuid);
        log::info!("Found that there's a dragon still alive ({id})");
        fight(level).dragon_killed = false;
        if !active_portal_exists {
            log::info!("But we didn't have a portal, let's remove it.");
            level.with_entity(id, |level, dragon| {
                level.remove_held_entity(dragon, RemovalReason::Discarded)
            });
            fight(level).dragon_uuid = None;
        }
    } else {
        fight(level).dragon_killed = true;
    }

    let f = fight(level);
    if !f.has_previously_killed_dragon && f.dragon_killed {
        f.dragon_killed = false;
    }
    f.set_dirty();
}

fn find_or_create_dragon(level: &mut ServerLevel) {
    let dragons = get_dragons(level);
    if let Some(&(_, uuid)) = dragons.first() {
        log::debug!("Haven't seen our dragon, but found another one to use.");
        let f = fight(level);
        f.dragon_uuid = Some(uuid);
        f.set_dirty();
    } else {
        log::debug!("Haven't seen the dragon, respawning it");
        create_new_dragon(level);
    }
}

/// `setRespawnStage(stage)`: the stages call it to move on.
pub(super) fn set_respawn_stage(level: &mut ServerLevel, stage: DragonRespawnStage) {
    let f = fight(level);
    assert!(
        f.respawn_stage.is_some(),
        "Dragon respawn isn't in progress, can't skip ahead in the animation."
    );
    f.respawn_time = 0;
    // Not implemented: the last stage (`DragonRespawnStage.END`: the fight ends the ritual and creates the new dragon; the
    // `SUMMONED_ENTITY` trigger for the players of the bar)
    f.respawn_stage = Some(stage);
    f.set_dirty();
}

/// `blockEntity instanceof TheEndPortalBlockEntity` (the gateway's block entity extends it).
fn is_end_portal_like(block_entity: &dyn crate::world::block_entity::BlockEntity) -> bool {
    matches!(
        block_entity.get_type(),
        BlockEntityType::END_PORTAL | BlockEntityType::END_GATEWAY
    )
}

fn has_active_exit_portal(level: &mut ServerLevel) -> bool {
    for x in -8..=8 {
        for z in -8..=8 {
            let chunk = level.chunk_blocking_mut(x, z);
            if chunk.block_entities.values().any(is_end_portal_like) {
                return true;
            }
        }
    }
    false
}

fn find_exit_portal(level: &mut ServerLevel) -> Option<BlockPatternMatch> {
    let origin = fight(level).origin;
    let chunk_origin = ChunkPos::containing(&origin);
    for x in -8 + chunk_origin.x..=8 + chunk_origin.x {
        for z in -8 + chunk_origin.z..=8 + chunk_origin.z {
            let portals: Vec<BlockPos> = level
                .chunk_blocking_mut(x, z)
                .block_entities
                .iter()
                .filter(|(_, be)| is_end_portal_like(*be))
                .map(|(pos, _)| *pos)
                .collect();
            for pos in portals {
                if let Some(mut found) = EXIT_PORTAL_PATTERN.find(&*level, pos) {
                    let pos_in_world = found.get_block(3, 3, 3).get_pos();
                    let f = fight(level);
                    if f.exit_portal_location.is_none() {
                        f.exit_portal_location = Some(pos_in_world);
                        f.set_dirty();
                    }
                    return Some(found);
                }
            }
        }
    }

    let end_podium_location = EnderDragonFight::get_podium_location(origin);
    let max_y = level
        .get_heightmap_pos(HeightmapType::MotionBlocking, end_podium_location)
        .y();
    let mut y = max_y;
    while y >= level.get_min_y() {
        let start = BlockPos::new(end_podium_location.x(), y, end_podium_location.z());
        if let Some(mut found) = EXIT_PORTAL_PATTERN.find(&*level, start) {
            let pos_in_world = found.get_block(3, 3, 3).get_pos();
            let f = fight(level);
            if f.exit_portal_location.is_none() {
                f.exit_portal_location = Some(pos_in_world);
                f.set_dirty();
            }
            return Some(found);
        }
        y -= 1;
    }
    None
}

fn is_arena_loaded(level: &ServerLevel) -> bool {
    let f = level
        .dragon_fight()
        .expect("this level has no dragon fight");
    if f.skip_arena_loaded_check {
        return true;
    }
    let chunk_origin = ChunkPos::containing(&f.origin);
    for x in -8 + chunk_origin.x..=8 + chunk_origin.x {
        // vanilla's inner loop starts at `8 + chunkOrigin.z`: only that one row of chunks is checked
        for z in 8 + chunk_origin.z..=8 + chunk_origin.z {
            if level.chunk_now(x, z).is_none() {
                return false;
            }
            let status = level
                .chunk_source
                .chunk_map
                .get_visible_chunk_if_present(ChunkPos::pack_xz(x, z))
                .map(|holder| holder.get_full_status());
            if !status.is_some_and(|s| s.is_or_after(FullChunkStatus::BlockTicking)) {
                return false;
            }
        }
    }
    ChunkPos::range_closed(&chunk_origin, 1).all(|pos| level.are_entities_loaded(pos.pack()))
}

fn update_players(level: &mut ServerLevel) {
    let origin = fight(level).origin;
    let (x, y, z) = (
        f64::from(origin.x()),
        f64::from(DRAGON_SPAWN_Y + origin.y()),
        f64::from(origin.z()),
    );
    let distance_sqr = 192.0 * 192.0;
    // `EntitySelector.ENTITY_STILL_ALIVE.and(withinDistance(x, 128 + y, z, 192.0))`
    let valid: Vec<_> = level
        .players()
        .iter()
        .filter_map(|&id| {
            let player = level.get_entity(id)?;
            let handle = player
                .as_any()
                .downcast_ref::<ServerPlayer>()?
                .handle
                .as_ref()?;
            (player.is_alive() && player.base().distance_to_sqr_xyz(x, y, z) <= distance_sqr)
                .then(|| (id, handle.connection.clone()))
        })
        .collect();
    let event = fight(level).event_mut();
    for (id, connection) in &valid {
        event.add_player(*id, connection);
    }
    let to_remove: Vec<EntityId> = event
        .get_players()
        .filter(|p| !valid.iter().any(|(id, _)| id == p))
        .collect();
    for player in to_remove {
        event.remove_player(player);
    }
}

fn update_crystal_count(level: &mut ServerLevel) {
    let mut alive = 0;
    for spike in get_spikes_for_level(level) {
        alive += end_crystals_in(level, spike.get_top_bounding_box()).len();
    }
    let f = fight(level);
    f.ticks_since_crystals_scanned = 0;
    f.alive_crystals = alive as i32;
    log::debug!("Found {alive} end crystals still alive");
}

/// `getEntitiesOfClass(EndCrystal.class, bb)`.
fn end_crystals_in(level: &ServerLevel, bb: &AABB) -> Vec<EntityId> {
    level.get_entities_of_class(bb, |e| e.get_type() == EntityType::END_CRYSTAL, |_| true)
}

/// `setDragonKilled(dragon)` for the dragon with `dragon_uuid`.
pub fn set_dragon_killed(level: &mut ServerLevel, dragon_uuid: Uuid) {
    if Some(dragon_uuid) != fight(level).dragon_uuid {
        return;
    }
    {
        let event = fight(level).event_mut();
        event.set_progress(0.0);
        event.set_visible(false);
    }
    spawn_exit_portal(level, true);
    spawn_new_gateway(level);
    let f = fight(level);
    if !f.has_previously_killed_dragon {
        let podium = EnderDragonFight::get_podium_location(f.origin);
        let pos = level.get_heightmap_pos(HeightmapType::MotionBlocking, podium);
        level.set_block_and_update(pos, Block::DRAGON_EGG.default_state());
    }
    let f = fight(level);
    f.has_previously_killed_dragon = true;
    f.dragon_killed = true;
    f.set_dirty();
}

fn spawn_new_gateway(level: &mut ServerLevel) {
    let Some(gateway) = fight(level).gateways.pop() else {
        return;
    };
    let angle = 2.0 * (-std::f64::consts::PI + (std::f64::consts::PI / 20.0) * f64::from(gateway));
    let x = mth::floor_f64(GATEWAY_DISTANCE * angle.cos());
    let z = mth::floor_f64(GATEWAY_DISTANCE * angle.sin());
    let pos = BlockPos::new(x, 75, z);
    level.level_event(None, 3000, pos, 0);
    level.place_feature_with_unique_seed("minecraft:end_gateway_delayed", pos);
    fight(level).set_dirty();
}

fn spawn_exit_portal(level: &mut ServerLevel, activated: bool) {
    if fight(level).exit_portal_location.is_none() {
        let podium = EnderDragonFight::get_podium_location(fight(level).origin);
        let mut location = level
            .get_heightmap_pos(HeightmapType::MotionBlockingNoLeaves, podium)
            .below();
        while level.get_block_state(location).block() == Block::BEDROCK && location.y() > 63 {
            location = location.below();
        }
        let y = (level.get_min_y() + 1).max(location.y());
        location = BlockPos::new(location.x(), y, location.z());
        let f = fight(level);
        f.exit_portal_location = Some(location);
        f.set_dirty();
    }

    let location = fight(level)
        .exit_portal_location
        .expect("the location was set above");
    let feature = if activated {
        "minecraft:end_podium_active"
    } else {
        "minecraft:end_podium_inactive"
    };
    if level.place_feature_with_unique_seed(feature, location) {
        // `Mth.positiveCeilDiv(4, 16)`
        let chunk_radius = 1;
        level
            .chunk_source
            .chunk_map
            .wait_for_light_before_sending(ChunkPos::containing(&location), chunk_radius);
    }
}

/// `createNewDragon()`: the id of the dragon that was added.
fn create_new_dragon(level: &mut ServerLevel) -> Option<EntityId> {
    let origin = fight(level).origin;
    level.chunk_blocking_mut(origin.x() >> 4, origin.z() >> 4);
    let request = EntitySpawnRequest {
        reason: EntitySpawnReason::Event,
        ignore_checks: false,
    };
    let mut entity = factory::create(EntityType::ENDER_DRAGON, level, request)?;
    let dragon = entity
        .as_any_mut()
        .downcast_mut::<EnderDragon>()
        .expect("the ender dragon type creates an EnderDragon");
    dragon.set_dragon_fight();
    dragon.set_fight_origin(origin);
    dragon.set_phase(level, EnderDragonPhase::HoldingPattern);
    let yaw = level.tick_state.random.next_float() * 360.0;
    entity.snap_to(
        f64::from(origin.x()),
        f64::from(DRAGON_SPAWN_Y + origin.y()),
        f64::from(origin.z()),
        yaw,
        0.0,
    );
    let (id, uuid) = (entity.id(), entity.uuid());
    level.add_fresh_entity(entity);
    let f = fight(level);
    f.dragon_uuid = Some(uuid);
    f.set_dirty();
    Some(id)
}

/// `updateDragon(dragon)`.
pub fn update_dragon(level: &mut ServerLevel, dragon: &dyn Entity) {
    if Some(dragon.uuid()) != fight(level).dragon_uuid {
        return;
    }
    let max_health = dragon.living().map_or(0.0, |l| l.get_max_health());
    let name = dragon
        .base()
        .has_custom_name()
        .then(|| dragon_display_name(level, dragon));
    let f = fight(level);
    f.event_mut().set_progress(dragon.get_health() / max_health);
    f.ticks_since_dragon_seen = 0;
    if let Some(name) = name {
        f.event_mut().set_name(name);
    }
}

/// `Entity.getDisplayName()`.
fn dragon_display_name(level: &ServerLevel, dragon: &dyn Entity) -> Component {
    let name = crate::commands::host::entity_name(dragon);
    let scoreboard = level.scoreboard();
    let team = crate::world::entity::team::get_team(level, &scoreboard, dragon);
    crate::commands::host::decorate_display_name(
        crate::world::scores::PlayerTeam::format_name_for_team(team, &name),
        dragon,
    )
}

/// `onCrystalDestroyed(crystal, source)`.
pub fn on_crystal_destroyed(level: &mut ServerLevel, crystal: &dyn Entity, source: &DamageSource) {
    let Some(f) = level.dragon_fight() else {
        return;
    };
    if f.respawn_stage.is_some() && f.respawn_crystals.contains(&crystal.uuid()) {
        abort_respawn_sequence(level);
        return;
    }
    update_crystal_count(level);
    let Some(dragon) = fight(level)
        .dragon_uuid
        .and_then(|uuid| level.get_entity_by_uuid(uuid))
        .filter(|e| e.get_type() == EntityType::ENDER_DRAGON)
        .map(|e| e.id())
    else {
        return;
    };
    let pos = crystal.base().block_position();
    level.with_entity(dragon, |level, entity| {
        if let Some(dragon) = entity.as_any_mut().downcast_mut::<EnderDragon>() {
            dragon.on_crystal_destroyed(level, crystal, pos, source);
        }
    });
}

fn abort_respawn_sequence(level: &mut ServerLevel) {
    log::debug!("Aborting respawn sequence");
    let f = fight(level);
    f.respawn_stage = None;
    f.respawn_time = 0;
    reset_spike_crystals(level);
    spawn_exit_portal(level, true);
    fight(level).set_dirty();
}

/// `tryRespawn()`.
pub fn try_respawn(level: &mut ServerLevel) {
    let f = fight(level);
    if !f.dragon_killed || f.respawn_stage.is_some() {
        return;
    }
    let mut location = f.exit_portal_location;
    if location.is_none() {
        log::debug!("Tried to respawn, but need to find the portal first.");
        if find_exit_portal(level).is_none() {
            log::debug!("Couldn't find a portal, so we made one.");
            spawn_exit_portal(level, true);
        } else {
            log::debug!("Found the exit portal & saved its location for next time.");
        }
        location = fight(level).exit_portal_location;
    }
    let center = location.expect("the exit portal location is set").above();
    let mut crystals: Vec<Uuid> = Vec::new();
    for direction in Direction::HORIZONTAL {
        let bb = AABB::from_block_pos(&center.relative_n(direction, 3));
        let found = end_crystals_in(level, &bb);
        if found.is_empty() {
            return;
        }
        crystals.extend(
            found
                .iter()
                .filter_map(|&id| level.get_entity(id))
                .map(|e| e.uuid()),
        );
    }
    log::debug!("Found all crystals, respawning dragon.");
    respawn_dragon(level, crystals);
}

fn respawn_dragon(level: &mut ServerLevel, crystals: Vec<Uuid>) {
    let f = fight(level);
    if !f.dragon_killed || f.respawn_stage.is_some() {
        return;
    }
    while let Some(mut portal) = find_exit_portal(level) {
        for x in 0..EXIT_PORTAL_PATTERN.get_width() {
            for y in 0..EXIT_PORTAL_PATTERN.get_height() {
                for z in 0..EXIT_PORTAL_PATTERN.get_depth() {
                    let block = portal.get_block(x, y, z);
                    let pos = block.get_pos();
                    if block.get_state(&*level).is_some_and(|s| {
                        s.block() == Block::BEDROCK || s.block() == Block::END_PORTAL
                    }) {
                        level.set_block_and_update(pos, Block::END_STONE.default_state());
                    }
                }
            }
        }
    }

    let f = fight(level);
    f.respawn_stage = Some(DragonRespawnStage::Start);
    f.respawn_time = 0;
    spawn_exit_portal(level, false);
    let f = fight(level);
    f.respawn_crystals = crystals;
    f.set_dirty();
}

/// `resetSpikeCrystals()`.
pub fn reset_spike_crystals(level: &mut ServerLevel) {
    for spike in get_spikes_for_level(level) {
        for id in end_crystals_in(level, spike.get_top_bounding_box()) {
            level.with_entity(id, |_, crystal| {
                crystal.base_mut().permanently_invulnerable = false;
                if let Some(crystal) = crystal.as_any_mut().downcast_mut::<EndCrystal>() {
                    crystal.set_beam_target(None);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_nbt::NbtAccounter;

    /// The file a fresh vanilla server wrote for its End.
    fn vanilla_file() -> Tag {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-world/tests/fixtures/level_storage/world/dimensions/minecraft/the_end/data/minecraft/ender_dragon_fight.dat");
        let file = mc_nbt::nbt_io::read_compressed_file(&path, &mut NbtAccounter::unlimited_heap())
            .expect("the fixture reads");
        file.get("data").expect("data tag").clone()
    }

    #[test]
    fn the_vanilla_file_decodes_and_encodes_to_itself() {
        let tag = vanilla_file();
        let fight = EnderDragonFight::decode(&tag)
            .map_err(|p| p.error)
            .expect("the file decodes");
        assert!(fight.needs_state_scanning());
        assert_eq!(fight.gateways().len(), 20);
        assert_eq!(fight.encode().expect("encodes"), tag);
    }

    #[test]
    fn every_field_survives_a_round_trip() {
        let uuid = Uuid::from_u128(0x0123_4567_89ab_cdef_0011_2233_4455_6677);
        let fight = EnderDragonFight::new(
            false,
            true,
            true,
            Some(DragonRespawnStage::SummoningPillars),
            42,
            Some(uuid),
            Some(BlockPos::new(1, 2, 3)),
            vec![5, 1, 9],
            vec![uuid],
        );
        let tag = fight.encode().expect("encodes");
        let back = EnderDragonFight::decode(&tag)
            .map_err(|p| p.error)
            .expect("decodes");
        assert!(!back.needs_state_scanning() && back.is_dragon_killed());
        assert!(back.has_previously_killed_dragon());
        assert_eq!(
            back.respawn_stage(),
            Some(DragonRespawnStage::SummoningPillars)
        );
        assert_eq!(back.respawn_time(), 42);
        assert_eq!(back.dragon_uuid(), Some(uuid));
        assert_eq!(back.exit_portal_location(), Some(BlockPos::new(1, 2, 3)));
        assert_eq!(back.gateways(), [5, 1, 9]);
        assert_eq!(back.respawn_crystal_count(), 1);
        assert_eq!(back.encode().expect("encodes"), tag);
    }

    #[test]
    fn the_default_fight_writes_only_its_flags() {
        let tag = EnderDragonFight::create_default()
            .encode()
            .expect("encodes");
        let tag = tag.as_compound().expect("compound");
        assert_eq!(tag.len(), 4);
        assert!(tag.get_boolean_or("needs_state_scanning", false));
    }
}
