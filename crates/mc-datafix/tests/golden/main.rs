//! Golden tests (T0/T1): replays of vanilla recordings and hermetic checks; one module per topic.

#[allow(
    dead_code,
    reason = "the oracle target also loads this module; the process-wide log capture and the runner of our fixer are used there"
)]
#[path = "../common/compare.rs"]
mod compare;
#[path = "../common/recorded_chain.rs"]
mod recorded_chain;
#[path = "../common/recorded_schemas.rs"]
mod recorded_schemas;

mod chain_parity;
mod compare_rules;
mod core_smoke;
mod json_compat;
mod library;
mod schema_types;
