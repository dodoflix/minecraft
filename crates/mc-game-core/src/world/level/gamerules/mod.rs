//! `net.minecraft.world.level.gamerules`: `GameRule<T>`, `GameRuleMap`, `GameRules`, `GameRuleCategory`,
//! `GameRuleType`, `GameRuleTypeVisitor`.
//!
//! The rule registry (`BuiltInRegistries.GAME_RULE`) is `mc_data::GameRule` (ids, names, defaults, bounds,
//! categories come from the extracted vanilla data); the constants below are typed handles over those ids exactly
//! like `GameRules.ADVANCE_TIME` etc. Feature requirements (only `max_minecart_speed`) are the one thing the
//! extract does not carry.
mod game_rule;
mod game_rule_map;
mod game_rules;

pub use game_rule::{
    AnyGameRule, GameRule, GameRuleCategory, GameRuleType, GameRuleTypeVisitor, GameRuleValue,
    GameRuleValueType,
};
pub use game_rule_map::{GameRuleMap, GameRuleMapBuilder};
pub use game_rules::{GameRuleListener, GameRules};
