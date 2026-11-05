//! Concrete block classes (`net.minecraft.world.level.block.*Block`), one file per Java class, each registering itself
//! with [`register_block_class!`](crate::register_block_class) / [`register_block!`](crate::register_block).
//! They hold the logic relevant to world generation and structure placement (`can_survive`, `update_shape`,
//! `rotate`/`mirror`, post-processing); gameplay methods are added to the same files by the gameplay block slice.

/// Defines `BlockBehaviour::update_shape` from a closure-like header:
/// `update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { .. })` (`level: &mut dyn LevelAccessor`).
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
pub(crate) use update_shape;

#[allow(unused_imports)]
pub(crate) mod prelude {
    pub(crate) use super::update_shape;
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
pub mod abstract_furnace_block;
pub mod amethyst_cluster_block;
pub mod anvil_block;
pub mod attached_stem_block;
pub mod azalea_block;
pub mod bamboo_sapling_block;
pub mod bamboo_stalk_block;
pub mod banner_block;
pub mod barrel_block;
pub mod base_coral_plant_type_block;
pub mod base_coral_wall_fan_block;
pub mod base_torch_block;
pub mod bubble_column_block;
pub mod cactus_block;
pub mod cactus_flower_block;
pub mod cake_block;
pub mod candle_block;
pub mod candle_cake_block;
pub mod carpet_block;
pub mod cave_vines_block;
pub mod cave_vines_plant_block;
pub mod ceiling_hanging_sign_block;
pub mod chain_block;
pub mod chest_block;
pub mod chiseled_book_shelf_block;
pub mod command_block;
pub mod copper_chest_block;
pub mod coral_block;
pub mod coral_fan_block;
pub mod coral_plant_block;
pub mod coral_wall_fan_block;
pub mod crop_block;
pub mod dispenser_block;
pub mod double_plant_block;
pub mod dry_vegetation_block;
pub mod end_portal_frame_block;
pub mod ender_chest_block;
pub mod face_attached_horizontal_directional_block;
pub mod frogspawn_block;
pub mod growing_plant_block;
pub mod growing_plant_body_block;
pub mod growing_plant_head_block;
pub mod hanging_roots_block;
pub mod hopper_block;
pub mod horizontal_directional_block;
pub mod infested_rotated_pillar_block;
pub mod kelp_block;
pub mod kelp_plant_block;
pub mod ladder_block;
pub mod lantern_block;
pub mod lectern_block;
pub mod lily_pad_block;
pub mod mangrove_propagule_block;
pub mod mangrove_roots_block;
pub mod mushroom_block;
pub mod nether_fungus_block;
pub mod nether_roots_block;
pub mod nether_sprouts_block;
pub mod nether_wart_block;
pub mod pitcher_crop_block;
pub mod redstone_wall_torch_block;
pub mod rotated_pillar_block;
pub mod sea_pickle_block;
pub mod seagrass_block;
pub mod shulker_box_block;
pub mod sign_block;
pub mod skull_block;
pub mod slab_block;
pub mod small_dripleaf_block;
pub mod spore_blossom_block;
pub mod stair_block;
pub mod standing_sign_block;
pub mod stem_block;
pub mod stonecutter_block;
pub mod sugar_cane_block;
pub mod tall_seagrass_block;
pub mod twisting_vines_block;
pub mod twisting_vines_plant_block;
pub mod vault_block;
pub mod vegetation_block;
pub mod wall_banner_block;
pub mod wall_hanging_sign_block;
pub mod wall_sign_block;
pub mod wall_skull_block;
pub mod wall_torch_block;
pub mod weeping_vines_block;
pub mod weeping_vines_plant_block;
pub mod wither_rose_block;
