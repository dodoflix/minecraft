//! The concrete structures (`net.minecraft.world.level.levelgen.structure.structures`): one file per vanilla class
//! (`NetherFortressStructure` -> `nether_fortress_structure.rs`, ...). Each structure registers its type in
//! `STRUCTURE_TYPES` and its piece types in `STRUCTURE_PIECE_TYPES` from its own file; the area owners add their
//! `pub mod` lines here.

pub mod desert_pyramid_piece;
pub mod desert_pyramid_structure;
pub mod jungle_temple_piece;
pub mod jungle_temple_structure;
pub mod buried_treasure_pieces;
pub mod buried_treasure_structure;
pub mod igloo_pieces;
pub mod igloo_structure;
pub mod nether_fossil_pieces;
pub mod nether_fossil_structure;
pub mod ocean_ruin_pieces;
pub mod ocean_ruin_structure;
pub mod ruined_portal_piece;
pub mod ruined_portal_structure;
pub mod shipwreck_pieces;
pub mod shipwreck_structure;
pub mod nether_fortress_pieces;
pub mod nether_fortress_structure;
pub mod mineshaft_pieces;
pub mod mineshaft_structure;
pub mod stronghold_pieces;
pub mod stronghold_structure;
pub mod end_city_pieces;
pub mod end_city_structure;
pub mod ocean_monument_pieces;
pub mod ocean_monument_structure;
pub mod structure_entities;
pub mod woodland_mansion_pieces;
pub mod woodland_mansion_structure;
pub mod jigsaw_structure;
pub mod spawned_mob_tag;
pub mod swamp_hut_piece;
pub mod swamp_hut_structure;
