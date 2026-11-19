//! `DataFixers.addFixers`, versions 1344..1459: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{v1451, v1451_1, v1451_2, v1451_3, v1451_4, v1451_5, v1451_6, v1458};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(1344, Schema::same_namespaced);
    b.add_schema(1446, Schema::same_namespaced);
    b.add_schema(1450, Schema::same_namespaced);
    b.add_schema(1451, v1451::new);
    b.add_schema_sub(1451, 1, v1451_1::new);
    b.add_schema_sub(1451, 2, v1451_2::new);
    b.add_schema_sub(1451, 3, v1451_3::new);
    b.add_schema_sub(1451, 4, v1451_4::new);
    b.add_schema_sub(1451, 5, v1451_5::new);
    b.add_schema_sub(1451, 6, v1451_6::new);
    b.add_schema_sub(1451, 7, Schema::same_namespaced);
    b.add_schema(1456, Schema::same_namespaced);
    b.add_schema(1458, v1458::new);
}
