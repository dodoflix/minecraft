//! `DataFixers.addFixers`, versions 3300..3999: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{
    v3325, v3326, v3327, v3328, v3438, v3439, v3439_1, v3448, v3682, v3683, v3685, v3689, v3799,
    v3807, v3808, v3808_1, v3808_2, v3813, v3816, v3818, v3818_3, v3818_4, v3818_5, v3825, v3938,
};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(3319, Schema::same_namespaced);
    b.add_schema(3322, Schema::same_namespaced);
    b.add_schema(3325, v3325::new);
    b.add_schema(3326, v3326::new);
    b.add_schema(3327, v3327::new);
    b.add_schema(3328, v3328::new);
    b.add_schema(3438, v3438::new);
    b.add_schema(3439, v3439::new);
    b.add_schema_sub(3439, 1, v3439_1::new);
    b.add_schema(3440, Schema::same_namespaced);
    b.add_schema(3447, Schema::same_namespaced);
    b.add_schema(3448, v3448::new);
    b.add_schema(3450, Schema::same_namespaced);
    b.add_schema(3451, Schema::same_namespaced);
    b.add_schema(3459, Schema::same_namespaced);
    b.add_schema(3564, Schema::same_namespaced);
    b.add_schema_sub(3564, 1, Schema::same_namespaced);
    b.add_schema(3565, Schema::same_namespaced);
    b.add_schema(3566, Schema::same_namespaced);
    b.add_schema(3568, Schema::same_namespaced);
    b.add_schema(3682, v3682::new);
    b.add_schema(3683, v3683::new);
    b.add_schema(3685, v3685::new);
    b.add_schema(3689, v3689::new);
    b.add_schema(3692, Schema::same_namespaced);
    b.add_schema(3799, v3799::new);
    b.add_schema(3800, Schema::same_namespaced);
    b.add_schema(3803, Schema::same_namespaced);
    b.add_schema(3807, v3807::new);
    b.add_schema_sub(3807, 1, Schema::same_namespaced);
    b.add_schema(3808, v3808::new);
    b.add_schema_sub(3808, 1, v3808_1::new);
    b.add_schema_sub(3808, 2, v3808_2::new);
    b.add_schema(3809, Schema::same_namespaced);
    b.add_schema(3812, Schema::same_namespaced);
    b.add_schema(3813, v3813::new);
    b.add_schema(3814, Schema::same_namespaced);
    b.add_schema(3816, v3816::new);
    b.add_schema(3818, v3818::new);
    b.add_schema_sub(3818, 1, Schema::same_namespaced);
    b.add_schema_sub(3818, 2, Schema::same_namespaced);
    b.add_schema_sub(3818, 3, v3818_3::new);
    b.add_schema_sub(3818, 4, v3818_4::new);
    b.add_schema_sub(3818, 5, v3818_5::new);
    b.add_schema_sub(3818, 6, Schema::same_namespaced);
    b.add_schema(3820, Schema::same_namespaced);
    b.add_schema(3825, v3825::new);
    b.add_schema(3828, Schema::same_namespaced);
    b.add_schema(3833, Schema::same_namespaced);
    b.add_schema(3938, v3938::new);
    b.add_schema(3939, Schema::same_namespaced);
    b.add_schema(3943, Schema::same_namespaced);
    b.add_schema(3945, Schema::same_namespaced);
}
