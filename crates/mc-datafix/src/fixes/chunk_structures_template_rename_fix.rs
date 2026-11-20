//! `ChunkStructuresTemplateRenameFix`: renames the template names of the pieces of the end city, mansion, igloo and
//! ocean ruin structures.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::fixes::tables::table;
use crate::schema::Schema;
use mc_dfu::Dynamic;
use mc_nbt::Tag;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The piece id of a structure and its template renames.
type PieceRenames = (&'static str, HashMap<&'static str, &'static str>);

/// `RENAMES`: structure id to the piece id and its template renames.
static RENAMES: LazyLock<HashMap<&'static str, PieceRenames>> = LazyLock::new(|| {
    table("chunk_structures_template_rename_fix")
        .field("RENAMES")
        .entries
        .iter()
        .map(|e| {
            let id = e
                .key
                .as_ref()
                .and_then(Tag::as_string)
                .expect("structure id");
            let data = e
                .value
                .as_ref()
                .and_then(Tag::as_compound)
                .expect("rename data");
            let piece = data.get_string("l").expect("piece id");
            let templates = data
                .get_list("r")
                .expect("templates")
                .compounds()
                .map(|c| {
                    (
                        c.get_string("k").expect("old"),
                        c.get_string("v").expect("new"),
                    )
                })
                .collect();
            (id, (piece, templates))
        })
        .collect()
});

/// `new ChunkStructuresTemplateRenameFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "ChunkStructuresTemplateRenameFix",
        output_schema,
        changes_type,
        |df| {
            let ty = df.input_schema().get_type(&References::STRUCTURE_FEATURE);
            df.fix_type_everywhere_typed("ChunkStructuresTemplateRenameFix", &ty, |input| {
                input.update_remainder(fix_children)
            })
        },
    )
}

/// `fixChildren(structure)`.
fn fix_children(structure: Dynamic) -> Dynamic {
    structure.update("Children", |children| {
        structure.create_list(
            children
                .as_stream()
                .into_iter()
                .map(|child| fix_tag(&structure, child)),
        )
    })
}

/// `fixTag(structure, child)`.
fn fix_tag(structure: &Dynamic, mut child: Dynamic) -> Dynamic {
    let id = structure.get("id").as_string_or("");
    if let Some((piece_id, templates)) = RENAMES.get(id.as_lossy_str())
        && *piece_id == child.get("id").as_string_or("")
    {
        let template = child.get("Template").as_string_or("");
        let renamed = templates
            .get(template.as_lossy_str())
            .copied()
            .unwrap_or(template.as_lossy_str());
        child = child.set("Template", child.create_string(renamed));
    }
    child
}
