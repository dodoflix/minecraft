//! `DataFixers.addFixers`, versions 4650..5023: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{v4656, v4771, v4881, v4885, v4996, v4996_1, v4997, v5000};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(4650, Schema::same_namespaced);
    b.add_schema(4651, Schema::same_namespaced);
    b.add_schema(4656, v4656::new);
    b.add_schema(4657, Schema::same_namespaced);
    b.add_schema(4658, Schema::same_namespaced);
    b.add_schema(4661, Schema::same_namespaced);
    b.add_schema(4763, Schema::same_namespaced);
    b.add_schema(4766, Schema::same_namespaced);
    b.add_schema(4769, Schema::same_namespaced);
    b.add_schema(4771, v4771::new);
    b.add_schema(4881, v4881::new);
    b.add_schema(4885, v4885::new);
    b.add_schema(4888, Schema::same_namespaced);
    b.add_schema(4892, Schema::same_namespaced);
    b.add_schema(4996, v4996::new);
    b.add_schema_sub(4996, 1, v4996_1::new);
    b.add_schema(4997, Schema::same_namespaced);
    b.add_schema(4997, v4997::new);
    b.add_schema(5000, v5000::new);
    b.add_schema(5001, Schema::same_namespaced);
    b.add_schema(5002, Schema::same_namespaced);
    b.add_schema(5006, Schema::same_namespaced);
    b.add_schema(5007, Schema::same_namespaced);
    b.add_schema(5008, Schema::same_namespaced);
    b.add_schema(5009, Schema::same_namespaced);
    b.add_schema(5012, Schema::same_namespaced);
    b.add_schema(5013, Schema::same_namespaced);
    b.add_schema(5014, Schema::same_namespaced);
    b.add_schema(5016, Schema::same_namespaced);
}
