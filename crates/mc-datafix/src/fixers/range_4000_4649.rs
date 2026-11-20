//! `DataFixers.addFixers`, versions 4000..4649: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{
    v4059, v4067, v4070, v4071, v4290, v4292, v4300, v4301, v4302, v4306, v4307, v4312, v4420,
    v4421, v4531, v4532, v4533, v4543, v4648,
};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(4054, Schema::same_namespaced);
    b.add_schema(4055, Schema::same_namespaced);
    b.add_schema(4059, v4059::new);
    b.add_schema(4061, Schema::same_namespaced);
    b.add_schema(4064, Schema::same_namespaced);
    b.add_schema(4067, v4067::new);
    b.add_schema(4068, Schema::same_namespaced);
    b.add_schema(4070, v4070::new);
    b.add_schema(4071, v4071::new);
    b.add_schema(4081, Schema::same_namespaced);
    b.add_schema(4173, Schema::same_namespaced);
    b.add_schema(4175, Schema::same_namespaced);
    b.add_schema(4176, Schema::same_namespaced);
    b.add_schema(4180, Schema::same_namespaced);
    b.add_schema(4181, Schema::same_namespaced);
    b.add_schema(4187, Schema::same_namespaced);
    b.add_schema(4290, v4290::new);
    b.add_schema(4291, Schema::same_namespaced);
    b.add_schema(4292, v4292::new);
    b.add_schema(4293, Schema::same_namespaced);
    b.add_schema(4294, Schema::same_namespaced);
    b.add_schema(4296, Schema::same_namespaced);
    b.add_schema(4297, Schema::same_namespaced);
    b.add_schema(4299, Schema::same_namespaced);
    b.add_schema(4300, v4300::new);
    b.add_schema(4301, v4301::new);
    b.add_schema(4302, v4302::new);
    b.add_schema(4303, Schema::same_namespaced);
    b.add_schema(4305, Schema::same_namespaced);
    b.add_schema(4306, v4306::new);
    b.add_schema(4307, v4307::new);
    b.add_schema(4309, Schema::same_namespaced);
    b.add_schema(4311, Schema::same_namespaced);
    b.add_schema(4312, v4312::new);
    b.add_schema(4314, Schema::same_namespaced);
    b.add_schema(4420, v4420::new);
    b.add_schema(4421, v4421::new);
    b.add_schema(4424, Schema::same_namespaced);
    b.add_schema(4531, v4531::new);
    b.add_schema(4532, v4532::new);
    b.add_schema(4533, v4533::new);
    b.add_schema(4535, Schema::same_namespaced);
    b.add_schema(4537, Schema::same_namespaced);
    b.add_schema(4541, Schema::same_namespaced);
    b.add_schema(4543, v4543::new);
    b.add_schema(4544, Schema::same_namespaced);
    b.add_schema(4548, Schema::same_namespaced);
    b.add_schema(4648, v4648::new);
    b.add_schema(4649, Schema::same_namespaced);
}
