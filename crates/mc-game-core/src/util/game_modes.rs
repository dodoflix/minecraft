//! The parts of `Difficulty` and `GameType` that mc-world's storage enums lack (display names, ability updates,
//! lookups), as extension traits on `mc_world::level_storage::{Difficulty, GameType}`.

use mc_text::Component;
use mc_world::level_storage::{Difficulty, GameType};

pub trait DifficultyExt {
    /// `getDisplayName`.
    fn get_display_name(self) -> Component;
    /// `getInfo`.
    fn get_info(self) -> Component;
}

impl DifficultyExt for Difficulty {
    fn get_display_name(self) -> Component {
        Component::translatable(
            format!("options.difficulty.{}", self.get_serialized_name()),
            vec![],
        )
    }

    fn get_info(self) -> Component {
        Component::translatable(
            format!("options.difficulty.{}.info", self.get_serialized_name()),
            vec![],
        )
    }
}

/// What `GameType.updatePlayerAbilities` assigns; `flying: None` leaves the flag untouched (creative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbilityUpdate {
    pub may_fly: bool,
    pub instabuild: bool,
    pub invulnerable: bool,
    pub flying: Option<bool>,
    pub may_build: bool,
}

pub trait GameTypeExt: Sized {
    fn get_long_display_name(self) -> Component;
    fn get_short_display_name(self) -> Component;
    /// `updatePlayerAbilities(abilities)` as data.
    fn ability_update(self) -> AbilityUpdate;
    fn is_block_placing_restricted(self) -> bool;
    fn is_creative(self) -> bool;
    fn is_survival(self) -> bool;
    /// `byName(name)`: unknown names are `SURVIVAL`.
    fn by_name(name: &str) -> GameType;
    /// `byName(name, default)`.
    fn by_name_or(name: &str, default: Option<GameType>) -> Option<GameType>;
    fn is_valid_id(id: i32) -> bool;
}

impl GameTypeExt for GameType {
    fn get_long_display_name(self) -> Component {
        Component::translatable(format!("gameMode.{}", self.get_name()), vec![])
    }

    fn get_short_display_name(self) -> Component {
        Component::translatable(format!("selectWorld.gameMode.{}", self.get_name()), vec![])
    }

    fn ability_update(self) -> AbilityUpdate {
        let (may_fly, instabuild, invulnerable, flying) = match self {
            GameType::Creative => (true, true, true, None),
            GameType::Spectator => (true, false, true, Some(true)),
            _ => (false, false, false, Some(false)),
        };
        AbilityUpdate {
            may_fly,
            instabuild,
            invulnerable,
            flying,
            may_build: !self.is_block_placing_restricted(),
        }
    }

    fn is_block_placing_restricted(self) -> bool {
        matches!(self, GameType::Adventure | GameType::Spectator)
    }

    fn is_creative(self) -> bool {
        self == GameType::Creative
    }

    fn is_survival(self) -> bool {
        matches!(self, GameType::Survival | GameType::Adventure)
    }

    fn by_name(name: &str) -> GameType {
        <GameType as GameTypeExt>::by_name_or(name, Some(GameType::Survival))
            .expect("default is Some")
    }

    fn by_name_or(name: &str, default: Option<GameType>) -> Option<GameType> {
        [
            GameType::Survival,
            GameType::Creative,
            GameType::Adventure,
            GameType::Spectator,
        ]
        .into_iter()
        .find(|g| g.get_name() == name)
        .or(default)
    }

    fn is_valid_id(id: i32) -> bool {
        (0..=3).contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_type_helpers() {
        assert_eq!(
            GameType::Spectator.ability_update(),
            AbilityUpdate {
                may_fly: true,
                instabuild: false,
                invulnerable: true,
                flying: Some(true),
                may_build: false
            }
        );
        assert_eq!(GameType::Creative.ability_update().flying, None);
        assert!(GameType::Adventure.is_survival() && !GameType::Creative.is_survival());
        assert_eq!(
            <GameType as GameTypeExt>::by_name("nope"),
            GameType::Survival
        );
        assert_eq!(<GameType as GameTypeExt>::by_name_or("nope", None), None);
        assert_eq!(
            <GameType as GameTypeExt>::by_name("spectator"),
            GameType::Spectator
        );
        assert_eq!(
            GameType::Creative.get_long_display_name(),
            Component::translatable("gameMode.creative", vec![])
        );
        assert_eq!(
            GameType::Creative.get_long_display_name().get_string(),
            "Creative Mode"
        );
        assert_eq!(
            Difficulty::Hard.get_info(),
            Component::translatable("options.difficulty.hard.info", vec![])
        );
        assert!(!GameType::is_valid_id(4));
    }
}
