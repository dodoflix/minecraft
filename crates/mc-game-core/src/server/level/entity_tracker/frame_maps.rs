//! The item frame branch of `ServerEntity.sendChanges`: every ten ticks a frame that holds a filled map lets every player
//! of the level tick that map (`MapItemSavedData.tickCarriedBy` with the frame, which records the frame marker) and sends
//! the pending map update to each of them.

use super::{Entities, send};
use crate::server::level::EntityDirectory;
use crate::server::level::PlayerRecord;
use crate::server::level::server_entity::encode_payload;
use crate::world::entity::decoration::item_frame::framed_map_of;
use crate::world::item::map_item::{player_view, remote_player_view};
use crate::world::level::DimensionKey;
use crate::world::level::saveddata::maps::{MapItemSavedData, MapStorage, MapTickEnv};
use mc_protocol::components::ItemStack;

/// What `tickCarriedBy` reads from the level the tracker belongs to.
pub struct FrameMapEnv<'a> {
    pub map_storage: &'a MapStorage,
    pub dimension: &'a DimensionKey,
    /// The players of the other levels.
    pub directory: &'a EntityDirectory,
    /// `Level.getGameTime()`.
    pub game_time: i64,
}

/// `ServerEntity.sendChanges` for the frame `frame_id` on a tick of its ten-tick period: nothing unless it holds a map that
/// has saved data. Players that are not in `entities` (taken out of the level while they act) are skipped.
pub(super) fn sync_framed_map(
    entities: &Entities<'_>,
    players: &[PlayerRecord],
    env: &FrameMapEnv<'_>,
    frame_id: i32,
) {
    let Some((map_id, stack, frame)) = entities.get_any(frame_id).and_then(framed_map_of) else {
        return;
    };
    let matcher = MapItemSavedData::matcher_for(&stack);
    env.map_storage.with(|storage| {
        let Some(data) = storage.get(&MapItemSavedData::type_(map_id)) else {
            return;
        };
        for player in players {
            let view = |e: &dyn crate::world::entity::Entity, m: &dyn Fn(&ItemStack) -> bool| {
                player_view(e, env.dimension, m)
            };
            let Some(ticking) = entities.get_any(player.id).and_then(|e| view(e, &matcher)) else {
                continue;
            };
            // a holder in another level is still a holder (a player is only `isRemoved` once it left the server)
            let other = |id, m: &dyn Fn(&ItemStack) -> bool| {
                entities.get_any(id).and_then(|e| view(e, m)).or_else(|| {
                    env.directory
                        .get_by_id(id)
                        .and_then(|remote| remote_player_view(&remote, m))
                })
            };
            data.tick_carried_by(
                &MapTickEnv {
                    game_time: env.game_time,
                    ticking,
                    other: &other,
                },
                &stack,
                Some(frame),
            );
            if let Some(packet) = data.get_update_packet(map_id, player.id) {
                send(&player.connection, &encode_payload(&packet));
            }
        }
    });
}
