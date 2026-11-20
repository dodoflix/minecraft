//! `FilteredBooksFix`: removes the filtered text of books.

use crate::datafix::DataFix;
use crate::fixes::item_stack_tag_fix;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;

/// `new FilteredBooksFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    item_stack_tag_fix::new(
        "FilteredBooksFix",
        output_schema,
        "Remove filtered text from books",
        |id| id == "minecraft:writable_book" || id == "minecraft:written_book",
        |tag| {
            write_and_read_typed_or_throw(&tag, tag.get_type(), |dynamic| {
                dynamic.remove("filtered_title").remove("filtered_pages")
            })
        },
    )
}
