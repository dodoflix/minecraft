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
pub mod barrier_block;
pub mod base_coral_plant_type_block;
pub mod base_coral_wall_fan_block;
pub mod base_fire_block;
pub mod base_pressure_plate_block;
pub mod base_rail_block;
pub mod base_torch_block;
pub mod beehive_block;
pub mod bell_block;
pub mod big_dripleaf_block;
pub mod big_dripleaf_stem_block;
pub mod brushable_block;
pub mod bubble_column_block;
pub mod button_block;
pub mod bush_block;
pub mod cactus_block;
pub mod cactus_flower_block;
pub mod cake_block;
pub mod calibrated_sculk_sensor_block;
pub mod campfire_block;
pub mod candle_block;
pub mod candle_cake_block;
pub mod carpet_block;
pub mod cave_vines_block;
pub mod cave_vines_plant_block;
pub mod ceiling_hanging_sign_block;
pub mod chain_block;
pub mod chest_block;
pub mod chiseled_book_shelf_block;
pub mod chorus_flower_block;
pub mod chorus_plant_block;
pub mod cocoa_block;
pub mod command_block;
pub mod comparator_block;
pub mod concrete_powder_block;
pub mod conduit_block;
pub mod copper_bulb_block;
pub mod copper_chest_block;
pub mod copper_golem_statue_block;
pub mod coral_block;
pub mod coral_fan_block;
pub mod coral_plant_block;
pub mod coral_wall_fan_block;
pub mod crafter_block;
pub mod creaking_heart_block;
pub mod crop_block;
pub mod cross_collision_block;
pub mod daylight_detector_block;
pub mod decorated_pot_block;
pub mod detector_rail_block;
pub mod diode_block;
pub mod dispenser_block;
pub mod door_block;
pub mod dropper_block;
pub mod double_plant_block;
pub mod dragon_egg_block;
pub mod dried_ghast_block;
pub mod drop_experience_block;
pub mod dry_vegetation_block;
pub mod end_portal_frame_block;
pub mod ender_chest_block;
pub mod eyeblossom_block;
pub mod face_attached_horizontal_directional_block;
pub mod falling_block;
pub mod farmland_block;
pub mod fence_block;
pub mod fence_gate_block;
pub mod fire_block;
pub mod firefly_bush_block;
pub mod flower_bed_block;
pub mod flower_block;
pub mod flower_pot_block;
pub mod frogspawn_block;
pub mod glow_lichen_block;
pub mod grass_block;
pub mod grindstone_block;
pub mod growing_plant_block;
pub mod growing_plant_body_block;
pub mod growing_plant_head_block;
pub mod hanging_moss_block;
pub mod hanging_roots_block;
pub mod hay_block;
pub mod heavy_core_block;
pub mod honey_block;
pub mod hopper_block;
pub mod horizontal_directional_block;
pub mod huge_mushroom_block;
pub mod ice_block;
pub mod infested_block;
pub mod infested_rotated_pillar_block;
pub mod iron_bars_block;
pub mod jigsaw_block;
pub mod kelp_block;
pub mod kelp_plant_block;
pub mod ladder_block;
pub mod lantern_block;
pub mod lava_cauldron_block;
pub mod leaf_litter_block;
pub mod leaves_block;
pub mod lectern_block;
pub mod lever_block;
pub mod light_block;
pub mod lightning_rod_block;
pub mod lily_pad_block;
pub mod liquid_block;
pub mod magma_block;
pub mod mangrove_propagule_block;
pub mod mangrove_roots_block;
pub mod mossy_carpet_block;
pub mod moving_piston_block;
pub mod multiface_block;
pub mod multiface_spreadeable_block;
pub mod multiface_spreader;
pub mod mushroom_block;
pub mod nether_fungus_block;
pub mod nether_portal_block;
pub mod nether_roots_block;
pub mod nether_sprouts_block;
pub mod nether_wart_block;
pub mod netherrack_block;
pub mod note_block;
pub mod nylium_block;
pub mod observer_block;
pub mod path_block;
pub mod piston_base_block;
pub mod piston_head_block;
pub mod pitcher_crop_block;
pub mod potent_sulfur_block;
pub mod powder_snow_block;
pub mod powered_rail_block;
pub mod rail_block;
pub mod powered_block;
pub mod red_stone_ore_block;
pub mod redstone_lamp_block;
pub mod redstone_torch_block;
pub mod redstone_wall_torch_block;
pub mod redstone_wire_block;
pub mod repeater_block;
pub mod rod_block;
pub mod rooted_dirt_block;
pub mod rotated_pillar_block;
pub mod sapling_block;
pub mod scaffolding_block;
pub mod sculk_block;
pub mod sculk_sensor_block;
pub mod sculk_shrieker_block;
pub mod sculk_vein_block;
pub mod sea_pickle_block;
pub mod seagrass_block;
pub mod shelf_block;
pub mod shelf_mushroom_block;
pub mod short_dry_grass_block;
pub mod shulker_box_block;
pub mod sign_block;
pub mod skull_block;
pub mod slab_block;
pub mod slime_block;
pub mod small_dripleaf_block;
pub mod sniffer_egg_block;
pub mod snow_layer_block;
pub mod snowy_block;
pub mod soul_fire_block;
pub mod speleothem_block;
pub mod sponge_block;
pub mod spore_blossom_block;
pub mod spreading_snowy_block;
pub mod stair_block;
pub mod standing_sign_block;
pub mod stem_block;
pub mod stonecutter_block;
pub mod structure_block;
pub mod sugar_cane_block;
pub mod sweet_berry_bush_block;
pub mod tall_dry_grass_block;
pub mod tall_flower_block;
pub mod tall_grass_block;
pub mod tall_seagrass_block;
pub mod target_block;
pub mod trap_door_block;
pub mod trip_wire_block;
pub mod trip_wire_hook_block;
pub mod turtle_egg_block;
pub mod twisting_vines_block;
pub mod twisting_vines_plant_block;
pub mod vault_block;
pub mod vegetation_block;
pub mod vine_block;
pub mod wall_banner_block;
pub mod wall_block;
pub mod wall_hanging_sign_block;
pub mod wall_sign_block;
pub mod wall_skull_block;
pub mod wall_torch_block;
pub mod waterlogged_transparent_block;
pub mod web_block;
pub mod weeping_vines_block;
pub mod weeping_vines_plant_block;
pub mod wet_sponge_block;
pub mod wither_rose_block;
