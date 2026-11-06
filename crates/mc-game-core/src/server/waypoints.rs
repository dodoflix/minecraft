//! `net.minecraft.server.waypoints.ServerWaypointManager`: which players (receivers) see which transmitting entities
//! on their locator bar, and the connections that keep the client informed.
//!
//! Vanilla keeps `HashSet<WaypointTransmitter>`/`HashSet<ServerPlayer>` (entities hash to their id: the iteration order
//! is the `HashMap` bucket order, reproduced by [`IdHashSet`]) and a Guava `HashBasedTable` (linked maps: insertion order).

use crate::world::waypoints::{Connection, WaypointContext};

/// A `java.util.HashSet<Entity>`: entities hash to their id, so the iteration order is the bucket order of the
/// table (`(h ^ h >>> 16) & (capacity - 1)`, insertion order inside a bucket; the table only ever grows).
#[derive(Clone, Debug)]
pub struct IdHashSet {
    insertion_order: Vec<i32>,
    capacity: usize,
}

impl Default for IdHashSet {
    fn default() -> Self {
        Self {
            insertion_order: Vec::new(),
            capacity: 16,
        }
    }
}

impl IdHashSet {
    pub fn contains(&self, id: i32) -> bool {
        self.insertion_order.contains(&id)
    }

    /// `add(id)`: false when present.
    pub fn add(&mut self, id: i32) -> bool {
        if self.contains(id) {
            return false;
        }
        self.insertion_order.push(id);
        if self.insertion_order.len() > self.capacity / 4 * 3 {
            self.capacity *= 2;
        }
        true
    }

    /// `remove(id)`: true when it was present.
    pub fn remove(&mut self, id: i32) -> bool {
        match self.insertion_order.iter().position(|&i| i == id) {
            Some(index) => {
                self.insertion_order.remove(index);
                true
            }
            None => false,
        }
    }

    pub fn len(&self) -> usize {
        self.insertion_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.insertion_order.is_empty()
    }

    /// The elements in `HashSet` iteration order.
    pub fn iter_in_order(&self) -> Vec<i32> {
        let mask = self.capacity - 1;
        let mut ids = self.insertion_order.clone();
        // the sort is stable: equal buckets keep their insertion order (also across resizes)
        ids.sort_by_key(|&id| {
            let h = id as u32;
            ((h ^ (h >> 16)) as usize) & mask
        });
        ids
    }
}

/// `HashBasedTable<ServerPlayer, WaypointTransmitter, Connection>` (`LinkedHashMap` rows and cells).
#[derive(Default)]
struct ConnectionTable {
    rows: Vec<(i32, Vec<(i32, Connection)>)>,
}

impl ConnectionTable {
    fn get(&self, player: i32, waypoint: i32) -> Option<&Connection> {
        self.rows
            .iter()
            .find(|(p, _)| *p == player)?
            .1
            .iter()
            .find(|(w, _)| *w == waypoint)
            .map(|(_, c)| c)
    }

    fn get_mut(&mut self, player: i32, waypoint: i32) -> Option<&mut Connection> {
        self.rows
            .iter_mut()
            .find(|(p, _)| *p == player)?
            .1
            .iter_mut()
            .find(|(w, _)| *w == waypoint)
            .map(|(_, c)| c)
    }

    /// `put(player, waypoint, connection)`: an existing cell keeps its position.
    fn put(&mut self, player: i32, waypoint: i32, connection: Connection) {
        match self.rows.iter_mut().find(|(p, _)| *p == player) {
            Some((_, row)) => match row.iter_mut().find(|(w, _)| *w == waypoint) {
                Some(cell) => cell.1 = connection,
                None => row.push((waypoint, connection)),
            },
            None => self.rows.push((player, vec![(waypoint, connection)])),
        }
    }

    /// `remove(player, waypoint)`: an emptied row disappears.
    fn remove(&mut self, player: i32, waypoint: i32) -> Option<Connection> {
        let row_index = self.rows.iter().position(|(p, _)| *p == player)?;
        let row = &mut self.rows[row_index].1;
        let cell = row.iter().position(|(w, _)| *w == waypoint)?;
        let (_, connection) = row.remove(cell);
        if row.is_empty() {
            self.rows.remove(row_index);
        }
        Some(connection)
    }

    /// `row(player).keySet()` in order.
    fn row_keys(&self, player: i32) -> Vec<i32> {
        self.rows
            .iter()
            .find(|(p, _)| *p == player)
            .map(|(_, row)| row.iter().map(|(w, _)| *w).collect())
            .unwrap_or_default()
    }

    /// `column(waypoint).keySet()` in order (rows in insertion order).
    fn column_keys(&self, waypoint: i32) -> Vec<i32> {
        self.rows
            .iter()
            .filter(|(_, row)| row.iter().any(|(w, _)| *w == waypoint))
            .map(|(p, _)| *p)
            .collect()
    }

    fn has_cell(&self, player: i32, waypoint: i32) -> bool {
        self.get(player, waypoint).is_some()
    }
}

/// `ServerWaypointManager`
#[derive(Default)]
pub struct ServerWaypointManager {
    waypoints: IdHashSet,
    players: IdHashSet,
    connections: ConnectionTable,
}

impl ServerWaypointManager {
    /// `trackWaypoint(waypoint)`
    pub fn track_waypoint(&mut self, ctx: &dyn WaypointContext, waypoint: i32) {
        self.waypoints.add(waypoint);
        for player in self.players.iter_in_order() {
            self.create_connection(ctx, player, waypoint);
        }
    }

    /// `updateWaypoint(waypoint)`
    pub fn update_waypoint(&mut self, ctx: &dyn WaypointContext, waypoint: i32) {
        if !self.waypoints.contains(waypoint) {
            return;
        }
        for player in self.connections.column_keys(waypoint) {
            self.update_connection(ctx, player, waypoint);
        }
        // `Sets.difference(players, column keys)` is a live view: evaluated after the updates above
        for player in self.players.iter_in_order() {
            if !self.connections.has_cell(player, waypoint) {
                self.create_connection(ctx, player, waypoint);
            }
        }
    }

    /// `untrackWaypoint(waypoint)`: every connection of it disconnects.
    pub fn untrack_waypoint(&mut self, ctx: &dyn WaypointContext, waypoint: i32) {
        for player in self.connections.column_keys(waypoint) {
            if let Some(connection) = self.connections.remove(player, waypoint) {
                connection.disconnect(ctx);
            }
        }
        self.waypoints.remove(waypoint);
    }

    /// `addPlayer(player)`: `is_transmitting` is `player.isTransmittingWaypoint()`.
    pub fn add_player(&mut self, ctx: &dyn WaypointContext, player: i32, is_transmitting: bool) {
        self.players.add(player);
        for waypoint in self.waypoints.iter_in_order() {
            self.create_connection(ctx, player, waypoint);
        }
        if is_transmitting {
            self.track_waypoint(ctx, player);
        }
    }

    /// `updatePlayer(player)`
    pub fn update_player(&mut self, ctx: &dyn WaypointContext, player: i32) {
        for waypoint in self.connections.row_keys(player) {
            self.update_connection(ctx, player, waypoint);
        }
        for waypoint in self.waypoints.iter_in_order() {
            if !self.connections.has_cell(player, waypoint) {
                self.create_connection(ctx, player, waypoint);
            }
        }
    }

    /// `removePlayer(player)`
    pub fn remove_player(&mut self, ctx: &dyn WaypointContext, player: i32) {
        for waypoint in self.connections.row_keys(player) {
            if let Some(connection) = self.connections.remove(player, waypoint) {
                connection.disconnect(ctx);
            }
        }
        self.untrack_waypoint(ctx, player);
        self.players.remove(player);
    }

    /// `breakAllConnections()`
    pub fn break_all_connections(&mut self, ctx: &dyn WaypointContext) {
        for (_, row) in std::mem::take(&mut self.connections.rows) {
            for (_, connection) in row {
                connection.disconnect(ctx);
            }
        }
    }

    /// `remakeConnections(waypoint)`
    pub fn remake_connections(&mut self, ctx: &dyn WaypointContext, waypoint: i32) {
        for player in self.players.iter_in_order() {
            self.create_connection(ctx, player, waypoint);
        }
    }

    /// `transmitters()`: the tracked transmitters in `HashSet` order.
    pub fn transmitters(&self) -> Vec<i32> {
        self.waypoints.iter_in_order()
    }

    /// Whether `waypoint` is tracked (`transmitters().contains`).
    pub fn is_tracked(&self, waypoint: i32) -> bool {
        self.waypoints.contains(waypoint)
    }

    /// Whether `player` is a registered receiver.
    pub fn has_player(&self, player: i32) -> bool {
        self.players.contains(player)
    }

    /// `createConnection(player, waypoint)`
    fn create_connection(&mut self, ctx: &dyn WaypointContext, player: i32, waypoint: i32) {
        if player == waypoint || !ctx.locator_bar_enabled() {
            return;
        }
        match Self::make(ctx, waypoint, player) {
            Some(connection) => {
                self.connections.put(player, waypoint, connection.clone());
                connection.connect(ctx);
            }
            None => {
                if let Some(connection) = self.connections.remove(player, waypoint) {
                    connection.disconnect(ctx);
                }
            }
        }
    }

    /// `updateConnection(player, waypoint, connection)`
    fn update_connection(&mut self, ctx: &dyn WaypointContext, player: i32, waypoint: i32) {
        if player == waypoint || !ctx.locator_bar_enabled() {
            return;
        }
        let Some(connection) = self.connections.get_mut(player, waypoint) else {
            return;
        };
        if !connection.is_broken(ctx) {
            connection.update(ctx);
            return;
        }
        match Self::make(ctx, waypoint, player) {
            Some(new_connection) => {
                new_connection.connect(ctx);
                self.connections.put(player, waypoint, new_connection);
            }
            None => {
                if let Some(old) = self.connections.remove(player, waypoint) {
                    old.disconnect(ctx);
                }
            }
        }
    }

    /// `waypoint.makeWaypointConnectionWith(player)`
    fn make(ctx: &dyn WaypointContext, waypoint: i32, player: i32) -> Option<Connection> {
        let (source, receiver) = (ctx.transmitter(waypoint)?, ctx.receiver(player)?);
        Connection::make(ctx, &source, &receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::waypoints::{Icon, ReceiverView, TransmitterView};
    use mc_core::phys::Vec3;
    use mc_core::pos::{BlockPos, ChunkPos};
    use mc_protocol::packets::play::{
        ClientboundTrackedWaypointPacket, TrackedWaypointData, WaypointOperation,
    };
    use std::cell::RefCell;
    use uuid::Uuid;

    struct Ctx {
        transmitters: Vec<TransmitterView>,
        receivers: Vec<ReceiverView>,
        visible_radius: i32,
        sent: RefCell<Vec<(i32, ClientboundTrackedWaypointPacket)>>,
    }

    fn transmitter(id: i32, x: f64, z: f64) -> TransmitterView {
        TransmitterView {
            id,
            uuid: Uuid::from_u128(id as u128),
            position: Vec3::new(x, 64.0, z),
            block_position: BlockPos::new(x.floor() as i32, 64, z.floor() as i32),
            chunk_position: ChunkPos::new((x.floor() as i32) >> 4, (z.floor() as i32) >> 4),
            first_tick: false,
            spectator: false,
            transmit_range: 6.0e7,
            icon: Icon::default(),
            team_color: None,
        }
    }

    fn receiver(id: i32, x: f64, z: f64) -> ReceiverView {
        ReceiverView {
            id,
            position: Vec3::new(x, 64.0, z),
            spectator: false,
            receive_range: 6.0e7,
        }
    }

    impl WaypointContext for Ctx {
        fn transmitter(&self, id: i32) -> Option<TransmitterView> {
            self.transmitters.iter().find(|t| t.id == id).cloned()
        }
        fn receiver(&self, id: i32) -> Option<ReceiverView> {
            self.receivers.iter().find(|r| r.id == id).cloned()
        }
        fn is_chunk_visible(&self, receiver: i32, chunk: ChunkPos) -> bool {
            let r = self.receiver(receiver).unwrap();
            let (cx, cz) = (
                (r.position.x.floor() as i32) >> 4,
                (r.position.z.floor() as i32) >> 4,
            );
            (chunk.x - cx).abs().max((chunk.z - cz).abs()) <= self.visible_radius
        }
        fn has_indirect_passenger(&self, _source: i32, _receiver: i32) -> bool {
            false
        }
        fn send(&self, receiver: i32, packet: ClientboundTrackedWaypointPacket) {
            self.sent.borrow_mut().push((receiver, packet));
        }
        fn locator_bar_enabled(&self) -> bool {
            true
        }
    }

    fn ctx(transmitters: Vec<TransmitterView>, receivers: Vec<ReceiverView>) -> Ctx {
        Ctx {
            transmitters,
            receivers,
            visible_radius: 2,
            sent: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn hash_set_order_is_bucket_order() {
        let mut set = IdHashSet::default();
        for id in [33, 1, 17, 2] {
            set.add(id);
        }
        // buckets (capacity 16): 33 -> 1, 1 -> 1, 17 -> 1, 2 -> 2: insertion order inside bucket 1
        assert_eq!(set.iter_in_order(), vec![33, 1, 17, 2]);
        for id in 100..110 {
            set.add(id);
        }
        // 14 elements > 12: capacity 32 now: 33 -> 1, 1 -> 1, 17 -> 17, 2 -> 2
        let order = set.iter_in_order();
        assert_eq!(&order[..3], &[33, 1, 2]);
    }

    #[test]
    fn nearby_transmitter_gets_a_block_connection_and_updates_on_move() {
        let mut c = ctx(
            vec![transmitter(1, 8.0, 8.0), transmitter(2, 40.0, 8.0)],
            vec![receiver(2, 40.0, 8.0)],
        );
        let mut manager = ServerWaypointManager::default();
        manager.track_waypoint(&c, 1);
        manager.add_player(&c, 2, true);
        {
            let sent = c.sent.borrow();
            assert_eq!(sent.len(), 1);
            let (to, packet) = &sent[0];
            assert_eq!(*to, 2);
            assert_eq!(packet.operation, WaypointOperation::Track);
            assert_eq!(
                packet.waypoint.data,
                TrackedWaypointData::Vec3i(mc_core::pos::vec3i::Vec3i::new(8, 64, 8))
            );
        }
        c.transmitters[0] = transmitter(1, 9.5, 8.0);
        manager.update_waypoint(&c, 1);
        let sent = c.sent.borrow();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1.operation, WaypointOperation::Update);
        assert_eq!(
            sent[1].1.waypoint.data,
            TrackedWaypointData::Vec3i(mc_core::pos::vec3i::Vec3i::new(9, 64, 8))
        );
    }

    #[test]
    fn far_transmitter_is_an_azimuth_and_out_of_view_a_chunk() {
        let c = ctx(
            vec![transmitter(1, 0.0, 0.0), transmitter(3, 1000.0, 0.0)],
            vec![receiver(2, 100.0, 0.0)],
        );
        let mut manager = ServerWaypointManager::default();
        manager.add_player(&c, 2, false);
        manager.track_waypoint(&c, 1);
        manager.track_waypoint(&c, 3);
        let sent = c.sent.borrow();
        assert!(
            matches!(sent[0].1.waypoint.data, TrackedWaypointData::Chunk(_)),
            "100 blocks away: outside the view distance of 2 chunks"
        );
        assert!(
            matches!(sent[1].1.waypoint.data, TrackedWaypointData::Azimuth(_)),
            "900 blocks away: really far"
        );
    }

    #[test]
    fn untrack_and_remove_player_disconnect() {
        let c = ctx(vec![transmitter(1, 8.0, 8.0)], vec![receiver(2, 20.0, 8.0)]);
        let mut manager = ServerWaypointManager::default();
        manager.track_waypoint(&c, 1);
        manager.add_player(&c, 2, false);
        manager.untrack_waypoint(&c, 1);
        let sent = c.sent.borrow();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1.operation, WaypointOperation::Untrack);
        assert!(manager.transmitters().is_empty());
    }
}
