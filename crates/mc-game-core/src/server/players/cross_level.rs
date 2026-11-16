//! Work on a player or a level that another level of the server holds, for code that runs inside one level (an arrow of
//! the overworld whose owner is in the nether). The other level and the running level cannot both be borrowed from there:
//! the work is queued as a server action, which the server runs once the running entity's tick is over.

use super::ender_pearl_ticket::{deliver, with_player_here};
use crate::advancements::triggers::TriggerMatches;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::DimensionKey;
use uuid::Uuid;

/// Like [`deliver`](super::ender_pearl_ticket::deliver), and `f` also gets `origin`, the level of the running action
/// (`None` when the player has come to that level meanwhile: `level` is the origin then).
pub fn deliver_from(
    server: &mut MinecraftServer,
    origin: &mut ServerLevel,
    uuid: Uuid,
    f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer, Option<&mut ServerLevel>) + Send + 'static,
) {
    let Some(dimension) = origin
        .server_state
        .entities
        .with(uuid, |remote| remote.dimension.clone())
        .or_else(|| {
            origin
                .get_player_in_any_dimension(uuid)
                .map(|handle| handle.dimension())
        })
    else {
        return;
    };
    if dimension == origin.dimension {
        with_player_here(origin, uuid, |level, player| f(level, player, None));
        return;
    }
    let mut f = Some(f);
    let reached = server.with_level(&dimension, |_, level| {
        let f = f.take().expect("runs once");
        with_player_here(level, uuid, |level, player| f(level, player, Some(origin)))
    });
    if reached.is_none()
        && let Some(f) = f
    {
        origin.defer_server_action(move |server, origin| deliver_from(server, origin, uuid, f));
    }
}

/// `server.getLevel(dimension)` work for code that runs inside one level (a villager's behavior reading or releasing a
/// point of interest of another level): `f` runs on that level when the server runs the queue, then `then` runs on the
/// running level with its result. Nothing runs when the server has no such level; a level that is held up the stack
/// (taken out of the server) is retried the next time the running level's queue runs.
pub fn run_in_level<R: Send + 'static>(
    level: &mut ServerLevel,
    dimension: DimensionKey,
    f: impl FnOnce(&mut ServerLevel) -> R + Send + 'static,
    then: impl FnOnce(&mut ServerLevel, R) + Send + 'static,
) {
    level.queue_server_action(move |server, origin| {
        run_in_level_now(server, origin, dimension, f, then);
    });
}

fn run_in_level_now<R: Send + 'static>(
    server: &mut MinecraftServer,
    origin: &mut ServerLevel,
    dimension: DimensionKey,
    f: impl FnOnce(&mut ServerLevel) -> R + Send + 'static,
    then: impl FnOnce(&mut ServerLevel, R) + Send + 'static,
) {
    if !server.level_keys().contains(&dimension) {
        return;
    }
    let mut f = Some(f);
    let reached = server.with_level(&dimension, |_, other| (f.take().expect("runs once"))(other));
    match reached {
        Some(result) => then(origin, result),
        None => {
            let f = f.expect("not run");
            origin.defer_server_action(move |server, origin| {
                run_in_level_now(server, origin, dimension, f, then);
            });
        }
    }
}

/// `f` on the player `uuid` in whichever level it is, once the server runs the queue (see [`deliver`]).
pub fn queue_for_player(
    level: &mut ServerLevel,
    uuid: Uuid,
    f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) + Send + 'static,
) {
    level.queue_server_action(move |server, level| deliver(server, level, uuid, f));
}

/// A trigger of the player `uuid` that reads the entities `ids` of the running level, while the player is in another
/// level: the criteria are matched in the running level (the entities' own, with the player visible there) and awarded
/// in the player's. The entities are read when the server runs the action, not when the trigger fires.
pub fn queue_trigger_in_origin(
    level: &mut ServerLevel,
    uuid: Uuid,
    ids: Vec<EntityId>,
    matcher: impl FnOnce(&ServerLevel, &ServerPlayer, &[&dyn Entity]) -> TriggerMatches + Send + 'static,
) {
    level.queue_server_action(move |server, level| {
        deliver_from(server, level, uuid, move |own, player, origin| {
            let matches = match_in(origin.unwrap_or(&mut *own), player, &ids, matcher);
            matches.award(own, player);
        });
    });
}

/// `matcher` on `ids` of `level`, with `player` (of another level) visible there.
pub fn match_in(
    level: &mut ServerLevel,
    player: &ServerPlayer,
    ids: &[EntityId],
    matcher: impl FnOnce(&ServerLevel, &ServerPlayer, &[&dyn Entity]) -> TriggerMatches,
) -> TriggerMatches {
    level.expose(player, |level| {
        let entities: Vec<&dyn Entity> = ids
            .iter()
            .filter_map(|&id| level.get_entity_or_removed(id))
            .collect();
        matcher(&*level, player, &entities)
    })
}

/// `ServerPlayer.awardKillScore(victim, source)` for the player `uuid` in another level than the running one, where the
/// victim (`snapshot`) died.
pub fn queue_award_kill_score(
    level: &mut ServerLevel,
    uuid: Uuid,
    snapshot: DamageEntity,
    source: DamageSource,
) {
    level.queue_server_action(move |server, level| {
        deliver_from(server, level, uuid, move |own, player, origin| {
            crate::world::entity::player::damage::award_kill_score_across_levels(
                player, own, origin, &snapshot, &source,
            );
        });
    });
}
