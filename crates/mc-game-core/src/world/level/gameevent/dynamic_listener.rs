use super::{DeliveryMode, ListenerOwner, PositionSource, RegisteredListener};
use crate::server::level::ServerLevel;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, SectionPos};

/// The `BiConsumer<DynamicGameEventListener, ServerLevel>` actions `ServerLevel.EntityCallbacks` hands to
/// `Entity.updateDynamicGameEventListener` (`DynamicGameEventListener::add` / `::remove` / `::move`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicListenerAction {
    Add,
    Remove,
    Move,
}

/// `DynamicGameEventListener`: keeps the registration of a listener that moves (an entity's) in the registry of the
/// chunk section it is in. The owning entity calls [`add`](Self::add) when it joins the level, [`move_to`](Self::move_to)
/// whenever its position may have changed section and [`remove`](Self::remove) when it leaves.
///
/// The entity passes its own position (`source.getPosition(level)`) because the level cannot find an entity that is
/// taken out of its slot while it ticks.
#[derive(Clone, Copy, Debug, Default)]
pub struct DynamicGameEventListener {
    last_section: Option<SectionPos>,
}

impl DynamicGameEventListener {
    pub fn new() -> Self {
        Self::default()
    }

    /// `add(level)`.
    pub fn add(
        &mut self,
        level: &mut ServerLevel,
        owner: ListenerOwner,
        source: PositionSource,
        radius: i32,
        delivery: DeliveryMode,
        position: Vec3,
    ) {
        self.move_to(level, owner, source, radius, delivery, position);
    }

    /// `remove(level)`.
    pub fn remove(&mut self, level: &mut ServerLevel, owner: ListenerOwner) {
        if let Some(section) = self.last_section {
            unregister(level, section, owner);
        }
    }

    /// Apply the action the level asked the owning entity for (`owner`'s position is `source.getPosition(level)`).
    pub fn apply(
        &mut self,
        action: DynamicListenerAction,
        level: &mut ServerLevel,
        owner: ListenerOwner,
        source: PositionSource,
        radius: i32,
        delivery: DeliveryMode,
        position: Vec3,
    ) {
        match action {
            DynamicListenerAction::Add => {
                self.add(level, owner, source, radius, delivery, position)
            }
            DynamicListenerAction::Remove => self.remove(level, owner),
            DynamicListenerAction::Move => {
                self.move_to(level, owner, source, radius, delivery, position)
            }
        }
    }

    /// `move(level)`: re-register when the position is in another section than the last one.
    pub fn move_to(
        &mut self,
        level: &mut ServerLevel,
        owner: ListenerOwner,
        source: PositionSource,
        radius: i32,
        delivery: DeliveryMode,
        position: Vec3,
    ) {
        let current_section = SectionPos::of_block_pos(&BlockPos::containing_position(&position));
        if self.last_section != Some(current_section) {
            if let Some(last) = self.last_section {
                unregister(level, last, owner);
            }
            self.last_section = Some(current_section);
            if let Some(chunk) = level.chunk_now_mut(current_section.x(), current_section.z()) {
                chunk
                    .game_event_registries
                    .entry(current_section.y())
                    .or_default()
                    .register(RegisteredListener {
                        owner,
                        source,
                        radius,
                        delivery,
                    });
            }
        }
    }
}

/// `ifChunkExists(level, section, registry -> registry.unregister(listener))` (an emptied registry is dropped).
fn unregister(level: &mut ServerLevel, section: SectionPos, owner: ListenerOwner) {
    if let Some(chunk) = level.chunk_now_mut(section.x(), section.z())
        && let Some(registry) = chunk.game_event_registries.get_mut(&section.y())
        && registry.unregister(owner)
    {
        chunk.game_event_registries.remove(&section.y());
    }
}
