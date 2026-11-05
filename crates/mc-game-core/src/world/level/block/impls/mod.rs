//! Concrete block classes (`net.minecraft.world.level.block.*Block`), one file per Java class, each registering itself
//! with [`register_block_class!`](crate::register_block_class) / [`register_block!`](crate::register_block).
//! They hold the logic relevant to world generation and structure placement (`can_survive`, `update_shape`,
//! `rotate`/`mirror`, post-processing); gameplay methods are added to the same files by the gameplay block slice.

/// Defines `BlockBehaviour::update_shape` from a closure-like header:
/// `update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { .. })` (`level: &mut dyn LevelAccessor`).
#[macro_export]
macro_rules! update_shape {
    (|$this:ident, $state:ident, $level:ident, $pos:ident, $direction:ident, $neighbour_pos:ident, $neighbour_state:ident| $body:block) => {
        #[allow(unused_variables)]
        fn update_shape(
            & $this,
            $state: BlockState,
            $level: &mut dyn LevelAccessor,
            $pos: BlockPos,
            $direction: Direction,
            $neighbour_pos: BlockPos,
            $neighbour_state: BlockState,
        ) -> BlockState $body
    };
}
pub use update_shape;

#[allow(unused_imports)]
pub mod prelude {
    pub use super::update_shape;
    pub use crate::register_block;
    pub use crate::register_block_class;
    pub use crate::world::level::block::helpers::{self, air};
    pub use crate::world::level::block::{BlockBehaviour, BlockStateExt};
    pub use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
    pub use mc_core::direction::{Axis, Direction};
    pub use mc_core::mirror::Mirror;
    pub use mc_core::pos::BlockPos;
    pub use mc_core::rotation::Rotation;
    pub use mc_data::block::{Block, BlockState, SupportType};
    pub use crate::world::level::material::FluidStateExt;
    pub use mc_data::props;
}

// MODS
pub mod abstract_bed_block;
pub mod abstract_candle_block;
pub mod anvil_block;
pub mod base_fire_block;
pub mod base_rail_block;
pub mod bell_block;
pub mod cake_block;
pub mod campfire_block;
pub mod candle_block;
pub mod candle_cake_block;
pub mod cave_vines;
pub mod chest_block;
pub mod composter_block;
pub mod chorus_flower_block;
pub mod chorus_plant_block;
pub mod copper_chest_block;
pub mod crop_block;
pub mod diode_block;
pub mod door_block;
pub mod double_plant_block;
pub mod face_attached_horizontal_directional_block;
pub mod falling_block;
pub mod fire_block;
pub mod glow_lichen_block;
pub mod infested_block;
pub mod lightning_rod_block;
pub mod mossy_carpet_block;
pub mod multiface_block;
pub mod multiface_spreadeable_block;
pub mod multiface_spreader;
pub mod redstone_wire_block;
pub mod rotated_pillar_block;
pub mod sculk_block;
pub mod sculk_sensor_block;
pub mod sculk_vein_block;
pub mod sign_block;
pub mod soul_fire_block;
pub mod turtle_egg_block;
pub mod tnt_block;
pub mod vegetation_block;
pub mod vine_block;
pub mod wall_hanging_sign_block;
