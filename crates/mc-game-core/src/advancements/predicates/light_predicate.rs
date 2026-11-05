//! `net.minecraft.advancements.predicates.LightPredicate`.

use crate::server::level::ServerLevel;
use crate::world::level::LevelReader;
use mc_core::pos::BlockPos;
use mc_dfu::Codec;
use mc_protocol::components::Ints;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct LightPredicate {
    pub composite: Ints,
}

/// `LevelReader.getMaxLocalRawBrightness(pos)`.
pub fn get_max_local_raw_brightness(level: &ServerLevel, pos: BlockPos) -> i32 {
    if pos.x() >= -30_000_000 && pos.z() >= -30_000_000 && pos.x() < 30_000_000 && pos.z() < 30_000_000 {
        level.get_raw_brightness(pos, level.get_sky_darken())
    } else {
        15
    }
}

impl LightPredicate {
    pub fn codec() -> Codec<LightPredicate> {
        static C: LazyLock<Codec<LightPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(LightPredicate { composite: Ints::codec().optional_field_of_default("light", Ints::any()) }).codec()
        });
        C.clone()
    }

    pub fn matches(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        level.is_loaded(pos) && self.composite.matches(get_max_local_raw_brightness(level, pos))
    }
}
