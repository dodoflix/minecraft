//! `net.minecraft.advancements`: advancements, progress, the advancement tree, rewards, the per-player state
//! (`PlayerAdvancements`), `ServerAdvancementManager` and the criterion triggers; `predicates` are the data-driven
//! predicates shared by loot conditions, enchantment effects, triggers and commands.

pub mod advancement;
pub mod advancement_progress;
pub mod advancement_requirements;
pub mod advancement_rewards;
pub mod advancement_tree;
pub mod advancement_type;
pub mod advancement_visibility_evaluator;
pub mod display_info;
pub mod java_hash;
pub mod player_advancements;
pub mod predicates;
pub mod server_advancement_manager;
pub mod tree_node_position;
pub mod triggers;

pub use advancement::{Advancement, AdvancementHolder};
pub use advancement_progress::{AdvancementProgress, CriterionProgress};
pub use advancement_requirements::AdvancementRequirements;
pub use advancement_rewards::AdvancementRewards;
pub use advancement_tree::{AdvancementNode, AdvancementTree, NodeId};
pub use advancement_type::AdvancementType;
pub use display_info::DisplayInfo;
pub use player_advancements::{PlayerAdvancements, SharedAdvancements};
pub use server_advancement_manager::ServerAdvancementManager;
