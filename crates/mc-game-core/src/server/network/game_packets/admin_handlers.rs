//! `ServerGamePacketListenerImpl.handleSetGameRule`, `handleEntityTagQuery` and `handleBlockEntityTagQuery` (the
//! dispatcher has already checked the game-master permission).

use super::PlayCtx;
use super::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::world::entity::Entity;
use crate::world::level::gamerules::AnyGameRule;
use crate::world::level::level_access::BlockGetter;
use linkme::distributed_slice;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{
    ClientboundTagQueryPacket, ServerboundBlockEntityTagQueryPacket,
    ServerboundEntityTagQueryPacket, ServerboundSetGameRulePacket,
};

struct AdminHandlers;

impl PlayHandlers for AdminHandlers {
    fn handle_set_game_rule(&self, ctx: &mut PlayCtx, packet: &ServerboundSetGameRulePacket) {
        // run after the packet: `onGameRuleChanged` acts on every level, including the one the player is taken out of
        let entries = packet.entries.clone();
        ctx.lis().deferred_tasks.push(Box::new(move |server| {
            for entry in &entries {
                let Some(rule) = AnyGameRule::by_identifier(&entry.game_rule_key) else {
                    log::warn!(
                        "Received request to set unknown game rule: ResourceKey[minecraft:game_rule / {}]",
                        entry.game_rule_key
                    );
                    continue;
                };
                // `DataResult.result()`: a partial (trailing characters) value is not applied
                let Ok(value) = rule.deserialize(&entry.value) else {
                    continue;
                };
                server.set_game_rule_value(rule, value);
                let message = crate::translatable!(
                    "commands.gamerule.set",
                    rule.id(),
                    rule.serialize(value)
                );
                for op in server.player_list.get_players() {
                    if server.player_list.is_op(&op.name_and_id()) {
                        op.send_system_message(&message, false);
                    }
                }
            }
        }));
    }

    fn handle_entity_tag_query(&self, ctx: &mut PlayCtx, packet: &ServerboundEntityTagQueryPacket) {
        // `level.getEntity(id)`; the player itself is taken out of the level
        let level = &*ctx.level;
        let entity: &dyn Entity = if packet.entity_id == ctx.player.id() && !ctx.player.is_removed()
        {
            &*ctx.player
        } else if let Some(entity) = level.get_entity(packet.entity_id) {
            entity
        } else {
            return;
        };
        let mut tag = CompoundTag::new();
        entity.save_without_id(&mut tag, &|id| {
            if id == ctx.player.id() && !ctx.player.is_removed() {
                Some(&*ctx.player as &dyn Entity)
            } else {
                level.get_entity(id)
            }
        });
        ctx.send(&ClientboundTagQueryPacket {
            transaction_id: packet.transaction_id,
            tag: Some(tag),
        });
    }

    fn handle_block_entity_tag_query(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundBlockEntityTagQueryPacket,
    ) {
        let tag = ctx.level.get_block_entity(packet.pos).map(|e| {
            let mut tag = CompoundTag::new();
            e.save_without_metadata(&mut tag);
            tag
        });
        ctx.send(&ClientboundTagQueryPacket {
            transaction_id: packet.transaction_id,
            tag,
        });
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static ADMIN_HANDLERS: &dyn PlayHandlers = &AdminHandlers;
