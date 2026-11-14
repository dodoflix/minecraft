//! `mc-game`: the public API of the gameplay server. The code lives in `mc-game-core` (the spine) and the area
//! crates; this facade re-exports them under the vanilla-shaped `mc_game::...` paths (see ARCHITECTURE.md).
//!
//! Every module that more than one crate contributes to is declared here once and lists exactly the crates that
//! extend it; modules owned by a single crate are reached through their parent's re-export.

pub use mc_game_core::*;
pub mod server {

    pub use mc_game_core::server::*;

    pub mod commands {
        pub use mc_game_cmds::server::commands::*;

        pub mod item {
            pub use mc_game_cmds::server::commands::item::*;
            pub use mc_game_core::server::commands::item::*;
        }
    }
}
pub mod world {

    pub use mc_game_core::world::*;

    pub mod block_entity {
        pub use mc_game_blocks::world::block_entity::*;
        pub use mc_game_core::world::block_entity::*;
        pub use mc_game_monsters::world::block_entity::*;
    }
    pub mod entity {

        pub use mc_game_animals::world::entity::*;
        pub use mc_game_core::world::entity::*;

        pub mod ai {

            pub use mc_game_core::world::entity::ai::*;

            pub mod behavior {
                pub use mc_game_ai::world::entity::ai::behavior::*;
                pub use mc_game_core::world::entity::ai::behavior::*;
                pub use mc_game_monsters::world::entity::ai::behavior::*;
            }
            pub mod goal {
                pub use mc_game_ai::world::entity::ai::goal::*;
                pub use mc_game_animals::world::entity::ai::goal::*;
                pub use mc_game_core::world::entity::ai::goal::*;
                pub mod target {
                    pub use mc_game_ai::world::entity::ai::goal::target::*;
                    pub use mc_game_core::world::entity::ai::goal::target::*;
                }
            }
        }
        pub mod animal {
            pub use mc_game_animals::world::entity::animal::*;
            pub use mc_game_core::world::entity::animal::*;

            pub mod equine {
                pub use mc_game_animals::world::entity::animal::equine::*;
                pub use mc_game_core::world::entity::animal::equine::*;
                pub use mc_game_monsters::world::entity::animal::equine::*;
            }
        }
        pub mod monster {
            pub use mc_game_core::world::entity::monster::*;
            pub use mc_game_monsters::world::entity::monster::*;
            pub mod cubemob {
                pub use mc_game_core::world::entity::monster::cubemob::*;
                pub use mc_game_monsters::world::entity::monster::cubemob::*;
            }
            pub mod illager {
                pub use mc_game_core::world::entity::monster::illager::*;
                pub use mc_game_monsters::world::entity::monster::illager::*;
            }
        }
        pub mod npc {
            pub use mc_game_animals::world::entity::npc::*;
            pub use mc_game_core::world::entity::npc::*;
        }
    }
    pub mod level {

        pub use mc_game_core::world::level::*;

        pub mod block {

            pub use mc_game_core::world::level::block::*;

            pub mod impls {
                pub use mc_game_blocks::world::level::block::impls::*;
                pub use mc_game_core::world::level::block::impls::*;
                pub use mc_game_monsters::world::level::block::impls::*;
            }
        }
        pub mod levelgen {
            pub use mc_game_core::world::level::levelgen::*;
            pub use mc_game_levelgen::world::level::levelgen::*;
            pub mod blockpredicates {
                pub use mc_game_core::world::level::levelgen::blockpredicates::*;
                pub use mc_game_levelgen::world::level::levelgen::blockpredicates::*;
            }
            pub mod feature {
                pub use mc_game_core::world::level::levelgen::feature::*;
                pub use mc_game_levelgen::world::level::levelgen::feature::*;
                pub mod tree {
                    pub use mc_game_core::world::level::levelgen::feature::tree::*;
                    pub use mc_game_levelgen::world::level::levelgen::feature::tree::*;
                }
            }
            pub mod placement {
                pub use mc_game_core::world::level::levelgen::placement::*;
                pub use mc_game_levelgen::world::level::levelgen::placement::*;
            }
            pub mod stateproviders {
                pub use mc_game_core::world::level::levelgen::stateproviders::*;
                pub use mc_game_levelgen::world::level::levelgen::stateproviders::*;
            }
            pub mod structure {
                pub use mc_game_core::world::level::levelgen::structure::*;
                pub use mc_game_levelgen::world::level::levelgen::structure::*;
                pub mod pools {
                    pub use mc_game_core::world::level::levelgen::structure::pools::*;
                    pub use mc_game_levelgen::world::level::levelgen::structure::pools::*;
                }
                pub mod templatesystem {
                    pub use mc_game_core::world::level::levelgen::structure::templatesystem::*;
                    pub use mc_game_levelgen::world::level::levelgen::structure::templatesystem::*;
                }
            }
        }
    }
}
