// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-blocks`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::block::impls::*;

pub mod abstract_banner_block;

pub mod abstract_cauldron_block;

pub mod abstract_furnace_block;

pub mod abstract_skull_block;

pub mod amethyst_block;

pub mod amethyst_cluster_block;

pub mod budding_amethyst_block;

pub mod attached_stem_block;

pub mod azalea_block;

pub mod bamboo_sapling_block;

pub mod bamboo_stalk_block;

pub mod beacon_block;

pub mod banner_block;

pub mod barrel_block;

pub mod barrier_block;

pub mod base_coral_plant_type_block;

pub mod base_coral_wall_fan_block;

pub mod base_pressure_plate_block;

pub mod base_torch_block;

pub mod beehive_block;

pub mod bonemealable_feature_placer_block;

pub mod big_dripleaf_block;

pub mod big_dripleaf_stem_block;

pub mod brushable_block;

pub mod bubble_column_block;

pub mod button_block;

pub mod bush_block;

pub mod cactus_block;

pub mod cactus_flower_block;

pub mod cauldron_block;

pub mod cauldron_interactions;

pub mod calibrated_sculk_sensor_block;

pub mod carpet_block;

pub mod cave_vines_block;

pub mod cave_vines_plant_block;

pub mod ceiling_hanging_sign_block;

pub mod chain_block;

pub mod chiseled_book_shelf_block;

pub mod jukebox_block;

pub mod selectable_slot_container;

pub mod cocoa_block;

pub mod command_block;

pub mod comparator_block;

pub mod concrete_powder_block;

pub mod conduit_block;

pub mod copper_bulb_block;

pub mod copper_golem_statue_block;

pub mod coral_block;

pub mod coral_fan_block;

pub mod coral_plant_block;

pub mod coral_wall_fan_block;

pub mod crafter_block;

pub mod cross_collision_block;

pub mod daylight_detector_block;

pub mod decorated_pot_block;

pub mod detector_rail_block;

pub mod dispenser_block;

pub mod dropper_block;

pub mod dragon_egg_block;

pub mod dried_ghast_block;

pub mod drop_experience_block;

pub mod dry_vegetation_block;

pub mod end_gateway_block;

pub mod end_portal_block;

pub mod end_portal_frame_block;

pub mod end_rod_block;

pub mod ender_chest_block;

pub mod eyeblossom_block;

pub mod farmland_block;

pub mod fence_block;

pub mod fence_gate_block;

pub mod firefly_bush_block;

pub mod flower_bed_block;

pub mod flower_block;

pub mod flower_pot_block;

pub mod frogspawn_block;

pub mod glazed_terracotta_block;

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

pub mod brewing_stand_block;

pub mod hopper_block;

pub mod horizontal_directional_block;

pub mod huge_mushroom_block;

pub mod ice_block;

pub mod infested_rotated_pillar_block;

pub mod iron_bars_block;

pub mod jigsaw_block;

pub mod kelp_block;

pub mod kelp_plant_block;

pub mod ladder_block;

pub mod lantern_block;

pub mod lava_cauldron_block;

pub mod layered_cauldron_block;

pub mod leaf_litter_block;

pub mod leaves_block;

pub mod lectern_block;

pub mod lever_block;

pub mod light_block;

pub mod lily_pad_block;

pub mod liquid_block;

pub mod magma_block;

pub mod mangrove_propagule_block;

pub mod mangrove_roots_block;

pub mod moving_piston_block;

pub mod mushroom_block;

pub mod nether_fungus_block;

pub mod nether_portal_block;

pub mod nether_roots_block;

pub mod nether_sprouts_block;

pub mod nether_vines;

pub mod nether_wart_block;

pub mod netherrack_block;

pub mod note_block;

pub mod nylium_block;

pub mod observer_block;

pub mod path_block;

pub mod piston_base_block;

pub mod piston_head_block;

pub mod pitcher_crop_block;

pub mod pointed_dripstone_block;

pub mod potent_sulfur_block;

pub mod powder_snow_block;

pub mod powered_rail_block;

pub mod rail_block;

pub mod powered_block;

pub mod red_stone_ore_block;

pub mod redstone_lamp_block;

pub mod redstone_torch_block;

pub mod redstone_wall_torch_block;

pub mod repeater_block;

pub mod respawn_anchor_block;

pub mod rod_block;

pub mod rooted_dirt_block;

pub mod sapling_block;

pub mod scaffolding_block;

pub mod sculk_catalyst_block;

pub mod sculk_shrieker_block;

pub mod segmentable_block;

pub mod sea_pickle_block;

pub mod seagrass_block;

pub mod shelf_block;

pub mod shelf_mushroom_block;

pub mod short_dry_grass_block;

pub mod shulker_box_block;

pub mod skull_block;

pub mod slab_block;

pub mod slime_block;

pub mod small_dripleaf_block;

pub mod sniffer_egg_block;

pub mod snow_layer_block;

pub mod snowy_block;

pub mod spawner_block;

pub mod speleothem_block;

pub mod sponge_block;

pub mod sulfur_spike_block;

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

pub mod test_block;

pub mod test_instance_block;

pub mod trap_door_block;

pub mod trip_wire_block;

pub mod trip_wire_hook_block;

pub mod twisting_vines_block;

pub mod twisting_vines_plant_block;

pub mod vault_block;

pub mod wall_banner_block;

pub mod wall_block;

pub mod wall_sign_block;

pub mod wall_skull_block;

pub mod wall_torch_block;

pub mod waterlogged_transparent_block;

pub mod web_block;

pub mod weeping_vines_block;

pub mod weeping_vines_plant_block;

pub mod wet_sponge_block;

pub mod wither_rose_block;

pub mod wither_skull_block;
