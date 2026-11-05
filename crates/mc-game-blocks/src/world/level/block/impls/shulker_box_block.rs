//! `net.minecraft.world.level.block.ShulkerBoxBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::BlockEntity;
use mc_game_core::world::block_entity::shulker_box_block_entity::{AnimationStatus, ShulkerBoxBlockEntity, lid_open_box, shulker_at};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_core::phys::shapes::{Shapes, VoxelShape};

pub struct ShulkerBoxBlock;

/// `SHAPES_OPEN_SUPPORT.get(direction)`: a one pixel plate on the `direction` face.
fn open_support_shape(direction: Direction) -> VoxelShape {
    let (min, max) = (1.0 / 16.0, 15.0 / 16.0);
    match direction {
        Direction::Down => Shapes::box_(0.0, 0.0, 0.0, 1.0, min, 1.0),
        Direction::Up => Shapes::box_(0.0, max, 0.0, 1.0, 1.0, 1.0),
        Direction::North => Shapes::box_(0.0, 0.0, 0.0, 1.0, 1.0, min),
        Direction::South => Shapes::box_(0.0, 0.0, max, 1.0, 1.0, 1.0),
        Direction::West => Shapes::box_(0.0, 0.0, 0.0, min, 1.0, 1.0),
        Direction::East => Shapes::box_(max, 0.0, 0.0, 1.0, 1.0, 1.0),
    }
}

/// `ShulkerBoxBlock.canOpen(state, level, pos, blockEntity)`.
pub fn can_open(level: &ServerLevel, state: BlockState, pos: BlockPos, shulker: &ShulkerBoxBlockEntity) -> bool {
    shulker.get_animation_status() != AnimationStatus::Closed || mc_game_core::world::level::collision::CollisionGetter::no_collision(level, None, None, &lid_open_box(state, pos))
}

fn shape(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
    match shulker_at(level, pos) {
        Some(s) => Shapes::create(s.get_bounding_box(state)),
        None => Shapes::block(),
    }
}

impl BlockBehaviour for ShulkerBoxBlock {
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::FACING, context.get_clicked_face()))
    }

    fn trigger_event(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
        helpers::base_entity_trigger_event(level, pos, b0, b1)
    }

    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer) -> BlockState {
        if let Some(shulker) = level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<ShulkerBoxBlockEntity>()) {
            if player.is_creative() && !mc_game_core::world::container::Container::is_empty(shulker) {
                let mut stack = ItemStack::new(state.block().item().expect("shulker box item"));
                for (t, v) in shulker.collect_components().iter() {
                    stack.set_value(t, Some(v.clone()));
                }
                let mut entity = ItemEntity::with_stack(level, f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 0.5, f64::from(pos.z()) + 0.5, stack);
                entity.set_default_pick_up_delay();
                level.add_fresh_entity(Box::new(entity));
            } else {
                level.with_block_entity(pos, |level, be| be.unpack_loot_table(&*level, Some(&*player as &dyn mc_game_core::world::entity::Entity)));
            }
        }
        mc_game_core::server::level::block_break::player_will_destroy(level, pos, state, player)
    }

    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> mc_game_core::world::item::InteractionResult {
        use mc_game_core::world::block_entity::container_menus::{BlockEntityMenuProvider, ContainerBlockKind};
        let title = level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<ShulkerBoxBlockEntity>()).filter(|s| can_open(level, state, pos, s)).map(|s| s.get_display_name());
        if let Some(title) = title {
            let provider = BlockEntityMenuProvider { kind: ContainerBlockKind::ShulkerBox, first: pos, second: None, title };
            if player.open_menu(level, &provider).is_some() {
                player.award_stat_once(&*level, mc_game_core::stats::stats::Stats::OPEN_SHULKER_BOX);
            }
        }
        mc_game_core::world::item::InteractionResult::SUCCESS
    }

    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn get_block_support_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        match shulker_at(level, pos) {
            Some(s) if !s.is_closed() => open_support_shape(state.get(props::FACING).get_opposite()),
            _ => Shapes::block(),
        }
    }

    fn get_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, _context: &mc_game_core::world::level::collision::CollisionContext) -> VoxelShape {
        shape(state, level, pos)
    }

    fn get_collision_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, _context: &mc_game_core::world::level::collision::CollisionContext) -> VoxelShape {
        shape(state, level, pos)
    }

    fn get_analog_output_signal(&self, _state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        mc_game_core::world::container::get_redstone_signal_from_block_entity(level.get_block_entity(pos))
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("ShulkerBoxBlock", |_| Box::new(ShulkerBoxBlock));
