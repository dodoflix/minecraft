//! Oracle tests (T2): checks against the vanilla reference world, which `mc_testkit::worldref` generates with the vanilla server on first use.

extern crate mc_game_core as mc_game;
mod chunk_generation;
mod chunk_world;
mod entity_roundtrip;
mod saved_data_worldref;
