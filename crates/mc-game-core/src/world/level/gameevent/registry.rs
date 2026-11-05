use super::{DeliveryMode, ListenerOwner, PositionSource};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use uuid::Uuid;

/// A listener as the registry sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegisteredListener {
    pub owner: ListenerOwner,
    pub source: PositionSource,
    pub radius: i32,
    pub delivery: DeliveryMode,
}

/// `EuclideanGameEventListenerRegistry` (one per chunk section that has listeners).
#[derive(Default, Debug)]
pub struct EuclideanGameEventListenerRegistry {
    listeners: Vec<RegisteredListener>,
}

impl EuclideanGameEventListenerRegistry {
    pub fn is_empty(&self) -> bool {
        self.listeners.is_empty()
    }

    /// `register(listener)`.
    pub fn register(&mut self, listener: RegisteredListener) {
        self.listeners.push(listener);
    }

    /// `unregister(listener)`: removes the first listener of `owner`; returns whether the registry is now empty
    /// (the owner then drops the registry: `onEmptyAction`).
    pub fn unregister(&mut self, owner: ListenerOwner) -> bool {
        if let Some(i) = self.listeners.iter().position(|l| l.owner == owner) {
            self.listeners.remove(i);
        }
        self.listeners.is_empty()
    }

    pub fn contains(&self, owner: ListenerOwner) -> bool {
        self.listeners.iter().any(|l| l.owner == owner)
    }

    /// `visitInRangeListeners`: every listener whose source position is within its own radius of `source_position`
    /// (`BlockPos.distSqr > radius^2` rejects), as `(listener, its position)`, in registration order.
    pub fn in_range_listeners(
        &self,
        source_position: Vec3,
        entity_position: &dyn Fn(Uuid) -> Option<Vec3>,
    ) -> Vec<(RegisteredListener, Vec3)> {
        let origin = BlockPos::containing_position(&source_position);
        self.listeners
            .iter()
            .filter_map(|l| {
                let position = l.source.get_position(entity_position)?;
                let distance = BlockPos::containing_position(&position).dist_sqr(&origin);
                (distance <= f64::from(l.radius * l.radius)).then_some((*l, position))
            })
            .collect()
    }
}
