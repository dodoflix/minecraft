//! Oracle tests (T2): the real vanilla data fixer in a JVM against the committed recordings and, as the chain lands,
//! against our fixer. Every test needs `ref/` and JDK 25 or newer (`scripts/datafix-oracle.sh`).

#[allow(
    dead_code,
    reason = "the golden target also loads this module; the runner of our fixer is used once the chain is registered"
)]
#[path = "../common/compare.rs"]
mod compare;
#[path = "../common/oracle_client.rs"]
mod oracle_client;
#[allow(
    dead_code,
    reason = "the golden target also loads this module and uses the parts this target does not"
)]
#[path = "../common/recorded_schemas.rs"]
mod recorded_schemas;

mod recordings;
mod sample_gen;
mod self_test;
mod tables;
