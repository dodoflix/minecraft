//! `ScoreboardSaveData` (`data/minecraft/scoreboard.dat`) and the `Packed` records of objectives, scores and teams.

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_text::Component;

use super::number_format::NumberFormat;
use super::objective_criteria::{ObjectiveCriteria, RenderType};
use super::score::Score;
use super::team::{CollisionRule, DisplaySlot, TeamColor, Visibility};
use crate::impl_saved_data;
use crate::util::codec::{self, DataResult, DecodeResult, Partial};
use crate::world::level::saveddata::{SavedData, SavedDataType};

fn component(tag: &Tag) -> DataResult<Component> {
    Component::from_nbt(tag).map_err(|e| e.to_string())
}

fn enum_by_name<T>(tag: &Tag, by_name: fn(&str) -> Option<T>) -> DataResult<T> {
    let s = codec::string(tag)?;
    by_name(s).ok_or_else(|| format!("Unknown element name:{s}"))
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
    fn decode(tag: &Tag) -> DataResult<PackedObjective> {
        let m = codec::compound(tag)?;
        Ok(PackedObjective {
            name: codec::string(codec::field(m, "Name")?)?.to_string(),
            criteria: codec::optional(m, "CriteriaName", |t| {
                let n = codec::string(t)?;
                ObjectiveCriteria::by_name(n)
                    .ok_or_else(|| format!("No scoreboard criteria with name: {n}"))
            })?
            .unwrap_or(ObjectiveCriteria::DUMMY),
            display_name: component(codec::field(m, "DisplayName")?)?,
            render_type: codec::optional(m, "RenderType", |t| {
                enum_by_name(t, RenderType::by_name)
            })?
            .unwrap_or(RenderType::Integer),
            display_auto_update: codec::optional(m, "display_auto_update", codec::boolean)?
                .unwrap_or(false),
            number_format: codec::optional(m, "format", NumberFormat::decode)?,
        })
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put_string("Name", self.name.clone());
        if self.criteria != ObjectiveCriteria::DUMMY {
            m.put_string("CriteriaName", self.criteria.get_name());
        }
        m.put(
            "DisplayName",
            self.display_name.to_nbt().map_err(|e| e.to_string())?,
        );
        if self.render_type != RenderType::Integer {
            m.put_string("RenderType", self.render_type.get_id());
        }
        if self.display_auto_update {
            m.put_boolean("display_auto_update", true);
        }
        if let Some(f) = &self.number_format {
            m.put("format", f.encode()?);
        }
        Ok(Tag::Compound(m))
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
    fn decode(tag: &Tag) -> DataResult<PackedScore> {
        let m = codec::compound(tag)?;
        Ok(PackedScore {
            owner: codec::string(codec::field(m, "Name")?)?.to_string(),
            objective: codec::string(codec::field(m, "Objective")?)?.to_string(),
            score: Score::decode_fields(m)?,
        })
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put_string("Name", self.owner.clone());
        m.put_string("Objective", self.objective.clone());
        self.score.encode_fields(&mut m)?;
        Ok(Tag::Compound(m))
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
    fn decode(tag: &Tag) -> DataResult<PackedTeam> {
        let m = codec::compound(tag)?;
        let empty = Component::empty();
        Ok(PackedTeam {
            name: codec::string(codec::field(m, "Name")?)?.to_string(),
            display_name: codec::optional(m, "DisplayName", component)?,
            color: codec::optional(m, "TeamColor", |t| enum_by_name(t, TeamColor::by_name))?,
            allow_friendly_fire: codec::optional(m, "AllowFriendlyFire", codec::boolean)?
                .unwrap_or(true),
            see_friendly_invisibles: codec::optional(m, "SeeFriendlyInvisibles", codec::boolean)?
                .unwrap_or(true),
            member_name_prefix: codec::optional(m, "MemberNamePrefix", component)?
                .unwrap_or_else(|| empty.clone()),
            member_name_suffix: codec::optional(m, "MemberNameSuffix", component)?.unwrap_or(empty),
            name_tag_visibility: codec::optional(m, "NameTagVisibility", |t| {
                enum_by_name(t, Visibility::by_name)
            })?
            .unwrap_or(Visibility::Always),
            death_message_visibility: codec::optional(m, "DeathMessageVisibility", |t| {
                enum_by_name(t, Visibility::by_name)
            })?
            .unwrap_or(Visibility::Always),
            collision_rule: codec::optional(m, "CollisionRule", |t| {
                enum_by_name(t, CollisionRule::by_name)
            })?
            .unwrap_or(CollisionRule::Always),
            players: codec::optional(m, "Players", |t| {
                codec::list(t)?
                    .iter()
                    .map(|e| codec::string(e).map(str::to_string))
                    .collect::<DataResult<Vec<_>>>()
            })?
            .unwrap_or_default(),
        })
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put_string("Name", self.name.clone());
        if let Some(d) = &self.display_name {
            m.put("DisplayName", d.to_nbt().map_err(|e| e.to_string())?);
        }
        if let Some(c) = self.color {
            m.put_string("TeamColor", c.get_serialized_name());
        }
        if !self.allow_friendly_fire {
            m.put_boolean("AllowFriendlyFire", false);
        }
        if !self.see_friendly_invisibles {
            m.put_boolean("SeeFriendlyInvisibles", false);
        }
        let empty = Component::empty();
        if self.member_name_prefix != empty {
            m.put(
                "MemberNamePrefix",
                self.member_name_prefix
                    .to_nbt()
                    .map_err(|e| e.to_string())?,
            );
        }
        if self.member_name_suffix != empty {
            m.put(
                "MemberNameSuffix",
                self.member_name_suffix
                    .to_nbt()
                    .map_err(|e| e.to_string())?,
            );
        }
        if self.name_tag_visibility != Visibility::Always {
            m.put_string(
                "NameTagVisibility",
                self.name_tag_visibility.get_serialized_name(),
            );
        }
        if self.death_message_visibility != Visibility::Always {
            m.put_string(
                "DeathMessageVisibility",
                self.death_message_visibility.get_serialized_name(),
            );
        }
        if self.collision_rule != CollisionRule::Always {
            m.put_string("CollisionRule", self.collision_rule.get_serialized_name());
        }
        if !self.players.is_empty() {
            let mut list = ListTag::new();
            for p in &self.players {
                list.push(Tag::String(p.clone()));
            }
            m.put("Players", list);
        }
        Ok(Tag::Compound(m))
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

/// `listOf()` decode with vanilla's partial semantics: bad elements are dropped and reported.
fn decode_list<T>(
    m: &CompoundTag,
    name: &str,
    element: fn(&Tag) -> DataResult<T>,
    errors: &mut Vec<String>,
) -> DataResult<Vec<T>> {
    let Some(tag) = m.get(name) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for e in codec::list(tag)? {
        match element(&e) {
            Ok(v) => out.push(v),
            Err(err) => errors.push(format!("{err} missed input: {e}")),
        }
    }
    Ok(out)
}

fn encode_list<T>(
    m: &mut CompoundTag,
    name: &str,
    items: &[T],
    element: fn(&T) -> DataResult<Tag>,
) -> DataResult<()> {
    if !items.is_empty() {
        let mut list = ListTag::new();
        for i in items {
            list.push(element(i)?);
        }
        m.put(name, list);
    }
    Ok(())
}

impl ScoreboardSaveDataPacked {
    pub fn decode(tag: &Tag) -> DecodeResult<ScoreboardSaveDataPacked> {
        let m = codec::compound(tag)?;
        let mut errors = Vec::new();
        let objectives = decode_list(m, "Objectives", PackedObjective::decode, &mut errors)?;
        let scores = decode_list(m, "PlayerScores", PackedScore::decode, &mut errors)?;
        let teams = decode_list(m, "Teams", PackedTeam::decode, &mut errors)?;
        let mut display_slots = BTreeMap::new();
        if let Some(t) = m.get("DisplaySlots") {
            let slots = codec::unbounded_map(
                t,
                |k| DisplaySlot::by_name(k).ok_or_else(|| format!("Unknown element name:{k}")),
                |v| codec::string(v).map(str::to_string),
            );
            match slots {
                Ok(s) => display_slots.extend(s),
                Err(Partial { value, error }) => {
                    display_slots.extend(value.unwrap_or_default());
                    errors.push(error);
                }
            }
        }
        let packed = ScoreboardSaveDataPacked {
            objectives,
            scores,
            display_slots,
            teams,
        };
        if errors.is_empty() {
            Ok(packed)
        } else {
            Err(Partial {
                value: Some(packed),
                error: errors.join("; "),
            })
        }
    }

    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        encode_list(
            &mut m,
            "Objectives",
            &self.objectives,
            PackedObjective::encode,
        )?;
        encode_list(&mut m, "PlayerScores", &self.scores, PackedScore::encode)?;
        if !self.display_slots.is_empty() {
            let mut slots = CompoundTag::new();
            for (slot, name) in &self.display_slots {
                slots.put_string(slot.get_serialized_name(), name.clone());
            }
            m.put("DisplaySlots", slots);
        }
        encode_list(&mut m, "Teams", &self.teams, PackedTeam::encode)?;
        Ok(Tag::Compound(m))
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
    #[expect(
        clippy::result_large_err,
        reason = "`Partial<T>` carries the partial value of `DataResult.error(msg, partial)` inline, shared by every saved-data decoder"
    )]
    pub fn type_() -> SavedDataType<ScoreboardSaveData> {
        SavedDataType::new(
            Identifier::parse("minecraft:scoreboard").expect("static id"),
            ScoreboardSaveData::default,
            |tag| mc_game_map(ScoreboardSaveDataPacked::decode(tag)),
            |d| d.data.encode(),
        )
    }
}

#[expect(
    clippy::result_large_err,
    reason = "`Partial<T>` carries the partial value of `DataResult.error(msg, partial)` inline, shared by every saved-data decoder"
)]
fn mc_game_map(r: DecodeResult<ScoreboardSaveDataPacked>) -> DecodeResult<ScoreboardSaveData> {
    codec::map_decoded(r, ScoreboardSaveData::new)
}
