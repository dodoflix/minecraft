//! `RuinedPortalPiece`: the ruined portal template piece (vertical placement, netherrack/magma spreading, vines and
//! leaves, the age/lava/blackstone processors).

use crate::world::level::block::BlockStateExt as _;
use crate::world::level::levelgen::codec::BlockSet;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext, StructurePieceType, get_random_horizontal_direction,
};
use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, mirror_legacy_name, read_mirror, read_rotation, rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::always_true_test::AlwaysTrueTest;
use crate::world::level::levelgen::structure::templatesystem::blackstone_replace_processor::BlackstoneReplaceProcessor;
use crate::world::level::levelgen::structure::templatesystem::block_age_processor::BlockAgeProcessor;
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::{STRUCTURE_AND_AIR, STRUCTURE_BLOCK};
use crate::world::level::levelgen::structure::templatesystem::block_match_test::BlockMatchTest;
use crate::world::level::levelgen::structure::templatesystem::lava_submerged_block_processor::LavaSubmergedBlockProcessor;
use crate::world::level::levelgen::structure::templatesystem::pos_always_true_test::PosAlwaysTrueTest;
use crate::world::level::levelgen::structure::templatesystem::processor_rule::ProcessorRule;
use crate::world::level::levelgen::structure::templatesystem::protected_block_processor::ProtectedBlockProcessor;
use crate::world::level::levelgen::structure::templatesystem::random_block_match_test::RandomBlockMatchTest;
use crate::world::level::levelgen::structure::templatesystem::rule::blockentity::passthrough::Passthrough;
use crate::world::level::levelgen::structure::templatesystem::rule_processor::RuleProcessor;
use crate::world::level::levelgen::structure::templatesystem::{StructurePlaceSettings, StructureTemplateManager};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::shapes::is_face_full;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_nbt::{CompoundTag, Tag};
use mc_world::heightmap::HeightmapType;
use serde_json::Value;
use std::sync::{Arc, LazyLock};

const PROBABILITY_OF_GOLD_GONE: f32 = 0.3;
const PROBABILITY_OF_MAGMA_INSTEAD_OF_NETHERRACK: f32 = 0.07;
const PROBABILITY_OF_MAGMA_INSTEAD_OF_LAVA: f32 = 0.2;

/// `BlockTags.FEATURES_CANNOT_REPLACE`.
static FEATURES_CANNOT_REPLACE: LazyLock<BlockSet> =
    LazyLock::new(|| BlockSet::from_json(&Value::String("#minecraft:features_cannot_replace".to_owned())).expect("features_cannot_replace tag"));

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static RUINED_PORTAL: StructurePieceType = StructurePieceType { id: "rupo", load: load_piece };

fn load_piece(context: &StructurePieceSerializationContext<'_>, tag: &CompoundTag) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(RuinedPortalPiece::load(context, tag)?))
}

/// `RuinedPortalPiece.VerticalPlacement`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalPlacement {
    OnLandSurface,
    PartlyBuried,
    OnOceanFloor,
    InMountain,
    Underground,
    InNether,
}

impl VerticalPlacement {
    pub const ALL: [VerticalPlacement; 6] = [
        VerticalPlacement::OnLandSurface,
        VerticalPlacement::PartlyBuried,
        VerticalPlacement::OnOceanFloor,
        VerticalPlacement::InMountain,
        VerticalPlacement::Underground,
        VerticalPlacement::InNether,
    ];

    /// `getSerializedName()`.
    pub fn get_serialized_name(self) -> &'static str {
        match self {
            VerticalPlacement::OnLandSurface => "on_land_surface",
            VerticalPlacement::PartlyBuried => "partly_buried",
            VerticalPlacement::OnOceanFloor => "on_ocean_floor",
            VerticalPlacement::InMountain => "in_mountain",
            VerticalPlacement::Underground => "underground",
            VerticalPlacement::InNether => "in_nether",
        }
    }

    /// `VerticalPlacement.CODEC` lookup.
    pub fn by_name(name: &str) -> Option<VerticalPlacement> {
        Self::ALL.into_iter().find(|p| p.get_serialized_name() == name)
    }
}

/// `RuinedPortalPiece.Properties`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Properties {
    pub cold: bool,
    pub mossiness: f32,
    pub air_pocket: bool,
    pub overgrown: bool,
    pub vines: bool,
    pub replace_with_blackstone: bool,
}

impl Properties {
    /// `Properties.CODEC` encode (a compound).
    pub fn to_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_boolean("cold", self.cold);
        tag.put_float("mossiness", self.mossiness);
        tag.put_boolean("air_pocket", self.air_pocket);
        tag.put_boolean("overgrown", self.overgrown);
        tag.put_boolean("vines", self.vines);
        tag.put_boolean("replace_with_blackstone", self.replace_with_blackstone);
        tag
    }

    /// `Properties.CODEC` decode: every field is required.
    pub fn from_tag(tag: Option<&Tag>) -> Result<Properties, String> {
        let Some(Tag::Compound(tag)) = tag else { return Err("Not a map".to_owned()) };
        let boolean = |name: &str| tag.get(name).and_then(Tag::as_boolean).ok_or_else(|| format!("No key {name} in MapLike"));
        Ok(Properties {
            cold: boolean("cold")?,
            mossiness: tag.get("mossiness").and_then(Tag::as_float).ok_or_else(|| "No key mossiness in MapLike".to_owned())?,
            air_pocket: boolean("air_pocket")?,
            overgrown: boolean("overgrown")?,
            vines: boolean("vines")?,
            replace_with_blackstone: boolean("replace_with_blackstone")?,
        })
    }
}

/// `getBlockReplaceRule(source, probability, target)`.
fn get_block_replace_rule_random(source: Block, probability: f32, target: Block) -> ProcessorRule {
    ProcessorRule::new(
        Arc::new(RandomBlockMatchTest { block: source, probability }),
        Arc::new(AlwaysTrueTest),
        Arc::new(PosAlwaysTrueTest),
        target.default_state(),
        Arc::new(Passthrough),
    )
}

/// `getBlockReplaceRule(source, target)`.
fn get_block_replace_rule(source: Block, target: Block) -> ProcessorRule {
    ProcessorRule::new(
        Arc::new(BlockMatchTest { block: source }),
        Arc::new(AlwaysTrueTest),
        Arc::new(PosAlwaysTrueTest),
        target.default_state(),
        Arc::new(Passthrough),
    )
}

/// `getLavaProcessorRule(verticalPlacement, properties)`.
fn get_lava_processor_rule(vertical_placement: VerticalPlacement, properties: &Properties) -> ProcessorRule {
    if vertical_placement == VerticalPlacement::OnOceanFloor {
        get_block_replace_rule(Block::LAVA, Block::MAGMA_BLOCK)
    } else if properties.cold {
        get_block_replace_rule(Block::LAVA, Block::NETHERRACK)
    } else {
        get_block_replace_rule_random(Block::LAVA, PROBABILITY_OF_MAGMA_INSTEAD_OF_LAVA, Block::MAGMA_BLOCK)
    }
}

/// `makeSettings(registries, mirror, rotation, verticalPlacement, pivot, properties)`.
fn make_settings(mirror: Mirror, rotation: Rotation, vertical_placement: VerticalPlacement, pivot: BlockPos, properties: &Properties) -> StructurePlaceSettings {
    let ignore_processor = if properties.air_pocket { STRUCTURE_BLOCK.clone() } else { STRUCTURE_AND_AIR.clone() };
    let mut rules = vec![get_block_replace_rule_random(Block::GOLD_BLOCK, PROBABILITY_OF_GOLD_GONE, Block::AIR), get_lava_processor_rule(vertical_placement, properties)];
    if !properties.cold {
        rules.push(get_block_replace_rule_random(Block::NETHERRACK, PROBABILITY_OF_MAGMA_INSTEAD_OF_NETHERRACK, Block::MAGMA_BLOCK));
    }
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_rotation(rotation)
        .set_mirror(mirror)
        .set_rotation_pivot(pivot)
        .add_processor(ignore_processor)
        .add_processor(Arc::new(RuleProcessor::new(rules)))
        .add_processor(Arc::new(BlockAgeProcessor::new(properties.mossiness)))
        .add_processor(Arc::new(ProtectedBlockProcessor { cannot_replace: FEATURES_CANNOT_REPLACE.clone() }))
        .add_processor(Arc::new(LavaSubmergedBlockProcessor));
    if properties.replace_with_blackstone {
        settings.add_processor(Arc::new(BlackstoneReplaceProcessor));
    }
    settings
}

/// `getHeightMapType(verticalPlacement)`.
pub fn get_height_map_type(vertical_placement: VerticalPlacement) -> HeightmapType {
    if vertical_placement == VerticalPlacement::OnOceanFloor { HeightmapType::OceanFloorWg } else { HeightmapType::WorldSurfaceWg }
}

pub struct RuinedPortalPiece {
    data: TemplateStructurePieceData,
    vertical_placement: VerticalPlacement,
    properties: Properties,
}

impl RuinedPortalPiece {
    /// `RuinedPortalPiece(registries, manager, templatePosition, verticalPlacement, properties, templateLocation,
    /// template, rotation, mirror, pivot)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_position: BlockPos,
        vertical_placement: VerticalPlacement,
        properties: Properties,
        template_location: &Identifier,
        rotation: Rotation,
        mirror: Mirror,
        pivot: BlockPos,
    ) -> RuinedPortalPiece {
        RuinedPortalPiece {
            data: TemplateStructurePieceData::new(
                &RUINED_PORTAL,
                0,
                structure_template_manager,
                template_location,
                template_location.to_string(),
                make_settings(mirror, rotation, vertical_placement, pivot, &properties),
                template_position,
            ),
            vertical_placement,
            properties,
        }
    }

    /// `RuinedPortalPiece(context, tag)`.
    pub fn load(context: &StructurePieceSerializationContext<'_>, tag: &CompoundTag) -> Result<RuinedPortalPiece, String> {
        let manager = &context.env.structure_template_manager;
        let vertical_placement = Self::read_vertical_placement(tag)?;
        let data = TemplateStructurePieceData::load(&RUINED_PORTAL, tag, manager, |location| {
            let template = manager.get_or_create(location);
            let size = template.get_size();
            let pivot = BlockPos::new(size.x / 2, 0, size.z / 2);
            Ok(make_settings(read_mirror(tag, "Mirror")?, read_rotation(tag, "Rotation")?, Self::read_vertical_placement(tag)?, pivot, &Properties::from_tag(tag.get("Properties"))?))
        })?;
        Ok(RuinedPortalPiece { data, vertical_placement, properties: Properties::from_tag(tag.get("Properties"))? })
    }

    fn read_vertical_placement(tag: &CompoundTag) -> Result<VerticalPlacement, String> {
        tag.get_string("VerticalPlacement").and_then(VerticalPlacement::by_name).ok_or_else(|| "No value present".to_owned())
    }

    fn can_block_be_replaced_by_netherrack_or_magma_at(&self, level: &dyn WorldGenLevel, pos: BlockPos) -> bool {
        self.can_block_be_replaced_by_netherrack_or_magma(level.get_block_state(pos))
    }

    /// `canBlockBeReplacedByNetherrackOrMagma(state)`.
    fn can_block_be_replaced_by_netherrack_or_magma(&self, state: BlockState) -> bool {
        state.block() != Block::AIR
            && state.block() != Block::OBSIDIAN
            && !FEATURES_CANNOT_REPLACE.contains(state.block())
            && (self.vertical_placement == VerticalPlacement::InNether || state.block() != Block::LAVA)
    }

    /// `placeNetherrackOrMagma(random, level, pos)`.
    fn place_netherrack_or_magma(&self, random: &mut dyn RandomSource, level: &mut dyn WorldGenLevel, pos: BlockPos) {
        if self.can_block_be_replaced_by_netherrack_or_magma_at(level, pos) {
            if !self.properties.cold && random.next_float() < 0.07 {
                level.set_block_and_update(pos, Block::MAGMA_BLOCK.default_state());
            } else {
                level.set_block_and_update(pos, Block::NETHERRACK.default_state());
            }
        }
    }

    /// `addNetherrackDripColumn(random, level, pos)`.
    fn add_netherrack_drip_column(&self, random: &mut dyn RandomSource, level: &mut dyn WorldGenLevel, pos: BlockPos) {
        let mut current_pos = pos;
        self.place_netherrack_or_magma(random, level, current_pos);
        let mut remaining_cap = 8;
        while remaining_cap > 0 && random.next_float() < 0.5 {
            current_pos = current_pos.below();
            remaining_cap -= 1;
            self.place_netherrack_or_magma(random, level, current_pos);
        }
    }

    /// `addNetherrackDripColumnsBelowPortal(random, level)`.
    fn add_netherrack_drip_columns_below_portal(&self, random: &mut dyn RandomSource, level: &mut dyn WorldGenLevel) {
        let bb = self.data.base.bounding_box;
        for x in bb.min_x() + 1..bb.max_x() {
            for z in bb.min_z() + 1..bb.max_z() {
                let pos = BlockPos::new(x, bb.min_y(), z);
                if level.get_block_state(pos).block() == Block::NETHERRACK {
                    self.add_netherrack_drip_column(random, level, pos.below());
                }
            }
        }
    }

    /// `spreadNetherrack(random, level)`.
    fn spread_netherrack(&self, random: &mut dyn RandomSource, level: &mut dyn WorldGenLevel) {
        let follow_ground_surface = self.vertical_placement == VerticalPlacement::OnLandSurface || self.vertical_placement == VerticalPlacement::OnOceanFloor;
        let bb = self.data.base.bounding_box;
        let center = bb.get_center();
        let center_x = center.x();
        let center_z = center.z();
        const NETHERRACK_PROBABILITY_BY_DISTANCE: [f32; 14] = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.9, 0.9, 0.8, 0.7, 0.6, 0.4, 0.2];
        let max_distance = NETHERRACK_PROBABILITY_BY_DISTANCE.len() as i32;
        let average_width = (bb.get_x_span() + bb.get_z_span()) / 2;
        let distance_adjustment = random.next_int_bounded(1.max(8 - average_width / 2));
        let max_y_diff = 3;
        for x in center_x - max_distance..=center_x + max_distance {
            for z in center_z - max_distance..=center_z + max_distance {
                let distance = (x - center_x).abs() + (z - center_z).abs();
                let adjusted_distance = 0.max(distance + distance_adjustment);
                if adjusted_distance < max_distance {
                    let probability_of_netherrack = NETHERRACK_PROBABILITY_BY_DISTANCE[adjusted_distance as usize];
                    if random.next_double() < f64::from(probability_of_netherrack) {
                        let surface_y = get_surface_y(level, x, z, self.vertical_placement);
                        let y = if follow_ground_surface { surface_y } else { bb.min_y().min(surface_y) };
                        let pos = BlockPos::new(x, y, z);
                        if (y - bb.min_y()).abs() <= max_y_diff && self.can_block_be_replaced_by_netherrack_or_magma_at(level, pos) {
                            self.place_netherrack_or_magma(random, level, pos);
                            if self.properties.overgrown {
                                self.maybe_add_leaves_above(random, level, pos);
                            }
                            self.add_netherrack_drip_column(random, level, pos.below());
                        }
                    }
                }
            }
        }
    }

    /// `maybeAddVines(random, level, pos)`.
    fn maybe_add_vines(&self, random: &mut dyn RandomSource, level: &mut dyn WorldGenLevel, pos: BlockPos) {
        let state = level.get_block_state(pos);
        if !state.is_air() && state.block() != Block::VINE {
            let direction = get_random_horizontal_direction(random);
            let neighbour_pos = pos.relative(direction);
            let neighour_state = level.get_block_state(neighbour_pos);
            if neighour_state.is_air() && is_face_full(&state.get_collision_shape(&*level, pos), direction) {
                // `VineBlock.getPropertyForFace(opposite)`
                let vine = Block::VINE.default_state();
                let vine = match direction.get_opposite() {
                    Direction::North => vine.set(props::NORTH, true),
                    Direction::East => vine.set(props::EAST, true),
                    Direction::South => vine.set(props::SOUTH, true),
                    Direction::West => vine.set(props::WEST, true),
                    _ => vine.set(props::UP, true),
                };
                level.set_block_and_update(neighbour_pos, vine);
            }
        }
    }

    /// `maybeAddLeavesAbove(random, level, pos)`.
    fn maybe_add_leaves_above(&self, random: &mut dyn RandomSource, level: &mut dyn WorldGenLevel, pos: BlockPos) {
        if random.next_float() < 0.5 && level.get_block_state(pos).block() == Block::NETHERRACK && level.get_block_state(pos.above()).is_air() {
            level.set_block_and_update(pos.above(), Block::JUNGLE_LEAVES.default_state().set(props::PERSISTENT, true));
        }
    }
}

/// `getSurfaceY(level, x, z, verticalPlacement)`.
fn get_surface_y(level: &dyn WorldGenLevel, x: i32, z: i32, vertical_placement: VerticalPlacement) -> i32 {
    level.get_height_at(get_height_map_type(vertical_placement), x, z) - 1
}

impl StructurePiece for RuinedPortalPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(&self, context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        self.data.add_additional_save_data(context, tag);
        tag.put_string("Rotation", rotation_legacy_name(self.data.place_settings.get_rotation()));
        tag.put_string("Mirror", mirror_legacy_name(self.data.place_settings.get_mirror()));
        tag.put_string("VerticalPlacement", self.vertical_placement.get_serialized_name());
        tag.put("Properties", self.properties.to_tag());
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
        let bounding_box = self.data.template.get_bounding_box(&self.data.place_settings, self.data.template_position);
        if chunk_bb.is_inside(&bounding_box.get_center()) {
            // vanilla widens the shared `chunkBB`; the copy only feeds the rest of this call (single-piece start)
            let mut chunk_bb = *chunk_bb;
            chunk_bb.encapsulate(&bounding_box);
            self.data.post_process(level, random, &chunk_bb, reference_pos, &mut |_, _, _, _, _| {});
            self.spread_netherrack(random, level);
            self.add_netherrack_drip_columns_below_portal(random, level);
            if self.properties.vines || self.properties.overgrown {
                for pos in BlockPos::between_closed_box(&self.data.base.bounding_box) {
                    if self.properties.vines {
                        self.maybe_add_vines(random, level, pos);
                    }
                    if self.properties.overgrown {
                        self.maybe_add_leaves_above(random, level, pos);
                    }
                }
            }
        }
    }
}
