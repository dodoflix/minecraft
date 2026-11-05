//! Tests that need a process of their own (T0/T1): they capture the process-wide log or change the working directory,
//! which any other test running in the same process would disturb.

#[path = "../golden/common/mod.rs"]
mod common;

mod default_world;
mod functions_system;
mod loot_load;
mod packs;
mod vanilla_oracle;
