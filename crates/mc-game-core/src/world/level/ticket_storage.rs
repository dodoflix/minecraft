//! `TicketStorage` (`world.level.TicketStorage`): the per-dimension chunk tickets, persisted as the saved data
//! `chunk_tickets` (only `persist` ticket types are written, active and deactivated ones alike).
//!
//! Vanilla notifies the loading/simulation chunk trackers synchronously through listener callbacks; here the
//! notifications are queued ([`ChunkUpdate`]) and the `DistanceManager` drains them at the start of its
//! `runAllUpdates` (the trackers only ever *read* chunk levels, which change inside `runAllUpdates`, so the result is
//! the same).
use crate::server::level::chunk_level;
use crate::server::level::fastutil::{LongOpenHashMap, LongOpenHashSet};
use crate::server::level::{FullChunkStatus, Ticket, TicketType};
use mc_core::pos::ChunkPos;
use mc_nbt::{CompoundTag, ListTag, Tag};

/// `TicketStorage.ChunkUpdated.update(node, newLevelFrom, onlyDecreased)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkUpdate {
    pub node: i64,
    pub new_level_from: i32,
    pub only_decreased: bool,
}

pub struct TicketStorage {
    tickets: LongOpenHashMap<Vec<Ticket>>,
    deactivated_tickets: LongOpenHashMap<Vec<Ticket>>,
    chunks_with_forced_tickets: LongOpenHashSet,
    pub(crate) loading_updates: Vec<ChunkUpdate>,
    pub(crate) simulation_updates: Vec<ChunkUpdate>,
    dirty: bool,
}

impl Default for TicketStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl TicketStorage {
    /// `new TicketStorage()`.
    pub fn new() -> Self {
        Self::with_tickets(LongOpenHashMap::with_expected(4), LongOpenHashMap::new())
    }

    fn with_tickets(
        tickets: LongOpenHashMap<Vec<Ticket>>,
        deactivated_tickets: LongOpenHashMap<Vec<Ticket>>,
    ) -> Self {
        let mut storage = Self {
            tickets,
            deactivated_tickets,
            chunks_with_forced_tickets: LongOpenHashSet::new(),
            loading_updates: Vec::new(),
            simulation_updates: Vec::new(),
            dirty: false,
        };
        storage.update_forced_chunks();
        storage
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty_to(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    fn set_dirty(&mut self) {
        self.dirty = true;
    }

    // ---------------------------------------------------------------- codec

    /// `TicketStorage.CODEC` encode: `{tickets: [{chunk_pos: [I;x,z], type, level, ticks_left?}, ...]}` (persisted
    /// types only; active tickets first, then deactivated, each in map order).
    pub fn to_tag(&self) -> Tag {
        let mut list = ListTag::new();
        let mut push = |pos: i64, ticket: &Ticket| {
            if ticket.get_type().persist() {
                let mut entry = CompoundTag::new();
                let chunk = ChunkPos::unpack(pos);
                entry.put_int_array("chunk_pos", vec![chunk.x, chunk.z]);
                ticket.write_fields(&mut entry);
                list.push(Tag::Compound(entry));
            }
        };
        for map in [&self.tickets, &self.deactivated_tickets] {
            for (pos, tickets) in map.iter() {
                for ticket in tickets {
                    push(pos, ticket);
                }
            }
        }
        let mut root = CompoundTag::new();
        // `optionalFieldOf("tickets", List.of())`: the default is omitted.
        if !list.is_empty() {
            root.put("tickets", list);
        }
        Tag::Compound(root)
    }

    /// `TicketStorage.CODEC` decode (`fromPacked`: everything loads as *deactivated* tickets). Malformed entries make
    /// the codec fail; the error text is returned for the saved-data layer to log (vanilla drops the whole value).
    pub fn from_tag(tag: &Tag) -> Result<TicketStorage, String> {
        let root = tag
            .as_compound()
            .ok_or_else(|| format!("Not a map: {tag}"))?;
        let mut deactivated: LongOpenHashMap<Vec<Ticket>> = LongOpenHashMap::new();
        if let Some(list) = root.get("tickets") {
            let Tag::List(list) = list else {
                return Err(format!("Not a list: {list}"));
            };
            for entry in list.iter() {
                let entry = entry
                    .as_compound()
                    .ok_or_else(|| format!("Not a map: {entry}"))?;
                let pos = match entry.get("chunk_pos") {
                    Some(Tag::IntArray(ints)) if ints.len() == 2 => ChunkPos::new(ints[0], ints[1]),
                    Some(Tag::IntArray(ints)) => {
                        return Err(format!(
                            "Input is not a list of 2 elements: {} elements",
                            ints.len()
                        ));
                    }
                    Some(_) => return Err("Not an int array".to_string()),
                    None => return Err("No key chunk_pos in MapLike[...]".to_string()),
                };
                let ticket = Ticket::read_fields(entry)?;
                deactivated
                    .get_or_insert_with(pos.pack(), || Vec::with_capacity(4))
                    .push(ticket);
            }
        }
        Ok(Self::with_tickets(
            LongOpenHashMap::with_expected(4),
            deactivated,
        ))
    }

    // ---------------------------------------------------------------- queries

    fn for_each_ticket(&self, mut output: impl FnMut(ChunkPos, &Ticket)) {
        for map in [&self.tickets, &self.deactivated_tickets] {
            for (key, tickets) in map.iter() {
                let pos = ChunkPos::unpack(key);
                for ticket in tickets {
                    output(pos, ticket);
                }
            }
        }
    }

    /// Every (chunk, ticket) pair, active then deactivated.
    pub fn all_tickets(&self) -> Vec<(ChunkPos, Ticket)> {
        let mut out = Vec::new();
        self.for_each_ticket(|pos, t| out.push((pos, t.clone())));
        out
    }

    pub fn has_tickets(&self) -> bool {
        !self.tickets.is_empty()
    }

    pub fn should_keep_dimension_active(&self) -> bool {
        self.tickets.values().any(|group| {
            group
                .iter()
                .any(|t| t.get_type().should_keep_dimension_active())
        })
    }

    pub fn get_tickets(&self, key: i64) -> &[Ticket] {
        self.tickets.get(key).map_or(&[], |v| v.as_slice())
    }

    pub fn get_ticket_level_at(&self, key: i64, simulation: bool) -> i32 {
        Self::ticket_level_at(self.get_tickets(key), simulation)
    }

    fn ticket_level_at(tickets: &[Ticket], simulation: bool) -> i32 {
        Self::lowest_ticket(tickets, simulation)
            .map_or(chunk_level::max_level() + 1, |t| t.get_ticket_level())
    }

    fn lowest_ticket(tickets: &[Ticket], simulation: bool) -> Option<&Ticket> {
        let mut t: Option<&Ticket> = None;
        for ticket in tickets {
            if t.is_none_or(|t| ticket.get_ticket_level() < t.get_ticket_level())
                && ((simulation && ticket.get_type().does_simulate())
                    || (!simulation && ticket.get_type().does_load()))
            {
                t = Some(ticket);
            }
        }
        t
    }

    pub fn get_ticket_debug_string(&self, key: i64, simulation: bool) -> String {
        Self::lowest_ticket(self.get_tickets(key), simulation)
            .map_or_else(|| "no_ticket".to_string(), |t| t.to_string())
    }

    pub fn get_force_loaded_chunks(&self) -> &LongOpenHashSet {
        &self.chunks_with_forced_tickets
    }

    // ---------------------------------------------------------------- mutation

    /// `activateAllDeactivatedTickets`.
    pub fn activate_all_deactivated_tickets(&mut self) {
        let mut pending: Vec<(i64, Ticket)> = Vec::new();
        for (key, tickets) in self.deactivated_tickets.iter() {
            for ticket in tickets {
                pending.push((key, ticket.clone()));
            }
        }
        for (key, ticket) in pending {
            self.add_ticket_at(key, ticket);
        }
        self.deactivated_tickets.clear();
    }

    pub fn add_ticket_with_radius(&mut self, ty: TicketType, chunk_pos: ChunkPos, radius: i32) {
        let ticket = Ticket::new(
            ty,
            chunk_level::by_full_status(FullChunkStatus::Full) - radius,
        );
        self.add_ticket_at(chunk_pos.pack(), ticket);
    }

    pub fn add_ticket(&mut self, ticket: Ticket, chunk_pos: ChunkPos) {
        self.add_ticket_at(chunk_pos.pack(), ticket);
    }

    /// `addTicket(long, Ticket)`: whether a new ticket was added (an identical one only has its timeout reset).
    pub fn add_ticket_at(&mut self, key: i64, ticket: Ticket) -> bool {
        let tickets = self
            .tickets
            .get_or_insert_with(key, || Vec::with_capacity(4));
        for t in tickets.iter_mut() {
            if is_ticket_same_type_and_level(&ticket, t) {
                t.reset_ticks_left();
                self.dirty = true;
                return false;
            }
        }
        let old_simulation = Self::ticket_level_at(tickets, true);
        let old_loading = Self::ticket_level_at(tickets, false);
        let (ty, level) = (ticket.get_type(), ticket.get_ticket_level());
        tickets.push(ticket);
        if ty.does_simulate() && level < old_simulation {
            self.simulation_updates.push(ChunkUpdate {
                node: key,
                new_level_from: level,
                only_decreased: true,
            });
        }
        if ty.does_load() && level < old_loading {
            self.loading_updates.push(ChunkUpdate {
                node: key,
                new_level_from: level,
                only_decreased: true,
            });
        }
        if ty == TicketType::FORCED {
            self.chunks_with_forced_tickets.add(key);
        }
        self.set_dirty();
        true
    }

    pub fn remove_ticket_with_radius(&mut self, ty: TicketType, chunk_pos: ChunkPos, radius: i32) {
        let ticket = Ticket::new(
            ty,
            chunk_level::by_full_status(FullChunkStatus::Full) - radius,
        );
        self.remove_ticket_at(chunk_pos.pack(), &ticket);
    }

    pub fn remove_ticket(&mut self, ticket: &Ticket, chunk_pos: ChunkPos) {
        self.remove_ticket_at(chunk_pos.pack(), ticket);
    }

    /// `removeTicket(long, Ticket)`.
    pub fn remove_ticket_at(&mut self, key: i64, ticket: &Ticket) -> bool {
        let Some(tickets) = self.tickets.get_mut(key) else {
            return false;
        };
        let Some(index) = tickets
            .iter()
            .position(|t| is_ticket_same_type_and_level(ticket, t))
        else {
            return false;
        };
        tickets.remove(index);
        let remaining_simulation = Self::ticket_level_at(tickets, true);
        let remaining_loading = Self::ticket_level_at(tickets, false);
        if tickets.is_empty() {
            self.tickets.remove(key);
        }
        if ticket.get_type().does_simulate() {
            self.simulation_updates.push(ChunkUpdate {
                node: key,
                new_level_from: remaining_simulation,
                only_decreased: false,
            });
        }
        if ticket.get_type().does_load() {
            self.loading_updates.push(ChunkUpdate {
                node: key,
                new_level_from: remaining_loading,
                only_decreased: false,
            });
        }
        if ticket.get_type() == TicketType::FORCED {
            self.update_forced_chunks();
        }
        self.set_dirty();
        true
    }

    fn update_forced_chunks(&mut self) {
        self.chunks_with_forced_tickets =
            self.get_all_chunks_with_ticket_that(|t| t.get_type() == TicketType::FORCED);
    }

    /// `purgeStaleTickets(chunkMap)`: `chunk_can_expire(chunkPos)` = `updatingChunk == null || isReadyForSaving()`.
    pub fn purge_stale_tickets(&mut self, chunk_can_expire: &dyn Fn(i64) -> bool) {
        self.remove_ticket_if(
            |ticket, chunk_pos| {
                if Self::can_ticket_expire(chunk_can_expire, ticket, chunk_pos) {
                    ticket.decrease_ticks_left();
                    ticket.is_timed_out()
                } else {
                    false
                }
            },
            false,
        );
        self.set_dirty();
    }

    fn can_ticket_expire(
        chunk_can_expire: &dyn Fn(i64) -> bool,
        ticket: &Ticket,
        chunk_pos: i64,
    ) -> bool {
        if !ticket.get_type().has_timeout() {
            false
        } else if ticket.get_type().can_expire_if_unloaded() {
            true
        } else {
            chunk_can_expire(chunk_pos)
        }
    }

    /// `deactivateTicketsOnClosing`.
    pub fn deactivate_tickets_on_closing(&mut self) {
        self.remove_ticket_if(|ticket, _| ticket.get_type() != TicketType::UNKNOWN, true);
    }

    /// `removeTicketIf(predicate, removedTickets)`; `move_to_deactivated` = `removedTickets == this.deactivatedTickets`.
    fn remove_ticket_if(
        &mut self,
        mut predicate: impl FnMut(&mut Ticket, i64) -> bool,
        move_to_deactivated: bool,
    ) {
        let mut removed_forced = false;
        let mut removed_for_deactivated: Vec<(i64, Ticket)> = Vec::new();
        let mut loading = Vec::new();
        let mut simulation = Vec::new();
        let mut dirty = false;
        self.tickets.drain_filter_in_order(|chunk_pos, tickets| {
            let mut removed_simulation = false;
            let mut removed_loading = false;
            let size_before = tickets.len();
            let mut kept = Vec::with_capacity(size_before);
            for mut ticket in std::mem::take(tickets) {
                if predicate(&mut ticket, chunk_pos) {
                    if ticket.get_type().does_load() {
                        removed_loading = true;
                    }
                    if ticket.get_type().does_simulate() {
                        removed_simulation = true;
                    }
                    if ticket.get_type() == TicketType::FORCED {
                        removed_forced = true;
                    }
                    if move_to_deactivated {
                        removed_for_deactivated.push((chunk_pos, ticket));
                    }
                } else {
                    kept.push(ticket);
                }
            }
            *tickets = kept;
            if removed_loading || removed_simulation {
                if removed_loading {
                    loading.push(ChunkUpdate {
                        node: chunk_pos,
                        new_level_from: Self::ticket_level_at(tickets, false),
                        only_decreased: false,
                    });
                }
                if removed_simulation {
                    simulation.push(ChunkUpdate {
                        node: chunk_pos,
                        new_level_from: Self::ticket_level_at(tickets, true),
                        only_decreased: false,
                    });
                }
                dirty = true;
                return tickets.is_empty();
            }
            false
        });
        self.loading_updates.extend(loading);
        self.simulation_updates.extend(simulation);
        for (pos, ticket) in removed_for_deactivated {
            self.deactivated_tickets
                .get_or_insert_with(pos, || Vec::with_capacity(4))
                .push(ticket);
        }
        if dirty {
            self.set_dirty();
        }
        if removed_forced {
            self.update_forced_chunks();
        }
    }

    /// `replaceTicketLevelOfType(newLevel, type)`.
    pub fn replace_ticket_level_of_type(&mut self, new_level: i32, ticket_type: TicketType) {
        let mut affected: Vec<(i64, Ticket)> = Vec::new();
        for (key, tickets) in self.tickets.iter() {
            for ticket in tickets {
                if ticket.get_type() == ticket_type {
                    affected.push((key, ticket.clone()));
                }
            }
        }
        for (key, ticket) in affected {
            self.remove_ticket_at(key, &ticket);
            self.add_ticket_at(key, Ticket::new(ticket.get_type(), new_level));
        }
    }

    /// `updateChunkForced(pos, forced)`: whether the forced state changed.
    pub fn update_chunk_forced(&mut self, chunk_pos: ChunkPos, forced: bool) -> bool {
        let ticket = Ticket::new(TicketType::FORCED, chunk_level::ENTITY_TICKING_LEVEL);
        if forced {
            self.add_ticket_at(chunk_pos.pack(), ticket)
        } else {
            self.remove_ticket_at(chunk_pos.pack(), &ticket)
        }
    }

    fn get_all_chunks_with_ticket_that(&self, check: impl Fn(&Ticket) -> bool) -> LongOpenHashSet {
        let mut chunks = LongOpenHashSet::new();
        for (key, tickets) in self.tickets.iter() {
            if tickets.iter().any(&check) {
                chunks.add(key);
            }
        }
        chunks
    }
}

fn is_ticket_same_type_and_level(ticket: &Ticket, t: &Ticket) -> bool {
    t.get_type() == ticket.get_type() && t.get_ticket_level() == ticket.get_ticket_level()
}

crate::impl_saved_data!(TicketStorage);

/// `TicketStorage.TYPE` (`chunk_tickets`).
#[expect(
    clippy::result_large_err,
    reason = "`Partial<T>` carries the partial value of `DataResult.error(msg, partial)` inline, shared by every saved-data decoder"
)]
pub fn saved_data_type() -> crate::world::level::saveddata::SavedDataType<TicketStorage> {
    use crate::util::codec::Partial;
    crate::world::level::saveddata::SavedDataType::new(
        mc_core::ident::Identifier::parse("minecraft:chunk_tickets").expect("valid id"),
        TicketStorage::new,
        |tag| TicketStorage::from_tag(tag).map_err(Partial::from),
        |storage| Ok(storage.to_tag()),
    )
}

impl TicketStorage {
    /// A copy for the saved data storage (all tickets as deactivated ones, which is also how they are written).
    pub fn snapshot_for_saving(&self) -> TicketStorage {
        TicketStorage::from_tag(&self.to_tag()).expect("own encoding decodes")
    }
}
