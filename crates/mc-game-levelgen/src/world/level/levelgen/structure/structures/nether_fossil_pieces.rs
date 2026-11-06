//! `NetherFossilPieces`.

use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, read_rotation, rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::STRUCTURE_AND_AIR;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{self, RandomSource};
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext,
    StructurePieceType,
};
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use mc_game_core::world::level::levelgen::structure::templatesystem::{
    StructurePlaceSettings, StructureTemplateManager,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;

const FOSSILS: [&str; 14] = [
    "nether_fossils/fossil_1",
    "nether_fossils/fossil_2",
    "nether_fossils/fossil_3",
    "nether_fossils/fossil_4",
    "nether_fossils/fossil_5",
    "nether_fossils/fossil_6",
    "nether_fossils/fossil_7",
    "nether_fossils/fossil_8",
    "nether_fossils/fossil_9",
    "nether_fossils/fossil_10",
    "nether_fossils/fossil_11",
    "nether_fossils/fossil_12",
    "nether_fossils/fossil_13",
    "nether_fossils/fossil_14",
];

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static NETHER_FOSSIL: StructurePieceType = StructurePieceType {
    id: "nefos",
    load: load_piece,
};

fn load_piece(
    context: &StructurePieceSerializationContext<'_>,
    tag: &CompoundTag,
) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(NetherFossilPiece::load(
        &context.env.structure_template_manager,
        tag,
    )?))
}

/// `NetherFossilPieces.addPieces(manager, builder, random, position)`.
pub fn add_pieces(
    structure_template_manager: &StructureTemplateManager,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    position: BlockPos,
) {
    let next_rotation = Rotation::get_random(random);
    let location = Identifier::with_default_namespace(
        FOSSILS[random.next_int_bounded(FOSSILS.len() as i32) as usize],
    )
    .expect("valid fossil template id");
    builder.add_piece(Box::new(NetherFossilPiece::new(
        structure_template_manager,
        &location,
        position,
        next_rotation,
    )));
}

pub struct NetherFossilPiece {
    data: TemplateStructurePieceData,
}

/// `makeSettings(rotation)`.
fn make_settings(rotation: Rotation) -> StructurePlaceSettings {
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_rotation(rotation)
        .set_mirror(Mirror::None)
        .add_processor(STRUCTURE_AND_AIR.clone());
    settings
}

impl NetherFossilPiece {
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_location: &Identifier,
        position: BlockPos,
        rotation: Rotation,
    ) -> NetherFossilPiece {
        NetherFossilPiece {
            data: TemplateStructurePieceData::new(
                &NETHER_FOSSIL,
                0,
                structure_template_manager,
                template_location,
                template_location.to_string(),
                make_settings(rotation),
                position,
            ),
        }
    }

    pub fn load(
        structure_template_manager: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<NetherFossilPiece, String> {
        Ok(NetherFossilPiece {
            data: TemplateStructurePieceData::load(
                &NETHER_FOSSIL,
                tag,
                structure_template_manager,
                |_| Ok(make_settings(read_rotation(tag, "Rot")?)),
            )?,
        })
    }

    /// `placeDriedGhast(level, random, fossilBB, chunkBB)`.
    fn place_dried_ghast(
        &self,
        level: &mut dyn WorldGenLevel,
        fossil_bb: &BoundingBox,
        chunk_bb: &BoundingBox,
    ) {
        let mut positional_random = random::create_thread_local_instance(level.get_seed())
            .fork_positional()
            .at_pos(fossil_bb.get_center());
        if positional_random.next_float() < 0.5 {
            let x = fossil_bb.min_x() + positional_random.next_int_bounded(fossil_bb.get_x_span());
            let y = fossil_bb.min_y();
            let z = fossil_bb.min_z() + positional_random.next_int_bounded(fossil_bb.get_z_span());
            let random_pos = BlockPos::new(x, y, z);
            if level.get_block_state(random_pos).is_air() && chunk_bb.is_inside(&random_pos) {
                let state = Block::DRIED_GHAST
                    .default_state()
                    .rotate(Rotation::get_random(&mut *positional_random));
                level.set_block_flags(random_pos, state, 2);
            }
        }
    }
}

impl StructurePiece for NetherFossilPiece {
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
        let fossil_bb = self
            .data
            .template
            .get_bounding_box(&self.data.place_settings, self.data.template_position);
        // vanilla widens the shared `chunkBB`; the copy only feeds the rest of this call (single-piece start)
        let mut chunk_bb = *chunk_bb;
        chunk_bb.encapsulate(&fossil_bb);
        self.data.post_process(
            level,
            random,
            &chunk_bb,
            reference_pos,
            &mut |_, _, _, _, _| {},
        );
        self.place_dried_ghast(level, &fossil_bb, &chunk_bb);
    }
}
