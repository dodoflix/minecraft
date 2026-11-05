//! `OceanRuinPieces`: the warm/cold ocean ruin template pieces, big ruins and clusters.

use super::ocean_ruin_structure::{OceanRuinStructure, OceanRuinType};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::levelgen::block_entity_data::set_block_entity_loot_table;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext, StructurePieceType,
};
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, read_rotation, rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::always_true_test::AlwaysTrueTest;
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::STRUCTURE_AND_AIR;
use crate::world::level::levelgen::structure::templatesystem::block_match_test::BlockMatchTest;
use mc_game_core::world::level::levelgen::structure::templatesystem::block_rot_processor::BlockRotProcessor;
use crate::world::level::levelgen::structure::templatesystem::capped_processor::CappedProcessor;
use crate::world::level::levelgen::structure::templatesystem::pos_always_true_test::PosAlwaysTrueTest;
use crate::world::level::levelgen::structure::templatesystem::processor_rule::ProcessorRule;
use crate::world::level::levelgen::structure::templatesystem::rule::blockentity::append_loot::AppendLoot;
use crate::world::level::levelgen::structure::templatesystem::rule_processor::RuleProcessor;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_processor::StructureProcessor;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::transform;
use mc_game_core::world::level::levelgen::structure::templatesystem::{StructurePlaceSettings, StructureTemplateManager};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_game_core::world::level::material::FluidStateExt as _;
use mc_game_core::world::level::{BlockGetter, LevelAccessor as _};
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::provider::IntProvider;
use std::sync::{Arc, LazyLock};

/// `archyRuleProcessor(candidateBlock, replacementBlock, lootTable)`.
fn archy_rule_processor(candidate_block: Block, replacement_block: Block, loot_table: &str) -> Arc<dyn StructureProcessor> {
    Arc::new(CappedProcessor::new(
        Arc::new(RuleProcessor::new(vec![ProcessorRule::new(
            Arc::new(BlockMatchTest { block: candidate_block }),
            Arc::new(AlwaysTrueTest),
            Arc::new(PosAlwaysTrueTest),
            replacement_block.default_state(),
            Arc::new(AppendLoot { loot_table: Identifier::parse(loot_table).expect("valid loot table id") }),
        )])),
        IntProvider::constant(5),
    ))
}

static WARM_SUSPICIOUS_BLOCK_PROCESSOR: LazyLock<Arc<dyn StructureProcessor>> =
    LazyLock::new(|| archy_rule_processor(Block::SAND, Block::SUSPICIOUS_SAND, "minecraft:archaeology/ocean_ruin_warm"));
static COLD_SUSPICIOUS_BLOCK_PROCESSOR: LazyLock<Arc<dyn StructureProcessor>> =
    LazyLock::new(|| archy_rule_processor(Block::GRAVEL, Block::SUSPICIOUS_GRAVEL, "minecraft:archaeology/ocean_ruin_cold"));

const WARM_RUINS: [&str; 8] = [
    "underwater_ruin/warm_1",
    "underwater_ruin/warm_2",
    "underwater_ruin/warm_3",
    "underwater_ruin/warm_4",
    "underwater_ruin/warm_5",
    "underwater_ruin/warm_6",
    "underwater_ruin/warm_7",
    "underwater_ruin/warm_8",
];
const RUINS_BRICK: [&str; 8] = [
    "underwater_ruin/brick_1",
    "underwater_ruin/brick_2",
    "underwater_ruin/brick_3",
    "underwater_ruin/brick_4",
    "underwater_ruin/brick_5",
    "underwater_ruin/brick_6",
    "underwater_ruin/brick_7",
    "underwater_ruin/brick_8",
];
const RUINS_CRACKED: [&str; 8] = [
    "underwater_ruin/cracked_1",
    "underwater_ruin/cracked_2",
    "underwater_ruin/cracked_3",
    "underwater_ruin/cracked_4",
    "underwater_ruin/cracked_5",
    "underwater_ruin/cracked_6",
    "underwater_ruin/cracked_7",
    "underwater_ruin/cracked_8",
];
const RUINS_MOSSY: [&str; 8] = [
    "underwater_ruin/mossy_1",
    "underwater_ruin/mossy_2",
    "underwater_ruin/mossy_3",
    "underwater_ruin/mossy_4",
    "underwater_ruin/mossy_5",
    "underwater_ruin/mossy_6",
    "underwater_ruin/mossy_7",
    "underwater_ruin/mossy_8",
];
const BIG_RUINS_BRICK: [&str; 4] = ["underwater_ruin/big_brick_1", "underwater_ruin/big_brick_2", "underwater_ruin/big_brick_3", "underwater_ruin/big_brick_8"];
const BIG_RUINS_MOSSY: [&str; 4] = ["underwater_ruin/big_mossy_1", "underwater_ruin/big_mossy_2", "underwater_ruin/big_mossy_3", "underwater_ruin/big_mossy_8"];
const BIG_RUINS_CRACKED: [&str; 4] = ["underwater_ruin/big_cracked_1", "underwater_ruin/big_cracked_2", "underwater_ruin/big_cracked_3", "underwater_ruin/big_cracked_8"];
const BIG_WARM_RUINS: [&str; 4] = ["underwater_ruin/big_warm_4", "underwater_ruin/big_warm_5", "underwater_ruin/big_warm_6", "underwater_ruin/big_warm_7"];

fn location(path: &str) -> Identifier {
    Identifier::with_default_namespace(path).expect("valid ocean ruin template id")
}

/// `Util.getRandom(array, random)`.
fn get_random(array: &[&'static str], random: &mut dyn RandomSource) -> Identifier {
    location(array[random.next_int_bounded(array.len() as i32) as usize])
}

/// `OceanRuinPieces.addPieces(manager, position, rotation, builder, random, structure)`.
pub fn add_pieces(
    structure_template_manager: &StructureTemplateManager,
    position: BlockPos,
    rotation: Rotation,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    structure: &OceanRuinStructure,
) {
    let is_large = random.next_float() <= structure.large_probability;
    let base_integrity = if is_large { 0.9f32 } else { 0.8f32 };
    add_piece(structure_template_manager, position, rotation, builder, random, structure, is_large, base_integrity);
    if is_large && random.next_float() <= structure.cluster_probability {
        add_cluster_ruins(structure_template_manager, random, rotation, position, structure, builder);
    }
}

/// `addClusterRuins(manager, random, rotation, p, structure, builder)`.
fn add_cluster_ruins(
    structure_template_manager: &StructureTemplateManager,
    random: &mut dyn RandomSource,
    rotation: Rotation,
    p: BlockPos,
    structure: &OceanRuinStructure,
    builder: &mut StructurePiecesBuilder,
) {
    let parent_pos = BlockPos::new(p.x(), 90, p.z());
    let parent_corner = transform(BlockPos::new(15, 0, 15), Mirror::None, rotation, BlockPos::ZERO).offset(parent_pos.x(), parent_pos.y(), parent_pos.z());
    let parent_bb = BoundingBox::from_corners(&parent_pos, &parent_corner);
    let parent_bottom_left = BlockPos::new(parent_pos.x().min(parent_corner.x()), parent_pos.y(), parent_pos.z().min(parent_corner.z()));
    let mut all_positions = all_positions(random, parent_bottom_left);
    let ruins = mth::next_int(random, 4, 8);
    for _ in 0..ruins {
        if !all_positions.is_empty() {
            let idx = random.next_int_bounded(all_positions.len() as i32) as usize;
            let pos = all_positions.remove(idx);
            let next_rotation = Rotation::get_random(random);
            let next_corner = transform(BlockPos::new(5, 0, 6), Mirror::None, next_rotation, BlockPos::ZERO).offset(pos.x(), pos.y(), pos.z());
            let next_bb = BoundingBox::from_corners(&pos, &next_corner);
            if !next_bb.intersects(&parent_bb) {
                add_piece(structure_template_manager, pos, next_rotation, builder, random, structure, false, 0.8);
            }
        }
    }
}

/// `allPositions(random, origin)`.
fn all_positions(random: &mut dyn RandomSource, origin: BlockPos) -> Vec<BlockPos> {
    let mut positions = Vec::with_capacity(8);
    positions.push(origin.offset(-16 + mth::next_int(random, 1, 8), 0, 16 + mth::next_int(random, 1, 7)));
    positions.push(origin.offset(-16 + mth::next_int(random, 1, 8), 0, mth::next_int(random, 1, 7)));
    positions.push(origin.offset(-16 + mth::next_int(random, 1, 8), 0, -16 + mth::next_int(random, 4, 8)));
    positions.push(origin.offset(mth::next_int(random, 1, 7), 0, 16 + mth::next_int(random, 1, 7)));
    positions.push(origin.offset(mth::next_int(random, 1, 7), 0, -16 + mth::next_int(random, 4, 6)));
    positions.push(origin.offset(16 + mth::next_int(random, 1, 7), 0, 16 + mth::next_int(random, 3, 8)));
    positions.push(origin.offset(16 + mth::next_int(random, 1, 7), 0, mth::next_int(random, 1, 7)));
    positions.push(origin.offset(16 + mth::next_int(random, 1, 7), 0, -16 + mth::next_int(random, 4, 8)));
    positions
}

/// `addPiece(manager, position, rotation, builder, random, structure, isLarge, baseIntegrity)`.
#[allow(clippy::too_many_arguments)]
fn add_piece(
    structure_template_manager: &StructureTemplateManager,
    position: BlockPos,
    rotation: Rotation,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    structure: &OceanRuinStructure,
    is_large: bool,
    base_integrity: f32,
) {
    match structure.biome_temp {
        OceanRuinType::Warm => {
            let start_piece_location = if is_large { get_random(&BIG_WARM_RUINS, random) } else { get_random(&WARM_RUINS, random) };
            builder.add_piece(Box::new(OceanRuinPiece::new(structure_template_manager, &start_piece_location, position, rotation, base_integrity, structure.biome_temp, is_large)));
        }
        OceanRuinType::Cold => {
            let bricks: &[&str] = if is_large { &BIG_RUINS_BRICK } else { &RUINS_BRICK };
            let cracked: &[&str] = if is_large { &BIG_RUINS_CRACKED } else { &RUINS_CRACKED };
            let mossy: &[&str] = if is_large { &BIG_RUINS_MOSSY } else { &RUINS_MOSSY };
            let idx = random.next_int_bounded(bricks.len() as i32) as usize;
            builder.add_piece(Box::new(OceanRuinPiece::new(structure_template_manager, &location(bricks[idx]), position, rotation, base_integrity, structure.biome_temp, is_large)));
            builder.add_piece(Box::new(OceanRuinPiece::new(structure_template_manager, &location(cracked[idx]), position, rotation, 0.7, structure.biome_temp, is_large)));
            builder.add_piece(Box::new(OceanRuinPiece::new(structure_template_manager, &location(mossy[idx]), position, rotation, 0.5, structure.biome_temp, is_large)));
        }
    }
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static OCEAN_RUIN: StructurePieceType = StructurePieceType { id: "orp", load: load_piece };

fn load_piece(context: &StructurePieceSerializationContext<'_>, tag: &CompoundTag) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(OceanRuinPiece::create(&context.env.structure_template_manager, tag)?))
}

pub struct OceanRuinPiece {
    data: TemplateStructurePieceData,
    biome_type: OceanRuinType,
    integrity: f32,
    is_large: bool,
}

/// `makeSettings(rotation, integrity, biomeType)`.
fn make_settings(rotation: Rotation, integrity: f32, biome_type: OceanRuinType) -> StructurePlaceSettings {
    let suspicious_block_processor = if biome_type == OceanRuinType::Cold { &COLD_SUSPICIOUS_BLOCK_PROCESSOR } else { &WARM_SUSPICIOUS_BLOCK_PROCESSOR };
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_rotation(rotation)
        .set_mirror(Mirror::None)
        .add_processor(Arc::new(BlockRotProcessor::new(None, integrity)))
        .add_processor(STRUCTURE_AND_AIR.clone())
        .add_processor(Arc::clone(suspicious_block_processor));
    settings
}

impl OceanRuinPiece {
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_location: &Identifier,
        position: BlockPos,
        rotation: Rotation,
        integrity: f32,
        biome_type: OceanRuinType,
        is_large: bool,
    ) -> OceanRuinPiece {
        OceanRuinPiece {
            data: TemplateStructurePieceData::new(
                &OCEAN_RUIN,
                0,
                structure_template_manager,
                template_location,
                template_location.to_string(),
                make_settings(rotation, integrity, biome_type),
                position,
            ),
            integrity,
            biome_type,
            is_large,
        }
    }

    /// `OceanRuinPiece.create(manager, tag)`.
    pub fn create(structure_template_manager: &StructureTemplateManager, tag: &CompoundTag) -> Result<OceanRuinPiece, String> {
        let rotation = read_rotation(tag, "Rot")?;
        let integrity = tag.get_float_or("Integrity", 0.0);
        let biome_type = tag.get_string("BiomeType").and_then(OceanRuinType::from_legacy_name).ok_or_else(|| "No value present".to_owned())?;
        let is_large = tag.get_boolean_or("IsLarge", false);
        let data = TemplateStructurePieceData::load(&OCEAN_RUIN, tag, structure_template_manager, |_| Ok(make_settings(rotation, integrity, biome_type)))?;
        Ok(OceanRuinPiece { data, biome_type, integrity, is_large })
    }

    /// `getHeight(pos, level, corner)`.
    fn get_height(pos: BlockPos, level: &dyn BlockGetter, corner: BlockPos) -> i32 {
        let mut new_y = pos.y();
        let mut min_y = 512;
        let top_y = new_y - 1;
        let mut area = 0;
        for p in BlockPos::between_closed(&pos, &corner) {
            let x = p.x();
            let z = p.z();
            let mut floor_y = pos.y() - 1;
            let mut temp_pos = BlockPos::new(x, floor_y, z);
            let mut temp_state = level.get_block_state(temp_pos);
            let mut temp_fluid = level.get_fluid_state(temp_pos);
            while (temp_state.is_air() || temp_fluid.is_tag("water") || temp_state.is_tag("ice")) && floor_y > level.get_min_y() + 1 {
                floor_y -= 1;
                temp_pos = BlockPos::new(x, floor_y, z);
                temp_state = level.get_block_state(temp_pos);
                temp_fluid = level.get_fluid_state(temp_pos);
            }
            min_y = min_y.min(floor_y);
            if floor_y < top_y - 2 {
                area += 1;
            }
        }
        let width = (pos.x() - corner.x()).abs();
        if top_y - min_y > 2 && area > width - 2 {
            new_y = min_y + 1;
        }
        new_y
    }
}

impl StructurePiece for OceanRuinPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(&self, context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        self.data.add_additional_save_data(context, tag);
        tag.put_string("Rot", rotation_legacy_name(self.data.place_settings.get_rotation()));
        tag.put_float("Integrity", self.integrity);
        tag.put_string("BiomeType", self.biome_type.legacy_name());
        tag.put_boolean("IsLarge", self.is_large);
    }

    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.data.move_by(dx, dy, dz);
    }

    fn get_rotation(&self) -> Rotation {
        self.data.get_rotation()
    }

    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        reference_pos: BlockPos,
    ) {
        let tp = self.data.template_position;
        let height = level.get_height_at(HeightmapType::OceanFloorWg, tp.x(), tp.z());
        let tp = BlockPos::new(tp.x(), height, tp.z());
        let size = self.data.template.get_size();
        let corner = transform(BlockPos::new(size.x - 1, 0, size.z - 1), Mirror::None, self.data.place_settings.get_rotation(), BlockPos::ZERO).offset(tp.x(), tp.y(), tp.z());
        self.data.template_position = BlockPos::new(tp.x(), Self::get_height(tp, level, corner), tp.z());
        let is_large = self.is_large;
        self.data.post_process(level, random, chunk_bb, reference_pos, &mut |marker_id, position, level, random, _chunk_bb| match marker_id {
            "chest" => {
                let waterlogged = level.get_fluid_state(position).is_tag("water");
                level.set_block_flags(position, Block::CHEST.default_state().set(props::WATERLOGGED, waterlogged), 2);
                let loot_table = if is_large { "minecraft:chests/underwater_ruin_big" } else { "minecraft:chests/underwater_ruin_small" };
                set_block_entity_loot_table(level, random, position, loot_table);
            }
            "drowned" => place_drowned(level, position),
            _ => {}
        });
    }
}

/// The `drowned` data marker: a persistent drowned at `position`, then the marker block becomes air above sea level and
/// water otherwise.
fn place_drowned(level: &mut dyn WorldGenLevel, position: BlockPos) {
    level.add_fresh_entity_nbt(super::spawned_mob_tag::drowned_entity_tag(position));
    if position.y() > level.get_sea_level() {
        level.set_block_flags(position, Block::AIR.default_state(), 2);
    } else {
        level.set_block_flags(position, Block::WATER.default_state(), 2);
    }
}
