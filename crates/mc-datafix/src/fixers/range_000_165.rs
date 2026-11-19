//! `DataFixers.addFixers`, versions 99..165: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{v99, v100, v102, v106, v107, v135, v143};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(99, v99::new);
    b.add_schema(100, v100::new);
    b.add_schema(101, Schema::same);
    b.add_schema(102, v102::new);
    b.add_schema(105, Schema::same);
    b.add_schema(106, v106::new);
    b.add_schema(107, v107::new);
    b.add_schema(108, Schema::same);
    b.add_schema(109, Schema::same);
    b.add_schema(110, Schema::same);
    b.add_schema(111, Schema::same);
    b.add_schema(113, Schema::same);
    b.add_schema(135, v135::new);
    b.add_schema(143, v143::new);
    b.add_schema(147, Schema::same);
    b.add_schema(165, Schema::same);
}
