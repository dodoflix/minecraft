//! `ServerLevel.saveDebugReport` and what it calls (`ChunkMap.dumpChunks`, `dumpEntities`, `dumpBlockEntityTickers`,
//! `Level.fillReportDetails`): the `levels/<namespace>/<path>/` files of a `/perf` report.

use std::fmt::Write as _;
use std::io;
use std::path::Path;

use super::chunk_holder::{ChunkSlot, PromotionFuture};
use super::chunk_map::ChunkMap;
use super::level_tick::WeatherState;
use super::{FullChunkStatus, ServerLevel};
use crate::crash_report::{CrashReport, ReportType, panic_message};
use crate::server::permissions::permissions;
use crate::util::csv_output::{CsvOutput, nullable};
use crate::world::damagesource::display_name::entity_display_name;
use crate::world::entity::{Entity, ServerPlayer};
use mc_core::math::java::java_double_to_string;
use mc_core::pos::ChunkPos;
use mc_world::status::ChunkStatus;
use mc_world::ticks::TickAccess;

impl ServerLevel {
    /// `saveDebugReport(rootDir)`: `stats.txt`, `example_crash.txt`, `chunks.csv`, `entity_chunks.csv`, `entities.csv` and
    /// `block_entities.csv` in `root`. `weather` is the server's weather data (`getWeatherData()`).
    pub fn save_debug_report(&mut self, root: &Path, weather: WeatherState) -> io::Result<()> {
        std::fs::write(root.join("stats.txt"), self.debug_stats())?;
        let mut report = CrashReport::new("Level dump", "java.lang.Exception: dummy");
        self.fill_report_details(&mut report, weather);
        std::fs::write(
            root.join("example_crash.txt"),
            report.get_friendly_report(ReportType::TEST),
        )?;
        std::fs::write(
            root.join("chunks.csv"),
            dump_chunks(&mut self.chunk_source.chunk_map),
        )?;
        std::fs::write(
            root.join("entity_chunks.csv"),
            self.entity_manager.dump_sections(),
        )?;
        std::fs::write(root.join("entities.csv"), self.dump_entities())?;
        std::fs::write(
            root.join("block_entities.csv"),
            self.dump_block_entity_tickers(),
        )
    }

    /// The `stats.txt` of `saveDebugReport`. analogue: `spawn_count.*` lines come in category order, one per category
    /// with a counted entity (vanilla's order is that of an identity-hashed map); `pending_tasks` is the chunk map's main
    /// thread queue.
    fn debug_stats(&mut self) -> String {
        let map = &mut self.chunk_source.chunk_map;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "spawning_chunks: {}",
            map.distance_manager.get_natural_spawn_chunk_count()
        );
        if let Some(state) = self.spawning.last_spawn_state() {
            for category in mc_data::entity_type::MOB_CATEGORIES.iter() {
                let count = state.get_mob_category_count(category.category);
                if count > 0 {
                    let _ = writeln!(out, "spawn_count.{}: {count}", category.serialized_name);
                }
            }
        }
        let ticks = self.tick_state.ticks.lock();
        let _ = writeln!(out, "entities: {}", self.entity_manager.gather_stats());
        let _ = writeln!(
            out,
            "block_entity_tickers: {}",
            self.tick_state.block_entity_tickers.len()
        );
        let _ = writeln!(out, "block_ticks: {}", ticks.block.count());
        let _ = writeln!(out, "fluid_ticks: {}", ticks.fluid.count());
        let _ = writeln!(
            out,
            "distance_manager: {}",
            map.distance_manager.get_debug_status()
        );
        let _ = writeln!(out, "pending_tasks: {}", map.main_tasks.len());
        out
    }

    /// `dumpEntities(output, getEntities().getAll())`.
    fn dump_entities(&self) -> String {
        let mut csv = CsvOutput::new(&[
            "x",
            "y",
            "z",
            "uuid",
            "type",
            "alive",
            "display_name",
            "custom_name",
        ]);
        for entity in self.entity_manager.get_all() {
            let position = entity.position();
            csv.write_row(&[
                &java_double_to_string(position.x),
                &java_double_to_string(position.y),
                &java_double_to_string(position.z),
                &entity.uuid(),
                &entity.get_type().name(),
                &entity.is_alive(),
                &entity_display_name(self, entity).get_string(),
                &nullable(
                    entity
                        .base()
                        .get_custom_name()
                        .map(|name| name.get_string()),
                ),
            ]);
        }
        csv.finish()
    }

    /// `dumpBlockEntityTickers(output)`. A ticker that was rebound to `NULL_TICKER` reads position 0,0,0 and an empty type.
    fn dump_block_entity_tickers(&self) -> String {
        let mut csv = CsvOutput::new(&["x", "y", "z", "type"]);
        for ticker in &self.tick_state.block_entity_tickers {
            let Some(pos) = ticker.0.get() else {
                csv.write_row(&[&0, &0, &0, &""]);
                continue;
            };
            let block_entity_type = self
                .chunk_now(pos.x() >> 4, pos.z() >> 4)
                .and_then(|chunk| chunk.block_entities.get(&pos))
                .map_or_else(String::new, |block_entity| {
                    block_entity.get_type().name().to_owned()
                });
            csv.write_row(&[&pos.x(), &pos.y(), &pos.z(), &block_entity_type]);
        }
        csv.finish()
    }

    /// `fillReportDetails(report)`: the `Affected level` category of `Level` plus the entity count and weather of
    /// `ServerLevel`.
    pub(crate) fn fill_report_details(&self, report: &mut CrashReport, weather: WeatherState) {
        let level_height = self.chunk_source.chunk_map.level_height;
        let category = report.add_category("Affected level");
        category.set_detail_with("All players", || {
            let infos: Vec<String> = self
                .players
                .iter()
                .filter_map(|&id| self.get_entity(id))
                .map(player_debug_info)
                .collect();
            format!("{} total; {}", self.players.len(), infos.join(", "))
        });
        category.set_detail_with("Chunk stats", || {
            self.chunk_source.get_loaded_chunks_count().to_string()
        });
        category.set_detail_with("Level dimension", || self.dimension.to_string());
        category.set_detail_with("Level time", || {
            format!(
                "{} game time, {} day time",
                self.game_time(),
                self.overworld_clock_time()
            )
        });
        let filled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.level_data
                .fill_crash_report_category(category, level_height);
        }));
        if let Err(payload) = filled {
            category.set_detail(
                "Level Data Unobtainable",
                format!("~~ERROR~~ {}", panic_message(payload.as_ref())),
            );
        }
        category.set_detail_with("Loaded entity count", || {
            self.entity_manager.count().to_string()
        });
        category.set_detail_with("Server weather", || {
            format!(
                "Rain time: {} (now: {}), thunder time: {} (now: {})",
                weather.rain_time,
                self.is_raining(),
                weather.thunder_time,
                self.is_thundering()
            )
        });
    }
}

/// `Player.debugInfo()`: Guava's `toStringHelper` over the player. An entity that is not a player (a player slot that
/// is out of the entity storage) reads empty.
fn player_debug_info(entity: &dyn Entity) -> String {
    let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() else {
        return String::new();
    };
    let granted = player.permissions();
    let permission = if granted.has_permission(&permissions::COMMANDS_OWNER) {
        "owner"
    } else if granted.has_permission(&permissions::COMMANDS_ADMIN) {
        "admin"
    } else if granted.has_permission(&permissions::COMMANDS_GAMEMASTER) {
        "gamemaster"
    } else if granted.has_permission(&permissions::COMMANDS_MODERATOR) {
        "moderator"
    } else {
        "none"
    };
    format!(
        "ServerPlayer{{name={}, id={}, pos={}, mode={}, permission={permission}}}",
        player.get_plain_text_name(),
        entity.id(),
        entity.position(),
        player
            .game_mode
            .game_mode_for_player
            .name()
            .to_ascii_uppercase()
    )
}

/// `ChunkMap.dumpChunks(output)`: the `chunks.csv` table, one row per holder of the visible chunk map.
fn dump_chunks(map: &mut ChunkMap) -> String {
    let mut csv = CsvOutput::new(&[
        "x",
        "z",
        "level",
        "in_memory",
        "status",
        "full_status",
        "accessible_ready",
        "ticking_ready",
        "entity_ticking_ready",
        "ticket",
        "spawning",
        "block_entity_count",
        "ticking_ticket",
        "ticking_level",
        "block_ticks",
        "fluid_ticks",
    ]);
    let keys: Vec<i64> = map.holders.visible.iter().copied().collect();
    for key in keys {
        let pos = ChunkPos::unpack(key);
        // what the holder says, read before the calls that need the map mutably
        let row = {
            let Some(holder) = map.holders.holder(key) else {
                continue;
            };
            let in_memory = holder.latest_chunk().is_some();
            // the latest chunk is the `LevelChunk` only once the FULL status completed; before that it is a proto chunk
            let full_chunk = match holder.latest_chunk() {
                Some(ChunkSlot::Level(chunk))
                    if holder.latest_chunk_status() == Some(ChunkStatus::Full) =>
                {
                    Some(chunk)
                }
                _ => None,
            };
            let (block_ticks, fluid_ticks) = full_chunk.map_or((0, 0), |chunk| {
                let shared = map.tick_containers.as_ref().map(|ticks| ticks.lock());
                let in_level = shared.as_ref().and_then(|ticks| {
                    Some((
                        TickAccess::count(ticks.block.container(pos)?),
                        TickAccess::count(ticks.fluid.container(pos)?),
                    ))
                });
                in_level.unwrap_or_else(|| {
                    (
                        TickAccess::count(&chunk.data.block_ticks),
                        TickAccess::count(&chunk.data.fluid_ticks),
                    )
                })
            });
            [
                holder.get_ticket_level().to_string(),
                in_memory.to_string(),
                nullable(in_memory.then(|| holder.get_persisted_status()).flatten()),
                nullable(full_chunk.map(|_| full_status_name(holder.get_full_status()))),
                print_future(holder.full_chunk_future).to_owned(),
                print_future(holder.ticking_chunk_future).to_owned(),
                print_future(holder.entity_ticking_chunk_future).to_owned(),
                full_chunk
                    .map_or(0, |chunk| chunk.block_entities.iter().count())
                    .to_string(),
                block_ticks.to_string(),
                fluid_ticks.to_string(),
            ]
        };
        let [
            level,
            in_memory,
            status,
            full_status,
            accessible_ready,
            ticking_ready,
            entity_ticking_ready,
            block_entity_count,
            block_ticks,
            fluid_ticks,
        ] = row;
        let spawning = map.any_player_close_enough_for_spawning(pos);
        let ticket = map.tickets.get_ticket_debug_string(key, false);
        let ticking_ticket = map.tickets.get_ticket_debug_string(key, true);
        let ticking_level = map
            .distance_manager
            .get_chunk_level(&mut map.holders, key, true);
        csv.write_row(&[
            &pos.x,
            &pos.z,
            &level,
            &in_memory,
            &status,
            &full_status,
            &accessible_ready,
            &ticking_ready,
            &entity_ticking_ready,
            &ticket,
            &spawning,
            &block_entity_count,
            &ticking_ticket,
            &ticking_level,
            &block_ticks,
            &fluid_ticks,
        ]);
    }
    csv.finish()
}

/// `ChunkMap.printFuture(future)`: a failed promotion carries the shared `ChunkResult` error, which reads `unloaded`.
fn print_future(future: PromotionFuture) -> &'static str {
    match future {
        PromotionFuture::Pending => "not completed",
        PromotionFuture::Done => "done",
        PromotionFuture::Unloaded | PromotionFuture::Failed => "unloaded",
    }
}

/// `FullChunkStatus.toString()` (the enum constant's name).
fn full_status_name(status: FullChunkStatus) -> &'static str {
    match status {
        FullChunkStatus::Inaccessible => "INACCESSIBLE",
        FullChunkStatus::Full => "FULL",
        FullChunkStatus::BlockTicking => "BLOCK_TICKING",
        FullChunkStatus::EntityTicking => "ENTITY_TICKING",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash_report::CrashReportCategory;
    use mc_core::ident::Identifier;
    use mc_world::chunk::LevelHeight;

    #[test]
    fn promotion_futures_and_full_status_read_like_vanilla() {
        assert_eq!(print_future(PromotionFuture::Pending), "not completed");
        assert_eq!(print_future(PromotionFuture::Done), "done");
        assert_eq!(print_future(PromotionFuture::Failed), "unloaded");
        assert_eq!(
            full_status_name(FullChunkStatus::BlockTicking),
            "BLOCK_TICKING"
        );
    }

    #[test]
    fn format_location_names_block_section_and_region() {
        assert_eq!(
            CrashReportCategory::format_location(LevelHeight::new(-64, 384), 17, -1, -513),
            "World: (17,-1,-513), Section: (at 1,15,15 in 1,-1,-33; chunk contains blocks 16,-64,-528 to 31,319,-513), \
             Region: (0,-2; contains chunks 0,-64 to 31,-33, blocks 0,-64,-1024 to 511,319,-513)"
        );
    }

    #[test]
    fn an_empty_level_dumps_every_file_with_vanilla_headers() {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        let dir = tempfile::tempdir().unwrap();
        level
            .save_debug_report(dir.path(), WeatherState::default())
            .unwrap();
        let read = |name: &str| std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(
            read("chunks.csv"),
            "x,z,level,in_memory,status,full_status,accessible_ready,ticking_ready,entity_ticking_ready,ticket,spawning,\
             block_entity_count,ticking_ticket,ticking_level,block_ticks,fluid_ticks\r\n"
        );
        assert_eq!(
            read("entity_chunks.csv"),
            "x,y,z,visibility,load_status,entity_count\r\n"
        );
        assert_eq!(
            read("entities.csv"),
            "x,y,z,uuid,type,alive,display_name,custom_name\r\n"
        );
        assert_eq!(read("block_entities.csv"), "x,y,z,type\r\n");
        let stats = read("stats.txt");
        assert!(stats.starts_with("spawning_chunks: 0\n"), "{stats}");
        assert!(
            stats.contains("\nblock_ticks: 0\nfluid_ticks: 0\n"),
            "{stats}"
        );
        assert!(stats.contains("\ndistance_manager: "), "{stats}");
        let crash = read("example_crash.txt");
        assert!(
            crash.starts_with("---- Minecraft Test Report ----"),
            "{crash}"
        );
        assert!(crash.contains("Description: Level dump\n\njava.lang.Exception: dummy"));
        assert!(crash.contains("-- Affected level --"), "{crash}");
        assert!(
            crash.contains("\tLevel dimension: minecraft:overworld"),
            "{crash}"
        );
        assert!(crash.contains("\tAll players: 0 total; "), "{crash}");
    }
}
