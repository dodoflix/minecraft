//! `net.minecraft.world.entity.decoration.*`: block-attached entities (item frames, paintings, leash knots, cushions),
//! the armor stand and the mannequin.

pub mod armor_stand;
pub mod block_attached;
pub mod cushion;
pub mod hanging;
pub mod item_frame;
pub mod mannequin;
pub mod map_seam;
pub mod painting;

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageEntity;
use crate::world::level::BlockGetter;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;

/// `level.findBlocksIn(box).filterState(pred).allMatched()`: every block position of the box (`from`/`to` are the
/// `BlockPos.containing` of the box corners) satisfies `pred`.
pub fn all_blocks_in(
    level: &dyn BlockGetter,
    aabb: &AABB,
    pred: impl Fn(BlockState) -> bool,
) -> bool {
    positions_in(aabb).all(|pos| pred(level.get_block_state(pos)))
}

/// `level.findBlocksIn(box).filterState(pred).anyMatched()`.
pub fn any_block_in(
    level: &dyn BlockGetter,
    aabb: &AABB,
    pred: impl Fn(BlockState) -> bool,
) -> bool {
    positions_in(aabb).any(|pos| pred(level.get_block_state(pos)))
}

fn positions_in(aabb: &AABB) -> impl Iterator<Item = BlockPos> + use<> {
    let from = BlockPos::containing_position(&aabb.get_min_position());
    let to = BlockPos::containing_position(&aabb.get_max_position());
    BlockPos::between_closed(&from, &to)
}

/// `source.getEntity() instanceof Mob` from what a damage source remembers: a living entity that is neither a player nor
/// an armor stand/mannequin (those are `LivingEntity`, not `Mob`).
pub fn caused_by_mob(cause: Option<&DamageEntity>) -> bool {
    cause.is_some_and(|c| {
        c.is_living
            && !c.is_player
            && c.entity_type != mc_data::EntityType::ARMOR_STAND
            && c.entity_type != mc_data::EntityType::MANNEQUIN
    })
}

/// `source.getEntity() instanceof Player player && !level.mayInteract(player, pos)`.
pub fn breaking_denied_at(
    level: &ServerLevel,
    cause: Option<&DamageEntity>,
    pos: BlockPos,
) -> bool {
    cause.is_some_and(|c| c.is_player && !level.may_interact_player_uuid(c.uuid, pos))
}
