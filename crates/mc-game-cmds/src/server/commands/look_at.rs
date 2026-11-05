//! `LookAt` (`LookAtEntity`, `LookAtPosition`): where `/teleport ... facing` and `/rotate ... facing` turn their targets.
use mc_game_core::commands::arguments::entity_anchor_argument::Anchor;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::{CommandSourceStack, host};
use mc_core::phys::Vec3;

#[derive(Clone)]
pub enum LookAt {
    Entity(EntityRef, Anchor),
    Position(Vec3),
}

impl LookAt {
    /// `perform(source, target)`.
    pub fn perform(&self, source: &CommandSourceStack, target: &EntityRef) {
        source.with_server(|server| match self {
            LookAt::Entity(entity, anchor) => host::look_at_entity(server, target, source.get_anchor(), entity, *anchor),
            LookAt::Position(pos) => host::look_at(server, target, source.get_anchor(), *pos),
        });
    }
}
