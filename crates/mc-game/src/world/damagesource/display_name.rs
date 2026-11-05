//! `Entity.getDisplayName()` (team-decorated name with hover/insertion) for live entities and [`DamageEntity`] snapshots.

use mc_data::EntityType;
use mc_text::Component;
use mc_text::events::{EntityTooltipInfo, HoverEvent};
use uuid::Uuid;

use super::damage_source::DamageEntity;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::scores::PlayerTeam;

fn decorate(level: &ServerLevel, name: &Component, entity_type: EntityType, uuid: Uuid, scoreboard_name: &str) -> Component {
    let formatted = {
        let scoreboard = level.scoreboard();
        PlayerTeam::format_name_for_team(scoreboard.get_players_team(scoreboard_name), name)
    };
    let hover = HoverEvent::ShowEntity(EntityTooltipInfo { entity_type: entity_type.identifier(), uuid: uuid.as_u128(), name: Some(Box::new(name.clone())) });
    formatted.with_style_fn(|s| s.with_hover_event(Some(hover.clone())).with_insertion(Some(uuid.to_string())))
}

/// `entity.getDisplayName()`.
pub fn entity_display_name(level: &ServerLevel, entity: &dyn Entity) -> Component {
    decorate(level, &crate::commands::host::entity_name(entity), entity.get_type(), entity.uuid(), &entity.get_scoreboard_name())
}

impl DamageEntity {
    /// `getDisplayName()` of the captured entity.
    pub fn display_name(&self, level: &ServerLevel) -> Component {
        decorate(level, &self.name, self.entity_type, self.uuid, &self.scoreboard_name)
    }
}
