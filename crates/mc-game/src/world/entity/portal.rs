//! `Portal` (the block-side interface of nether/end portals and end gateways) and `PortalProcessor`, the per-entity
//! state machine of `Entity.handlePortal()`.
//!
//! A portal that decides to teleport cannot be resolved inside the entity's tick: the destination needs the server's
//! other levels (`getPortalDestination(level, entity, entryPos)`). `Entity::handle_portal` therefore queues the
//! entity in `ServerLevel::tick_state.pending_portal_teleports` and the level tick runs
//! [`LevelTickEnv::teleport_through_portal`](crate::server::level::level_tick::LevelTickEnv) right after the entity's
//! tick (vanilla teleports in the middle of `baseTick`).

use super::entity::Entity;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::players::dimension_change::TeleportTransition;
use mc_core::pos::BlockPos;

/// `Portal.Transition`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PortalTransition {
    Confusion,
    None,
}

/// `Portal`.
pub trait Portal: Send + Sync {
    /// `getPortalTransitionTime(level, entity)`.
    fn get_portal_transition_time(&self, _level: &ServerLevel, _entity: &dyn Entity) -> i32 {
        0
    }

    /// `getPortalDestination(currentLevel, entity, portalEntryPos)`; `server` gives access to the other levels.
    fn get_portal_destination(&self, server: &mut MinecraftServer, current_level: &mut ServerLevel, entity: &mut dyn Entity, portal_entry_pos: BlockPos) -> Option<TeleportTransition>;

    /// `getLocalTransition()`.
    fn get_local_transition(&self) -> PortalTransition {
        PortalTransition::None
    }
}

/// `PortalProcessor`.
#[derive(Clone)]
pub struct PortalProcessor {
    portal: &'static dyn Portal,
    entry_position: BlockPos,
    portal_time: i32,
    inside_portal_this_tick: bool,
}

impl PortalProcessor {
    pub fn new(portal: &'static dyn Portal, portal_entry_position: BlockPos) -> Self {
        Self { portal, entry_position: portal_entry_position, portal_time: 0, inside_portal_this_tick: true }
    }

    /// `processPortalTeleportation(serverLevel, entity, allowedToTeleport)`.
    pub fn process_portal_teleportation(&mut self, level: &ServerLevel, entity: &dyn Entity, allowed_to_teleport: bool) -> bool {
        if !self.inside_portal_this_tick {
            self.decay_tick();
            false
        } else {
            self.inside_portal_this_tick = false;
            allowed_to_teleport && {
                let old = self.portal_time;
                self.portal_time = self.portal_time.wrapping_add(1);
                old >= self.portal.get_portal_transition_time(level, entity)
            }
        }
    }

    /// `getPortalDestination(serverLevel, entity)`.
    pub fn get_portal_destination(&self, server: &mut MinecraftServer, level: &mut ServerLevel, entity: &mut dyn Entity) -> Option<TeleportTransition> {
        self.portal.get_portal_destination(server, level, entity, self.entry_position)
    }

    pub fn get_portal_local_transition(&self) -> PortalTransition {
        self.portal.get_local_transition()
    }

    fn decay_tick(&mut self) {
        self.portal_time = (self.portal_time - 4).max(0);
    }

    pub fn has_expired(&self) -> bool {
        self.portal_time <= 0
    }

    pub fn get_entry_position(&self) -> BlockPos {
        self.entry_position
    }

    pub fn update_entry_position(&mut self, entry_position: BlockPos) {
        self.entry_position = entry_position;
    }

    pub fn get_portal_time(&self) -> i32 {
        self.portal_time
    }

    pub fn is_inside_portal_this_tick(&self) -> bool {
        self.inside_portal_this_tick
    }

    pub fn set_as_inside_portal_this_tick(&mut self, inside_portal: bool) {
        self.inside_portal_this_tick = inside_portal;
    }

    pub fn is_same_portal(&self, portal: &dyn Portal) -> bool {
        std::ptr::addr_eq(self.portal as *const dyn Portal, portal as *const dyn Portal)
    }
}
