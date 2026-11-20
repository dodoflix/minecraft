//! `BlockStateStructureTemplateFix`: upgrades legacy block states of structure template palettes.

use crate::datafix::DataFix;
use crate::fixes::block_state_data;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::{Dyn, Dynamic, NBT};

/// `new BlockStateStructureTemplateFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "BlockStateStructureTemplateFix",
        output_schema,
        changes_type,
        |df| {
            df.fix_type_everywhere_typed(
                "BlockStateStructureTemplateFix",
                &df.input_schema().get_type(&References::BLOCK_STATE),
                |input| input.update_remainder(upgrade_block_state),
            )
        },
    )
}

/// `BlockStateData::upgradeBlockStateTag` on a `Dynamic`: only one over NBT ops can match a legacy state.
fn upgrade_block_state(old: Dynamic) -> Dynamic {
    if old.ops() != NBT {
        return old;
    }
    let Ok(old_tag) = old.value().to_tag() else {
        return old;
    };
    let upgraded = block_state_data::upgrade_block_state_tag(&old_tag);
    if std::ptr::eq(upgraded, &old_tag) {
        old
    } else {
        Dynamic::new(NBT, Dyn::from(upgraded))
    }
}
