//! `EmptyItemInVillagerTradeFix`: drops the second buy item of a trade when it is air or empty.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;

/// `new EmptyItemInVillagerTradeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("EmptyItemInVillagerTradeFix", output_schema, false, |df| {
        let trade_type = df.input_schema().get_type(&References::VILLAGER_TRADE);
        df.write_fix_and_read(
            "EmptyItemInVillagerTradeFix",
            &trade_type,
            &trade_type,
            |input| {
                let buy_b = input.get("buyB").or_else_empty_map();
                let id = ensure_namespaced(&buy_b.get("id").as_string_or("minecraft:air"));
                let count = buy_b.get("count").as_int_or(0);
                if id != "minecraft:air" && count != 0 {
                    input
                } else {
                    input.remove("buyB")
                }
            },
        )
    })
}
