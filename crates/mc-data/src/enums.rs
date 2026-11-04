//! Java enums that appear in generated data (block property values, push reactions, mob categories, ...).
//! `Direction` and `Axis` are mc-core's. Every enum has `ALL`, `ordinal()`, `from_ordinal()`, `name()` (Java constant name)
//! and `serialized_name()` (`StringRepresentable`).

include!(concat!(env!("OUT_DIR"), "/enums.rs"));
