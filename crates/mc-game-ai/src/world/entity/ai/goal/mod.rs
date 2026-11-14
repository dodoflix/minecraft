pub mod target;

mod open_door;

pub use open_door::OpenDoorGoal;

mod golem_random_stroll_in_village;

pub use golem_random_stroll_in_village::GolemRandomStrollInVillageGoal;

mod stroll_through_village;

pub use stroll_through_village::StrollThroughVillageGoal;

mod interact_goal;

pub use interact_goal::InteractGoal;

mod offer_flower;

pub use offer_flower::{OfferFlowerFn, OfferFlowerGoal};

mod use_item;

pub use use_item::UseItemGoal;

mod look_at_trading_player;

pub use look_at_trading_player::LookAtTradingPlayerGoal;

mod trade_with_player;

pub use trade_with_player::TradeWithPlayerGoal;

mod avoid_entity;

pub use avoid_entity::{AvoidEntityGoal, EntityTest};

mod breath_air;

pub use breath_air::BreathAirGoal;

mod climb_on_top_of_powder_snow;

pub use climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal;

mod flee_sun;

pub use flee_sun::FleeSunGoal;

mod jump_goal;

pub use jump_goal::JUMP_GOAL_FLAGS;

mod move_towards_restriction;

pub use move_towards_restriction::MoveTowardsRestrictionGoal;

mod move_towards_target;

pub use move_towards_target::MoveTowardsTargetGoal;

mod random_swimming;

pub use random_swimming::{RandomSwimmingGoal, get_random_swimmable_pos};

mod restrict_sun;

pub use restrict_sun::RestrictSunGoal;

mod try_find_liquid;

pub use try_find_liquid::TryFindLiquidGoal;

mod dolphin_jump_goal;

pub use dolphin_jump_goal::DolphinJumpGoal;

mod eat_block;

pub use eat_block::EatBlockGoal;

mod leap_at_target;

pub use leap_at_target::LeapAtTargetGoal;

mod follow_player_ridden_entity_goal;

pub use follow_player_ridden_entity_goal::FollowPlayerRiddenEntityGoal;

mod ocelot_attack;

pub use ocelot_attack::OcelotAttackGoal;

mod ranged_bow_attack;

pub use ranged_bow_attack::RangedBowAttackGoal;

mod ranged_crossbow_attack;

pub use ranged_crossbow_attack::RangedCrossbowAttackGoal;

mod swell;

pub use swell::SwellGoal;

#[cfg(test)]
mod tests;
