//! `DataFixers.addFixers`, versions 2200..2699: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{
    v2501, v2502, v2505, v2509, v2511_1, v2519, v2522, v2551, v2568, v2571, v2684, v2686, v2688,
};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(2202, Schema::same_namespaced);
    b.add_schema(2209, Schema::same_namespaced);
    b.add_schema(2211, Schema::same_namespaced);
    b.add_schema(2218, Schema::same_namespaced);
    b.add_schema(2501, v2501::new);
    b.add_schema(2502, v2502::new);
    b.add_schema(2503, Schema::same_namespaced);
    b.add_schema(2505, v2505::new);
    b.add_schema(2508, Schema::same_namespaced);
    b.add_schema(2509, v2509::new);
    b.add_schema(2511, Schema::same_namespaced);
    b.add_schema_sub(2511, 1, v2511_1::new);
    b.add_schema(2514, Schema::same_namespaced);
    b.add_schema(2516, Schema::same_namespaced);
    b.add_schema(2518, Schema::same_namespaced);
    b.add_schema(2519, v2519::new);
    b.add_schema(2522, v2522::new);
    b.add_schema(2523, Schema::same_namespaced);
    b.add_schema(2527, Schema::same_namespaced);
    b.add_schema(2528, Schema::same_namespaced);
    b.add_schema(2529, Schema::same_namespaced);
    b.add_schema(2531, Schema::same_namespaced);
    b.add_schema(2533, Schema::same_namespaced);
    b.add_schema(2535, Schema::same_namespaced);
    b.add_schema(2537, Schema::same_namespaced);
    b.add_schema(2538, Schema::same_namespaced);
    b.add_schema(2550, Schema::same_namespaced);
    b.add_schema(2551, v2551::new);
    b.add_schema(2552, Schema::same_namespaced);
    b.add_schema(2553, Schema::same_namespaced);
    b.add_schema(2556, Schema::same_namespaced);
    b.add_schema(2558, Schema::same_namespaced);
    b.add_schema(2568, v2568::new);
    b.add_schema(2571, v2571::new);
    b.add_schema(2679, Schema::same_namespaced);
    b.add_schema(2680, Schema::same_namespaced);
    b.add_schema(2684, v2684::new);
    b.add_schema(2686, v2686::new);
    b.add_schema(2688, v2688::new);
    b.add_schema(2690, Schema::same_namespaced);
    b.add_schema(2691, Schema::same_namespaced);
    b.add_schema(2693, Schema::same_namespaced);
    b.add_schema(2696, Schema::same_namespaced);
}
