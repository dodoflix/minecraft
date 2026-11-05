//! Deferred `BlockEntity.setChanged()` notifications of block entities used as `Container`s.
//!
//! Vanilla's `setChanged()` calls `level.blockEntityChanged(pos)` and, for non-air states,
//! `level.updateNeighbourForOutputSignal(pos, block)` (comparators). `Container::set_changed` has no level at hand,
//! so a block entity container records its position here; whoever drives the container operation (hopper, dropper,
//! menus) calls [`flush`] right after, which replays the notifications in their original order.

use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use mc_core::pos::BlockPos;
use std::cell::RefCell;

thread_local! {
    static CHANGED: RefCell<Vec<BlockPos>> = const { RefCell::new(Vec::new()) };
}

/// Records `setChanged()` of the block entity at `pos`.
pub fn push(pos: BlockPos) {
    CHANGED.with(|c| c.borrow_mut().push(pos));
}

/// Whether `setChanged()` notifications are waiting for [`flush`].
pub fn has_pending() -> bool {
    CHANGED.with(|c| !c.borrow().is_empty())
}

/// Replays the recorded `setChanged()` calls (`BlockEntity.setChanged(level, pos, state)`), oldest first.
pub fn flush(level: &mut ServerLevel) {
    loop {
        let pending = CHANGED.with(|c| std::mem::take(&mut *c.borrow_mut()));
        if pending.is_empty() {
            return;
        }
        for pos in pending {
            set_changed(level, pos);
        }
    }
}

/// `BlockEntity.setChanged(level, pos, state)` with the state currently at `pos`.
fn set_changed(level: &mut ServerLevel, pos: BlockPos) {
    let state = BlockGetter::get_block_state(level, pos);
    super::set_changed(level, pos, state);
}
