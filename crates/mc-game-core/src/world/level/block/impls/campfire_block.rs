//! `net.minecraft.world.level.block.CampfireBlock`.
use super::prelude::*;
use crate::world::level::collision::CollisionContext;
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use std::sync::LazyLock;

/// `SHAPE_VIRTUAL_POST`.
static SHAPE_VIRTUAL_POST: LazyLock<VoxelShape> =
    LazyLock::new(|| crate::world::level::shapes::column(4.0, 0.0, 16.0));

pub struct CampfireBlock;

impl CampfireBlock {
    /// `isLitCampfire(state)`.
    pub fn is_lit_campfire(state: BlockState) -> bool {
        state.has_property(props::LIT) && state.is_tag("campfires") && state.get(props::LIT)
    }

    /// `isSmokeyPos(level, pos)`: a lit campfire up to 5 blocks below with no smoke-blocking collision in between.
    pub fn is_smokey_pos(level: &dyn BlockGetter, pos: BlockPos) -> bool {
        for i in 1..=5 {
            let pos_to_check = pos.below_n(i);
            let state = level.get_block_state(pos_to_check);
            if Self::is_lit_campfire(state) {
                return true;
            }
            use crate::world::level::block::BlockStateExt as _;
            let blocked = Shapes::join_is_not_empty(
                &SHAPE_VIRTUAL_POST,
                &state.get_collision_shape_ctx(level, pos, &CollisionContext::empty()),
                BooleanOp::AND,
            );
            if blocked {
                return Self::is_lit_campfire(level.get_block_state(pos_to_check.below()));
            }
        }
        false
    }
}

/// `CampfireBlock.douse(source, level, pos, state)`: the particles are client side, only the game event remains.
pub fn douse(
    source: Option<crate::world::entity::EntityId>,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
) {
    level.game_event(
        mc_data::GameEvent::BLOCK_CHANGE,
        mc_core::phys::Vec3::at_center_of(&pos),
        &crate::world::level::gameevent::GameEventContext::of_entity(source),
    );
}

/// `CampfireBlock.canLight(state)`.
pub fn can_light(state: BlockState) -> bool {
    state.is_tag("campfires")
        && state.has_property(props::WATERLOGGED)
        && state.has_property(props::LIT)
        && !state.get(props::WATERLOGGED)
        && !state.get(props::LIT)
}

impl BlockBehaviour for CampfireBlock {
    fn use_item_on(
        &self,
        stack: &crate::world::item::ItemStack,
        state: BlockState,
        level: &mut crate::server::level::ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        hand: crate::world::item::InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> crate::world::item::InteractionResult {
        use crate::world::block_entity::campfire_block_entity::CampfireBlockEntity;
        use crate::world::item::InteractionResult;
        use crate::world::item::crafting::RecipePropertySet;
        if level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<CampfireBlockEntity>())
            && level
                .recipe_access()
                .property_set(RecipePropertySet::CAMPFIRE_INPUT)
                .test(stack)
        {
            let mut placed = stack.clone();
            let source = player.base_id();
            let infinite = player.has_infinite_materials();
            let success = level
                .with_block_entity(pos, |level, be| {
                    be.as_any_mut()
                        .downcast_mut::<CampfireBlockEntity>()
                        .is_some_and(|campfire| {
                            campfire.place_food(level, Some(source), infinite, &mut placed)
                        })
                })
                .unwrap_or(false);
            if success {
                player.set_item_in_hand(hand, placed);
                player.award_stat_once(level, crate::stats::stats::Stats::INTERACT_WITH_CAMPFIRE);
                return InteractionResult::SUCCESS_SERVER;
            }
            return InteractionResult::CONSUME;
        }
        let douses = mc_data::tags::get::<mc_data::Item>("douses_campfires")
            .is_some_and(|tag| tag.contains(stack.item()));
        if douses && state.get(props::LIT) {
            level.level_event(None, 1009, pos, 0);
            douse(Some(player.base_id()), level, pos);
            level.set_block_and_update(pos, state.set(props::LIT, false));
            let mut used = stack.clone();
            crate::world::item::hurt_and_break(
                &mut used,
                1,
                level,
                player,
                hand.as_equipment_slot(),
            );
            player.set_item_in_hand(hand, used);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut crate::server::level::ServerLevel,
        context: &crate::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_water = level.get_fluid_state(pos).get_type() == mc_data::fluid::Fluid::WATER;
        Some(
            block
                .default_state()
                .set(props::WATERLOGGED, replaced_water)
                .set(
                    props::SIGNAL_FIRE,
                    level
                        .get_block_state(pos.below())
                        .is_block(Block::HAY_BLOCK),
                )
                .set(props::LIT, !replaced_water)
                .set(props::HORIZONTAL_FACING, context.get_horizontal_direction()),
        )
    }
    crate::simple_waterlogged!(no_place_liquid);
    fn place_liquid(
        &self,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        state: BlockState,
        fluid_state: mc_data::fluid::FluidState,
    ) -> bool {
        if !state.get(props::WATERLOGGED) && fluid_state.get_type() == mc_data::fluid::Fluid::WATER
        {
            if state.get(props::LIT) {
                level.play_sound(
                    None,
                    pos,
                    mc_data::SoundEvent::ENTITY_GENERIC_EXTINGUISH_FIRE,
                    mc_protocol::packets::play::level::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                douse(None, level, pos);
            }
            level.set_block_and_update(
                pos,
                state.set(props::WATERLOGGED, true).set(props::LIT, false),
            );
            let delay = crate::world::level::material::fluid_behaviour(fluid_state.get_type())
                .get_tick_delay(&*level);
            level.schedule_fluid_tick(
                pos,
                fluid_state.get_type(),
                delay,
                mc_world::ticks::TickPriority::Normal,
            );
            true
        } else {
            false
        }
    }
    fn on_projectile_hit(
        &self,
        level: &mut crate::server::level::ServerLevel,
        state: BlockState,
        hit: &mc_core::phys::BlockHitResult,
        projectile: &mut dyn crate::world::entity::projectile::Projectile,
    ) {
        let pos = hit.block_pos;
        if projectile.is_on_fire()
            && projectile.may_interact(level, pos)
            && !state.get(props::LIT)
            && !state.get(props::WATERLOGGED)
        {
            level.set_block_flags(pos, state.set(props::LIT, true), 11);
        }
    }
    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut crate::server::level::ServerLevel,
        _pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if state.get(props::LIT) && entity.living().is_some() {
            // `fireDamage`: 1 for campfires, 2 for soul campfires
            let fire_damage = if state.block() == Block::SOUL_CAMPFIRE {
                2.0
            } else {
                1.0
            };
            let source = level.damage_sources().campfire();
            entity.hurt_server(level, &source, fire_damage);
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            if direction == Direction::Down {
                state.set(
                    props::SIGNAL_FIRE,
                    neighbour_state.is_block(Block::HAY_BLOCK),
                )
            } else {
                state
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CampfireBlock", |_| Box::new(CampfireBlock));
