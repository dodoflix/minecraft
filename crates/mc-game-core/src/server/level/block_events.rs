//! Block events (`Level.blockEvent` / `ServerLevel.runBlockEvents`): `note block`, `piston`, `chest lid`... queue an
//! event that is delivered to `BlockBehaviour::trigger_event` later in the same tick and broadcast to nearby players
//! when the block accepted it. The queue is an insertion-ordered set (`ObjectLinkedOpenHashSet`): re-adding an
//! event that is already queued keeps its original position.

use mc_core::pos::BlockPos;
use mc_data::block::Block;
use std::collections::{HashSet, VecDeque};

/// `BlockEventData(pos, block, paramA, paramB)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockEventData {
    pub pos: BlockPos,
    pub block: Block,
    pub param_a: i32,
    pub param_b: i32,
}

/// `ObjectLinkedOpenHashSet<BlockEventData>` (+ the `blockEventsToReschedule` list).
#[derive(Default, Debug)]
pub struct BlockEvents {
    order: VecDeque<BlockEventData>,
    members: HashSet<BlockEventData>,
    to_reschedule: Vec<BlockEventData>,
}

impl BlockEvents {
    /// `blockEvents.add(event)`: false (no change) when an equal event is already queued.
    pub fn add(&mut self, event: BlockEventData) -> bool {
        if self.members.insert(event) {
            self.order.push_back(event);
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// `blockEvents.removeFirst()`.
    pub fn remove_first(&mut self) -> Option<BlockEventData> {
        let event = self.order.pop_front()?;
        self.members.remove(&event);
        Some(event)
    }

    /// `blockEventsToReschedule.add(event)`.
    pub fn reschedule(&mut self, event: BlockEventData) {
        self.to_reschedule.push(event);
    }

    /// `blockEventsToReschedule.clear()` (start of `runBlockEvents`).
    pub fn clear_reschedule(&mut self) {
        self.to_reschedule.clear();
    }

    /// `blockEvents.addAll(blockEventsToReschedule)` (end of `runBlockEvents`).
    pub fn add_rescheduled(&mut self) {
        for event in std::mem::take(&mut self.to_reschedule) {
            self.add(event);
        }
    }

    /// `blockEvents.removeIf(e -> bb.isInside(e.pos()))` (`clearBlockEvents`).
    pub fn remove_if(&mut self, mut test: impl FnMut(&BlockEventData) -> bool) {
        let members = &mut self.members;
        self.order.retain(|e| {
            let remove = test(e);
            if remove {
                members.remove(e);
            }
            !remove
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(x: i32, a: i32) -> BlockEventData {
        BlockEventData {
            pos: BlockPos::new(x, 64, 0),
            block: Block::NOTE_BLOCK,
            param_a: a,
            param_b: 0,
        }
    }

    #[test]
    fn duplicates_are_dropped_and_keep_first_position() {
        let mut q = BlockEvents::default();
        assert!(q.add(ev(1, 0)));
        assert!(q.add(ev(2, 0)));
        assert!(!q.add(ev(1, 0)));
        assert!(q.add(ev(1, 1)));
        assert_eq!(q.len(), 3);
        assert_eq!(q.remove_first(), Some(ev(1, 0)));
        // once removed the same event can be queued again (at the back)
        assert!(q.add(ev(1, 0)));
        let rest: Vec<_> = std::iter::from_fn(|| q.remove_first()).collect();
        assert_eq!(rest, [ev(2, 0), ev(1, 1), ev(1, 0)]);
    }

    #[test]
    fn rescheduled_events_are_appended_in_order() {
        let mut q = BlockEvents::default();
        q.clear_reschedule();
        q.reschedule(ev(3, 0));
        q.reschedule(ev(4, 0));
        q.add(ev(9, 9));
        q.add_rescheduled();
        let all: Vec<_> = std::iter::from_fn(|| q.remove_first()).collect();
        assert_eq!(all, [ev(9, 9), ev(3, 0), ev(4, 0)]);
    }

    #[test]
    fn remove_if_updates_membership() {
        let mut q = BlockEvents::default();
        q.add(ev(1, 0));
        q.add(ev(2, 0));
        q.remove_if(|e| e.pos.x() == 1);
        assert!(q.add(ev(1, 0)), "removed event is no longer a member");
        assert_eq!(q.len(), 2);
    }
}
