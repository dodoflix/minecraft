//! `JigsawReplacementProcessor` (`jigsaw_replacement`): jigsaw blocks become their `final_state`.

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::commands::arguments::blocks::block_state_parser::BlockStateParser;
use crate::commands::command_build_context::{HolderLookupProvider, StaticRegistries};
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::TypeEntry;
use mc_brigadier::StringReader;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use std::sync::{Arc, LazyLock};

pub struct JigsawReplacementProcessor;

/// `JigsawReplacementProcessor.INSTANCE`.
pub static INSTANCE: LazyLock<Arc<dyn StructureProcessor>> = LazyLock::new(|| Arc::new(JigsawReplacementProcessor));

impl StructureProcessor for JigsawReplacementProcessor {
    fn process_block(
        &self,
        _level: &dyn LevelReader,
        target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        let state = processed_block_info.state;
        // `SharedConstants.DEBUG_KEEP_JIGSAW_BLOCKS_DURING_STRUCTURE_GEN` is false in release builds
        if state.block() != Block::by_name("minecraft:jigsaw").expect("jigsaw block") {
            return Some(processed_block_info);
        }
        let Some(nbt) = &processed_block_info.nbt else {
            log::warn!("Jigsaw block at {target_position} is missing nbt, will not replace");
            return Some(processed_block_info);
        };
        let state_string = nbt.get_string_or("final_state", "minecraft:air");
        let block_state = match parse_final_state(state_string) {
            Ok(state) => state,
            Err(message) => {
                log::error!("Failed to parse jigsaw replacement state '{state_string}' at {target_position}: {message}");
                return None;
            }
        };
        if block_state.block() == Block::by_name("minecraft:structure_void").expect("structure_void block") {
            None
        } else {
            Some(StructureBlockInfo::new(processed_block_info.pos, block_state, None))
        }
    }
}

/// `BlockStateParser.parseForBlock(level.holderLookup(Registries.BLOCK), string, true)` (trailing input is not an
/// error there either).
fn parse_final_state(text: &str) -> Result<mc_data::block::BlockState, String> {
    let mut reader = StringReader::new(text);
    let blocks = StaticRegistries.lookup("minecraft:block").ok_or("no block registry")?;
    BlockStateParser::parse_for_block(&*blocks, &mut reader, true).map(|result| result.block_state).map_err(|e| e.to_string())
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry { name: "jigsaw_replacement", decode: |_, _| Ok(INSTANCE.clone()) };
