//! `IglooMetadataRemovalFix`: structure features made only of igloo pieces become a plain `Igloo`; other igloo pieces are
//! dropped from the children.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new IglooMetadataRemovalFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "IglooMetadataRemovalFix",
        output_schema,
        changes_type,
        |df| {
            let ty = df.input_schema().get_type(&References::STRUCTURE_FEATURE);
            df.fix_type_everywhere_typed("IglooMetadataRemovalFix", &ty, |typed| {
                typed.update_remainder(fix_tag)
            })
        },
    )
}

/// `fixTag(input)`.
fn fix_tag(input: Dynamic) -> Dynamic {
    let is_igloo_only = input
        .get("Children")
        .as_stream_opt()
        .map(|s| s.iter().all(is_igloo_piece))
        .result()
        .unwrap_or(false);
    if is_igloo_only {
        input
            .set("id", input.create_string("Igloo"))
            .remove("Children")
    } else {
        input.update("Children", remove_igloo_pieces)
    }
}

/// `removeIglooPieces(children)`.
fn remove_igloo_pieces(children: Dynamic) -> Dynamic {
    match children.as_stream_opt().result() {
        Some(s) => children.create_list(s.into_iter().filter(|v| !is_igloo_piece(v))),
        None => children,
    }
}

/// `isIglooPiece(tag)`.
fn is_igloo_piece(tag: &Dynamic) -> bool {
    tag.get("id").as_string_or("") == "Iglu"
}
