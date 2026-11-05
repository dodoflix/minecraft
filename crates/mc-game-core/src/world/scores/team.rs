//! `TeamColor`, `DisplaySlot`, `Team.CollisionRule`, `Team.Visibility`.

use mc_text::{Component, TextColor};

macro_rules! by_name {
    ($ty:ty) => {
        impl $ty {
            /// `StringRepresentable.fromEnum(...).byName(name)`.
            pub fn by_name(name: &str) -> Option<$ty> {
                Self::VALUES.iter().copied().find(|v| v.get_serialized_name() == name)
            }
        }
    };
}

/// `TeamColor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TeamColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl TeamColor {
    pub const VALUES: [TeamColor; 16] = [
        TeamColor::Black,
        TeamColor::DarkBlue,
        TeamColor::DarkGreen,
        TeamColor::DarkAqua,
        TeamColor::DarkRed,
        TeamColor::DarkPurple,
        TeamColor::Gold,
        TeamColor::Gray,
        TeamColor::DarkGray,
        TeamColor::Blue,
        TeamColor::Green,
        TeamColor::Aqua,
        TeamColor::Red,
        TeamColor::LightPurple,
        TeamColor::Yellow,
        TeamColor::White,
    ];

    pub fn id(self) -> i32 {
        self as i32
    }

    /// `BY_ID` with `OutOfBoundsStrategy.ZERO`.
    pub fn by_id(id: i32) -> TeamColor {
        usize::try_from(id).ok().and_then(|i| Self::VALUES.get(i).copied()).unwrap_or(TeamColor::Black)
    }

    pub fn get_serialized_name(self) -> &'static str {
        ["black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "dark_purple", "gold", "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white"]
            [self as usize]
    }

    pub fn text_color(self) -> TextColor {
        [
            TextColor::BLACK,
            TextColor::DARK_BLUE,
            TextColor::DARK_GREEN,
            TextColor::DARK_AQUA,
            TextColor::DARK_RED,
            TextColor::DARK_PURPLE,
            TextColor::GOLD,
            TextColor::GRAY,
            TextColor::DARK_GRAY,
            TextColor::BLUE,
            TextColor::GREEN,
            TextColor::AQUA,
            TextColor::RED,
            TextColor::LIGHT_PURPLE,
            TextColor::YELLOW,
            TextColor::WHITE,
        ][self as usize]
    }

    pub fn display_slot(self) -> DisplaySlot {
        DisplaySlot::VALUES[3 + self as usize]
    }

    pub fn rgb(self) -> i32 {
        self.text_color().value()
    }
}
by_name!(TeamColor);

/// `DisplaySlot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DisplaySlot {
    List,
    Sidebar,
    BelowName,
    TeamBlack,
    TeamDarkBlue,
    TeamDarkGreen,
    TeamDarkAqua,
    TeamDarkRed,
    TeamDarkPurple,
    TeamGold,
    TeamGray,
    TeamDarkGray,
    TeamBlue,
    TeamGreen,
    TeamAqua,
    TeamRed,
    TeamLightPurple,
    TeamYellow,
    TeamWhite,
}

impl DisplaySlot {
    pub const VALUES: [DisplaySlot; 19] = [
        DisplaySlot::List,
        DisplaySlot::Sidebar,
        DisplaySlot::BelowName,
        DisplaySlot::TeamBlack,
        DisplaySlot::TeamDarkBlue,
        DisplaySlot::TeamDarkGreen,
        DisplaySlot::TeamDarkAqua,
        DisplaySlot::TeamDarkRed,
        DisplaySlot::TeamDarkPurple,
        DisplaySlot::TeamGold,
        DisplaySlot::TeamGray,
        DisplaySlot::TeamDarkGray,
        DisplaySlot::TeamBlue,
        DisplaySlot::TeamGreen,
        DisplaySlot::TeamAqua,
        DisplaySlot::TeamRed,
        DisplaySlot::TeamLightPurple,
        DisplaySlot::TeamYellow,
        DisplaySlot::TeamWhite,
    ];

    pub fn id(self) -> i32 {
        self as i32
    }

    /// `BY_ID` with `OutOfBoundsStrategy.ZERO`.
    pub fn by_id(id: i32) -> DisplaySlot {
        usize::try_from(id).ok().and_then(|i| Self::VALUES.get(i).copied()).unwrap_or(DisplaySlot::List)
    }

    pub fn get_serialized_name(self) -> &'static str {
        match self {
            DisplaySlot::List => "list",
            DisplaySlot::Sidebar => "sidebar",
            DisplaySlot::BelowName => "below_name",
            team => TEAM_SLOT_NAMES[team as usize - 3],
        }
    }
}
by_name!(DisplaySlot);

const TEAM_SLOT_NAMES: [&str; 16] = [
    "sidebar.team.black",
    "sidebar.team.dark_blue",
    "sidebar.team.dark_green",
    "sidebar.team.dark_aqua",
    "sidebar.team.dark_red",
    "sidebar.team.dark_purple",
    "sidebar.team.gold",
    "sidebar.team.gray",
    "sidebar.team.dark_gray",
    "sidebar.team.blue",
    "sidebar.team.green",
    "sidebar.team.aqua",
    "sidebar.team.red",
    "sidebar.team.light_purple",
    "sidebar.team.yellow",
    "sidebar.team.white",
];

/// `Team.CollisionRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollisionRule {
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

impl CollisionRule {
    pub const VALUES: [CollisionRule; 4] = [CollisionRule::Always, CollisionRule::Never, CollisionRule::PushOtherTeams, CollisionRule::PushOwnTeam];

    pub fn id(self) -> i32 {
        self as i32
    }

    pub fn by_id(id: i32) -> CollisionRule {
        usize::try_from(id).ok().and_then(|i| Self::VALUES.get(i).copied()).unwrap_or(CollisionRule::Always)
    }

    pub fn get_serialized_name(self) -> &'static str {
        ["always", "never", "pushOtherTeams", "pushOwnTeam"][self as usize]
    }

    pub fn get_display_name(self) -> Component {
        Component::translatable(format!("team.collision.{}", self.get_serialized_name()), vec![])
    }
}
by_name!(CollisionRule);

/// `Team.Visibility`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

impl Visibility {
    pub const VALUES: [Visibility; 4] = [Visibility::Always, Visibility::Never, Visibility::HideForOtherTeams, Visibility::HideForOwnTeam];

    pub fn id(self) -> i32 {
        self as i32
    }

    pub fn by_id(id: i32) -> Visibility {
        usize::try_from(id).ok().and_then(|i| Self::VALUES.get(i).copied()).unwrap_or(Visibility::Always)
    }

    pub fn get_serialized_name(self) -> &'static str {
        ["always", "never", "hideForOtherTeams", "hideForOwnTeam"][self as usize]
    }

    pub fn get_display_name(self) -> Component {
        Component::translatable(format!("team.visibility.{}", self.get_serialized_name()), vec![])
    }
}
by_name!(Visibility);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        assert_eq!(TeamColor::Red.display_slot(), DisplaySlot::TeamRed);
        assert_eq!(DisplaySlot::TeamRed.get_serialized_name(), "sidebar.team.red");
        assert_eq!(DisplaySlot::by_name("sidebar.team.light_purple"), Some(DisplaySlot::TeamLightPurple));
        assert_eq!(TeamColor::by_name("gold").unwrap().rgb(), 16755200);
        assert_eq!(TeamColor::by_id(99), TeamColor::Black);
        assert_eq!(Visibility::by_name("hideForOwnTeam"), Some(Visibility::HideForOwnTeam));
        assert_eq!(CollisionRule::by_id(2).get_serialized_name(), "pushOtherTeams");
        for (i, s) in DisplaySlot::VALUES.iter().enumerate() {
            assert_eq!(s.id(), i as i32);
        }
    }
}
