//! `net.minecraft.advancements.predicates.LocationPredicate`.

use super::block_predicate::BlockPredicateLevelExt;
use super::fluid_predicate::FluidPredicate;
use super::light_predicate::LightPredicate;
use crate::server::level::ServerLevel;
use crate::world::level::LevelReader;
use crate::world::level::block::impls::campfire_block::CampfireBlock;
use crate::world::level::levelgen::structure::manager::StructureDataSource;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{BOOL, Codec};
use mc_protocol::components::{
    BlockPredicate, Doubles, Holder, HolderSet, NoDirect, data_driven_set_codec,
    holder_set_contains,
};
use std::sync::LazyLock;

/// `LocationPredicate.PositionPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionPredicate {
    pub x: Doubles,
    pub y: Doubles,
    pub z: Doubles,
}

impl PositionPredicate {
    pub fn codec() -> Codec<PositionPredicate> {
        static C: LazyLock<Codec<PositionPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(PositionPredicate {
                x: Doubles::codec().optional_field_of_default("x", Doubles::any()),
                y: Doubles::codec().optional_field_of_default("y", Doubles::any()),
                z: Doubles::codec().optional_field_of_default("z", Doubles::any()),
            })
            .codec()
        });
        C.clone()
    }

    /// `PositionPredicate.of(x, y, z)`: empty when all three are `ANY`.
    pub fn of(x: Doubles, y: Doubles, z: Doubles) -> Option<PositionPredicate> {
        if x.is_any() && y.is_any() && z.is_any() {
            None
        } else {
            Some(PositionPredicate { x, y, z })
        }
    }

    pub fn matches(&self, x: f64, y: f64, z: f64) -> bool {
        self.x.matches(x) && self.y.matches(y) && self.z.matches(z)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocationPredicate {
    pub position: Option<PositionPredicate>,
    pub biomes: Option<HolderSet<NoDirect>>,
    pub structures: Option<HolderSet<NoDirect>>,
    pub dimension: Option<Identifier>,
    pub smokey: Option<bool>,
    pub light: Option<LightPredicate>,
    pub block: Option<BlockPredicate>,
    pub fluid: Option<FluidPredicate>,
    pub can_see_sky: Option<bool>,
}

/// The loaded chunks of a level as `StructureDataSource` (`getChunk(x, z, STRUCTURE_REFERENCES, false)`).
struct LoadedChunks<'a>(&'a ServerLevel);

impl StructureDataSource for LoadedChunks<'_> {
    fn structure_data(&self, pos: ChunkPos) -> Option<&mc_world::chunk::StructureData> {
        self.0
            .chunk_now(pos.x, pos.z)
            .map(|chunk| &chunk.data.access.structures)
    }
}

/// `level.structureManager().getStructureWithPieceAt(pos, matcher).isValid()` over the loaded chunks of `level`.
pub fn has_structure_piece_at(
    level: &ServerLevel,
    pos: BlockPos,
    matcher: &dyn Fn(&Identifier) -> bool,
) -> bool {
    level.is_loaded(pos)
        && level
            .chunk_source
            .chunk_map
            .gen_context
            .hooks
            .has_structure_piece_at(&LoadedChunks(level), pos, matcher)
}

impl LocationPredicate {
    pub fn codec() -> Codec<LocationPredicate> {
        static C: LazyLock<Codec<LocationPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(LocationPredicate {
                position: PositionPredicate::codec().optional_field_of("position"),
                biomes: data_driven_set_codec("minecraft:worldgen/biome")
                    .optional_field_of("biomes"),
                structures: data_driven_set_codec("minecraft:worldgen/structure")
                    .optional_field_of("structures"),
                dimension: IDENTIFIER.optional_field_of("dimension"),
                smokey: BOOL.optional_field_of("smokey"),
                light: LightPredicate::codec().optional_field_of("light"),
                block: BlockPredicate::codec().optional_field_of("block"),
                fluid: FluidPredicate::codec().optional_field_of("fluid"),
                can_see_sky: BOOL.optional_field_of("can_see_sky"),
            })
            .codec()
        });
        C.clone()
    }

    pub fn matches(&self, level: &ServerLevel, x: f64, y: f64, z: f64) -> bool {
        if self.position.as_ref().is_some_and(|p| !p.matches(x, y, z)) {
            return false;
        }
        if self
            .dimension
            .as_ref()
            .is_some_and(|d| *d != level.dimension)
        {
            return false;
        }
        let pos = BlockPos::containing(x, y, z);
        let loaded = level.is_loaded(pos);
        if let Some(biomes) = &self.biomes {
            let in_set = loaded
                && level
                    .get_biome_id(pos)
                    .zip(level.tick_state.biomes.as_ref())
                    .is_some_and(|(id, registry)| {
                        holder_set_contains(
                            biomes,
                            "worldgen/biome",
                            &Holder::<NoDirect>::Reference(registry.key(id).clone()),
                        )
                    });
            if !in_set {
                return false;
            }
        }
        if let Some(structures) = &self.structures {
            let matcher = |id: &Identifier| {
                holder_set_contains(
                    structures,
                    "worldgen/structure",
                    &Holder::<NoDirect>::Reference(id.clone()),
                )
            };
            if !(loaded
                && level
                    .chunk_source
                    .chunk_map
                    .gen_context
                    .hooks
                    .has_structure_piece_at(&LoadedChunks(level), pos, &matcher))
            {
                return false;
            }
        }
        if let Some(smokey) = self.smokey {
            if !(loaded && smokey == CampfireBlock::is_smokey_pos(level, pos)) {
                return false;
            }
        }
        if self.light.as_ref().is_some_and(|l| !l.matches(level, pos)) {
            return false;
        }
        if self.block.as_ref().is_some_and(|b| !b.matches(level, pos)) {
            return false;
        }
        if self.fluid.as_ref().is_some_and(|f| !f.matches(level, pos)) {
            return false;
        }
        self.can_see_sky.is_none_or(|c| c == level.can_see_sky(pos))
    }
}
