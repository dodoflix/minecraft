//! Golden tests (T0/T1): replays of vanilla recordings and hermetic checks; one module per topic.

#[allow(
    dead_code,
    reason = "the oracle target also loads this module; the process-wide log capture and the runner of our fixer are used there"
)]
#[path = "../common/compare.rs"]
mod compare;
#[path = "../common/corpus_samples.rs"]
mod corpus_samples;
#[path = "../common/recorded_chain.rs"]
mod recorded_chain;
#[path = "../common/recorded_schemas.rs"]
mod recorded_schemas;
#[allow(
    dead_code,
    reason = "each band of fixes replays its own recorded samples through this module"
)]
#[path = "../common/step_samples.rs"]
mod step_samples;

mod base_fixes;
mod chain_parity;
mod compare_rules;
mod core_smoke;
mod facade;
mod final_state;
mod fix_coverage;
mod json_compat;
mod legacy_fixes;
mod library;
mod rule_cost;
mod samples_corpus;
mod samples_hand_written;
mod samples_json_files;
mod samples_saved_data;
mod samples_step_1344_1459;
mod samples_step_1460_2199;
mod samples_step_2200_2699;
mod samples_step_2700_3299;
mod samples_step_3300_3999;
mod samples_step_4000_4898;
mod samples_step_99_1343;
mod samples_tail;
mod schema_hooks;
mod schema_types;
mod tables;
