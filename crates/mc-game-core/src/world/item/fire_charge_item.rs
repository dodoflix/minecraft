//! `FireChargeItem`: `useOn` lights campfires, candles and candle cakes, or places fire against the clicked face;
//! `ProjectileItem` builds the dispenser's `SmallFireball`.

use super::{
    DispenseConfig, InteractionResult, ItemBehaviour, ItemStack, PositionFunction, ProjectileItem,
    UseOnContext,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ServerPlayer;
use crate::world::entity::projectile::Projectile;
use crate::world::entity::projectile::hurting::small_fireball::SmallFireball;
use crate::world::level::block::impls::{
    base_fire_block, campfire_block, candle_block, candle_cake_block,
};
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::{GameEvent, SoundEvent, props};
use mc_protocol::packets::play::SoundSource;

pub struct FireChargeItem;

impl ItemBehaviour for FireChargeItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let mut pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        if !campfire_block::can_light(state)
            && !candle_block::can_light(state)
            && !candle_cake_block::can_light(state)
        {
            pos = pos.relative(context.get_clicked_face());
            if !base_fire_block::can_be_placed_at(level, pos, context.get_horizontal_direction()) {
                return InteractionResult::FAIL;
            }
            play_sound(level, pos);
            let fire_state = base_fire_block::get_state(&*level, pos);
            level.set_block_and_update(pos, fire_state);
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::BLOCK_PLACE,
                Vec3::at_center_of(&pos),
            );
        } else {
            play_sound(level, pos);
            level.set_block_and_update(pos, state.set(props::LIT, true));
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::BLOCK_CHANGE,
                Vec3::at_center_of(&pos),
            );
        }
        context.item_stack.shrink(1);
        InteractionResult::SUCCESS
    }

    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

impl ProjectileItem for FireChargeItem {
    fn as_projectile(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
        item_stack: &ItemStack,
        direction: Direction,
    ) -> Box<dyn Entity> {
        let random = level.random();
        let dir_x = random.triangle(f64::from(direction.get_step_x()), 0.11485000000000001);
        let dir_y = random.triangle(f64::from(direction.get_step_y()), 0.11485000000000001);
        let dir_z = random.triangle(f64::from(direction.get_step_z()), 0.11485000000000001);
        let dir = Vec3::new(dir_x, dir_y, dir_z);
        let mut fireball =
            SmallFireball::at(level, position.x, position.y, position.z, dir.normalize());
        fireball.set_item(item_stack);
        Box::new(fireball)
    }

    /// The fireball flies along the direction it was created with: nothing is shot.
    fn shoot(&self, _: &mut dyn Projectile, _: f64, _: f64, _: f64, _: f32, _: f32) {}

    fn create_dispense_config(&self) -> DispenseConfig {
        DispenseConfig {
            position_function: PositionFunction::Dispenser {
                scale: 1.0,
                offset: Vec3::ZERO,
            },
            uncertainty: 6.6666665,
            power: 1.0,
            override_dispense_event: Some(1018),
        }
    }
}

fn play_sound(level: &mut ServerLevel, pos: BlockPos) {
    let random = level.random();
    let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
    level.play_sound(
        None,
        pos,
        SoundEvent::ITEM_FIRECHARGE_USE,
        SoundSource::Blocks,
        1.0,
        pitch,
    );
}

register_item_class!("FireChargeItem", |_| Box::new(FireChargeItem));
