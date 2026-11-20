//! `MergeTerrainChunkStatusFix`: the `carvers`, `noise` and `surface` chunk statuses merge into `terrain` and `biomes`,
//! and chunks that stopped at the intermediate ones lose their block states.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::{Type, ensure_namespaced};
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::OptionalDynamic;

const OLD_INTERMEDIATE_TERRAIN_STATUSES: [&str; 2] = ["minecraft:noise", "minecraft:surface"];

/// `new MergeTerrainChunkStatusFix(schema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("MergeTerrainChunkStatusFix", schema, false, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let sections_f = chunk_type.find_field("sections");
        let section_type = sections_f
            .ty()
            .as_list()
            .expect("the sections of a chunk are a list")
            .clone();
        let biomes_type = section_type.find_field_type("biomes");
        let block_states_type = section_type.find_field_type("block_states");
        let expected_section_type: Type = DSL::and_type3(
            &DSL::optional_type(&DSL::field_type("biomes", &biomes_type)),
            &DSL::optional_type(&DSL::field_type("block_states", &block_states_type)),
            &DSL::remainder_type(),
        );
        if section_type != expected_section_type {
            panic!(
                "java.lang.IllegalStateException: {section_type} did not match {expected_section_type}"
            );
        }
        let section_f = DSL::type_finder(&expected_section_type);
        df.fix_type_everywhere_typed("MergeTerrainChunkStatusFix", &chunk_type, move |chunk| {
            let (mut chunk, old_status, old_target_status) = fix_statuses(chunk);
            let status_intermediate = is_intermediate(&old_status);
            let target_status_intermediate =
                old_target_status.as_deref().is_some_and(is_intermediate);
            if status_intermediate || target_status_intermediate {
                let only_remove_blocks_below_zero =
                    old_target_status.is_some() && !target_status_intermediate;
                chunk = remove_block_states(
                    chunk,
                    &sections_f,
                    &section_f,
                    only_remove_blocks_below_zero,
                );
            }
            chunk
        })
    })
}

fn is_intermediate(status: &str) -> bool {
    OLD_INTERMEDIATE_TERRAIN_STATUSES.contains(&status)
}

fn remove_block_states(
    chunk: Typed,
    sections_f: &OpticFinder,
    section_f: &OpticFinder,
    only_below_zero: bool,
) -> Typed {
    chunk
        .update_remainder(|remainder| remainder.remove("Heightmaps").remove("blending_data"))
        .update_typed(sections_f, |sections| {
            sections.update(section_f, |section| {
                let y = section
                    .second()
                    .second()
                    .as_dynamic()
                    .get("Y")
                    .as_byte_or(0);
                if y >= 0 && only_below_zero {
                    section
                } else {
                    section.map_second(|block_states_and_remainder| {
                        block_states_and_remainder
                            .map_first(|_| Value::none())
                            .map_second(|section_remainder| {
                                Value::Dynamic(
                                    section_remainder.into_dynamic().remove("block_states"),
                                )
                            })
                    })
                }
            })
        })
}

/// `fixStatuses(chunk)`: the chunk with fixed statuses, the old status and the old target status of the retrogen.
fn fix_statuses(chunk: Typed) -> (Typed, String, Option<String>) {
    let mut remainder = chunk.get_remainder();
    let old_status = unwrap_status(&remainder.get("Status"));
    remainder = remainder.set("Status", remainder.create_string(fix_status(&old_status)));
    let mut old_target_status = None;
    if let Some(mut below_zero_retrogen) = remainder.get("below_zero_retrogen").result() {
        let old = unwrap_status(&below_zero_retrogen.get("target_status"));
        below_zero_retrogen =
            below_zero_retrogen.set("target_status", remainder.create_string(fix_status(&old)));
        remainder = remainder.set("below_zero_retrogen", below_zero_retrogen);
        old_target_status = Some(old);
    }
    (
        chunk.set_remainder(remainder),
        old_status,
        old_target_status,
    )
}

fn unwrap_status(status: &OptionalDynamic) -> String {
    ensure_namespaced(status.as_string_or("minecraft:empty").as_lossy_str())
}

fn fix_status(status: &str) -> &str {
    if status == "minecraft:carvers" {
        "minecraft:terrain"
    } else if is_intermediate(status) {
        "minecraft:biomes"
    } else {
        status
    }
}
