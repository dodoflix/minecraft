//! `Score`, `Score.Packed`, `PlayerScoreEntry`, `ScoreHolder`.

use mc_nbt::CompoundTag;
use mc_text::{Component, HoverEvent};

use super::number_format::NumberFormat;
use crate::util::codec::{self, DataResult};

/// `Score`.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub(crate) value: i32,
    pub(crate) locked: bool,
    pub(crate) display: Option<Component>,
    pub(crate) number_format: Option<NumberFormat>,
}

impl Default for Score {
    fn default() -> Self {
        Score {
            value: 0,
            locked: true,
            display: None,
            number_format: None,
        }
    }
}

impl Score {
    pub fn value(&self) -> i32 {
        self.value
    }
    pub fn is_locked(&self) -> bool {
        self.locked
    }
    pub fn display(&self) -> Option<&Component> {
        self.display.as_ref()
    }
    pub fn number_format(&self) -> Option<&NumberFormat> {
        self.number_format.as_ref()
    }
    /// `ReadOnlyScoreInfo.formatValue(defaultFormat)`.
    pub fn format_value(&self, default_format: &NumberFormat) -> Component {
        self.number_format
            .as_ref()
            .unwrap_or(default_format)
            .format(self.value)
    }

    /// `Score.Packed.MAP_CODEC` decode (fields live in the surrounding compound).
    pub(crate) fn decode_fields(m: &CompoundTag) -> DataResult<Score> {
        Ok(Score {
            value: codec::optional(m, "Score", codec::int)?.unwrap_or(0),
            locked: codec::optional(m, "Locked", codec::boolean)?.unwrap_or(false),
            display: codec::optional(m, "display", |t| {
                Component::from_nbt(t).map_err(|e| e.to_string())
            })?,
            number_format: codec::optional(m, "format", NumberFormat::decode)?,
        })
    }

    /// `Score.Packed.MAP_CODEC` encode into `m` (defaults omitted).
    pub(crate) fn encode_fields(&self, m: &mut CompoundTag) -> DataResult<()> {
        if self.value != 0 {
            m.put_int("Score", self.value);
        }
        if self.locked {
            m.put_boolean("Locked", true);
        }
        if let Some(d) = &self.display {
            m.put("display", d.to_nbt().map_err(|e| e.to_string())?);
        }
        if let Some(f) = &self.number_format {
            m.put("format", f.encode()?);
        }
        Ok(())
    }
}

/// `PlayerScoreEntry`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerScoreEntry {
    pub owner: String,
    pub value: i32,
    pub display: Option<Component>,
    pub number_format_override: Option<NumberFormat>,
}

impl PlayerScoreEntry {
    pub fn is_hidden(&self) -> bool {
        self.owner.starts_with('#')
    }

    pub fn owner_name(&self) -> Component {
        self.display
            .clone()
            .unwrap_or_else(|| Component::literal(self.owner.clone()))
    }

    pub fn format_value(&self, default: &NumberFormat) -> Component {
        self.number_format_override
            .as_ref()
            .unwrap_or(default)
            .format(self.value)
    }
}

/// `ScoreHolder`: anything that can own scores (entities implement it in the entity code).
pub trait ScoreHolder {
    fn get_scoreboard_name(&self) -> String;

    fn get_display_name(&self) -> Option<Component> {
        None
    }

    fn get_feedback_display_name(&self) -> Component {
        match self.get_display_name() {
            Some(name) => name.with_style_fn(|s| {
                s.with_hover_event(Some(HoverEvent::show_text(Component::literal(
                    self.get_scoreboard_name(),
                ))))
            }),
            None => Component::literal(self.get_scoreboard_name()),
        }
    }
}

/// `ScoreHolder.forNameOnly(name)` (and `WILDCARD` for `*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameOnlyHolder(pub String);

impl NameOnlyHolder {
    pub const WILDCARD_NAME: &'static str = "*";
}

impl ScoreHolder for NameOnlyHolder {
    fn get_scoreboard_name(&self) -> String {
        self.0.clone()
    }

    fn get_feedback_display_name(&self) -> Component {
        if self.0 == Self::WILDCARD_NAME {
            Component::literal(self.get_scoreboard_name())
        } else {
            Component::literal(self.0.clone())
        }
    }
}

/// `ScoreHolder.forNameOnly`.
pub fn for_name_only(name: &str) -> NameOnlyHolder {
    NameOnlyHolder(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_defaults_omitted() {
        let mut m = CompoundTag::new();
        Score {
            value: 0,
            locked: false,
            display: None,
            number_format: None,
        }
        .encode_fields(&mut m)
        .unwrap();
        assert!(m.is_empty());
        let mut m = CompoundTag::new();
        Score::default().encode_fields(&mut m).unwrap();
        assert_eq!(
            mc_nbt::Tag::Compound(m.clone()),
            mc_nbt::snbt::parse("{Locked:1b}").unwrap()
        );
        assert_eq!(Score::decode_fields(&m).unwrap(), Score::default());
        assert!(!Score::decode_fields(&CompoundTag::new()).unwrap().locked);
    }
}
