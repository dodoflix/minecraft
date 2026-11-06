//! `net.minecraft.advancements.predicates.GameTypePredicate`.

use mc_data::enums::GameType;
use mc_dfu::{Codec, string_resolver};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct GameTypePredicate {
    pub types: Vec<GameType>,
}

/// `GameType.CODEC` (`StringRepresentable`).
fn game_type_codec() -> Codec<GameType> {
    string_resolver(
        |g: &GameType| g.serialized_name().map(str::to_owned),
        GameType::by_serialized_name,
    )
}

impl GameTypePredicate {
    /// `GameTypePredicate.ANY`.
    pub fn any() -> GameTypePredicate {
        GameTypePredicate {
            types: GameType::ALL.to_vec(),
        }
    }

    /// `GameTypePredicate.SURVIVAL_LIKE`.
    pub fn survival_like() -> GameTypePredicate {
        GameTypePredicate {
            types: vec![GameType::Survival, GameType::Adventure],
        }
    }

    pub fn codec() -> Codec<GameTypePredicate> {
        static C: LazyLock<Codec<GameTypePredicate>> = LazyLock::new(|| {
            game_type_codec().list_of().xmap(
                |types| GameTypePredicate { types },
                |p: &GameTypePredicate| p.types.clone(),
            )
        });
        C.clone()
    }

    pub fn matches(&self, game_type: GameType) -> bool {
        self.types.contains(&game_type)
    }
}
