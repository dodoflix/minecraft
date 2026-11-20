//! Sample fixtures of the data fixers from the worlds of old vanilla servers: `extract` reads the files a server
//! loads (chunks, entity chunks, POI chunks, level data, saved data, player data, statistics, advancements) into
//! candidate samples, `select` keeps a deduplicated set cover of them within a size budget and expands every sample
//! into one update request per target version, `pack` pairs the requests with vanilla's answers into a fixture and
//! `dump` writes one stored input back out. See `main.rs` for the command line.

pub mod candidate;
pub mod extract;
pub mod features;
pub mod pack;
pub mod select;
