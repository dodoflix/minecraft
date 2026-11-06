//! `net.minecraft.server.permissions`: the 26.3 permission model (command levels + atomic permissions).

use std::sync::LazyLock;

use mc_core::ident::Identifier;

/// `PermissionLevel`: ALL/MODERATORS/GAMEMASTERS/ADMINS/OWNERS (ids 0..=4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum PermissionLevel {
    All = 0,
    Moderators = 1,
    Gamemasters = 2,
    Admins = 3,
    Owners = 4,
}

impl PermissionLevel {
    pub const VALUES: [PermissionLevel; 5] = [
        PermissionLevel::All,
        PermissionLevel::Moderators,
        PermissionLevel::Gamemasters,
        PermissionLevel::Admins,
        PermissionLevel::Owners,
    ];

    pub fn is_equal_or_higher_than(self, other: PermissionLevel) -> bool {
        self.id() >= other.id()
    }

    /// `PermissionLevel.byId` (`OutOfBoundsStrategy.CLAMP`).
    pub fn by_id(level: i32) -> PermissionLevel {
        Self::VALUES[level.clamp(0, 4) as usize]
    }

    pub fn id(self) -> i32 {
        self as i32
    }

    /// `getSerializedName`.
    pub fn get_serialized_name(self) -> &'static str {
        match self {
            PermissionLevel::All => "all",
            PermissionLevel::Moderators => "moderators",
            PermissionLevel::Gamemasters => "gamemasters",
            PermissionLevel::Admins => "admins",
            PermissionLevel::Owners => "owners",
        }
    }

    /// Java `Enum.name()`.
    pub fn name(self) -> &'static str {
        match self {
            PermissionLevel::All => "ALL",
            PermissionLevel::Moderators => "MODERATORS",
            PermissionLevel::Gamemasters => "GAMEMASTERS",
            PermissionLevel::Admins => "ADMINS",
            PermissionLevel::Owners => "OWNERS",
        }
    }

    pub fn by_serialized_name(name: &str) -> Option<PermissionLevel> {
        Self::VALUES
            .into_iter()
            .find(|l| l.get_serialized_name() == name)
    }
}

/// `Permission`: either a named atom (`chat/send_messages`) or a command-level requirement.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Permission {
    Atom(Identifier),
    HasCommandLevel(PermissionLevel),
}

impl Permission {
    /// `Permission.Atom.create(String)` (default namespace).
    pub fn atom(name: &str) -> Permission {
        Permission::Atom(Identifier::with_default_namespace(name).expect("valid permission atom"))
    }
}

/// `Permissions` constants.
pub mod permissions {
    use super::*;

    pub const COMMANDS_MODERATOR: Permission =
        Permission::HasCommandLevel(PermissionLevel::Moderators);
    pub const COMMANDS_GAMEMASTER: Permission =
        Permission::HasCommandLevel(PermissionLevel::Gamemasters);
    pub const COMMANDS_ADMIN: Permission = Permission::HasCommandLevel(PermissionLevel::Admins);
    pub const COMMANDS_OWNER: Permission = Permission::HasCommandLevel(PermissionLevel::Owners);
    pub static COMMANDS_ENTITY_SELECTORS: LazyLock<Permission> =
        LazyLock::new(|| Permission::atom("commands/entity_selectors"));
    pub static CHAT_SEND_MESSAGES: LazyLock<Permission> =
        LazyLock::new(|| Permission::atom("chat/send_messages"));
    pub static CHAT_SEND_COMMANDS: LazyLock<Permission> =
        LazyLock::new(|| Permission::atom("chat/send_commands"));
    pub static CHAT_RECEIVE_PLAYER_MESSAGES: LazyLock<Permission> =
        LazyLock::new(|| Permission::atom("chat/receive_player_messages"));
    pub static CHAT_RECEIVE_SYSTEM_MESSAGES: LazyLock<Permission> =
        LazyLock::new(|| Permission::atom("chat/receive_system_messages"));

    /// `Permissions.CHAT_PERMISSIONS`.
    pub fn chat_permissions() -> [&'static Permission; 4] {
        [
            &CHAT_SEND_MESSAGES,
            &CHAT_SEND_COMMANDS,
            &CHAT_RECEIVE_PLAYER_MESSAGES,
            &CHAT_RECEIVE_SYSTEM_MESSAGES,
        ]
    }
}

/// `LevelBasedPermissionSet`: a permission set fully described by one command level.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LevelBasedPermissionSet(PermissionLevel);

impl LevelBasedPermissionSet {
    pub const ALL: LevelBasedPermissionSet = LevelBasedPermissionSet(PermissionLevel::All);
    pub const MODERATOR: LevelBasedPermissionSet =
        LevelBasedPermissionSet(PermissionLevel::Moderators);
    pub const GAMEMASTER: LevelBasedPermissionSet =
        LevelBasedPermissionSet(PermissionLevel::Gamemasters);
    pub const ADMIN: LevelBasedPermissionSet = LevelBasedPermissionSet(PermissionLevel::Admins);
    pub const OWNER: LevelBasedPermissionSet = LevelBasedPermissionSet(PermissionLevel::Owners);

    pub const fn for_level(level: PermissionLevel) -> LevelBasedPermissionSet {
        LevelBasedPermissionSet(level)
    }

    pub fn level(self) -> PermissionLevel {
        self.0
    }

    pub fn has_permission(self, permission: &Permission) -> bool {
        match permission {
            Permission::HasCommandLevel(level) => self.0.is_equal_or_higher_than(*level),
            other => {
                *other == *permissions::COMMANDS_ENTITY_SELECTORS
                    && self.0.is_equal_or_higher_than(PermissionLevel::Gamemasters)
            }
        }
    }
}

impl std::fmt::Display for LevelBasedPermissionSet {
    /// Java `toString`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "permission level: {}", self.0.name())
    }
}

/// `PermissionSet`: a closed model of vanilla's interface (`NO_PERMISSIONS`, `ALL_PERMISSIONS`, level based, union).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PermissionSet {
    NoPermissions,
    AllPermissions,
    LevelBased(LevelBasedPermissionSet),
    /// `PermissionSetUnion`: never contains another union; members are distinct (vanilla compares by reference,
    /// the level-based singletons compare equal exactly when their levels do).
    Union(Vec<PermissionSet>),
}

impl PermissionSet {
    pub fn has_permission(&self, permission: &Permission) -> bool {
        match self {
            PermissionSet::NoPermissions => false,
            PermissionSet::AllPermissions => true,
            PermissionSet::LevelBased(set) => set.has_permission(permission),
            PermissionSet::Union(sets) => sets.iter().any(|s| s.has_permission(permission)),
        }
    }

    /// `PermissionSet.union` including the overrides in `LevelBasedPermissionSet` and `PermissionSetUnion`
    /// (vanilla's level-based union keeps the *lower* of the two levels).
    pub fn union(&self, other: &PermissionSet) -> PermissionSet {
        match (self, other) {
            (PermissionSet::LevelBased(a), PermissionSet::LevelBased(b)) => {
                PermissionSet::LevelBased(if a.level().is_equal_or_higher_than(b.level()) {
                    *b
                } else {
                    *a
                })
            }
            (PermissionSet::Union(a), PermissionSet::Union(b)) => {
                PermissionSet::Union(Self::merge(a, b))
            }
            (PermissionSet::Union(a), b) => {
                PermissionSet::Union(Self::merge(a, std::slice::from_ref(b)))
            }
            (a, PermissionSet::Union(b)) => {
                PermissionSet::Union(Self::merge(std::slice::from_ref(a), b))
            }
            (a, b) => PermissionSet::Union(Self::merge(
                std::slice::from_ref(a),
                std::slice::from_ref(b),
            )),
        }
    }

    fn merge(first: &[PermissionSet], second: &[PermissionSet]) -> Vec<PermissionSet> {
        let mut out: Vec<PermissionSet> = Vec::with_capacity(first.len() + second.len());
        for set in first.iter().chain(second) {
            if !out.contains(set) {
                out.push(set.clone());
            }
        }
        out
    }
}

impl From<LevelBasedPermissionSet> for PermissionSet {
    fn from(set: LevelBasedPermissionSet) -> Self {
        PermissionSet::LevelBased(set)
    }
}

/// `PermissionSetSupplier`.
pub trait PermissionSetSupplier {
    fn permissions(&self) -> PermissionSet;
}

/// `PermissionCheck` (`AlwaysPass` / `Require`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PermissionCheck {
    AlwaysPass,
    Require(Permission),
}

impl PermissionCheck {
    pub fn check(&self, source: &PermissionSet) -> bool {
        match self {
            PermissionCheck::AlwaysPass => true,
            PermissionCheck::Require(permission) => source.has_permission(permission),
        }
    }

    /// `PermissionProviderCheck.test`.
    pub fn test(&self, supplier: &impl PermissionSetSupplier) -> bool {
        self.check(&supplier.permissions())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_by_id_clamps() {
        assert_eq!(PermissionLevel::by_id(-5), PermissionLevel::All);
        assert_eq!(PermissionLevel::by_id(2), PermissionLevel::Gamemasters);
        assert_eq!(PermissionLevel::by_id(99), PermissionLevel::Owners);
    }

    #[test]
    fn level_based_checks() {
        let gm = LevelBasedPermissionSet::GAMEMASTER;
        assert!(gm.has_permission(&permissions::COMMANDS_MODERATOR));
        assert!(gm.has_permission(&permissions::COMMANDS_GAMEMASTER));
        assert!(!gm.has_permission(&permissions::COMMANDS_ADMIN));
        assert!(gm.has_permission(&permissions::COMMANDS_ENTITY_SELECTORS));
        assert!(
            !LevelBasedPermissionSet::MODERATOR
                .has_permission(&permissions::COMMANDS_ENTITY_SELECTORS)
        );
        assert!(!gm.has_permission(&permissions::CHAT_SEND_MESSAGES));
        assert_eq!(
            LevelBasedPermissionSet::OWNER.to_string(),
            "permission level: OWNERS"
        );
    }

    #[test]
    fn unions() {
        let admin: PermissionSet = LevelBasedPermissionSet::ADMIN.into();
        let moderator: PermissionSet = LevelBasedPermissionSet::MODERATOR.into();
        // vanilla quirk: level-based union keeps the lower level
        assert_eq!(admin.union(&moderator), moderator);
        let u = PermissionSet::NoPermissions.union(&admin);
        assert!(u.has_permission(&permissions::COMMANDS_ADMIN));
        let u2 = u.union(&PermissionSet::AllPermissions);
        assert!(matches!(&u2, PermissionSet::Union(v) if v.len() == 3));
        assert!(u2.has_permission(&permissions::CHAT_SEND_MESSAGES));
        let u3 = u.union(&admin);
        assert!(matches!(&u3, PermissionSet::Union(v) if v.len() == 2));
    }
}
