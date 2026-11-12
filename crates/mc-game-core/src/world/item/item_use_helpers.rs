//! `Player`/`Level` checks shared by item `use`/`useOn` code.

use super::{ItemStack, can_place_on_block_in_adventure_mode};
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::SoundEvent;
use mc_protocol::packets::play::SoundSource;

/// `level.mayInteract(player, pos)` (`ServerLevel.mayInteract`): spawn protection and the world border.
pub fn may_interact(level: &ServerLevel, player: &ServerPlayer, pos: BlockPos) -> bool {
    !(level.is_under_spawn_protection(player, pos) || !level.is_within_world_border(pos))
}

/// `Player.mayUseItemAt(pos, direction, itemStack)`.
pub fn may_use_item_at(
    level: &ServerLevel,
    player: &ServerPlayer,
    pos: BlockPos,
    direction: Direction,
    stack: &ItemStack,
) -> bool {
    if player.player.abilities.may_build {
        return true;
    }
    let target = pos.relative(direction.get_opposite());
    can_place_on_block_in_adventure_mode(stack, level, target)
}

/// `Player.playSound(sound, volume, pitch)`: heard by everyone but the player (the client plays it itself).
pub fn player_play_sound(
    level: &mut ServerLevel,
    player: &dyn Entity,
    sound: SoundEvent,
    volume: f32,
    pitch: f32,
) {
    let p = player.position();
    level.play_sound_at(
        Some(player.id()),
        p.x,
        p.y,
        p.z,
        sound,
        SoundSource::Players,
        volume,
        pitch,
    );
}
