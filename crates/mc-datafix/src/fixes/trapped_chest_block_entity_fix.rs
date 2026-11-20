//! `TrappedChestBlockEntityFix`: turns the `minecraft:chest` block entities that sit at a trapped chest block into
//! `minecraft:trapped_chest` ones (the block entity id was added together with the block).

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::add_new_choices;
use crate::fixes::leaves_fix::{self, Ctx, Section};
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::value::Value;
use std::collections::HashSet;

const SIZE: i32 = 4096;
const SIZE_BITS: i32 = 12;

/// `TrappedChestBlockEntityFix.TrappedChestSection`.
struct TrappedChestSection {
    base: Section,
    chest_ids: HashSet<i32>,
}

impl TrappedChestSection {
    /// `new TrappedChestSection(section, inputSchema)`.
    fn new(section: &Typed, ctx: &Ctx) -> TrappedChestSection {
        let mut chest_ids = HashSet::new();
        // `skippable()`: collects the palette ids of trapped chests
        let base = Section::new(section, ctx, |palette| {
            chest_ids = (0..palette.len())
                .filter(|&i| palette[i].get("Name").as_string_or("") == "minecraft:trapped_chest")
                .map(|i| i as i32)
                .collect();
            chest_ids.is_empty()
        });
        TrappedChestSection { base, chest_ids }
    }

    /// `isTrappedChest(block)`.
    fn is_trapped_chest(&self, block: i32) -> bool {
        self.chest_ids.contains(&block)
    }
}

/// `new TrappedChestBlockEntityFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "TrappedChestBlockEntityFix",
        output_schema,
        changes_type,
        |df| {
            let chunk_type = df.output_schema().get_type(&References::CHUNK);
            let level_type = chunk_type.find_field_type("Level");
            let tile_entity_list_type = level_type.find_field_type("TileEntities");
            if tile_entity_list_type.as_list().is_none() {
                panic!("java.lang.IllegalStateException: Tile entity type is not a list type.");
            }
            let tile_entities_f = DSL::field_finder("TileEntities", &tile_entity_list_type);
            let input_schema = df.input_schema();
            let chunk_type1 = input_schema.get_type(&References::CHUNK);
            let level_finder = chunk_type1.find_field("Level");
            let sections_finder = level_finder.ty().find_field("Sections");
            let Some(section_type) = sections_finder.ty().as_list().cloned() else {
                panic!("java.lang.IllegalStateException: Expecting sections to be a list.");
            };
            let section_finder = DSL::type_finder(&section_type);
            let ctx = Ctx::new(input_schema);
            TypeRewriteRule::seq(
                add_new_choices::new(
                    &df.output_schema(),
                    "AddTrappedChestFix",
                    &References::BLOCK_ENTITY,
                )
                .get_rule(),
                df.fix_type_everywhere_typed("Trapped Chest fix", &chunk_type1, move |chunk| {
                    chunk.update_typed(&level_finder, |level| {
                        let Some(sections) = level.get_optional_typed(&sections_finder) else {
                            return level;
                        };
                        let section_list = sections.get_all_typed(&section_finder);
                        let mut chest_locations: HashSet<i32> = HashSet::new();
                        for section in &section_list {
                            let trapped_chest_section = TrappedChestSection::new(section, &ctx);
                            if !trapped_chest_section.base.is_skippable() {
                                for i in 0..SIZE {
                                    let block = trapped_chest_section.base.get_block(i);
                                    if trapped_chest_section.is_trapped_chest(block) {
                                        chest_locations.insert(
                                            trapped_chest_section.base.index << SIZE_BITS | i,
                                        );
                                    }
                                }
                            }
                        }
                        let level_tag = level.get_remainder();
                        let chunk_x = level_tag.get("xPos").as_int_or(0);
                        let chunk_z = level_tag.get("zPos").as_int_or(0);
                        let tile_entity_choice_type =
                            ctx.schema().find_choice_type(&References::BLOCK_ENTITY);
                        let choice_finder = DSL::type_finder(tile_entity_choice_type.ty());
                        level.update_typed(&tile_entities_f, |tile_entities| {
                            tile_entities.update_typed(&choice_finder, |tile_entity| {
                                let tag = tile_entity
                                    .get_or_create(&DSL::remainder_finder())
                                    .into_dynamic();
                                let x = tag.get("x").as_int_or(0).wrapping_sub(chunk_x << 4);
                                let y = tag.get("y").as_int_or(0);
                                let z = tag.get("z").as_int_or(0).wrapping_sub(chunk_z << 4);
                                if chest_locations.contains(&leaves_fix::get_index(x, y, z)) {
                                    tile_entity.update(&choice_finder, |string_pair| {
                                        string_pair.map_first(|s| {
                                            if s.as_str() != "minecraft:chest" {
                                                log::warn!(
                                                    "Block Entity was expected to be a chest"
                                                );
                                            }
                                            Value::Str("minecraft:trapped_chest".to_string())
                                        })
                                    })
                                } else {
                                    tile_entity
                                }
                            })
                        })
                    })
                }),
            )
        },
    )
}
