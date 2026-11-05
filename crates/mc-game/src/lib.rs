// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game`: the public API of the gameplay server. The code lives in `mc-game-core` and the area crates; this crate
//! merges their module trees so every historical `mc_game::...` path keeps working (see docs/crate-split.md).

pub use mc_game_core::*;
pub mod server {
    pub use mc_game_cmds::server::*;
    pub use mc_game_levelgen::server::*;
    pub mod commands {
        pub use mc_game_cmds::server::commands::*;
        pub use mc_game_levelgen::server::commands::*;
        pub use mc_game_cmds::server::commands::item;
    }
}
pub mod world {
    pub use mc_game_blocks::world::*;
    pub use mc_game_levelgen::world::*;
    pub use mc_game_monsters::world::*;
    pub mod block_entity {
        pub use mc_game_blocks::world::block_entity::*;
        pub use mc_game_monsters::world::block_entity::*;
    }
    pub use mc_game_monsters::world::entity;
    pub mod level {
        pub use mc_game_blocks::world::level::*;
        pub use mc_game_levelgen::world::level::*;
        pub use mc_game_monsters::world::level::*;
        pub mod block {
            pub use mc_game_blocks::world::level::block::*;
            pub use mc_game_monsters::world::level::block::*;
            pub mod impls {
                pub use mc_game_blocks::world::level::block::impls::*;
                pub use mc_game_monsters::world::level::block::impls::*;
            }
        }
        pub use mc_game_levelgen::world::level::levelgen;
    }
}
