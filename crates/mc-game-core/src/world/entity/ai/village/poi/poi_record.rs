//! `PoiRecord` (and `PoiRecord.Packed`, the same three fields): a point of interest and its free ticket count.

use super::poi_type::PoiTypeHolder;
use mc_core::pos::BlockPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoiRecord {
    pos: BlockPos,
    poi_type: PoiTypeHolder,
    free_tickets: i32,
}

impl PoiRecord {
    /// `new PoiRecord(pos, type, setDirty)`: all tickets free.
    pub fn new(pos: BlockPos, poi_type: PoiTypeHolder) -> Self {
        Self { pos, poi_type, free_tickets: poi_type.max_tickets() }
    }

    /// `PoiRecord.Packed.unpack` / the private constructor with explicit tickets.
    pub fn with_free_tickets(pos: BlockPos, poi_type: PoiTypeHolder, free_tickets: i32) -> Self {
        Self { pos, poi_type, free_tickets }
    }

    /// `getFreeTickets()`.
    pub fn get_free_tickets(&self) -> i32 {
        self.free_tickets
    }

    /// `acquireTicket()` (the caller marks the section dirty when this returns `true`).
    pub(super) fn acquire_ticket(&mut self) -> bool {
        if self.free_tickets <= 0 {
            false
        } else {
            self.free_tickets -= 1;
            true
        }
    }

    /// `releaseTicket()` (the caller marks the section dirty when this returns `true`).
    pub(super) fn release_ticket(&mut self) -> bool {
        if self.free_tickets >= self.poi_type.max_tickets() {
            false
        } else {
            self.free_tickets += 1;
            true
        }
    }

    pub fn has_space(&self) -> bool {
        self.free_tickets > 0
    }

    pub fn is_occupied(&self) -> bool {
        self.free_tickets != self.poi_type.max_tickets()
    }

    pub fn get_pos(&self) -> BlockPos {
        self.pos
    }

    pub fn get_poi_type(&self) -> PoiTypeHolder {
        self.poi_type
    }
}
