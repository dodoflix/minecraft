//! `net.minecraft.world.level.block.ButtonBlock` (extends `FaceAttachedHorizontalDirectionalBlock`; all wooden/stone
//! buttons, parameterized by `BlockSetType` and `ticksToStayPressed`).
use super::lever_block::update_neighbours;
use mc_data::GameEvent;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{Entity as _, EntityId, ServerPlayer};
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::block::block_set_type::BlockSetType;
use mc_game_core::world::level::block::impls::face_attached_horizontal_directional_block::{
    self as face_attached, get_connected_direction,
};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

pub struct ButtonBlock {
    ty: &'static BlockSetType,
    ticks_to_stay_pressed: i32,
}

impl ButtonBlock {
    /// `new ButtonBlock(type, ticksToStayPressed, ...)` as constructed by `Blocks` (`stone`/`polished_blackstone` 20, else 30).
    pub fn new(block: Block) -> Self {
        let ty = BlockSetType::for_block(block).expect("every button block has a block set type");
        Self {
            ty,
            ticks_to_stay_pressed: if ty.name == "stone" { 20 } else { 30 },
        }
    }

    /// `ButtonBlock.press(state, level, pos, player)`.
    pub fn press(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: Option<EntityId>,
    ) {
        level.set_block_and_update(pos, state.set(props::POWERED, true));
        update_neighbours(state, level, pos);
        level.schedule_tick_block(pos, state.block(), self.ticks_to_stay_pressed);
        self.play_sound(player, level, pos, true);
        level.game_event_at_block(
            GameEvent::BLOCK_ACTIVATE,
            pos,
            &GameEventContext::of_entity(player),
        );
    }

    /// `ButtonBlock.playSound(player, level, pos, pressed)`.
    fn play_sound(
        &self,
        player: Option<EntityId>,
        level: &mut ServerLevel,
        pos: BlockPos,
        pressed: bool,
    ) {
        let sound = if pressed {
            self.ty.button_click_on
        } else {
            self.ty.button_click_off
        };
        level.play_sound(
            if pressed { player } else { None },
            pos,
            sound,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }

    /// `ButtonBlock.checkPressed(state, level, pos)`.
    /// `inside` is the arrow whose `entityInside` is running (it is out of the entity storage meanwhile).
    pub fn check_pressed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        inside: Option<&dyn mc_game_core::world::entity::Entity>,
    ) {
        let first_arrow = if self.ty.can_button_be_activated_by_arrows {
            let bounds = state.get_shape(&*level, pos).bounds().move_(
                f64::from(pos.x()),
                f64::from(pos.y()),
                f64::from(pos.z()),
            );
            let found = level
                .get_entities_of_class(
                    &bounds,
                    |e| e.tracking_kind() == mc_game_core::world::entity::TrackingKind::Arrow,
                    |_| true,
                )
                .into_iter()
                .next();
            found.or_else(|| {
                inside
                    .filter(|e| {
                        e.tracking_kind() == mc_game_core::world::entity::TrackingKind::Arrow
                            && e.get_bounding_box().intersects_aabb(&bounds)
                    })
                    .map(|e| e.id())
            })
        } else {
            None
        };
        let should_be_pressed = first_arrow.is_some();
        let was_pressed = state.get(props::POWERED);
        if should_be_pressed != was_pressed {
            level.set_block_and_update(pos, state.set(props::POWERED, should_be_pressed));
            update_neighbours(state, level, pos);
            self.play_sound(None, level, pos, should_be_pressed);
            level.game_event_at_block(
                if should_be_pressed {
                    GameEvent::BLOCK_ACTIVATE
                } else {
                    GameEvent::BLOCK_DEACTIVATE
                },
                pos,
                &GameEventContext::of_entity(first_arrow),
            );
        }
        if should_be_pressed {
            level.schedule_tick_block(pos, state.block(), self.ticks_to_stay_pressed);
        }
    }
}

impl BlockBehaviour for ButtonBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        face_attached::can_survive(state, level, pos)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        face_attached::get_state_for_placement(block, &*level, context)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            face_attached::update_shape(state, &*level, pos, direction)
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if state.get(props::POWERED) {
            InteractionResult::CONSUME
        } else {
            self.press(state, level, pos, Some(player.id()));
            InteractionResult::SUCCESS
        }
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        moved_by_piston: bool,
    ) {
        if !moved_by_piston && state.get(props::POWERED) {
            update_neighbours(state, level, pos);
        }
    }
    fn get_direct_signal(
        &self,
        state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if state.get(props::POWERED) && get_connected_direction(state) == direction {
            15
        } else {
            0
        }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if self.ty.can_button_be_activated_by_arrows && !state.get(props::POWERED) {
            self.check_pressed(state, level, pos, Some(&*entity));
        }
    }
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn mc_game_core::world::level::explosion::Explosion,
        on_hit: &mut dyn FnMut(mc_game_core::world::item::ItemStack, BlockPos),
    ) {
        if explosion.can_trigger_blocks() && !state.get(props::POWERED) {
            self.press(state, level, pos, None);
        }
        mc_game_core::world::level::explosion::default_on_explosion_hit(
            state, level, pos, explosion, on_hit,
        );
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::POWERED) {
            self.check_pressed(state, level, pos, None);
        }
    }
}

register_block_class!("ButtonBlock", |block| Box::new(ButtonBlock::new(block)));
