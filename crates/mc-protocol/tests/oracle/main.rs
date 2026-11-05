//! Oracle tests (T2): talk to the real vanilla server or run vanilla classes through `scripts/vanilla-java.sh`; all `#[ignore]`d.
//! Run with `MC_ACCEPT_EULA=1 cargo test -p mc-protocol --test oracle -- --ignored`.

mod auth;
mod common;
mod vanilla_play;
mod vanilla_smoke;
