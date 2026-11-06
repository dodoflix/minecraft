//! `CuboidPlacement` (placement modifier `cuboid`): every position of a random-sized box starting at the origin.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_bool};
use mc_game_core::world::level::levelgen::placement::{
    PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier,
};
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct CuboidPlacement {
    pub xz_size: IntProvider,
    pub y_size: IntProvider,
    pub include_edges: bool,
    pub include_interior: bool,
}

impl PlacementModifier for CuboidPlacement {
    fn modify(
        &self,
        _context: &PlacementContext<'_>,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        output: &mut Vec<BlockPos>,
    ) {
        let height = self.y_size.sample(random);
        let width = self.xz_size.sample(random);
        let length = self.xz_size.sample(random);

        for x in 0..=width {
            for y in 0..=height {
                for z in 0..=length {
                    let (ex, ey, ez) = (
                        x == 0 || x == width,
                        y == 0 || y == height,
                        z == 0 || z == length,
                    );
                    if (self.include_edges || !ex || !ey)
                        && (self.include_edges || !ez || !ey)
                        && (self.include_edges || !ex || !ez)
                        && (self.include_interior || ex || ey || ez)
                    {
                        output.push(BlockPos::new(
                            x + origin.x(),
                            y + origin.y(),
                            z + origin.z(),
                        ));
                    }
                }
            }
        }
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static CUBOID: TypeEntry<dyn PlacementModifier> = TypeEntry {
    name: "cuboid",
    decode: |fields, _| {
        Ok(Arc::new(CuboidPlacement {
            xz_size: fields.required("xz_size", |v| IntProvider::from_json_range(v, 1, 16))?,
            y_size: fields.required("y_size", |v| IntProvider::from_json_range(v, 1, 16))?,
            include_edges: fields
                .optional("include_edges", parse_bool)?
                .unwrap_or(true),
            include_interior: fields
                .optional("include_interior", parse_bool)?
                .unwrap_or(true),
        }))
    },
};
