//! `EnderEyeItem`: filling an end portal frame (`useOn`) and the launch of the eye (`use`).

use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStackExt, UseOnContext,
    player_pov_hit_result_fluid,
};
use crate::advancements::triggers::CriteriaTriggers;
use crate::register_item_class;
use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::entity::projectile::eye_of_ender::EyeOfEnder;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers::push_entities_up;
use crate::world::level::block::pattern::{BlockInWorld, BlockPattern, BlockPatternBuilder};
use crate::world::level::collision::ClipFluid;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::block::Block;
use mc_data::props;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::packets::play::{SoundSource, Stat};
use std::sync::LazyLock;

/// `BlockInWorld.hasState(BlockStatePredicate.forBlock(END_PORTAL_FRAME).where(HAS_EYE, true).where(FACING, facing))`.
fn frame_with_eye_facing(
    facing: Direction,
) -> impl Fn(&mut BlockInWorld, &dyn LevelReader) -> bool + Send + Sync + 'static {
    move |block, level| {
        block.get_state(level).is_some_and(|s| {
            s.is_block(Block::END_PORTAL_FRAME)
                && s.get(props::EYE)
                && s.get(props::HORIZONTAL_FACING) == facing
        })
    }
}

/// `EndPortalFrameBlock.getOrCreatePortalShape()`: the ring of twelve frames that holds an eye each.
fn get_or_create_portal_shape() -> &'static BlockPattern {
    static PORTAL_SHAPE: LazyLock<BlockPattern> = LazyLock::new(|| {
        BlockPatternBuilder::start()
            .aisle(&["?vvv?", ">???<", ">???<", ">???<", "?^^^?"])
            .where_('?', |block, level| block.get_state(level).is_some())
            .where_('^', frame_with_eye_facing(Direction::South))
            .where_('>', frame_with_eye_facing(Direction::West))
            .where_('v', frame_with_eye_facing(Direction::North))
            .where_('<', frame_with_eye_facing(Direction::East))
            .build()
    });
    &PORTAL_SHAPE
}

pub struct EnderEyeItem;

impl ItemBehaviour for EnderEyeItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        _player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let target_state = level.get_block_state(pos);
        if !target_state.is_block(Block::END_PORTAL_FRAME) || target_state.get(props::EYE) {
            return InteractionResult::PASS;
        }
        let new_state = target_state.set(props::EYE, true);
        push_entities_up(level, target_state, new_state, pos);
        level.set_block_flags(pos, new_state, 2);
        level.update_neighbour_for_output_signal(pos, Block::END_PORTAL_FRAME);
        context.item_stack.shrink(1);
        level.level_event(None, 1503, pos, 0);
        if let Some(found) = get_or_create_portal_shape().find(&*level, pos) {
            let origin = found.get_front_top_left().offset(-3, 0, -3);
            for x in 0..3 {
                for z in 0..3 {
                    let portal_pos = origin.offset(x, 0, z);
                    level.destroy_block(portal_pos, true, None, 512);
                    level.set_block_flags(portal_pos, Block::END_PORTAL.default_state(), 2);
                }
            }
            level.global_level_event(1038, origin.offset(1, 0, 1), 0);
        }
        InteractionResult::SUCCESS
    }

    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let hit = player_pov_hit_result_fluid(level, player, ClipFluid::None);
        if !hit.miss
            && level
                .get_block_state(hit.block_pos)
                .is_block(Block::END_PORTAL_FRAME)
        {
            return InteractionResult::PASS;
        }
        let started_using = !player.is_using_item();
        player.start_using_item(level, hand);
        let mut stack = player.get_item_in_hand(hand);
        let tag = Identifier::new("minecraft", "eye_of_ender_located").expect("valid id");
        let wanted: Vec<Identifier> = {
            let registries = &level.registries;
            match (
                registries.registry(keys::STRUCTURE),
                registries.tag(keys::STRUCTURE, &tag),
            ) {
                (Some(registry), Some(ids)) => {
                    ids.iter().filter_map(|id| registry.name_of(*id)).collect()
                }
                _ => Vec::new(),
            }
        };
        let Some((nearest_map_feature, _)) =
            level.find_nearest_map_structure(&wanted, player.block_position(), 100, false)
        else {
            return InteractionResult::CONSUME;
        };
        let mut eye = EyeOfEnder::at(
            level,
            player.base().x(),
            player.base().get_y_progress(0.5),
            player.base().z(),
        );
        eye.set_item(&stack);
        eye.signal_to(Vec3::at_lower_corner_of(&nearest_map_feature));
        level.game_event_from_entity_ref(&*player, GameEvent::PROJECTILE_SHOOT, eye.position());
        level.add_fresh_entity(Box::new(eye));
        CriteriaTriggers::USED_ENDER_EYE.trigger(level, player, nearest_map_feature);
        let pitch = mth::lerp_f32(level.random().next_float(), 0.33, 0.5);
        let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
        level.play_sound_at(
            None,
            x,
            y,
            z,
            SoundEvent::ENTITY_ENDER_EYE_LAUNCH,
            SoundSource::Neutral,
            1.0,
            pitch,
        );
        stack.consume(1, player.player.abilities.instabuild);
        player.set_item_in_hand(hand, stack.clone());
        if started_using {
            // vanilla's used item is the hand's stack object: it shrinks with it
            player.player.living.use_item = stack;
        }
        // `this`, not the stack's item: the consumed last eye leaves an empty stack
        player.award_stat_once(level, Stat::Used(Item::ENDER_EYE));
        InteractionResult::SUCCESS_SERVER
    }
}

register_item_class!("EnderEyeItem", |_| Box::new(EnderEyeItem));
