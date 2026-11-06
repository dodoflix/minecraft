//! `IglooPieces`: the igloo top, its ladder shaft and laboratory as template pieces.

use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, read_rotation, rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::STRUCTURE_BLOCK;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_game_core::world::level::BlockGetter as _;
use mc_game_core::world::level::LevelAccessor as _;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::levelgen::block_entity_data::set_block_entity_loot_table;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext,
    StructurePieceType,
};
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::calculate_relative_position;
use mc_game_core::world::level::levelgen::structure::templatesystem::{
    LiquidSettings, StructurePlaceSettings, StructureTemplateManager,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;
use mc_world::heightmap::HeightmapType;

pub const GENERATION_HEIGHT: i32 = 90;
const STRUCTURE_LOCATION_IGLOO: &str = "igloo/top";
const STRUCTURE_LOCATION_LADDER: &str = "igloo/middle";
const STRUCTURE_LOCATION_LABORATORY: &str = "igloo/bottom";

/// `PIVOTS.get(templateLocation)`.
fn pivot(template_location: &Identifier) -> BlockPos {
    match template_location.path() {
        STRUCTURE_LOCATION_IGLOO => BlockPos::new(3, 5, 5),
        STRUCTURE_LOCATION_LADDER => BlockPos::new(1, 3, 1),
        STRUCTURE_LOCATION_LABORATORY => BlockPos::new(3, 6, 7),
        other => panic!("no igloo pivot for {other}"),
    }
}

/// `OFFSETS.get(templateLocation)`.
fn offset(template_location: &Identifier) -> BlockPos {
    match template_location.path() {
        STRUCTURE_LOCATION_IGLOO => BlockPos::ZERO,
        STRUCTURE_LOCATION_LADDER => BlockPos::new(2, -3, 4),
        STRUCTURE_LOCATION_LABORATORY => BlockPos::new(0, -3, -2),
        other => panic!("no igloo offset for {other}"),
    }
}

fn location(path: &str) -> Identifier {
    Identifier::with_default_namespace(path).expect("valid igloo template id")
}

/// `IglooPieces.addPieces(manager, position, rotation, builder, random)`.
pub fn add_pieces(
    structure_template_manager: &StructureTemplateManager,
    position: BlockPos,
    rotation: Rotation,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
) {
    if random.next_double() < 0.5 {
        let depth = random.next_int_bounded(8) + 4;
        builder.add_piece(Box::new(IglooPiece::new(
            structure_template_manager,
            &location(STRUCTURE_LOCATION_LABORATORY),
            position,
            rotation,
            depth * 3,
        )));
        for i in 0..depth - 1 {
            builder.add_piece(Box::new(IglooPiece::new(
                structure_template_manager,
                &location(STRUCTURE_LOCATION_LADDER),
                position,
                rotation,
                i * 3,
            )));
        }
    }
    builder.add_piece(Box::new(IglooPiece::new(
        structure_template_manager,
        &location(STRUCTURE_LOCATION_IGLOO),
        position,
        rotation,
        0,
    )));
}

/// `StructurePieceType.IGLOO` (`Iglu`).
#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static IGLOO: StructurePieceType = StructurePieceType {
    id: "iglu",
    load: load_piece,
};

fn load_piece(
    context: &StructurePieceSerializationContext<'_>,
    tag: &CompoundTag,
) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(IglooPiece::load(
        &context.env.structure_template_manager,
        tag,
    )?))
}

pub struct IglooPiece {
    data: TemplateStructurePieceData,
}

/// `makeSettings(rotation, templateLocation)`.
fn make_settings(rotation: Rotation, template_location: &Identifier) -> StructurePlaceSettings {
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_rotation(rotation)
        .set_mirror(Mirror::None)
        .set_rotation_pivot(pivot(template_location))
        .add_processor(STRUCTURE_BLOCK.clone())
        .set_liquid_settings(LiquidSettings::IgnoreWaterlogging);
    settings
}

impl IglooPiece {
    /// `IglooPiece(manager, templateLocation, position, rotation, depth)`.
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_location: &Identifier,
        position: BlockPos,
        rotation: Rotation,
        depth: i32,
    ) -> IglooPiece {
        let template_position = position
            .offset_vec(&offset(template_location))
            .below_n(depth);
        IglooPiece {
            data: TemplateStructurePieceData::new(
                &IGLOO,
                0,
                structure_template_manager,
                template_location,
                template_location.to_string(),
                make_settings(rotation, template_location),
                template_position,
            ),
        }
    }

    /// `IglooPiece(manager, tag)`.
    pub fn load(
        structure_template_manager: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<IglooPiece, String> {
        let rotation = read_rotation(tag, "Rot")?;
        Ok(IglooPiece {
            data: TemplateStructurePieceData::load(
                &IGLOO,
                tag,
                structure_template_manager,
                |location| Ok(make_settings(rotation, location)),
            )?,
        })
    }
}

impl StructurePiece for IglooPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(
        &self,
        context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.data.add_additional_save_data(context, tag);
        tag.put_string(
            "Rot",
            rotation_legacy_name(self.data.place_settings.get_rotation()),
        );
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
        let template_location =
            Identifier::parse(&self.data.template_name).expect("saved igloo template name");
        let settings = make_settings(self.data.place_settings.get_rotation(), &template_location);
        let offset = offset(&template_location);
        let entrance_pos = self
            .data
            .template_position
            .offset_vec(&calculate_relative_position(
                &settings,
                BlockPos::new(3 - offset.x(), 0, -offset.z()),
            ));
        let height = level.get_height_at(
            HeightmapType::WorldSurfaceWg,
            entrance_pos.x(),
            entrance_pos.z(),
        );
        let old_template_pos = self.data.template_position;
        self.data.template_position =
            self.data
                .template_position
                .offset(0, height - GENERATION_HEIGHT - 1, 0);
        self.data.post_process(
            level,
            random,
            chunk_bb,
            reference_pos,
            &mut |marker_id, position, level, random, _chunk_bb| {
                if marker_id == "chest" {
                    level.set_block_and_update(position, Block::AIR.default_state());
                    set_block_entity_loot_table(
                        level,
                        random,
                        position.below(),
                        "minecraft:chests/igloo_chest",
                    );
                }
            },
        );
        if template_location.path() == STRUCTURE_LOCATION_IGLOO {
            let trap_door_pos =
                self.data
                    .template_position
                    .offset_vec(&calculate_relative_position(
                        &settings,
                        BlockPos::new(3, 0, 5),
                    ));
            let below_state = level.get_block_state(trap_door_pos.below());
            if !below_state.is_air() && below_state.block() != Block::LADDER {
                level.set_block_and_update(trap_door_pos, Block::SNOW_BLOCK.default_state());
            }
        }
        self.data.template_position = old_template_pos;
    }
}
