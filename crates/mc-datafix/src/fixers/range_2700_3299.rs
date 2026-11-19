//! `DataFixers.addFixers`, versions 2700..3299: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{
    v2704, v2707, v2831, v2832, v2842, v3076, v3078, v3081, v3082, v3083, v3202, v3203, v3204,
};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(2700, Schema::same_namespaced);
    b.add_schema(2701, Schema::same_namespaced);
    b.add_schema(2702, Schema::same_namespaced);
    b.add_schema(2704, v2704::new);
    b.add_schema(2707, v2707::new);
    b.add_schema(2710, Schema::same_namespaced);
    b.add_schema(2717, Schema::same_namespaced);
    b.add_schema(2825, Schema::same_namespaced);
    b.add_schema(2831, v2831::new);
    b.add_schema(2832, v2832::new);
    b.add_schema(2833, Schema::same_namespaced);
    b.add_schema(2838, Schema::same_namespaced);
    b.add_schema(2841, Schema::same_namespaced);
    b.add_schema(2842, v2842::new);
    b.add_schema(2843, Schema::same_namespaced);
    b.add_schema(2846, Schema::same_namespaced);
    b.add_schema(2852, Schema::same_namespaced);
    b.add_schema(2967, Schema::same_namespaced);
    b.add_schema(2970, Schema::same_namespaced);
    b.add_schema(3076, v3076::new);
    b.add_schema(3077, Schema::same_namespaced);
    b.add_schema(3078, v3078::new);
    b.add_schema(3081, v3081::new);
    b.add_schema(3082, v3082::new);
    b.add_schema(3083, v3083::new);
    b.add_schema(3084, Schema::same_namespaced);
    b.add_schema(3086, Schema::same_namespaced);
    b.add_schema(3087, Schema::same_namespaced);
    b.add_schema(3090, Schema::same_namespaced);
    b.add_schema(3093, Schema::same_namespaced);
    b.add_schema(3094, Schema::same_namespaced);
    b.add_schema(3097, Schema::same_namespaced);
    b.add_schema(3108, Schema::same_namespaced);
    b.add_schema(3201, Schema::same_namespaced);
    b.add_schema(3202, v3202::new);
    b.add_schema(3203, v3203::new);
    b.add_schema(3204, v3204::new);
    b.add_schema(3209, Schema::same_namespaced);
    b.add_schema(3214, Schema::same_namespaced);
}
