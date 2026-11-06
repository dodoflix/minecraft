//! `ObjectiveCriteria` and `ObjectiveCriteria.RenderType`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use mc_core::ident::Identifier;

use super::team::TeamColor;

/// `ObjectiveCriteria.RenderType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderType {
    Integer,
    Hearts,
}

impl RenderType {
    pub fn get_id(self) -> &'static str {
        match self {
            RenderType::Integer => "integer",
            RenderType::Hearts => "hearts",
        }
    }

    /// `CODEC.byName(key)`.
    pub fn by_name(name: &str) -> Option<RenderType> {
        [RenderType::Integer, RenderType::Hearts]
            .into_iter()
            .find(|r| r.get_id() == name)
    }

    /// `byId(key)`: unknown keys give `INTEGER`.
    pub fn by_id(key: &str) -> RenderType {
        Self::by_name(key).unwrap_or(RenderType::Integer)
    }
}

/// `ObjectiveCriteria`: identified by name (vanilla caches one instance per name).
#[derive(Debug, Clone)]
pub struct ObjectiveCriteria {
    name: Cow<'static, str>,
    read_only: bool,
    render_type: RenderType,
}

impl PartialEq for ObjectiveCriteria {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl Eq for ObjectiveCriteria {}
impl std::hash::Hash for ObjectiveCriteria {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

/// `StatType.getRegistry()` of each stat type: the registry whose entries the stat is about.
pub fn stat_type_registry(stat_type: &str) -> Option<&'static str> {
    Some(match stat_type {
        "mined" => "minecraft:block",
        "crafted" | "used" | "broken" | "picked_up" | "dropped" => "minecraft:item",
        "killed" | "killed_by" => "minecraft:entity_type",
        "custom" => "minecraft:custom_stat",
        _ => return None,
    })
}

impl ObjectiveCriteria {
    const fn custom(
        name: &'static str,
        read_only: bool,
        render_type: RenderType,
    ) -> ObjectiveCriteria {
        ObjectiveCriteria {
            name: Cow::Borrowed(name),
            read_only,
            render_type,
        }
    }

    pub const DUMMY: ObjectiveCriteria = Self::custom("dummy", false, RenderType::Integer);
    pub const TRIGGER: ObjectiveCriteria = Self::custom("trigger", false, RenderType::Integer);
    pub const DEATH_COUNT: ObjectiveCriteria =
        Self::custom("deathCount", false, RenderType::Integer);
    pub const KILL_COUNT_PLAYERS: ObjectiveCriteria =
        Self::custom("playerKillCount", false, RenderType::Integer);
    pub const KILL_COUNT_ALL: ObjectiveCriteria =
        Self::custom("totalKillCount", false, RenderType::Integer);
    pub const HEALTH: ObjectiveCriteria = Self::custom("health", true, RenderType::Hearts);
    pub const FOOD: ObjectiveCriteria = Self::custom("food", true, RenderType::Integer);
    pub const AIR: ObjectiveCriteria = Self::custom("air", true, RenderType::Integer);
    pub const ARMOR: ObjectiveCriteria = Self::custom("armor", true, RenderType::Integer);
    pub const EXPERIENCE: ObjectiveCriteria = Self::custom("xp", true, RenderType::Integer);
    pub const LEVEL: ObjectiveCriteria = Self::custom("level", true, RenderType::Integer);

    /// `TEAM_KILL.get(color)`.
    pub fn team_kill(color: TeamColor) -> ObjectiveCriteria {
        ObjectiveCriteria {
            name: Cow::Owned(format!("teamkill.{}", color.get_serialized_name())),
            read_only: false,
            render_type: RenderType::Integer,
        }
    }

    /// `KILLED_BY_TEAM.get(color)`.
    pub fn killed_by_team(color: TeamColor) -> ObjectiveCriteria {
        ObjectiveCriteria {
            name: Cow::Owned(format!("killedByTeam.{}", color.get_serialized_name())),
            read_only: false,
            render_type: RenderType::Integer,
        }
    }

    /// A stat criterion (`Stat` extends `ObjectiveCriteria`): `<type>:<value>` with `.` for `:`.
    pub fn stat(name: String) -> ObjectiveCriteria {
        ObjectiveCriteria {
            name: Cow::Owned(name),
            read_only: false,
            render_type: RenderType::Integer,
        }
    }

    /// `getName`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn get_default_render_type(&self) -> RenderType {
        self.render_type
    }

    fn custom_table() -> &'static HashMap<String, ObjectiveCriteria> {
        static TABLE: LazyLock<HashMap<String, ObjectiveCriteria>> = LazyLock::new(|| {
            let mut m = HashMap::new();
            for c in [
                ObjectiveCriteria::DUMMY,
                ObjectiveCriteria::TRIGGER,
                ObjectiveCriteria::DEATH_COUNT,
                ObjectiveCriteria::KILL_COUNT_PLAYERS,
                ObjectiveCriteria::KILL_COUNT_ALL,
                ObjectiveCriteria::HEALTH,
                ObjectiveCriteria::FOOD,
                ObjectiveCriteria::AIR,
                ObjectiveCriteria::ARMOR,
                ObjectiveCriteria::EXPERIENCE,
                ObjectiveCriteria::LEVEL,
            ] {
                m.insert(c.name.to_string(), c);
            }
            for color in TeamColor::VALUES {
                for c in [
                    ObjectiveCriteria::team_kill(color),
                    ObjectiveCriteria::killed_by_team(color),
                ] {
                    m.insert(c.name.to_string(), c);
                }
            }
            m
        });
        &TABLE
    }

    /// `getCustomCriteriaNames`.
    pub fn get_custom_criteria_names() -> Vec<String> {
        Self::custom_table().keys().cloned().collect()
    }

    /// `byName`: custom criteria, else `<stat_type>:<value>` (`Stat`, named by `Stat.buildName`).
    pub fn by_name(name: &str) -> Option<ObjectiveCriteria> {
        if let Some(c) = Self::custom_table().get(name) {
            return Some(c.clone());
        }
        let colon = name.find(':')?;
        let stat_type = Identifier::try_by_separator(&name[..colon], '.')?;
        let value = Identifier::try_by_separator(&name[colon + 1..], '.')?;
        let stat = crate::stats::stat::by_names(&stat_type, &value).ok()?;
        Some(ObjectiveCriteria::stat(crate::stats::stat::build_name(
            &stat,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        assert_eq!(
            ObjectiveCriteria::by_name("dummy"),
            Some(ObjectiveCriteria::DUMMY)
        );
        assert!(ObjectiveCriteria::by_name("health").unwrap().is_read_only());
        assert_eq!(
            ObjectiveCriteria::by_name("health")
                .unwrap()
                .get_default_render_type(),
            RenderType::Hearts
        );
        assert_eq!(
            ObjectiveCriteria::by_name("teamkill.red"),
            Some(ObjectiveCriteria::team_kill(TeamColor::Red))
        );
        assert_eq!(
            ObjectiveCriteria::by_name("killedByTeam.dark_blue")
                .unwrap()
                .get_name(),
            "killedByTeam.dark_blue"
        );
        assert_eq!(ObjectiveCriteria::by_name("nope"), None);
        assert_eq!(
            ObjectiveCriteria::by_name("minecraft.mined:minecraft.stone")
                .unwrap()
                .get_name(),
            "minecraft.mined:minecraft.stone"
        );
        assert_eq!(
            ObjectiveCriteria::by_name("mined:stone")
                .unwrap()
                .get_name(),
            "minecraft.mined:minecraft.stone",
            "named by `Stat.buildName`"
        );
        assert_eq!(
            ObjectiveCriteria::by_name("minecraft.mined:minecraft.nothing"),
            None
        );
        assert_eq!(
            ObjectiveCriteria::get_custom_criteria_names().len(),
            11 + 32
        );
        assert_eq!(RenderType::by_id("bogus"), RenderType::Integer);
    }
}
