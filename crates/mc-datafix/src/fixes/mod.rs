//! `net.minecraft.util.datafix.fixes.*`: one module per Java class (snake_case). The `References` constants and the
//! abstract base and family classes (`NamedEntityFix`, `ItemRenameFix`, ...) are ported; each group below receives the
//! `pub mod` lines of its classes, in alphabetical order, between its `begin`/`end` markers.

pub mod block_state_data;
pub mod references;
pub mod tables;

// begin: base and family classes
pub mod abstract_poi_section_fix;
pub mod abstract_uuid_fix;
pub mod add_field_fix;
pub mod add_flag_if_not_present_fix;
pub mod add_new_choices;
pub mod advancements_rename_fix;
pub mod attributes_rename_fix;
pub mod block_entity_rename_fix;
pub mod data_component_remainder_fix;
pub mod entity_rename_fix;
pub mod item_rename_fix;
pub mod item_stack_tag_fix;
pub mod item_stack_tag_remainder_fix;
pub mod legacy_abstract_block_property_fix;
pub mod legacy_block_rename_fix;
pub mod named_entity_convert_unchecked_fix;
pub mod named_entity_fix;
pub mod named_entity_write_read_fix;
pub mod namespaced_type_rename_fix;
pub mod simple_entity_rename_fix;
pub mod simplest_entity_rename_fix;
pub mod write_and_read_fix;
// end: base and family classes

// begin: versions 99..1343
pub mod bed_item_color_fix;
pub mod block_entity_id_fix;
pub mod block_entity_shulker_box_color_fix;
pub mod chunk_bed_block_entity_injecter_fix;
pub mod entity_armor_stand_silent_fix;
pub mod entity_elder_guardian_split_fix;
pub mod entity_equipment_to_armor_and_hand_fix;
pub mod entity_health_fix;
pub mod entity_horse_saddle_fix;
pub mod entity_horse_split_fix;
pub mod entity_id_fix;
pub mod entity_minecart_identifiers_fix;
pub mod entity_painting_item_frame_direction_fix;
pub mod entity_redundant_chance_tags_fix;
pub mod entity_riding_to_passengers_fix;
pub mod entity_shulker_color_fix;
pub mod entity_skeleton_split_fix;
pub mod entity_string_uuid_fix;
pub mod entity_tipped_arrow_fix;
pub mod entity_zombie_split_fix;
pub mod entity_zombie_villager_type_fix;
pub mod item_banner_color_fix;
pub mod item_id_fix;
pub mod item_potion_fix;
pub mod item_shulker_box_color_fix;
pub mod item_spawn_egg_fix;
pub mod item_water_potion_fix;
pub mod mob_spawner_entity_identifiers_fix;
pub mod options_force_vbo_fix;
pub mod options_lower_case_language_fix;
pub mod sign_text_strict_json_fix;
pub mod villager_set_can_pick_up_loot_fix;
pub mod written_book_pages_strict_json_fix;
// end: versions 99..1343

// begin: versions 1344..1459
// end: versions 1344..1459

// begin: versions 1460..2199
// end: versions 1460..2199

// begin: versions 2200..2699
pub mod attributes_rename_legacy;
pub mod biome_fix;
pub mod bit_storage_align_fix;
pub mod block_entity_uuid_fix;
pub mod cauldron_rename_fix;
pub mod chunk_biome_fix;
pub mod entity_projectile_owner_fix;
pub mod entity_shulker_rotation_fix;
pub mod entity_uuid_fix;
pub mod entity_zombified_piglin_rename_fix;
pub mod force_poi_rebuild;
pub mod furnace_recipe_fix;
pub mod gossip_uuid_fix;
pub mod item_stack_uuid_fix;
pub mod jigsaw_properties_fix;
pub mod legacy_dimension_id_fix;
pub mod legacy_jigsaw_rotation_fix;
pub mod level_legacy_world_gen_settings_fix;
pub mod level_uuid_fix;
pub mod memory_expiry_data_fix;
pub mod missing_dimension_fix;
pub mod options_fancy_graphics_to_graphics_mode_fix;
pub mod options_rename_field_fix;
pub mod player_uuid_fix;
pub mod poi_type_rename_fix;
pub mod redstone_wire_connections_fix;
pub mod saved_data_uuid_fix;
pub mod strider_gravity_fix;
pub mod structure_reference_count_fix;
pub mod villager_follow_range_fix;
pub mod wall_property_fix;
pub mod world_gen_settings_fix;
// end: versions 2200..2699

// begin: versions 2700..3299
pub mod abstract_arrow_pickup_fix;
pub mod blending_data_remove_from_nether_end_fix;
pub mod caves_and_cliffs_renames;
pub mod chunk_delete_ignored_light_data_fix;
pub mod chunk_height_and_biome_fix;
pub mod chunk_proto_tick_list_fix;
pub mod chunk_renames_fix;
pub mod criteria_rename_fix;
pub mod entity_fields_rename_fix;
pub mod entity_goat_missing_state_fix;
pub mod entity_variant_fix;
pub mod filtered_books_fix;
pub mod filtered_signs_fix;
pub mod goat_horn_id_fix;
pub mod item_stack_spawn_egg_fix;
pub mod options_ambient_occlusion_fix;
pub mod options_programmer_art_fix;
pub mod overreaching_tick_fix;
pub mod poi_type_remove_fix;
pub mod saved_data_feature_pool_element_fix;
pub mod spawner_data_fix;
pub mod stats_rename_fix;
pub mod structure_settings_flatten_fix;
pub mod structures_become_configured_fix;
pub mod variant_rename_fix;
pub mod world_gen_settings_disallow_old_custom_worlds_fix;
pub mod world_gen_settings_height_and_biome_fix;
// end: versions 2700..3299

// begin: versions 3300..3999
pub mod area_effect_cloud_potion_fix;
pub mod attribute_modifier_id_fix;
pub mod banner_entity_custom_name_to_override_component_fix;
pub mod banner_pattern_format_fix;
pub mod beehive_field_rename_fix;
pub mod block_entity_sign_double_sided_editable_text_fix;
pub mod block_pos_format_and_renames_fix;
pub mod chested_horses_inventory_zero_indexing_fix;
pub mod chunk_delete_light_fix;
pub mod decorated_pot_field_rename_fix;
pub mod drop_invalid_sign_data_fix;
pub mod effect_duration_fix;
pub mod empty_item_in_hotbar_fix;
pub mod empty_item_in_villager_trade_fix;
pub mod entity_brushable_block_fields_rename_fix;
pub mod feature_flag_remove_fix;
pub mod fix_projectile_stored_item;
pub mod fix_wolf_health;
pub mod horse_body_armor_item_fix;
pub mod item_stack_componentization_fix;
pub mod item_stack_custom_name_to_override_component_fix;
pub mod jukebox_ticks_since_song_started_fix;
pub mod legacy_dragon_fight_fix;
pub mod lodestone_compass_component_fix;
pub mod map_banner_block_pos_format_fix;
pub mod mob_effect_id_fix;
pub mod options_accessibility_onboard_fix;
pub mod options_menu_blurriness_fix;
pub mod particle_unflattening_fix;
pub mod player_head_block_profile_fix;
pub mod primed_tnt_block_state_fixer;
pub mod projectile_stored_weapon_fix;
pub mod random_sequence_settings_fix;
pub mod remap_chunk_status_fix;
pub mod remove_empty_item_in_brushable_block_fix;
pub mod rename_enchantments_fix;
pub mod scoreboard_display_slot_fix;
pub mod tipped_arrow_potion_to_item_fix;
pub mod trial_spawner_config_fix;
// end: versions 3300..3999

// begin: versions 4000..4898
// end: versions 4000..4898

// begin: versions 4899..5023
pub mod blending_data_fix;
pub mod blending_data_no_value_height_fix;
pub mod block_state_field_names_fix;
pub mod debug_profile_rename_entry_fix;
pub mod explorer_map_item_fix;
pub mod map_color_component_removal_fix;
pub mod merge_terrain_chunk_status_fix;
pub mod pot_decorations_block_entity_unflattening_fix;
pub mod pot_decorations_component_unflattening_fix;
pub mod remove_block_transformer_component_fix;
pub mod signs_op_flag_fix;
pub mod swing_animation_component_split_fix;
pub mod unfilled_buried_treasure_map_fix;
// end: versions 4899..5023
