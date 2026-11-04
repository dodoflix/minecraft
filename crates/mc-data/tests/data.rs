//! Spot checks of generated per-block / fluid / item / entity / registry data against known vanilla values.

mod common;
use common::read_json;
use mc_core::direction::Direction;
use mc_data::block::{Block, OffsetType};
use mc_data::enums::{MobCategory, NoteBlockInstrument, PushReaction};
use mc_data::fluid::{Fluid, FluidState};
use mc_data::item::Item;
use mc_data::shape::{ShapeId, shape_count};
use mc_data::{entity_type::EntityType, ids, misc, registry_data};

#[test]
fn block_properties() {
    let stone = Block::STONE.data();
    assert_eq!((stone.destroy_time, stone.explosion_resistance), (1.5, 6.0));
    assert_eq!(stone.class, "Block");
    assert!(stone.has(mc_data::block::block_flags::REQUIRES_CORRECT_TOOL_FOR_DROPS));
    assert_eq!(stone.loot_table, Some("minecraft:blocks/stone"));
    assert_eq!(stone.item, Some(Item::STONE));
    assert_eq!(Block::STONE.sound_type().step_sound, ids::SoundEvent::BLOCK_STONE_STEP);
    assert_eq!(Block::OAK_STAIRS.data().class, "StairBlock");
    assert_eq!(Block::OAK_STAIRS.data().instrument, NoteBlockInstrument::Bass);
    assert_eq!(Block::ICE.data().friction, 0.98);
    assert_eq!(Block::SOUL_SAND.data().speed_factor, 0.4);
    assert_eq!(Block::HONEY_BLOCK.data().jump_factor, 0.5);
    assert_eq!(Block::SLIME_BLOCK.data().bounce_restitution, 1.0);
    assert_eq!(Block::WHITE_BED.data().bounce_restitution, 0.75);
    assert_eq!(Block::PISTON.data().push_reaction, PushReaction::Immoveable);
    assert_eq!(Block::BEDROCK.data().destroy_time, -1.0);
    assert_eq!(Block::POPPY.data().offset_type, OffsetType::Xz);
    assert_eq!(Block::STONE.data().offset_type, OffsetType::None);
    assert_eq!(Block::OBSIDIAN.data().explosion_resistance, 1200.0);
    assert!(Block::AIR.is_air() && Block::CAVE_AIR.is_air() && Block::VOID_AIR.is_air());
    assert_eq!(Block::AIR.default_state().id(), 0);
    assert_eq!(Block::STONE.default_state().id(), 1);
    assert_eq!(Block::FURNACE.item(), Some(Item::FURNACE));
    assert!(Block::FURNACE.data().has(mc_data::block::block_flags::HAS_BLOCK_ENTITY));
}

#[test]
fn state_data() {
    assert_eq!(Block::GLOWSTONE.default_state().get_light_emission(), 15);
    assert_eq!(Block::TORCH.default_state().get_light_emission(), 14);
    assert_eq!(Block::WATER.default_state().get_light_block(), 1);
    assert_eq!(Block::STONE.default_state().get_light_block(), 15);
    assert!(Block::STONE.default_state().is_solid_render());
    assert!(!Block::GLASS.default_state().is_solid_render());
    assert!(Block::STONE.default_state().is_redstone_conductor());
    assert!(Block::STONE.default_state().is_suffocating());
    assert!(!Block::GLASS.default_state().is_suffocating());
    assert!(Block::OAK_LEAVES.default_state().is_randomly_ticking());
    assert!(Block::WATER.default_state().liquid());
    assert_eq!(Block::WATER.default_state().get_fluid_state().fluid(), Fluid::WATER);
    assert!(Block::WATER.default_state().get_fluid_state().is_source());
    assert_eq!(Block::LAVA.default_state().get_fluid_state().fluid(), Fluid::LAVA);
    let s = Block::STONE.default_state();
    assert!(s.is_face_sturdy(Direction::Up, mc_data::block::SupportType::Full));
    // full block collision shape
    let full = s.collision_shape().data();
    assert_eq!(full.aabbs, [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]);
    assert_eq!(full.kind, mc_data::shape::ShapeKind::Cube);
    assert_eq!(full.size, [1, 1, 1]);
    assert!(full.is_full(0, 0, 0));
    assert!(Block::AIR.default_state().collision_shape().is_empty());
    // bottom slab
    let slab = Block::OAK_SLAB.default_state().collision_shape().data();
    assert_eq!(slab.aabbs, [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]);
    assert_eq!(slab.coords[1], [0.0, 0.5, 1.0]);
    assert!(shape_count() > 900);
    assert_eq!(ShapeId::EMPTY.data().aabbs.len(), 0);
}

#[test]
fn fluids() {
    assert_eq!(Fluid::EMPTY.default_state(), FluidState::EMPTY);
    assert_eq!(Fluid::WATER.data().bucket, Some(Item::WATER_BUCKET));
    let w = Fluid::WATER.default_state().data();
    assert!(w.is_source);
    assert_eq!(w.amount, 8);
    assert_eq!(w.legacy_block_state, Block::WATER.default_state());
    assert_eq!(Fluid::FLOWING_WATER.states().len(), 16);
    assert_eq!(Fluid::LAVA.default_state().create_legacy_block(), Block::LAVA.default_state());
    // report: fluid registry has 5 entries
    assert_eq!(Fluid::registry().len(), 5);
}

#[test]
fn items() {
    let d = Item::DIAMOND_PICKAXE;
    assert_eq!(d.max_stack_size(), 1);
    let comps = d.default_components();
    assert_eq!(comps["minecraft:max_stack_size"], 1);
    assert!(comps.get("minecraft:tool").is_some());
    assert_eq!(Item::STONE.block(), Some(Block::STONE));
    assert_eq!(Item::STONE.data().class, "BlockItem");
    assert_eq!(Item::DIAMOND.block(), None);
    assert_eq!(Item::registry().len(), 1658);
    assert_eq!(Item::ENDER_PEARL.max_stack_size(), 16);
    assert_eq!(Item::WATER_BUCKET.data().crafting_remainder, Some(Item::BUCKET));
    assert_eq!(Item::DIAMOND.data().crafting_remainder, None);
    // report components: every item with non-trivial components exists in the datagen components report
    let report = read_json("ref/generated/reports/minecraft/components/item/diamond_pickaxe.json");
    for (k, v) in report["components"].as_object().unwrap() {
        assert_eq!(&comps[k], v, "{k}");
    }
}

#[test]
fn entity_types() {
    assert_eq!(EntityType::registry().len(), 161);
    let pig = EntityType::PIG.data();
    assert_eq!((pig.width, pig.height, pig.eye_height), (0.9, 0.9, 0.765));
    assert_eq!(pig.category, MobCategory::Creature);
    assert_eq!(pig.client_tracking_range, 10);
    assert!(pig.default_attributes.iter().any(|(a, v)| *a == ids::Attribute::MAX_HEALTH && *v == 10.0));
    assert!(pig.spawn_placement.is_some());
    let player = EntityType::PLAYER.data();
    assert_eq!((player.width, player.height), (0.6, 1.8));
    assert_eq!(player.category, MobCategory::Misc);
    assert!(EntityType::WITHER.data().fire_immune);
    assert_eq!(MobCategory::Monster.data().max_instances_per_chunk, 70);
    assert_eq!(EntityType::ARROW.data().update_interval, 20);
}

#[test]
fn registry_data_tables() {
    assert_eq!(ids::Attribute::registry().len(), 40);
    let h = ids::Attribute::MAX_HEALTH.data();
    assert_eq!((h.default_value, h.min_value, h.max_value), (20.0, Some(1.0), Some(1024.0)));
    assert!(h.syncable);
    let speed = ids::MobEffect::SPEED.data();
    assert_eq!(speed.color, 3402751);
    assert_eq!(speed.attribute_modifiers.len(), 1);
    assert_eq!(ids::GameEvent::BLOCK_ACTIVATE.data().notification_radius, 16);
    assert_eq!(ids::Potion::AWKWARD.data().effects.len(), 0);
    assert_eq!(ids::Potion::HEALING.data().effects.len(), 1);
    assert!(ids::BlockEntityType::CHEST.data().valid_blocks.contains(&Block::CHEST));
    assert!(ids::DataComponentType::MAX_STACK_SIZE.data().persistent);
    assert_eq!(ids::DataComponentType::MAX_STACK_SIZE.data().const_name, Some("MAX_STACK_SIZE"));
    assert_eq!(ids::SoundEvent::registry().len(), 1991);
}

#[test]
fn misc_tables() {
    assert_eq!(misc::fire_odds(Block::OAK_PLANKS), Some((5, 20)));
    assert_eq!(misc::fire_odds(Block::STONE), None);
    assert_eq!(misc::waxed(Block::COPPER_BLOCK), Some(Block::WAXED_COPPER_BLOCK));
    assert_eq!(misc::weathering_next(Block::COPPER_BLOCK), Some(Block::EXPOSED_COPPER));
    assert_eq!(misc::potted_by_content(Block::OAK_SAPLING), Some(Block::POTTED_OAK_SAPLING));
    assert_eq!(misc::GAME_RULE_DATA.len(), 59);
    assert_eq!(misc::DYE_COLORS.len(), 16);
    assert_eq!(misc::NOTE_BLOCK_INSTRUMENTS.len(), 27);
    assert_eq!(misc::HEIGHTMAP_TYPES.len(), 6);
    assert!(misc::ENTITY_DATA_ACCESSORS.iter().any(|a| a.class == "Entity" && a.id == 0));
    let _ = registry_data::AttributeData { description_id: "", default_value: 0.0, min_value: None, max_value: None, syncable: false, sentiment: mc_data::enums::AttributeSentiment::Positive };
}
