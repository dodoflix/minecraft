//! `RemapChunkStatusFix`: renames the chunk statuses of a chunk.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;
use std::sync::Arc;

type Mapper = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// `new RemapChunkStatusFix(schema, name, mapper)`.
pub fn new(
    schema: &Schema,
    name: &str,
    mapper: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let mapper: Mapper = Arc::new(mapper);
    DataFix::new("RemapChunkStatusFix", schema, false, move |df| {
        let mapper = mapper.clone();
        df.fix_type_everywhere_typed(
            &name,
            &df.input_schema().get_type(&References::CHUNK),
            move |input| {
                input.update_remainder(|data| {
                    data.update("Status", |status| fix_status(&mapper, status))
                        .update("below_zero_retrogen", |retrogen| {
                            retrogen.update("target_status", |status| fix_status(&mapper, status))
                        })
                })
            },
        )
    })
}

fn fix_status(mapper: &Mapper, dynamic: Dynamic) -> Dynamic {
    match dynamic.as_string().result() {
        Some(status) => dynamic.create_string(mapper(&ensure_namespaced(status.as_lossy_str()))),
        None => dynamic,
    }
}
