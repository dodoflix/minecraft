//! `DataFixers.addFixers`, versions 166..1343: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{v501, v700, v701, v702, v703, v704, v705, v808, v1022, v1125};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(501, v501::new);
    b.add_schema(502, Schema::same);
    b.add_schema(505, Schema::same);
    b.add_schema(700, v700::new);
    b.add_schema(701, v701::new);
    b.add_schema(702, v702::new);
    b.add_schema(703, v703::new);
    b.add_schema(704, v704::new);
    b.add_schema(705, v705::new);
    b.add_schema(804, Schema::same_namespaced);
    b.add_schema(806, Schema::same_namespaced);
    b.add_schema(808, v808::new);
    b.add_schema_sub(808, 1, Schema::same_namespaced);
    b.add_schema(813, Schema::same_namespaced);
    b.add_schema(816, Schema::same_namespaced);
    b.add_schema(820, Schema::same_namespaced);
    b.add_schema(1022, v1022::new);
    b.add_schema(1125, v1125::new);
}
