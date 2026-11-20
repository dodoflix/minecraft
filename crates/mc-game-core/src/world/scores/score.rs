//! `Score`, `Score.Packed`, `PlayerScoreEntry`, `ScoreHolder`.

use mc_dfu::{BOOL, INT, MapCodec};
use mc_protocol::components::component_codec;
use mc_text::{Component, HoverEvent};

use super::number_format::NumberFormat;

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

    /// `Score.Packed.MAP_CODEC` (the fields live in the surrounding map).
    pub(crate) fn map_codec() -> MapCodec<Score> {
        mc_dfu::record!(Score {
            value: INT.optional_field_of_default("Score", 0),
            locked: BOOL.optional_field_of_default("Locked", false),
            display: component_codec().optional_field_of("display"),
            number_format: NumberFormat::codec().optional_field_of("format"),
        })
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
    use crate::world::level::saveddata::{decode_codec, encode_codec};
    use mc_nbt::{CompoundTag, Tag};

    #[test]
    fn packed_defaults_omitted() {
        let codec = Score::map_codec().codec();
        let encode = |score: &Score| encode_codec(&codec, score).unwrap();
        let unlocked = Score {
            value: 0,
            locked: false,
            display: None,
            number_format: None,
        };
        assert_eq!(encode(&unlocked), Tag::Compound(CompoundTag::new()));
        assert_eq!(
            encode(&Score::default()),
            mc_nbt::snbt::parse("{Locked:1b}").unwrap()
        );
        let decode = |tag: &Tag| decode_codec(&codec, tag).map_err(|e| e.error).unwrap();
        assert_eq!(decode(&encode(&Score::default())), Score::default());
        assert!(!decode(&Tag::Compound(CompoundTag::new())).locked);
    }
}
