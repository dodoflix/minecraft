//! `net.minecraft.advancements.Advancement` + `AdvancementHolder`.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{BOOL, Codec, DataResult, STRING};
use mc_text::{Component, HoverEvent, Style, util};

use super::advancement_requirements::AdvancementRequirements;
use super::advancement_rewards::AdvancementRewards;
use super::display_info::DisplayInfo;
use super::java_hash::JavaHash;
use super::triggers::Criterion;
use crate::world::level::storage::loot::codec_ext::OptionalDefaultWith;
use crate::world::level::storage::loot::validation::{ProblemCollector, ValidationContextSource};

#[derive(Clone)]
pub struct Advancement {
    pub parent: Option<Identifier>,
    pub display: Option<DisplayInfo>,
    pub rewards: AdvancementRewards,
    /// `Map<String, Criterion<?>>` in data pack file order (vanilla's `Map.copyOf` order is unspecified).
    pub criteria: Vec<(String, Criterion)>,
    pub requirements: AdvancementRequirements,
    pub sends_telemetry_event: bool,
    /// `decorateName(display)`.
    pub name: Option<Component>,
}

/// The parsed form of the record codec before `requirements` defaults to `allOf(criteria)`.
#[derive(Clone)]
struct Raw {
    parent: Option<Identifier>,
    display: Option<DisplayInfo>,
    rewards: AdvancementRewards,
    criteria: Vec<(String, Criterion)>,
    requirements: Option<AdvancementRequirements>,
    sends_telemetry_event: bool,
}

fn decorate_name(display: &DisplayInfo) -> Component {
    let display_title = display.title.clone();
    let color = display.type_.chat_color();
    let tooltip = util::merge_styles(display_title.copy(), &Style::EMPTY.with_color_formatting(Some(color)))
        .append(Component::literal("\n"))
        .append(display.description.clone());
    let title = display_title.copy().with_style_fn(|s| s.with_hover_event(Some(HoverEvent::show_text(tooltip))));
    util::wrap_in_square_brackets(title).with_format(color)
}

impl Advancement {
    /// `new Advancement(parent, display, rewards, criteria, requirements, sendsTelemetryEvent)`.
    pub fn new(
        parent: Option<Identifier>,
        display: Option<DisplayInfo>,
        rewards: AdvancementRewards,
        criteria: Vec<(String, Criterion)>,
        requirements: AdvancementRequirements,
        sends_telemetry_event: bool,
    ) -> Advancement {
        let name = display.as_ref().map(decorate_name);
        Advancement { parent, display, rewards, criteria, requirements, sends_telemetry_event, name }
    }

    /// `Advancement.CODEC` (including `validate`).
    pub fn codec() -> Codec<Advancement> {
        static C: LazyLock<Codec<Advancement>> = LazyLock::new(|| {
            let criteria_codec = STRING.unbounded_map(&Criterion::codec()).validate(|criteria| {
                if criteria.is_empty() {
                    DataResult::error(|| "Advancement criteria cannot be empty".to_owned())
                } else {
                    DataResult::success(criteria)
                }
            });
            mc_dfu::record!(Raw {
                parent: IDENTIFIER.optional_field_of("parent"),
                display: DisplayInfo::codec().optional_field_of("display"),
                rewards: AdvancementRewards::codec().optional_field_of_default_with("rewards", AdvancementRewards::empty()),
                criteria: criteria_codec.field_of("criteria"),
                requirements: AdvancementRequirements::codec().optional_field_of("requirements"),
                sends_telemetry_event: BOOL.optional_field_of_default("sends_telemetry_event", false),
            })
            .codec()
            .xmap(
                |raw: Raw| {
                    let requirements = raw.requirements.unwrap_or_else(|| AdvancementRequirements::all_of(raw.criteria.iter().map(|(k, _)| k)));
                    Advancement::new(raw.parent, raw.display, raw.rewards, raw.criteria, requirements, raw.sends_telemetry_event)
                },
                |a: &Advancement| Raw {
                    parent: a.parent.clone(),
                    display: a.display.clone(),
                    rewards: a.rewards.clone(),
                    criteria: a.criteria.clone(),
                    requirements: Some(a.requirements.clone()),
                    sends_telemetry_event: a.sends_telemetry_event,
                },
            )
            .validate(Advancement::validate_shape)
        });
        C.clone()
    }

    fn validate_shape(advancement: Advancement) -> DataResult<Advancement> {
        let is_root = advancement.parent.is_none();
        let is_visible = advancement.display.is_some();
        let has_background = advancement.display.as_ref().is_some_and(|d| d.background.is_some());
        if is_root && is_visible && !has_background {
            return DataResult::error(|| "Visible advancement roots must have background".to_owned());
        }
        if !is_root && has_background {
            return DataResult::error(|| "Only advancement roots can have background".to_owned());
        }
        let keys: Vec<String> = advancement.criteria.iter().map(|(k, _)| k.clone()).collect();
        match advancement.requirements.validate(&keys) {
            Ok(()) => DataResult::success(advancement),
            Err(m) => DataResult::error(move || m.clone()),
        }
    }

    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// `validate(reporter, lootData)`: every criterion's trigger instance against `loot_data`; the problems as
    /// `(path, description)` with the criterion name as the path root.
    pub fn validate(&self, loot_data: &crate::registry::RegistryAccess) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for (name, criterion) in &self.criteria {
            let problems = ProblemCollector::new();
            criterion.trigger_instance.validate(&ValidationContextSource::new(&problems, loot_data));
            problems.for_each(|path, problem| found.push((format!("{name}{path}"), problem.description.clone())));
        }
        found
    }

    pub fn criterion(&self, name: &str) -> Option<&Criterion> {
        self.criteria.iter().find(|(k, _)| k == name).map(|(_, c)| c)
    }

    /// `ClientboundUpdateAdvancementsPacket` form (`Advancement.STREAM_CODEC`).
    pub fn to_network(&self) -> mc_protocol::packets::play::Advancement {
        mc_protocol::packets::play::Advancement {
            parent: self.parent.clone(),
            display: self.display.as_ref().map(DisplayInfo::to_network),
            requirements: (&self.requirements).into(),
            sends_telemetry_event: self.sends_telemetry_event,
        }
    }
}

/// `Advancement.name(holder)`.
pub fn name(holder: &AdvancementHolder) -> Component {
    holder.value.name.clone().unwrap_or_else(|| Component::literal(holder.id.to_string()))
}

/// `AdvancementHolder`: equality and hash are the id's.
#[derive(Clone)]
pub struct AdvancementHolder {
    pub id: Identifier,
    pub value: Arc<Advancement>,
}

impl AdvancementHolder {
    pub fn new(id: Identifier, value: Arc<Advancement>) -> Self {
        AdvancementHolder { id, value }
    }

    pub fn id(&self) -> &Identifier {
        &self.id
    }

    pub fn value(&self) -> &Advancement {
        &self.value
    }
}

impl PartialEq for AdvancementHolder {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for AdvancementHolder {}

impl Hash for AdvancementHolder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl JavaHash for AdvancementHolder {
    fn java_hash(&self) -> i32 {
        self.id.java_hash()
    }
}

impl std::fmt::Display for AdvancementHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl std::fmt::Debug for AdvancementHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}
