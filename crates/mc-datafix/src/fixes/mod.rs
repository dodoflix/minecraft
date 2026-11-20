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
pub mod block_entity_id_fix;
// end: versions 99..1343

// begin: versions 1344..1459
// end: versions 1344..1459

// begin: versions 1460..2199
// end: versions 1460..2199

// begin: versions 2200..2699
// end: versions 2200..2699

// begin: versions 2700..3299
// end: versions 2700..3299

// begin: versions 3300..3999
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
