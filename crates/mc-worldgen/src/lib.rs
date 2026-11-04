//! Worldgen: noise synthesis, density functions, noise router/settings and `RandomState`.
// `DensityFunction` memoizes `range()`/`domain_axes()` in OnceLocks; neither participates in Eq/Hash.
#![allow(clippy::mutable_key_type)]

pub mod data;
pub mod densityfunction;
pub mod noise_router;
pub mod noise_settings;
pub mod noises;
pub mod random_state;
pub mod synth;
pub mod util;
