//! `PoolElementStructurePiece` (piece type `jigsaw`): a placed pool element with its position, rotation and the junctions
//! to its neighbours.

use super::manager::StructureManager;
use super::piece::{PieceBase, StructurePiece, StructurePieceSerializationContext, StructurePieceType};
use super::pools::jigsaw_junction::JigsawJunction;
use super::pools::structure_pool_element::{StructurePoolElement, decode_element_from_nbt, encode_element};
use super::pools::structure_template_pool::Projection;
use super::templatesystem::{LiquidSettings, StructureTemplateManager};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_worldgen::beardifier::{JigsawJunction as BeardJunction, StructurePieceKind};
use std::sync::Arc;

/// `JigsawStructure.DEFAULT_LIQUID_SETTINGS`.
pub const DEFAULT_LIQUID_SETTINGS: LiquidSettings = LiquidSettings::ApplyWaterlogging;

#[linkme::distributed_slice(super::piece::STRUCTURE_PIECE_TYPES)]
pub static JIGSAW: StructurePieceType = StructurePieceType {
    id: "jigsaw",
    load: |context, tag| PoolElementStructurePiece::load(context, tag).map(|p| Box::new(p) as Box<dyn StructurePiece>),
};

pub struct PoolElementStructurePiece {
    base: PieceBase,
    element: Arc<dyn StructurePoolElement>,
    position: BlockPos,
    ground_level_delta: i32,
    rotation: Rotation,
    junctions: Vec<JigsawJunction>,
    structure_template_manager: Arc<StructureTemplateManager>,
    liquid_settings: LiquidSettings,
}

impl PoolElementStructurePiece {
    /// `PoolElementStructurePiece(structureTemplateManager, element, position, groundLevelDelta, rotation, boundingBox,
    /// liquidSettings)`.
    pub fn new(
        structure_template_manager: Arc<StructureTemplateManager>,
        element: Arc<dyn StructurePoolElement>,
        position: BlockPos,
        ground_level_delta: i32,
        rotation: Rotation,
        bounding_box: BoundingBox,
        liquid_settings: LiquidSettings,
    ) -> PoolElementStructurePiece {
        PoolElementStructurePiece {
            base: PieceBase::new(&JIGSAW, 0, bounding_box),
            element,
            position,
            ground_level_delta,
            rotation,
            junctions: Vec::new(),
            structure_template_manager,
            liquid_settings,
        }
    }

    /// `PoolElementStructurePiece(context, tag)`.
    pub fn load(context: &StructurePieceSerializationContext<'_>, tag: &CompoundTag) -> Result<PoolElementStructurePiece, String> {
        let mut base = PieceBase::load(&JIGSAW, tag)?;
        let manager = context.env.structure_template_manager.clone();
        let position = BlockPos::new(tag.get_int_or("PosX", 0), tag.get_int_or("PosY", 0), tag.get_int_or("PosZ", 0));
        let ground_level_delta = tag.get_int_or("ground_level_delta", 0);
        let element_tag = tag.get_compound("pool_element").ok_or("Invalid pool element found")?;
        let element = decode_element_from_nbt(element_tag, context.env).map_err(|_| "Invalid pool element found".to_owned())?;
        // `Rotation.LEGACY_CODEC`: the enum constant name
        let rotation = match tag.get_string("rotation") {
            Some("NONE") => Rotation::None,
            Some("CLOCKWISE_90") => Rotation::Clockwise90,
            Some("CLOCKWISE_180") => Rotation::Clockwise180,
            Some("COUNTERCLOCKWISE_90") => Rotation::Counterclockwise90,
            _ => return Err("No value present".to_owned()),
        };
        base.bounding_box = element.get_bounding_box(&manager, position, rotation);
        let empty = CompoundTag::new();
        let junctions = tag.get_list_or_empty("junctions").iter().map(|j| JigsawJunction::deserialize(j.as_compound().unwrap_or(&empty))).collect();
        let liquid_settings = tag.get_string("liquid_settings").and_then(LiquidSettings::by_name).unwrap_or(DEFAULT_LIQUID_SETTINGS);
        Ok(PoolElementStructurePiece { base, element, position, ground_level_delta, rotation, junctions, structure_template_manager: manager, liquid_settings })
    }

    /// `place(level, structureManager, generator, random, chunkBB, referencePos, keepJigsaws)`.
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        structure_manager: &StructureManager,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        reference_pos: BlockPos,
        keep_jigsaws: bool,
    ) {
        self.element.place(
            &self.structure_template_manager,
            level,
            structure_manager,
            generator,
            self.position,
            reference_pos,
            self.rotation,
            chunk_bb,
            random,
            self.liquid_settings,
            keep_jigsaws,
        );
    }

    pub fn get_element(&self) -> &Arc<dyn StructurePoolElement> {
        &self.element
    }

    pub fn get_position(&self) -> BlockPos {
        self.position
    }

    pub fn get_ground_level_delta(&self) -> i32 {
        self.ground_level_delta
    }

    /// `addJunction(junction)`.
    pub fn add_junction(&mut self, junction: JigsawJunction) {
        self.junctions.push(junction);
    }

    pub fn get_junctions(&self) -> &[JigsawJunction] {
        &self.junctions
    }

    pub fn get_liquid_settings(&self) -> LiquidSettings {
        self.liquid_settings
    }
}

impl StructurePiece for PoolElementStructurePiece {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn add_additional_save_data(&self, _context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_int("PosX", self.position.x());
        tag.put_int("PosY", self.position.y());
        tag.put_int("PosZ", self.position.z());
        tag.put_int("ground_level_delta", self.ground_level_delta);
        tag.put("pool_element", Tag::Compound(encode_element(&*self.element)));
        // `Rotation.LEGACY_CODEC`: `Enum::toString`
        tag.put_string(
            "rotation",
            match self.rotation {
                Rotation::None => "NONE",
                Rotation::Clockwise90 => "CLOCKWISE_90",
                Rotation::Clockwise180 => "CLOCKWISE_180",
                Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
            },
        );
        let mut junctions = ListTag::new();
        for junction in &self.junctions {
            junctions.push(Tag::Compound(junction.serialize()));
        }
        tag.put("junctions", Tag::List(junctions));
        if self.liquid_settings != DEFAULT_LIQUID_SETTINGS {
            tag.put_string("liquid_settings", self.liquid_settings.get_serialized_name());
        }
    }

    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        structure_manager: &StructureManager,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        reference_pos: BlockPos,
    ) {
        self.place(level, structure_manager, generator, random, chunk_bb, reference_pos, false);
    }

    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.base.bounding_box.move_(dx, dy, dz);
        self.position = self.position.offset(dx, dy, dz);
    }

    fn get_rotation(&self) -> Rotation {
        self.rotation
    }

    fn beardifier_kind(&self) -> StructurePieceKind {
        StructurePieceKind::PoolElement {
            rigid: self.element.get_projection() == Projection::Rigid,
            ground_level_delta: self.ground_level_delta,
            junctions: self.junctions.iter().map(|j| BeardJunction { source_x: j.source_x, source_ground_y: j.source_ground_y, source_z: j.source_z }).collect(),
        }
    }
}
