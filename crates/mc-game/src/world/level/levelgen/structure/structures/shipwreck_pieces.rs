//! `ShipwreckPieces`: the shipwreck template piece (beached / ocean variants, loot through the data markers).

use crate::world::level::levelgen::block_entity_data::set_block_entity_loot_table;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext, StructurePieceType,
};
use crate::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, read_rotation, rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::STRUCTURE_AND_AIR;
use crate::world::level::levelgen::structure::templatesystem::{StructurePlaceSettings, StructureTemplateManager};
use crate::world::level::{BlockGetter as _, LevelReader as _};
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_nbt::CompoundTag;
use mc_world::heightmap::HeightmapType;

const PIVOT: BlockPos = BlockPos::new(4, 0, 15);
const NUMBER_OF_BLOCKS_ALLOWED_IN_WORLD_GEN_REGION: i32 = 32;

const STRUCTURE_LOCATION_BEACHED: [&str; 11] = [
    "shipwreck/with_mast",
    "shipwreck/sideways_full",
    "shipwreck/sideways_fronthalf",
    "shipwreck/sideways_backhalf",
    "shipwreck/rightsideup_full",
    "shipwreck/rightsideup_fronthalf",
    "shipwreck/rightsideup_backhalf",
    "shipwreck/with_mast_degraded",
    "shipwreck/rightsideup_full_degraded",
    "shipwreck/rightsideup_fronthalf_degraded",
    "shipwreck/rightsideup_backhalf_degraded",
];

const STRUCTURE_LOCATION_OCEAN: [&str; 20] = [
    "shipwreck/with_mast",
    "shipwreck/upsidedown_full",
    "shipwreck/upsidedown_fronthalf",
    "shipwreck/upsidedown_backhalf",
    "shipwreck/sideways_full",
    "shipwreck/sideways_fronthalf",
    "shipwreck/sideways_backhalf",
    "shipwreck/rightsideup_full",
    "shipwreck/rightsideup_fronthalf",
    "shipwreck/rightsideup_backhalf",
    "shipwreck/with_mast_degraded",
    "shipwreck/upsidedown_full_degraded",
    "shipwreck/upsidedown_fronthalf_degraded",
    "shipwreck/upsidedown_backhalf_degraded",
    "shipwreck/sideways_full_degraded",
    "shipwreck/sideways_fronthalf_degraded",
    "shipwreck/sideways_backhalf_degraded",
    "shipwreck/rightsideup_full_degraded",
    "shipwreck/rightsideup_fronthalf_degraded",
    "shipwreck/rightsideup_backhalf_degraded",
];

/// `MARKERS_TO_LOOT`.
fn marker_loot_table(marker_id: &str) -> Option<&'static str> {
    match marker_id {
        "map_chest" => Some("minecraft:chests/shipwreck_map"),
        "treasure_chest" => Some("minecraft:chests/shipwreck_treasure"),
        "supply_chest" => Some("minecraft:chests/shipwreck_supply"),
        _ => None,
    }
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static SHIPWRECK_PIECE: StructurePieceType = StructurePieceType { id: "shipwreck", load: load_piece };

fn load_piece(context: &StructurePieceSerializationContext<'_>, tag: &CompoundTag) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(ShipwreckPiece::load(&context.env.structure_template_manager, tag)?))
}

/// `ShipwreckPieces.addRandomPiece(manager, position, rotation, builder, random, isBeached)`: returns the index of the
/// piece in the builder (vanilla returns the piece itself, which the structure then adjusts).
pub fn add_random_piece(
    structure_template_manager: &StructureTemplateManager,
    position: BlockPos,
    rotation: Rotation,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    is_beached: bool,
) -> usize {
    let locations: &[&str] = if is_beached { &STRUCTURE_LOCATION_BEACHED } else { &STRUCTURE_LOCATION_OCEAN };
    // `Util.getRandom(array, random)`
    let identifier = Identifier::with_default_namespace(locations[random.next_int_bounded(locations.len() as i32) as usize]).expect("valid shipwreck template id");
    let piece = ShipwreckPiece::new(structure_template_manager, &identifier, position, rotation, is_beached);
    builder.add_piece(Box::new(piece));
    builder.len() - 1
}

pub struct ShipwreckPiece {
    data: TemplateStructurePieceData,
    is_beached: bool,
    height_adjusted: bool,
}

impl ShipwreckPiece {
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_location: &Identifier,
        position: BlockPos,
        rotation: Rotation,
        is_beached: bool,
    ) -> ShipwreckPiece {
        ShipwreckPiece {
            data: TemplateStructurePieceData::new(
                &SHIPWRECK_PIECE,
                0,
                structure_template_manager,
                template_location,
                template_location.to_string(),
                make_settings(rotation),
                position,
            ),
            is_beached,
            height_adjusted: false,
        }
    }

    pub fn load(structure_template_manager: &StructureTemplateManager, tag: &CompoundTag) -> Result<ShipwreckPiece, String> {
        let data = TemplateStructurePieceData::load(&SHIPWRECK_PIECE, tag, structure_template_manager, |_| Ok(make_settings(read_rotation(tag, "Rot")?)))?;
        Ok(ShipwreckPiece { data, is_beached: tag.get_boolean_or("isBeached", false), height_adjusted: tag.get_boolean_or("height_adjusted", false) })
    }

    /// `isTooBigToFitInWorldGenRegion()`.
    pub fn is_too_big_to_fit_in_world_gen_region(&self) -> bool {
        let size = self.data.template.get_size();
        size.x > NUMBER_OF_BLOCKS_ALLOWED_IN_WORLD_GEN_REGION || size.y > NUMBER_OF_BLOCKS_ALLOWED_IN_WORLD_GEN_REGION
    }

    /// `calculateBeachedPosition(minY, random)`.
    pub fn calculate_beached_position(&self, min_y: i32, random: &mut dyn RandomSource) -> i32 {
        min_y - self.data.template.get_size().y / 2 - random.next_int_bounded(3)
    }

    /// `adjustPositionHeight(newHeight)`.
    pub fn adjust_position_height(&mut self, new_height: i32) {
        self.height_adjusted = true;
        self.data.template_position = BlockPos::new(self.data.template_position.x(), new_height, self.data.template_position.z());
    }
}

/// `makeSettings(rotation)`.
fn make_settings(rotation: Rotation) -> StructurePlaceSettings {
    let mut settings = StructurePlaceSettings::new();
    settings.set_rotation(rotation).set_mirror(Mirror::None).set_rotation_pivot(PIVOT).add_processor(STRUCTURE_AND_AIR.clone());
    settings
}

impl StructurePiece for ShipwreckPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(&self, context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        self.data.add_additional_save_data(context, tag);
        tag.put_boolean("isBeached", self.is_beached);
        tag.put_string("Rot", rotation_legacy_name(self.data.place_settings.get_rotation()));
        tag.put_boolean("height_adjusted", self.height_adjusted);
    }

    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.data.move_by(dx, dy, dz);
    }

    fn get_rotation(&self) -> Rotation {
        self.data.get_rotation()
    }

    fn post_process(
        &mut self,
        level: &mut dyn crate::world::level::levelgen::world_gen_level::WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        reference_pos: BlockPos,
    ) {
        if !self.height_adjusted && !self.is_too_big_to_fit_in_world_gen_region() {
            let mut min_y = level.get_max_y() + 1;
            let mut mean = 0i32;
            let template_size = self.data.template.get_size();
            let heightmap_type = if self.is_beached { HeightmapType::WorldSurfaceWg } else { HeightmapType::OceanFloorWg };
            let base_size = template_size.x * template_size.z;
            let template_position = self.data.template_position;
            if base_size == 0 {
                mean = level.get_height_at(heightmap_type, template_position.x(), template_position.z());
            } else {
                let corner = template_position.offset(template_size.x - 1, 0, template_size.z - 1);
                for p in BlockPos::between_closed(&template_position, &corner) {
                    let heightmap = level.get_height_at(heightmap_type, p.x(), p.z());
                    mean += heightmap;
                    min_y = min_y.min(heightmap);
                }
                mean /= base_size;
            }
            let new_height = if self.is_beached { self.calculate_beached_position(min_y, random) } else { mean };
            self.adjust_position_height(new_height);
        }
        self.data.post_process(level, random, chunk_bb, reference_pos, &mut |marker_id, position, level, random, _chunk_bb| {
            if let Some(loot_table) = marker_loot_table(marker_id) {
                set_block_entity_loot_table(level, random, position.below(), loot_table);
            }
        });
    }
}
