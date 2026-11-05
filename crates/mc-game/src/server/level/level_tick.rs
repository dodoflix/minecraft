//! `ServerLevel.tick(haveTime)` in vanilla order, plus the per-tick pieces it calls that belong to level mutation and
//! ticking: scheduled block/fluid ticks, block events, random ticks of loaded chunks (`tickChunk`), precipitation and
//! thunder, weather cycle, block entity tickers, sleeping.
//!
//! Everything the tick reads from outside the level (tick rate manager, game rules, clocks, world border, weather
//! saved data, scheduled events, raids, player-wide broadcasts) comes through [`LevelTickEnv`], implemented by the
//! server.

use super::ServerLevel;
use super::block_events::BlockEventData;
use super::level_tick_state::{EMPTY_TIME_NO_TICK, MAX_SCHEDULED_TICKS_PER_TICK};
use super::sleep_status::SleepInfo;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::Entity;
use crate::world::level::biome_ext::{should_freeze, should_snow};
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use crate::world::level::precipitation::Precipitation;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::math::mth;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_data::props;
use mc_protocol::packets::play::ClientboundGameEventPacket;
use mc_world::heightmap::HeightmapType;
use mc_world::level_storage::Difficulty;
use mc_world::ticks::LevelTickAccess;

pub use super::level_rules::LevelRules;

/// `WeatherData` of the level (`rain_time`, `thunder_time`, ... as saved).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeatherState {
    pub clear_weather_time: i32,
    pub thunder_time: i32,
    pub rain_time: i32,
    pub thundering: bool,
    pub raining: bool,
}

/// What `ServerLevel.tick` needs from the server for this level.
pub trait LevelTickEnv {
    /// `tickRateManager().runsNormally()`.
    fn runs_normally(&self) -> bool;
    /// `tickRateManager().isEntityFrozen(entity)`.
    fn is_entity_frozen(&self, entity: &dyn Entity) -> bool;
    /// `getWeatherData()` read / write.
    fn weather(&self) -> WeatherState;
    fn set_weather(&mut self, weather: WeatherState);
    /// `server.clockManager().moveToTimeMarker(defaultClock, WAKE_UP_FROM_SLEEP)` when the dimension has a default clock.
    fn wake_up_from_sleep(&mut self);
    /// `getDifficulty()`.
    fn difficulty(&self) -> Difficulty;
    /// `getOverworldClockTime()` (ticks).
    fn overworld_clock_time(&self) -> i64;
    /// `server.getScheduledEvents().tick(server, gameTime)`.
    fn tick_scheduled_events(&mut self, game_time: i64);
    /// `raids.tick(level)`.
    fn tick_raids(&mut self, level: &mut ServerLevel);
    /// `server.getCommands().performPrefixedCommand(source, command)` for a source located in `level` (the ticking level, lent
    /// back to the server while the command runs).
    fn perform_prefixed_command(&mut self, level: &mut ServerLevel, source: crate::commands::CommandSourceStack, command: &str);
    /// The command block game rules, read live (`level.getGameRules()` inside command block code).
    fn command_rules(&self) -> super::level_commands::CommandRules;
    /// `PlayerList.broadcastAll(packet)`: every player of every dimension.
    fn broadcast_game_event_to_all(&mut self, packet: &ClientboundGameEventPacket);
    /// `setGameTime(time)` on the level data (`tickTime` levels only; also stores it for `getGameTime`).
    fn set_game_time(&mut self, game_time: i64);
    /// The `PortalProcessor` of `entity` (which just ticked and is out of `level`) reached its transition time:
    /// `getPortalDestination` + `Entity.teleportToPortalDestination`.
    fn teleport_through_portal(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity);
}

/// `ServerLevel.THUNDER_DELAY` / `THUNDER_DURATION` / `RAIN_DELAY` / `RAIN_DURATION` (`UniformInt.of(min, max)`).
const THUNDER_DELAY: (i32, i32) = (12000, 180000);
const THUNDER_DURATION: (i32, i32) = (3600, 15600);
const RAIN_DELAY: (i32, i32) = (12000, 180000);
const RAIN_DURATION: (i32, i32) = (12000, 24000);

fn sample(random: &mut dyn RandomSource, (min, max): (i32, i32)) -> i32 {
    random.next_int_between_inclusive(min, max)
}

impl ServerLevel {
    // ------------------------------------------------------------------------------------------------ positions

    /// `Level.getBlockRandomPos(xo, yo, zo, yMask)` (the level's own LCG, not the `RandomSource`).
    pub fn get_block_random_pos(&mut self, xo: i32, yo: i32, zo: i32, y_mask: i32) -> BlockPos {
        let st = &mut self.tick_state;
        st.rand_value = st.rand_value.wrapping_mul(3).wrapping_add(1013904223);
        let val = st.rand_value >> 2;
        BlockPos::new(xo.wrapping_add(val & 15), yo.wrapping_add(val >> 16 & y_mask), zo.wrapping_add(val >> 8 & 15))
    }

    /// `getHeightmapPos(type, pos)`.
    pub fn get_heightmap_pos(&self, ty: HeightmapType, pos: BlockPos) -> BlockPos {
        BlockPos::new(pos.x(), self.get_height_at(ty, pos.x(), pos.z()), pos.z())
    }

    /// `shouldTickBlocksAt(chunkPos)`.
    pub fn should_tick_blocks_at(&self, chunk_key: i64) -> bool {
        self.chunk_source.chunk_map.distance_manager.in_block_ticking_range(chunk_key)
    }

    /// `shouldTickBlocksAt(pos)`.
    pub fn should_tick_blocks_at_pos(&self, pos: BlockPos) -> bool {
        self.should_tick_blocks_at(ChunkPos::pack_xz(pos.x() >> 4, pos.z() >> 4))
    }

    /// `areEntitiesLoaded(chunkKey)`.
    pub fn are_entities_loaded(&self, chunk_key: i64) -> bool {
        self.entity_manager.are_entities_loaded(chunk_key)
    }

    /// `isPositionTickingWithEntitiesLoaded(key)`.
    pub fn is_position_ticking_with_entities_loaded(&self, key: i64) -> bool {
        self.are_entities_loaded(key) && self.chunk_source.is_position_ticking(key, self.should_tick_blocks_at(key))
    }

    // ------------------------------------------------------------------------------------------------ weather

    /// `canHaveWeather()`.
    pub fn can_have_weather(&self) -> bool {
        self.tick_state.can_have_weather
    }

    /// `getRainLevel(a)`.
    pub fn get_rain_level(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.tick_state.o_rain_level, self.tick_state.rain_level)
    }

    /// `getThunderLevel(a)`.
    pub fn get_thunder_level(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.tick_state.o_thunder_level, self.tick_state.thunder_level) * self.get_rain_level(a)
    }

    /// `setRainLevel(level)`.
    pub fn set_rain_level(&mut self, rain_level: f32) {
        let clamped = mth::clamp_f32(rain_level, 0.0, 1.0);
        self.tick_state.o_rain_level = clamped;
        self.tick_state.rain_level = clamped;
    }

    /// `setThunderLevel(level)`.
    pub fn set_thunder_level(&mut self, thunder_level: f32) {
        let clamped = mth::clamp_f32(thunder_level, 0.0, 1.0);
        self.tick_state.o_thunder_level = clamped;
        self.tick_state.thunder_level = clamped;
    }

    /// `isRaining()`.
    pub fn is_raining(&self) -> bool {
        self.can_have_weather() && f64::from(self.get_rain_level(1.0)) > 0.2
    }

    /// `isThundering()`.
    pub fn is_thundering(&self) -> bool {
        self.can_have_weather() && f64::from(self.get_thunder_level(1.0)) > 0.9
    }

    /// `prepareWeather(weatherData)` (level construction).
    pub fn prepare_weather(&mut self, weather: WeatherState) {
        if weather.raining {
            self.tick_state.rain_level = 1.0;
            if weather.thundering {
                self.tick_state.thunder_level = 1.0;
            }
        }
    }

    /// `isRainingAt(pos)`.
    pub fn is_raining_at(&self, pos: BlockPos) -> bool {
        self.precipitation_at(pos) == Precipitation::Rain
    }

    /// `precipitationAt(pos)`.
    pub fn precipitation_at(&self, pos: BlockPos) -> Precipitation {
        if !self.is_raining() || !self.can_see_sky(pos) || self.get_heightmap_pos(HeightmapType::MotionBlocking, pos).y() > pos.y() {
            return Precipitation::None;
        }
        match self.get_biome(pos) {
            Some(biome) => biome.get_precipitation_at(pos, self.get_sea_level()),
            None => Precipitation::None,
        }
    }

    /// `advanceWeatherCycle()`.
    fn advance_weather_cycle(&mut self, env: &mut dyn LevelTickEnv) {
        let was_raining = self.is_raining();
        if self.can_have_weather() {
            let mut weather = env.weather();
            if self.advance_weather() {
                let (mut clear_weather_time, mut thunder_time, mut rain_time) = (weather.clear_weather_time, weather.thunder_time, weather.rain_time);
                let (mut thundering, mut raining) = (weather.thundering, weather.raining);
                if clear_weather_time > 0 {
                    clear_weather_time -= 1;
                    thunder_time = if thundering { 0 } else { 1 };
                    rain_time = if raining { 0 } else { 1 };
                    thundering = false;
                    raining = false;
                } else {
                    let random = &mut self.tick_state.random;
                    if thunder_time > 0 {
                        thunder_time -= 1;
                        if thunder_time == 0 {
                            thundering = !thundering;
                        }
                    } else if thundering {
                        thunder_time = sample(random, THUNDER_DURATION);
                    } else {
                        thunder_time = sample(random, THUNDER_DELAY);
                    }
                    if rain_time > 0 {
                        rain_time -= 1;
                        if rain_time == 0 {
                            raining = !raining;
                        }
                    } else if raining {
                        rain_time = sample(random, RAIN_DURATION);
                    } else {
                        rain_time = sample(random, RAIN_DELAY);
                    }
                }
                weather = WeatherState { clear_weather_time, thunder_time, rain_time, thundering, raining };
                env.set_weather(weather);
            }
            let st = &mut self.tick_state;
            st.o_thunder_level = st.thunder_level;
            if weather.thundering {
                st.thunder_level += 0.01;
            } else {
                st.thunder_level -= 0.01;
            }
            st.thunder_level = mth::clamp_f32(st.thunder_level, 0.0, 1.0);
            st.o_rain_level = st.rain_level;
            if weather.raining {
                st.rain_level += 0.01;
            } else {
                st.rain_level -= 0.01;
            }
            st.rain_level = mth::clamp_f32(st.rain_level, 0.0, 1.0);
        }
        let (o_rain, rain, o_thunder, thunder) = {
            let st = &self.tick_state;
            (st.o_rain_level, st.rain_level, st.o_thunder_level, st.thunder_level)
        };
        if o_rain != rain {
            self.broadcast_to_level_players(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::RAIN_LEVEL_CHANGE, param: rain });
        }
        if o_thunder != thunder {
            self.broadcast_to_level_players(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::THUNDER_LEVEL_CHANGE, param: thunder });
        }
        if was_raining != self.is_raining() {
            let toggle = if was_raining { ClientboundGameEventPacket::STOP_RAINING } else { ClientboundGameEventPacket::START_RAINING };
            env.broadcast_game_event_to_all(&ClientboundGameEventPacket { event: toggle, param: 0.0 });
            env.broadcast_game_event_to_all(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::RAIN_LEVEL_CHANGE, param: rain });
            env.broadcast_game_event_to_all(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::THUNDER_LEVEL_CHANGE, param: thunder });
        }
    }

    /// `resetWeatherCycle()`.
    pub fn reset_weather_cycle(&mut self, env: &mut dyn LevelTickEnv) {
        let mut weather = env.weather();
        weather.rain_time = 0;
        weather.raining = false;
        weather.thunder_time = 0;
        weather.thundering = false;
        env.set_weather(weather);
    }

    /// `PlayerList.broadcastAll(packet, dimension)`: every player of this level.
    pub fn broadcast_to_level_players<P: mc_protocol::packet::Packet>(&self, packet: &P) {
        let payload = super::level_events::encode_packet(packet);
        for &id in &self.players {
            if let Some(entity) = self.get_entity(id)
                && let Some(player) = entity.as_any().downcast_ref::<crate::world::entity::player::ServerPlayer>()
                && let Some(connection) = &player.connection
            {
                connection.send_payload(payload.clone());
            }
        }
    }

    // ----------------------------------------------------------------------------------------------- biomes

    /// `getBiome(pos)`: the fuzzy block-resolution biome (`BiomeManager.getBiome`) from the loaded chunks' noise biomes.
    pub fn get_biome(&self, pos: BlockPos) -> Option<&mc_worldgen::biome::Biome> {
        let id = self.get_biome_id(pos)?;
        Some(self.tick_state.biomes.as_ref()?.get(id))
    }

    /// `getBiome(pos)` as the registry id of the biome (`None` while a needed chunk is missing).
    pub fn get_biome_id(&self, pos: BlockPos) -> Option<mc_world::palette::BiomeId> {
        self.tick_state.biomes.as_ref()?;
        let resolver = |qx: i32, qy: i32, qz: i32| {
            self.chunk_now(qx >> 2, qz >> 2).map(|c| c.data.access.get_noise_biome(qx, qy, qz))
        };
        // The manager needs a total resolver; a missing chunk aborts the lookup (vanilla would load it).
        let missing = std::cell::Cell::new(false);
        let total = |qx: i32, qy: i32, qz: i32| match resolver(qx, qy, qz) {
            Some(b) => b,
            None => {
                missing.set(true);
                mc_world::palette::BiomeId(0)
            }
        };
        let id = mc_worldgen::biome::BiomeManager::new(&total, self.tick_state.biome_zoom_seed).get_biome(pos);
        if missing.get() { None } else { Some(id) }
    }

    // ----------------------------------------------------------------------------------------------- random ticks

    /// `ServerLevel.tickChunk(chunk, tickSpeed)`.
    pub fn tick_chunk(&mut self, chunk_pos: ChunkPos, tick_speed: i32) {
        let min_x = chunk_pos.get_min_block_x();
        let min_z = chunk_pos.get_min_block_z();
        for _ in 0..tick_speed {
            if self.tick_state.random.next_int_bounded(48) == 0 {
                let pos = self.get_block_random_pos(min_x, 0, min_z, 15);
                self.tick_precipitation(pos);
            }
        }
        if tick_speed > 0 {
            let section_count = match self.chunk_now(chunk_pos.x, chunk_pos.z) {
                Some(chunk) => chunk.data.access.sections.len(),
                None => return,
            };
            for section_index in 0..section_count {
                let (randomly_ticking, section_y) = match self.chunk_now(chunk_pos.x, chunk_pos.z) {
                    Some(chunk) => (chunk.data.access.sections[section_index].is_randomly_ticking(), chunk.data.access.level_height.min_section_y() + section_index as i32),
                    None => return,
                };
                if !randomly_ticking {
                    continue;
                }
                let min_y_in_section = SectionPos::section_to_block_coord(section_y);
                for _ in 0..tick_speed {
                    let pos = self.get_block_random_pos(min_x, min_y_in_section, min_z, 15);
                    let Some(chunk) = self.chunk_now(chunk_pos.x, chunk_pos.z) else { return };
                    let block_state = chunk.data.access.sections[section_index].get_block_state(pos.x() - min_x, pos.y() - min_y_in_section, pos.z() - min_z);
                    if block_state.is_randomly_ticking() {
                        block_state.random_tick(self, pos);
                    }
                    let fluid_state = block_state.get_fluid_state();
                    if fluid_state.is_randomly_ticking() {
                        fluid_state.random_tick(self, pos);
                    }
                }
            }
        }
    }

    /// `tickPrecipitation(pos)`.
    pub fn tick_precipitation(&mut self, pos: BlockPos) {
        let top_pos = self.get_heightmap_pos(HeightmapType::MotionBlocking, pos);
        let below_pos = top_pos.below();
        let Some(biome_id) = self.biome_id_at(top_pos) else { return };
        let sea_level = self.get_sea_level();
        let freeze = {
            let biome = self.tick_state.biomes.as_ref().expect("biome registry present").get(biome_id);
            should_freeze(biome, self, below_pos)
        };
        if freeze {
            self.set_block_and_update(below_pos, Block::ICE.default_state());
        }
        if self.is_raining() {
            let max_height = self.max_snow_accumulation_height();
            let snow = {
                let biome = self.tick_state.biomes.as_ref().expect("biome registry present").get(biome_id);
                max_height > 0 && should_snow(biome, self, top_pos)
            };
            if snow {
                let state = BlockGetter::get_block_state(self, top_pos);
                if state.block() == Block::SNOW {
                    let current_layers = state.get(props::LAYERS);
                    if current_layers < max_height.min(8) {
                        let new_state = state.set(props::LAYERS, current_layers + 1);
                        crate::world::level::block::helpers::push_entities_up(self, state, new_state, top_pos);
                        self.set_block_and_update(top_pos, new_state);
                    }
                } else {
                    self.set_block_and_update(top_pos, Block::SNOW.default_state());
                }
            }
            let precipitation = self.tick_state.biomes.as_ref().expect("biome registry present").get(biome_id).get_precipitation_at(below_pos, sea_level);
            if precipitation != Precipitation::None {
                let below_state = BlockGetter::get_block_state(self, below_pos);
                crate::world::level::block::behaviour(below_state.block()).handle_precipitation(below_state, self, below_pos, precipitation);
            }
        }
    }

    fn biome_id_at(&self, pos: BlockPos) -> Option<mc_world::palette::BiomeId> {
        let registry = self.tick_state.biomes.as_ref()?;
        let _ = registry;
        let missing = std::cell::Cell::new(false);
        let total = |qx: i32, qy: i32, qz: i32| match self.chunk_now(qx >> 2, qz >> 2) {
            Some(c) => c.data.access.get_noise_biome(qx, qy, qz),
            None => {
                missing.set(true);
                mc_world::palette::BiomeId(0)
            }
        };
        let id = mc_worldgen::biome::BiomeManager::new(&total, self.tick_state.biome_zoom_seed).get_biome_at(pos.x(), pos.y(), pos.z());
        if missing.get() { None } else { Some(id) }
    }

    /// `tickThunder(chunk)`; `env` supplies the regional difficulty inputs and the spawn hook creates the entities.
    pub fn tick_thunder(&mut self, chunk_pos: ChunkPos, env: &dyn LevelTickEnv) {
        let raining = self.is_raining();
        let min_x = chunk_pos.get_min_block_x();
        let min_z = chunk_pos.get_min_block_z();
        if raining && self.is_thundering() && self.tick_state.random.next_int_bounded(100000) == 0 {
            let random_pos = self.get_block_random_pos(min_x, 0, min_z, 15);
            let pos = self.find_lightning_target_around(random_pos);
            if self.is_raining_at(pos) {
                let difficulty = self.get_current_difficulty_at(pos, env);
                let is_trap = self.spawn_mobs()
                    && self.tick_state.random.next_double() < f64::from(difficulty.get_effective_difficulty()) * 0.01
                    && !BlockGetter::get_block_state(self, pos.below()).is_tag("lightning_rods");
                if let Some(spawn) = self.tick_state.lightning_spawn {
                    spawn(self, pos, is_trap);
                }
            }
        }
    }

    /// `getCurrentDifficultyAt(pos)`.
    pub fn get_current_difficulty_at(&self, pos: BlockPos, env: &dyn LevelTickEnv) -> DifficultyInstance {
        let (mut local_time, mut moon_brightness) = (0i64, 0.0f32);
        if let Some(chunk) = self.chunk_now(pos.x() >> 4, pos.z() >> 4) {
            local_time = chunk.data.access.inhabited_time();
            moon_brightness = self.get_moon_brightness(pos);
        }
        DifficultyInstance::new(env.difficulty(), env.overworld_clock_time(), local_time, moon_brightness)
    }

    /// `findLightningTargetAround(pos)`: lightning rods (point of interest lookup) are the POI slice's; entities in the
    /// column are the fallback.
    fn find_lightning_target_around(&mut self, pos: BlockPos) -> BlockPos {
        let mut center = self.get_heightmap_pos(HeightmapType::MotionBlocking, pos);
        if let Some(rod) = self.tick_state.find_lightning_rod.and_then(|find| find(self, center)) {
            return rod.above();
        }
        let search = mc_core::phys::AABB::encapsulating_full_blocks(&center, &BlockPos::new(center.x(), self.get_max_y() + 1, center.z())).inflate_scalar(3.0);
        let candidates = self.get_entities_of_class(
            &search,
            |e| e.living().is_some(),
            |e| e.is_alive() && self.can_see_sky(e.block_position()),
        );
        if !candidates.is_empty() {
            let index = self.tick_state.random.next_int_bounded(candidates.len() as i32) as usize;
            if let Some(entity) = self.get_entity(candidates[index]) {
                return entity.block_position();
            }
        }
        if center.y() == self.get_min_y() - 1 {
            center = center.above_n(2);
        }
        center
    }

    // ----------------------------------------------------------------------------------------- scheduled ticks

    fn tick_block(&mut self, pos: BlockPos, ty: Block) {
        let state = BlockGetter::get_block_state(self, pos);
        if state.block() == ty {
            state.tick(self, pos);
        }
    }

    fn tick_fluid(&mut self, pos: BlockPos, ty: Fluid) {
        let block_state = BlockGetter::get_block_state(self, pos);
        let fluid_state = block_state.get_fluid_state();
        if fluid_state.fluid() == ty {
            fluid_state.tick(self, pos, block_state);
        }
    }

    /// `blockTicks.tick(tick, 65536, this::tickBlock)`.
    fn run_block_ticks(&mut self, game_time: i64) {
        let ticks = self.tick_state.ticks.clone();
        ticks.lock().block.collect_ticks(game_time, MAX_SCHEDULED_TICKS_PER_TICK, |key| self.is_position_ticking_with_entities_loaded(key));
        loop {
            let next = ticks.lock().block.poll_collected_tick();
            let Some((pos, block)) = next else { break };
            self.tick_block(pos, block);
        }
        ticks.lock().block.cleanup_after_tick();
    }

    /// `fluidTicks.tick(tick, 65536, this::tickFluid)`.
    fn run_fluid_ticks(&mut self, game_time: i64) {
        let ticks = self.tick_state.ticks.clone();
        ticks.lock().fluid.collect_ticks(game_time, MAX_SCHEDULED_TICKS_PER_TICK, |key| self.is_position_ticking_with_entities_loaded(key));
        loop {
            let next = ticks.lock().fluid.poll_collected_tick();
            let Some((pos, fluid)) = next else { break };
            self.tick_fluid(pos, fluid);
        }
        ticks.lock().fluid.cleanup_after_tick();
    }

    // ---------------------------------------------------------------------------------------------- block events

    /// `blockEvent(pos, block, b0, b1)`.
    pub fn block_event(&mut self, pos: BlockPos, block: Block, b0: i32, b1: i32) {
        self.tick_state.block_events.add(BlockEventData { pos, block, param_a: b0, param_b: b1 });
    }

    /// `runBlockEvents()`.
    fn run_block_events(&mut self) {
        self.tick_state.block_events.clear_reschedule();
        while let Some(event) = self.tick_state.block_events.remove_first() {
            if self.should_tick_blocks_at_pos(event.pos) {
                if self.do_block_event(&event) {
                    self.broadcast_block_event(&event);
                }
            } else {
                self.tick_state.block_events.reschedule(event);
            }
        }
        self.tick_state.block_events.add_rescheduled();
    }

    fn do_block_event(&mut self, event: &BlockEventData) -> bool {
        let state = BlockGetter::get_block_state(self, event.pos);
        state.block() == event.block && state.trigger_event(self, event.pos, event.param_a, event.param_b)
    }

    /// `clearBlockEvents(bb)`.
    pub fn clear_block_events(&mut self, bounding_box: &mc_core::bounding_box::BoundingBox) {
        self.tick_state.block_events.remove_if(|e| bounding_box.is_inside(&e.pos));
    }

    // ------------------------------------------------------------------------------------------ block entities

    /// `Level.tickBlockEntities()`.
    fn tick_block_entities(&mut self, env: &dyn LevelTickEnv) {
        self.tick_state.ticking_block_entities = true;
        if !self.tick_state.pending_block_entity_tickers.is_empty() {
            let pending = std::mem::take(&mut self.tick_state.pending_block_entity_tickers);
            self.tick_state.block_entity_tickers.extend(pending);
        }
        let tick_block_entities = env.runs_normally();
        let tickers = std::mem::take(&mut self.tick_state.block_entity_tickers);
        let mut kept = Vec::with_capacity(tickers.len());
        for ticker in tickers {
            let Some(pos) = ticker.0.get() else { continue };
            let removed = self.chunk_now(pos.x() >> 4, pos.z() >> 4).and_then(|c| c.block_entities.get(&pos)).is_none_or(|be| be.is_removed());
            if removed {
                continue;
            }
            if tick_block_entities && self.should_tick_blocks_at_pos(pos) {
                self.tick_one_block_entity(pos);
            }
            kept.push(ticker);
        }
        // tickers added while ticking went to `pending`; the kept ones keep their order in front
        let added = std::mem::take(&mut self.tick_state.block_entity_tickers);
        kept.extend(added);
        self.tick_state.block_entity_tickers = kept;
        self.tick_state.ticking_block_entities = false;
    }

    /// `LevelChunk.BoundTickingBlockEntity.tick()`.
    fn tick_one_block_entity(&mut self, pos: BlockPos) {
        let (cx, cz) = (pos.x() >> 4, pos.z() >> 4);
        if !self.is_within_world_border(pos) {
            return;
        }
        let ticking = self.full_chunk_status(cx, cz).is_or_after(super::FullChunkStatus::BlockTicking) && self.are_entities_loaded(ChunkPos::pack_xz(cx, cz));
        if !ticking {
            return;
        }
        let state = BlockGetter::get_block_state(self, pos);
        let valid = self.chunk_now(cx, cz).and_then(|c| c.block_entities.get(&pos)).is_some_and(|be| be.is_valid_block_state(state));
        if valid {
            self.with_block_entity(pos, |level, be| be.server_tick(level));
        } else if let Some(be) = self.get_block_entity(pos) {
            log::warn!("Block entity {} @ {} state {} invalid for ticking:", be.get_type().name(), pos.to_short_string(), state);
        }
    }

    // ------------------------------------------------------------------------------------------------ the tick

    /// `updateSkyBrightness()`.
    pub fn update_sky_brightness(&mut self) {
        self.sky_darken = (15.0f32 - self.dimension_float(crate::world::attribute::attributes::SKY_LIGHT_LEVEL)) as i32;
    }

    /// `environmentAttributes().getValue(MOON_PHASE, pos)` (ordinal).
    pub fn environment_moon_phase(&self, pos: BlockPos) -> u8 {
        match self.environment_value_at(crate::world::attribute::attributes::MOON_PHASE, pos) {
            crate::world::attribute::AttrValue::MoonPhase(phase) => phase,
            _ => 0,
        }
    }

    /// `getMoonBrightness(pos)`: `DimensionType.MOON_BRIGHTNESS_PER_PHASE[moonPhase]`.
    pub fn get_moon_brightness(&self, pos: BlockPos) -> f32 {
        crate::registry::typed::dimension_type::MOON_BRIGHTNESS_PER_PHASE[usize::from(self.environment_moon_phase(pos))]
    }

    fn sleep_infos(&self) -> Vec<SleepInfo> {
        self.players
            .iter()
            .filter_map(|&id| self.get_entity(id))
            .map(|p| SleepInfo { spectator: p.is_spectator(), sleeping: is_sleeping(p), sleeping_long_enough: is_sleeping(p) && p.player().is_some_and(|d| d.sleep_counter >= 100) })
            .collect()
    }

    /// `updateSleepingPlayerList()`.
    pub fn update_sleeping_player_list(&mut self) -> bool {
        if self.players.is_empty() {
            return false;
        }
        let infos = self.sleep_infos();
        self.tick_state.sleep_status.update(&infos)
    }

    /// `ServerLevel.tick(haveTime)`.
    pub fn tick(&mut self, env: &mut dyn LevelTickEnv, have_time: &mut dyn FnMut() -> bool) {
        self.tick_state.handling_tick = true;
        self.tick_state.attributes.invalidate_tick_cache();
        let runs = env.runs_normally();
        self.tick_state.command_rules = env.command_rules();
        if runs {
            self.get_world_border().tick();
            self.advance_weather_cycle(env);
        }

        let percentage = self.players_sleeping_percentage();
        let infos = self.sleep_infos();
        if self.tick_state.sleep_status.are_enough_sleeping(percentage) && self.tick_state.sleep_status.are_enough_deep_sleeping(percentage, &infos) {
            if self.advance_time() {
                env.wake_up_from_sleep();
            }
            self.wake_up_all_players();
            if self.advance_weather() && self.is_raining() {
                self.reset_weather_cycle(env);
            }
        }

        self.update_sky_brightness();
        if runs {
            self.tick_time(env);
        }

        if !self.is_debug && runs {
            let tick = self.game_time();
            self.with_command_env(&mut *env, |level| level.run_block_ticks(tick));
            self.run_fluid_ticks(tick);
        }

        if runs {
            env.tick_raids(self);
        }

        {
            let have_time = std::cell::RefCell::new(&mut *have_time);
            self.tick_chunk_source_with_env(&*env, &|| (have_time.borrow_mut())(), runs);
        }
        if runs {
            self.run_block_events();
        }

        self.tick_state.handling_tick = false;
        if self.chunk_source.has_active_tickets() {
            self.reset_empty_time();
        }
        if runs {
            self.tick_state.empty_time += 1;
        }

        if self.tick_state.empty_time < EMPTY_TIME_NO_TICK {
            if runs && let Some(tick) = self.tick_state.dragon_fight_tick {
                tick(self);
            }
            for id in self.entity_manager.tick_list.snapshot() {
                self.tick_listed_entity(env, id);
            }
            self.tick_block_entities(env);
        }
        self.tick_entity_management();
    }

    /// `tickTime()` for levels that own the game time: `serverLevelData.setGameTime(time + 1)` and the scheduled events.
    fn tick_time(&mut self, env: &mut dyn LevelTickEnv) {
        if self.tick_state.tick_time {
            let time = self.game_time() + 1;
            env.set_game_time(time);
            self.set_game_time(time);
            env.tick_scheduled_events(time);
        }
    }

    /// `tickNonPassenger(entity)` is [`ServerLevel::tick_listed_entity`] (in `server_level_riding.rs`).

    /// `resetEmptyTime()`.
    pub fn reset_empty_time(&mut self) {
        self.tick_state.empty_time = 0;
    }

    /// `wakeUpAllPlayers()`.
    fn wake_up_all_players(&mut self) {
        self.tick_state.sleep_status.remove_all_sleepers();
        let ids: Vec<_> = self.players.clone();
        for id in ids {
            self.with_entity(id, |level, p| {
                if is_sleeping(p) {
                    p.stop_sleep_in_bed(level, false, false);
                }
            });
        }
    }
}

fn is_sleeping(entity: &dyn Entity) -> bool {
    entity.living().is_some_and(|l| l.is_sleeping())
}
