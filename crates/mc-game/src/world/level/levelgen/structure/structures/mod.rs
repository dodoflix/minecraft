//! The concrete structures (`net.minecraft.world.level.levelgen.structure.structures`): one file per vanilla class
//! (`NetherFortressStructure` -> `nether_fortress_structure.rs`, ...). Each structure registers its type in
//! `STRUCTURE_TYPES` and its piece types in `STRUCTURE_PIECE_TYPES` from its own file; the area owners add their
//! `pub mod` lines here.

pub mod desert_pyramid_piece;
pub mod desert_pyramid_structure;
pub mod jungle_temple_piece;
pub mod jungle_temple_structure;
