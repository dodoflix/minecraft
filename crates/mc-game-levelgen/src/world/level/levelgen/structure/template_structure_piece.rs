//! `TemplateStructurePiece`: the state and placement of the pieces that place a [`StructureTemplate`] (shipwreck, ocean
//! ruin, ruined portal, nether fossil, igloo, end city, woodland mansion). A piece embeds [`TemplateStructurePieceData`]
//! (its `base()` is `data.base`), overrides `get_rotation`/`move_by` like vanilla and calls [`TemplateStructurePieceData::post_process`]
//! with its `handleDataMarker`.

use mc_brigadier::StringReader;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_game_core::commands::arguments::blocks::block_state_parser::BlockStateParser;
use mc_game_core::commands::command_build_context::{HolderLookupProvider, StaticRegistries};
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, StructurePieceSerializationContext, StructurePieceType,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::StructureTemplate;
use mc_game_core::world::level::levelgen::structure::templatesystem::{
    StructurePlaceSettings, StructureTemplateManager,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;
use std::sync::Arc;

/// `handleDataMarker(markerId, position, level, random, chunkBB)`.
pub type DataMarkerHandler<'a> =
    dyn FnMut(&str, BlockPos, &mut dyn WorldGenLevel, &mut dyn RandomSource, &BoundingBox) + 'a;

/// `Rotation.LEGACY_CODEC` encode (`Enum.toString()`).
pub fn rotation_legacy_name(rotation: Rotation) -> &'static str {
    match rotation {
        Rotation::None => "NONE",
        Rotation::Clockwise90 => "CLOCKWISE_90",
        Rotation::Clockwise180 => "CLOCKWISE_180",
        Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
    }
}

/// `Rotation.LEGACY_CODEC` decode (`Rotation.valueOf`).
pub fn rotation_from_legacy_name(name: &str) -> Option<Rotation> {
    Rotation::ALL
        .into_iter()
        .find(|r| rotation_legacy_name(*r) == name)
}

/// `Mirror.LEGACY_CODEC` encode.
pub fn mirror_legacy_name(mirror: Mirror) -> &'static str {
    match mirror {
        Mirror::None => "NONE",
        Mirror::LeftRight => "LEFT_RIGHT",
        Mirror::FrontBack => "FRONT_BACK",
    }
}

/// `Mirror.LEGACY_CODEC` decode.
pub fn mirror_from_legacy_name(name: &str) -> Option<Mirror> {
    Mirror::ALL
        .into_iter()
        .find(|m| mirror_legacy_name(*m) == name)
}

/// `tag.read(key, Rotation.LEGACY_CODEC).orElseThrow()`.
pub fn read_rotation(tag: &CompoundTag, key: &str) -> Result<Rotation, String> {
    tag.get_string(key)
        .and_then(rotation_from_legacy_name)
        .ok_or_else(|| "No value present".to_owned())
}

/// `tag.read(key, Mirror.LEGACY_CODEC).orElseThrow()`.
pub fn read_mirror(tag: &CompoundTag, key: &str) -> Result<Mirror, String> {
    tag.get_string(key)
        .and_then(mirror_from_legacy_name)
        .ok_or_else(|| "No value present".to_owned())
}

pub struct TemplateStructurePieceData {
    pub base: PieceBase,
    /// `templateName`.
    pub template_name: String,
    pub template: Arc<StructureTemplate>,
    pub place_settings: StructurePlaceSettings,
    pub template_position: BlockPos,
}

impl TemplateStructurePieceData {
    /// `TemplateStructurePiece(type, genDepth, manager, templateLocation, templateName, placeSettings, position)`.
    pub fn new(
        piece_type: &'static StructurePieceType,
        gen_depth: i32,
        structure_template_manager: &StructureTemplateManager,
        template_location: &Identifier,
        template_name: String,
        place_settings: StructurePlaceSettings,
        position: BlockPos,
    ) -> Self {
        let template = structure_template_manager.get_or_create(template_location);
        let mut base = PieceBase::new(
            piece_type,
            gen_depth,
            template.get_bounding_box(&place_settings, position),
        );
        base.set_orientation(Some(Direction::North));
        TemplateStructurePieceData {
            base,
            template_name,
            template,
            place_settings,
            template_position: position,
        }
    }

    /// `TemplateStructurePiece(type, tag, manager, structurePlaceSettingsSupplier)` (`makeTemplateLocation` =
    /// `Identifier.parse(templateName)`).
    pub fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
        structure_template_manager: &StructureTemplateManager,
        structure_place_settings_supplier: impl FnOnce(
            &Identifier,
        )
            -> Result<StructurePlaceSettings, String>,
    ) -> Result<Self, String> {
        Self::load_with_location(
            piece_type,
            tag,
            structure_template_manager,
            |name| Identifier::parse(name).map_err(|e| e.to_string()),
            structure_place_settings_supplier,
        )
    }

    /// As [`Self::load`] for the pieces overriding `makeTemplateLocation()` (end city, woodland mansion).
    pub fn load_with_location(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
        structure_template_manager: &StructureTemplateManager,
        make_template_location: impl FnOnce(&str) -> Result<Identifier, String>,
        structure_place_settings_supplier: impl FnOnce(
            &Identifier,
        )
            -> Result<StructurePlaceSettings, String>,
    ) -> Result<Self, String> {
        let mut base = PieceBase::load(piece_type, tag)?;
        base.set_orientation(Some(Direction::North));
        let template_name = tag.get_string_or("Template", "").to_owned();
        let template_position = BlockPos::new(
            tag.get_int_or("TPX", 0),
            tag.get_int_or("TPY", 0),
            tag.get_int_or("TPZ", 0),
        );
        let template_location = make_template_location(&template_name)?;
        let template = structure_template_manager.get_or_create(&template_location);
        let place_settings = structure_place_settings_supplier(&template_location)?;
        base.bounding_box = template.get_bounding_box(&place_settings, template_position);
        Ok(TemplateStructurePieceData {
            base,
            template_name,
            template,
            place_settings,
            template_position,
        })
    }

    /// `addAdditionalSaveData(context, tag)`.
    pub fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        tag.put_int("TPX", self.template_position.x());
        tag.put_int("TPY", self.template_position.y());
        tag.put_int("TPZ", self.template_position.z());
        tag.put_string("Template", self.template_name.clone());
    }

    /// `postProcess(level, structureManager, generator, random, chunkBB, chunkPos, referencePos)`: places the template
    /// inside `chunk_bb`, then runs the data markers through `handle_data_marker` and replaces the jigsaw blocks with
    /// their `final_state`.
    pub fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        reference_pos: BlockPos,
        handle_data_marker: &mut DataMarkerHandler<'_>,
    ) {
        self.place_settings.set_bounding_box(*chunk_bb);
        self.base.bounding_box = self
            .template
            .get_bounding_box(&self.place_settings, self.template_position);
        if !self.template.place_in_world(
            level,
            self.template_position,
            reference_pos,
            &self.place_settings,
            random,
            2,
        ) {
            return;
        }
        for data_marker in self.template.filter_blocks(
            self.template_position,
            &self.place_settings,
            Block::STRUCTURE_BLOCK,
        ) {
            let Some(nbt) = &data_marker.nbt else {
                continue;
            };
            // `StructureMode.LEGACY_CODEC` (`orElseThrow`): a data pack template always stores a valid mode.
            if nbt.get_string("mode") == Some("DATA") {
                handle_data_marker(
                    nbt.get_string_or("metadata", ""),
                    data_marker.pos,
                    level,
                    random,
                    chunk_bb,
                );
            }
        }
        for jigsaw_block in
            self.template
                .filter_blocks(self.template_position, &self.place_settings, Block::JIGSAW)
        {
            let Some(nbt) = &jigsaw_block.nbt else {
                continue;
            };
            let state_string = nbt.get_string_or("final_state", "minecraft:air");
            let mut target_state = Block::AIR.default_state();
            let parsed = StaticRegistries
                .lookup("minecraft:block")
                .ok_or(())
                .and_then(|blocks| {
                    BlockStateParser::parse_for_block(
                        &*blocks,
                        &mut StringReader::new(state_string),
                        true,
                    )
                    .map_err(|_| ())
                });
            match parsed {
                Ok(result) => target_state = result.block_state,
                Err(()) => log::error!(
                    "Error while parsing blockstate {} in jigsaw block @ {}",
                    state_string,
                    jigsaw_block.pos
                ),
            }
            level.set_block_and_update(jigsaw_block.pos, target_state);
        }
    }

    /// `move(dx, dy, dz)`: the box and the template position.
    pub fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.base.bounding_box.move_(dx, dy, dz);
        self.template_position = self.template_position.offset(dx, dy, dz);
    }

    /// `getRotation()`.
    pub fn get_rotation(&self) -> Rotation {
        self.place_settings.get_rotation()
    }
}
