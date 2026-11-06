//! `EndCityPieces`: the end city (house towers, towers, bridges, fat towers, the ship) as a tree of template pieces, and the
//! data markers (`Chest` loot, `Sentry` shulkers, `Elytra` item frames) of `EndCityPiece`.
//!
//! Vanilla's `SectionGenerator`s are the four [`Section`]s here. Piece lists are `Vec<EndCityPiece>`; vanilla's references to
//! pieces of a list (`parent`, `lastPiece`) are indices into it, and a generator's `parent` lives in the caller's list while its
//! children go into a fresh one ([`recursive_children`]).

use super::structure_entities;
use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, read_rotation, rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::{
    STRUCTURE_AND_AIR, STRUCTURE_BLOCK,
};
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::is_in_spawnable_bounds;
use mc_game_core::world::level::levelgen::block_entity_data::set_block_entity_loot_table;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext,
    StructurePieceType, find_collision_piece,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::{
    StructurePlaceSettings, StructureTemplateManager,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;

const MAX_GEN_DEPTH: i32 = 8;
const END_CITY_TREASURE: &str = "minecraft:chests/end_city_treasure";

const TOWER_BRIDGES: [(Rotation, BlockPos); 4] = [
    (Rotation::None, BlockPos::new(1, -1, 0)),
    (Rotation::Clockwise90, BlockPos::new(6, -1, 1)),
    (Rotation::Counterclockwise90, BlockPos::new(0, -1, 5)),
    (Rotation::Clockwise180, BlockPos::new(5, -1, 6)),
];

const FAT_TOWER_BRIDGES: [(Rotation, BlockPos); 4] = [
    (Rotation::None, BlockPos::new(4, -1, 0)),
    (Rotation::Clockwise90, BlockPos::new(12, -1, 4)),
    (Rotation::Counterclockwise90, BlockPos::new(0, -1, 8)),
    (Rotation::Clockwise180, BlockPos::new(8, -1, 12)),
];

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static END_CITY_PIECE: StructurePieceType = StructurePieceType {
    id: "ecp",
    load: load_piece,
};

fn load_piece(
    context: &StructurePieceSerializationContext<'_>,
    tag: &CompoundTag,
) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(EndCityPiece::load(
        &context.env.structure_template_manager,
        tag,
    )?))
}

/// `EndCityPieces.EndCityPiece`.
pub struct EndCityPiece {
    data: TemplateStructurePieceData,
    /// `placeSettings.getProcessors().get(0) == BlockIgnoreProcessor.STRUCTURE_BLOCK`.
    overwrite: bool,
}

impl EndCityPiece {
    /// `EndCityPiece(manager, templateName, position, rotation, overwrite)`.
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_name: &str,
        position: BlockPos,
        rotation: Rotation,
        overwrite: bool,
    ) -> EndCityPiece {
        let data = TemplateStructurePieceData::new(
            &END_CITY_PIECE,
            0,
            structure_template_manager,
            &make_identifier(template_name),
            template_name.to_owned(),
            make_settings(overwrite, rotation),
            position,
        );
        EndCityPiece { data, overwrite }
    }

    /// `EndCityPiece(manager, tag)`.
    pub fn load(
        structure_template_manager: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<EndCityPiece, String> {
        let overwrite = tag.get_boolean_or("OW", false);
        let data = TemplateStructurePieceData::load_with_location(
            &END_CITY_PIECE,
            tag,
            structure_template_manager,
            |name| Ok(make_identifier(name)),
            |_| Ok(make_settings(overwrite, read_rotation(tag, "Rot")?)),
        )?;
        Ok(EndCityPiece { data, overwrite })
    }

    fn rotation(&self) -> Rotation {
        self.data.place_settings.get_rotation()
    }
}

/// `makeSettings(overwrite, rotation)`.
fn make_settings(overwrite: bool, rotation: Rotation) -> StructurePlaceSettings {
    let processor = if overwrite {
        STRUCTURE_BLOCK.clone()
    } else {
        STRUCTURE_AND_AIR.clone()
    };
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_ignore_entities(true)
        .add_processor(processor)
        .set_rotation(rotation);
    settings
}

/// `makeIdentifier(templateName)`.
fn make_identifier(template_name: &str) -> Identifier {
    Identifier::with_default_namespace(&format!("end_city/{template_name}"))
        .expect("valid end city template id")
}

impl StructurePiece for EndCityPiece {
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
        tag.put_string("Rot", rotation_legacy_name(self.rotation()));
        tag.put_boolean("OW", self.overwrite);
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
        let rotation = self.rotation();
        self.data.post_process(
            level,
            random,
            chunk_bb,
            reference_pos,
            &mut |marker_id, position, level, random, chunk_bb| {
                handle_data_marker(rotation, marker_id, position, level, random, chunk_bb)
            },
        );
    }
}

/// `EndCityPiece.handleDataMarker(markerId, position, level, random, chunkBB)`.
fn handle_data_marker(
    rotation: Rotation,
    marker_id: &str,
    position: BlockPos,
    level: &mut dyn WorldGenLevel,
    random: &mut dyn RandomSource,
    chunk_bb: &BoundingBox,
) {
    if marker_id.starts_with("Chest") {
        let chest_position = position.below();
        if chunk_bb.is_inside(&chest_position) {
            set_block_entity_loot_table(level, random, chest_position, END_CITY_TREASURE);
        }
    } else if chunk_bb.is_inside(&position) && is_in_spawnable_bounds(position) {
        if marker_id.starts_with("Sentry") {
            level.add_fresh_entity_nbt(structure_entities::shulker(position));
        } else if marker_id.starts_with("Elytra") {
            level.add_fresh_entity_nbt(structure_entities::elytra_item_frame(
                position,
                rotation.rotate(Direction::South),
            ));
        }
    }
}

/// The four `SectionGenerator`s (`HOUSE_TOWER_GENERATOR`, `TOWER_GENERATOR`, `TOWER_BRIDGE_GENERATOR`, `FAT_TOWER_GENERATOR`).
#[derive(Clone, Copy)]
enum Section {
    HouseTower,
    Tower,
    TowerBridge,
    FatTower,
}

/// The only mutable state of the generators: `TOWER_BRIDGE_GENERATOR.shipCreated` (reset by `init()`).
struct Generators {
    ship_created: bool,
}

/// `addPiece(manager, parent, offset, templateName, rotation, overwrite)`.
fn add_piece(
    manager: &StructureTemplateManager,
    parent: &EndCityPiece,
    offset: BlockPos,
    template_name: &str,
    rotation: Rotation,
    overwrite: bool,
) -> EndCityPiece {
    let mut child = EndCityPiece::new(
        manager,
        template_name,
        parent.data.template_position,
        rotation,
        overwrite,
    );
    let origin = parent.data.template.calculate_connected_position(
        &parent.data.place_settings,
        offset,
        &child.data.place_settings,
        BlockPos::ZERO,
    );
    child.move_by(origin.x(), origin.y(), origin.z());
    child
}

/// `addHelper(pieces, piece)`: returns the index of the added piece.
fn add_helper(pieces: &mut Vec<EndCityPiece>, piece: EndCityPiece) -> usize {
    pieces.push(piece);
    pieces.len() - 1
}

/// `addHelper(pieces, addPiece(manager, pieces[parent], ..))` for a parent in the same list.
fn add_child(
    manager: &StructureTemplateManager,
    pieces: &mut Vec<EndCityPiece>,
    parent: usize,
    offset: BlockPos,
    template_name: &str,
    rotation: Rotation,
    overwrite: bool,
) -> usize {
    let child = add_piece(
        manager,
        &pieces[parent],
        offset,
        template_name,
        rotation,
        overwrite,
    );
    add_helper(pieces, child)
}

/// `startHouseTower(manager, origin, rotation, pieces, random)`.
pub fn start_house_tower(
    manager: &StructureTemplateManager,
    origin: BlockPos,
    rotation: Rotation,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) {
    let mut generators = Generators {
        ship_created: false,
    };
    let mut last = add_helper(
        pieces,
        EndCityPiece::new(manager, "base_floor", origin, rotation, true),
    );
    last = add_child(
        manager,
        pieces,
        last,
        BlockPos::new(-1, 0, -1),
        "second_floor_1",
        rotation,
        false,
    );
    last = add_child(
        manager,
        pieces,
        last,
        BlockPos::new(-1, 4, -1),
        "third_floor_1",
        rotation,
        false,
    );
    last = add_child(
        manager,
        pieces,
        last,
        BlockPos::new(-1, 8, -1),
        "third_roof",
        rotation,
        true,
    );
    recursive_children(
        manager,
        &mut generators,
        Section::Tower,
        1,
        last,
        None,
        pieces,
        random,
    );
}

/// `recursiveChildren(manager, generator, genDepth, parent, offset, pieces, random)`: `parent` is an index into `pieces`.
#[allow(clippy::too_many_arguments)]
fn recursive_children(
    manager: &StructureTemplateManager,
    generators: &mut Generators,
    section: Section,
    gen_depth: i32,
    parent: usize,
    offset: Option<BlockPos>,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) -> bool {
    if gen_depth > MAX_GEN_DEPTH {
        return false;
    }
    let mut child_pieces = Vec::new();
    if generate(
        manager,
        generators,
        section,
        gen_depth,
        &pieces[parent],
        offset,
        &mut child_pieces,
        random,
    ) {
        let mut collision = false;
        let child_tag = random.next_int();
        for child in &mut child_pieces {
            child.data.base.gen_depth = child_tag;
            let collision_piece = find_collision_piece(
                pieces.iter().map(|p| p as &dyn StructurePiece),
                &child.get_bounding_box(),
            );
            if collision_piece
                .is_some_and(|c| c.get_gen_depth() != pieces[parent].data.base.gen_depth)
            {
                collision = true;
                break;
            }
        }
        if !collision {
            pieces.append(&mut child_pieces);
            return true;
        }
    }
    false
}

/// `SectionGenerator.generate(manager, genDepth, parent, offset, pieces, random)`.
#[allow(clippy::too_many_arguments)]
fn generate(
    manager: &StructureTemplateManager,
    generators: &mut Generators,
    section: Section,
    gen_depth: i32,
    parent: &EndCityPiece,
    offset: Option<BlockPos>,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) -> bool {
    match section {
        Section::HouseTower => generate_house_tower(
            manager,
            generators,
            gen_depth,
            parent,
            offset.expect("house tower offset"),
            pieces,
            random,
        ),
        Section::Tower => generate_tower(manager, generators, gen_depth, parent, pieces, random),
        Section::TowerBridge => {
            generate_tower_bridge(manager, generators, gen_depth, parent, pieces, random)
        }
        Section::FatTower => {
            generate_fat_tower(manager, generators, gen_depth, parent, pieces, random)
        }
    }
}

fn generate_house_tower(
    manager: &StructureTemplateManager,
    generators: &mut Generators,
    gen_depth: i32,
    parent: &EndCityPiece,
    offset: BlockPos,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) -> bool {
    if gen_depth > 8 {
        return false;
    }
    let rotation = parent.rotation();
    let first = add_piece(manager, parent, offset, "base_floor", rotation, true);
    let mut last = add_helper(pieces, first);
    let num_floors = random.next_int_bounded(3);
    if num_floors == 0 {
        add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 4, -1),
            "base_roof",
            rotation,
            true,
        );
    } else if num_floors == 1 {
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 0, -1),
            "second_floor_2",
            rotation,
            false,
        );
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 8, -1),
            "second_roof",
            rotation,
            false,
        );
        recursive_children(
            manager,
            generators,
            Section::Tower,
            gen_depth + 1,
            last,
            None,
            pieces,
            random,
        );
    } else if num_floors == 2 {
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 0, -1),
            "second_floor_2",
            rotation,
            false,
        );
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 4, -1),
            "third_floor_2",
            rotation,
            false,
        );
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 8, -1),
            "third_roof",
            rotation,
            true,
        );
        recursive_children(
            manager,
            generators,
            Section::Tower,
            gen_depth + 1,
            last,
            None,
            pieces,
            random,
        );
    }
    true
}

fn generate_tower(
    manager: &StructureTemplateManager,
    generators: &mut Generators,
    gen_depth: i32,
    parent: &EndCityPiece,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) -> bool {
    let rotation = parent.rotation();
    // Java evaluates the two `nextInt(2)` of the offset left to right
    let base_x = 3 + random.next_int_bounded(2);
    let base_z = 3 + random.next_int_bounded(2);
    let base = add_piece(
        manager,
        parent,
        BlockPos::new(base_x, -3, base_z),
        "tower_base",
        rotation,
        true,
    );
    let mut last = add_helper(pieces, base);
    last = add_child(
        manager,
        pieces,
        last,
        BlockPos::new(0, 7, 0),
        "tower_piece",
        rotation,
        true,
    );
    let mut bridge_piece = if random.next_int_bounded(3) == 0 {
        Some(last)
    } else {
        None
    };
    let tower_height = 1 + random.next_int_bounded(3);
    for i in 0..tower_height {
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(0, 4, 0),
            "tower_piece",
            rotation,
            true,
        );
        if i < tower_height - 1 && random.next_boolean() {
            bridge_piece = Some(last);
        }
    }
    if let Some(bridge_piece) = bridge_piece {
        for (bridge_rotation, bridge_offset) in TOWER_BRIDGES {
            if random.next_boolean() {
                let bridge_start = add_child(
                    manager,
                    pieces,
                    bridge_piece,
                    bridge_offset,
                    "bridge_end",
                    rotation.get_rotated(bridge_rotation),
                    true,
                );
                recursive_children(
                    manager,
                    generators,
                    Section::TowerBridge,
                    gen_depth + 1,
                    bridge_start,
                    None,
                    pieces,
                    random,
                );
            }
        }
        add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 4, -1),
            "tower_top",
            rotation,
            true,
        );
    } else {
        if gen_depth != 7 {
            return recursive_children(
                manager,
                generators,
                Section::FatTower,
                gen_depth + 1,
                last,
                None,
                pieces,
                random,
            );
        }
        add_child(
            manager,
            pieces,
            last,
            BlockPos::new(-1, 4, -1),
            "tower_top",
            rotation,
            true,
        );
    }
    true
}

fn generate_tower_bridge(
    manager: &StructureTemplateManager,
    generators: &mut Generators,
    gen_depth: i32,
    parent: &EndCityPiece,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) -> bool {
    let rotation = parent.rotation();
    let bridge_length = random.next_int_bounded(4) + 1;
    let first = add_piece(
        manager,
        parent,
        BlockPos::new(0, 0, -4),
        "bridge_piece",
        rotation,
        true,
    );
    let mut last = add_helper(pieces, first);
    pieces[last].data.base.gen_depth = -1;
    let mut next_y = 0;
    for _ in 0..bridge_length {
        if random.next_boolean() {
            last = add_child(
                manager,
                pieces,
                last,
                BlockPos::new(0, next_y, -4),
                "bridge_piece",
                rotation,
                true,
            );
            next_y = 0;
        } else {
            if random.next_boolean() {
                last = add_child(
                    manager,
                    pieces,
                    last,
                    BlockPos::new(0, next_y, -4),
                    "bridge_steep_stairs",
                    rotation,
                    true,
                );
            } else {
                last = add_child(
                    manager,
                    pieces,
                    last,
                    BlockPos::new(0, next_y, -8),
                    "bridge_gentle_stairs",
                    rotation,
                    true,
                );
            }
            next_y = 4;
        }
    }
    if !generators.ship_created && random.next_int_bounded(10 - gen_depth) == 0 {
        // Java evaluates the two offset components left to right
        let ship_x = -8 + random.next_int_bounded(8);
        let ship_z = -70 + random.next_int_bounded(10);
        add_child(
            manager,
            pieces,
            last,
            BlockPos::new(ship_x, next_y, ship_z),
            "ship",
            rotation,
            true,
        );
        generators.ship_created = true;
    } else if !recursive_children(
        manager,
        generators,
        Section::HouseTower,
        gen_depth + 1,
        last,
        Some(BlockPos::new(-3, next_y + 1, -11)),
        pieces,
        random,
    ) {
        return false;
    }
    last = add_child(
        manager,
        pieces,
        last,
        BlockPos::new(4, next_y, 0),
        "bridge_end",
        rotation.get_rotated(Rotation::Clockwise180),
        true,
    );
    pieces[last].data.base.gen_depth = -1;
    true
}

fn generate_fat_tower(
    manager: &StructureTemplateManager,
    generators: &mut Generators,
    gen_depth: i32,
    parent: &EndCityPiece,
    pieces: &mut Vec<EndCityPiece>,
    random: &mut dyn RandomSource,
) -> bool {
    let rotation = parent.rotation();
    let base = add_piece(
        manager,
        parent,
        BlockPos::new(-3, 4, -3),
        "fat_tower_base",
        rotation,
        true,
    );
    let mut last = add_helper(pieces, base);
    last = add_child(
        manager,
        pieces,
        last,
        BlockPos::new(0, 4, 0),
        "fat_tower_middle",
        rotation,
        true,
    );
    let mut i = 0;
    while i < 2 && random.next_int_bounded(3) != 0 {
        last = add_child(
            manager,
            pieces,
            last,
            BlockPos::new(0, 8, 0),
            "fat_tower_middle",
            rotation,
            true,
        );
        for (bridge_rotation, bridge_offset) in FAT_TOWER_BRIDGES {
            if random.next_boolean() {
                let bridge_start = add_child(
                    manager,
                    pieces,
                    last,
                    bridge_offset,
                    "bridge_end",
                    rotation.get_rotated(bridge_rotation),
                    true,
                );
                recursive_children(
                    manager,
                    generators,
                    Section::TowerBridge,
                    gen_depth + 1,
                    bridge_start,
                    None,
                    pieces,
                    random,
                );
            }
        }
        i += 1;
    }
    add_child(
        manager,
        pieces,
        last,
        BlockPos::new(-2, 8, -2),
        "fat_tower_top",
        rotation,
        true,
    );
    true
}
