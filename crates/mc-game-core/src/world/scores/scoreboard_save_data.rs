//! `ScoreboardSaveData` (`data/minecraft/scoreboard.dat`) and the `Packed` records of objectives, scores and teams.

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_dfu::{BOOL, Codec, STRING, string_resolver};
use mc_nbt::Tag;
use mc_protocol::components::component_codec;
use mc_text::Component;

use super::number_format::NumberFormat;
use super::objective_criteria::{ObjectiveCriteria, RenderType};
use super::score::Score;
use super::team::{CollisionRule, DisplaySlot, TeamColor, Visibility};
use crate::impl_saved_data;
use crate::util::codec::{DataResult, DecodeResult};
use crate::world::level::saveddata::{SavedData, SavedDataType, decode_codec, encode_codec};

/// `StringRepresentable.fromEnum(values)`: the serialized name, "Unknown element name:x" otherwise.
fn enum_codec<E: Copy + 'static>(
    by_name: fn(&str) -> Option<E>,
    name: fn(E) -> &'static str,
) -> Codec<E> {
    string_resolver(move |e: &E| Some(name(*e).to_string()), by_name)
}

/// `Objective.Packed`.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedObjective {
    pub name: String,
    pub criteria: ObjectiveCriteria,
    pub display_name: Component,
    pub render_type: RenderType,
    pub display_auto_update: bool,
    pub number_format: Option<NumberFormat>,
}

impl PackedObjective {
    fn codec() -> Codec<PackedObjective> {
        mc_dfu::record!(PackedObjective {
            name: STRING.field_of("Name"),
            criteria: ObjectiveCriteria::codec()
                .optional_field_of_default("CriteriaName", ObjectiveCriteria::DUMMY),
            display_name: component_codec().field_of("DisplayName"),
            render_type: enum_codec(RenderType::by_name, RenderType::get_id)
                .optional_field_of_default("RenderType", RenderType::Integer),
            display_auto_update: BOOL.optional_field_of_default("display_auto_update", false),
            number_format: NumberFormat::codec().optional_field_of("format"),
        })
        .codec()
    }
}

/// `Scoreboard.PackedScore`.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedScore {
    pub owner: String,
    pub objective: String,
    pub score: Score,
}

impl PackedScore {
    fn codec() -> Codec<PackedScore> {
        mc_dfu::record!(PackedScore {
            owner: STRING.field_of("Name"),
            objective: STRING.field_of("Objective"),
            score: Score::map_codec(),
        })
        .codec()
    }
}

/// `PlayerTeam.Packed`.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedTeam {
    pub name: String,
    pub display_name: Option<Component>,
    pub color: Option<TeamColor>,
    pub allow_friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub member_name_prefix: Component,
    pub member_name_suffix: Component,
    pub name_tag_visibility: Visibility,
    pub death_message_visibility: Visibility,
    pub collision_rule: CollisionRule,
    pub players: Vec<String>,
}

impl PackedTeam {
    fn codec() -> Codec<PackedTeam> {
        let visibility = || enum_codec(Visibility::by_name, Visibility::get_serialized_name);
        mc_dfu::record!(PackedTeam {
            name: STRING.field_of("Name"),
            display_name: component_codec().optional_field_of("DisplayName"),
            color: enum_codec(TeamColor::by_name, TeamColor::get_serialized_name)
                .optional_field_of("TeamColor"),
            allow_friendly_fire: BOOL.optional_field_of_default("AllowFriendlyFire", true),
            see_friendly_invisibles: BOOL.optional_field_of_default("SeeFriendlyInvisibles", true),
            member_name_prefix: component_codec()
                .optional_field_of_default("MemberNamePrefix", Component::empty()),
            member_name_suffix: component_codec()
                .optional_field_of_default("MemberNameSuffix", Component::empty()),
            name_tag_visibility: visibility()
                .optional_field_of_default("NameTagVisibility", Visibility::Always),
            death_message_visibility: visibility()
                .optional_field_of_default("DeathMessageVisibility", Visibility::Always),
            collision_rule: enum_codec(CollisionRule::by_name, CollisionRule::get_serialized_name)
                .optional_field_of_default("CollisionRule", CollisionRule::Always),
            players: STRING
                .list_of()
                .optional_field_of_default("Players", Vec::new()),
        })
        .codec()
    }
}

/// `ScoreboardSaveData.Packed`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScoreboardSaveDataPacked {
    pub objectives: Vec<PackedObjective>,
    pub scores: Vec<PackedScore>,
    pub display_slots: BTreeMap<DisplaySlot, String>,
    pub teams: Vec<PackedTeam>,
}

impl ScoreboardSaveDataPacked {
    /// `ScoreboardSaveData.Packed.CODEC`.
    pub fn codec() -> Codec<ScoreboardSaveDataPacked> {
        let slots = enum_codec(DisplaySlot::by_name, DisplaySlot::get_serialized_name)
            .unbounded_map(&STRING)
            .xmap(
                |entries| entries.into_iter().collect::<BTreeMap<_, _>>(),
                |slots: &BTreeMap<DisplaySlot, String>| {
                    slots.iter().map(|(k, v)| (*k, v.clone())).collect()
                },
            );
        mc_dfu::record!(ScoreboardSaveDataPacked {
            objectives: PackedObjective::codec()
                .list_of()
                .optional_field_of_default("Objectives", Vec::new()),
            scores: PackedScore::codec()
                .list_of()
                .optional_field_of_default("PlayerScores", Vec::new()),
            display_slots: slots.optional_field_of_default("DisplaySlots", BTreeMap::new()),
            teams: PackedTeam::codec()
                .list_of()
                .optional_field_of_default("Teams", Vec::new()),
        })
        .codec()
    }

    pub fn decode(tag: &Tag) -> DecodeResult<ScoreboardSaveDataPacked> {
        decode_codec(&ScoreboardSaveDataPacked::codec(), tag)
    }

    pub fn encode(&self) -> DataResult<Tag> {
        encode_codec(&ScoreboardSaveDataPacked::codec(), self)
    }
}

/// `ScoreboardSaveData`.
#[derive(Debug, Default)]
pub struct ScoreboardSaveData {
    dirty: bool,
    data: ScoreboardSaveDataPacked,
}

impl_saved_data!(ScoreboardSaveData);

impl ScoreboardSaveData {
    pub fn new(data: ScoreboardSaveDataPacked) -> ScoreboardSaveData {
        ScoreboardSaveData { dirty: false, data }
    }

    pub fn get_data(&self) -> &ScoreboardSaveDataPacked {
        &self.data
    }

    pub fn set_data(&mut self, data: ScoreboardSaveDataPacked) {
        if data != self.data {
            self.data = data;
            self.set_dirty();
        }
    }

    /// `ScoreboardSaveData.TYPE`.
    pub fn type_() -> SavedDataType<ScoreboardSaveData> {
        SavedDataType::from_codec(
            Identifier::parse("minecraft:scoreboard").expect("static id"),
            ScoreboardSaveData::default,
            ScoreboardSaveDataPacked::codec()
                .xmap(ScoreboardSaveData::new, |d: &ScoreboardSaveData| {
                    d.data.clone()
                }),
            mc_datafix::DataFixTypes::SAVED_DATA_SCOREBOARD,
        )
    }
}
