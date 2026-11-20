//! `ChunkRenamesFix`: purges the `Level` tag of a chunk and lower-cases its child field names.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::Dynamic;

/// `new ChunkRenamesFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ChunkRenamesFix", output_schema, true, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_finder = chunk_type.find_field("Level");
        let structure_finder = level_finder.ty().find_field("Structures");
        let new_chunk_type = df.output_schema().get_type(&References::CHUNK);
        let new_structures_type = new_chunk_type.find_field_type("structures");
        df.fix_type_everywhere_typed_new(
            "Chunk Renames; purge Level-tag",
            &chunk_type,
            &new_chunk_type,
            move |chunk| {
                let level = chunk.get_typed(&level_finder);
                let chunk_typed = append_chunk_name(&level);
                let level_remainder = level.get(&DSL::remainder_finder()).into_dynamic();
                let chunk_typed = chunk_typed.set(
                    &DSL::remainder_finder(),
                    merge_remainders(&chunk, level_remainder),
                );
                let chunk_typed = rename_field(chunk_typed, "TileEntities", "block_entities");
                let chunk_typed = rename_field(chunk_typed, "TileTicks", "block_ticks");
                let chunk_typed = rename_field(chunk_typed, "Entities", "entities");
                let chunk_typed = rename_field(chunk_typed, "Sections", "sections");
                let chunk_typed =
                    chunk_typed.update_typed_as(&structure_finder, &new_structures_type, |s| {
                        rename_field(s, "Starts", "starts")
                    });
                let chunk_typed = rename_field(chunk_typed, "Structures", "structures");
                chunk_typed.update_remainder(|remainder| remainder.remove("Level"))
            },
        )
    })
}

fn rename_field(input: Typed, old_name: &str, new_name: &str) -> Typed {
    let field_type = input.ty.find_field_type(old_name);
    let old_type = DSL::optional_type(&DSL::field_type(old_name, &field_type));
    let new_type = DSL::optional_type(&DSL::field_type(new_name, &field_type));
    input
        .update_as(&old_type.finder(), &new_type, |value| value)
        .update_remainder(|tag| tag.remove(old_name))
}

fn append_chunk_name(input: &Typed) -> Typed {
    Typed::new(
        DSL::named_type("chunk", &input.ty),
        input.ops,
        Value::named("chunk", input.value.clone()),
    )
}

fn merge_remainders(chunk: &Typed, level_remainder: Dynamic) -> Dynamic {
    let ops = level_remainder.ops();
    let chunk_remainder = chunk
        .get(&DSL::remainder_finder())
        .into_dynamic()
        .convert(&ops);
    let to_map = ops
        .get_map(level_remainder.value())
        .flat_map(|map| ops.merge_to_map(chunk_remainder.value(), map.entries()));
    match to_map.result() {
        Some(value) => Dynamic::new(ops, value),
        None => level_remainder,
    }
}
