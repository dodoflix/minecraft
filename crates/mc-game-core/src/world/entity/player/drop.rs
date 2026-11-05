//! `LivingEntity.createItemStackToDrop` / `LivingEntity.drop` / `ServerPlayer.drop` / `Player.drop(all)`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::item::ItemEntity;
use crate::world::inventory::Prediction;
use crate::world::item::ItemStack;
use mc_core::math::mth;
use mc_core::random::RandomSource;

const PI_F: f32 = std::f32::consts::PI;

impl ServerPlayer {
    /// `createItemStackToDrop(itemStack, randomly, thrownFromHand)`.
    pub fn create_item_stack_to_drop(&mut self, level: &ServerLevel, stack: ItemStack, randomly: bool, thrown_from_hand: bool) -> Option<ItemEntity> {
        if stack.is_empty() {
            return None;
        }
        let y_hand_pos = self.base().get_eye_y() - f64::from(0.3f32);
        let mut entity = ItemEntity::with_stack(level, self.base().x(), y_hand_pos, self.base().z(), stack);
        entity.set_pick_up_delay(40);
        if thrown_from_hand {
            entity.set_thrower(self.base().uuid);
        }
        let random = &mut self.base_mut().random;
        if randomly {
            let pow = random.next_float() * 0.5f32;
            let dir = random.next_float() * (PI_F * 2.0);
            entity.base_mut().set_delta_movement_xyz(f64::from(-mth::sin(f64::from(dir)) * pow), f64::from(0.2f32), f64::from(mth::cos(f64::from(dir)) * pow));
        } else {
            let (x_rot, y_rot) = (self.base().x_rot(), self.base().y_rot());
            let deg = PI_F / 180.0;
            let sin_x = mth::sin(f64::from(x_rot * deg));
            let cos_x = mth::cos(f64::from(x_rot * deg));
            let sin_y = mth::sin(f64::from(y_rot * deg));
            let cos_y = mth::cos(f64::from(y_rot * deg));
            let random = &mut self.base_mut().random;
            let dir = random.next_float() * (PI_F * 2.0);
            let pow2 = 0.02f32 * random.next_float();
            let dx = f64::from(-sin_y * cos_x * 0.3f32) + f64::from(dir).cos() * f64::from(pow2);
            let a = random.next_float();
            let b = random.next_float();
            let dy = f64::from(-sin_x * 0.3f32 + 0.1f32 + (a - b) * 0.1f32);
            let dz = f64::from(cos_y * cos_x * 0.3f32) + f64::from(dir).sin() * f64::from(pow2);
            entity.base_mut().set_delta_movement_xyz(dx, dy, dz);
        }
        Some(entity)
    }

    /// `ServerPlayer.drop(itemStack, thrownFromHand, prediction)` (`LivingEntity.drop` + stats): spawns the item entity.
    pub fn drop(&mut self, level: &mut ServerLevel, stack: ItemStack, thrown_from_hand: bool, prediction: Prediction) -> bool {
        if stack.is_empty() {
            return false;
        }
        self.swing(level, crate::world::item::InteractionHand::MainHand, super::item_use::SWING_ANIMATION_DEFAULT, prediction != Prediction::Predicted);
        let Some(entity) = self.create_item_stack_to_drop(level, stack, false, thrown_from_hand) else { return false };
        // `awardStat(ITEM_DROPPED, DROP)`: statistics are not ported yet
        level.add_fresh_entity(Box::new(entity))
    }

    /// `drop(boolean all)`: the "drop item" key (`removeFromSelected`, remote slot fix-up, item entity).
    pub fn drop_selected(&mut self, level: &mut ServerLevel, all: bool) {
        let removed = self.player.inventory.remove_from_selected(all);
        let selected = self.player.inventory.get_selected_slot() as usize;
        let selected_now = self.player.inventory.get_selected_item().clone();
        let key = crate::world::inventory::ContainerKey::PlayerInventory;
        let current = self.player.menus.current_mut();
        if let Some(slot_index) = current.base().find_slot(&key, selected) {
            current.base_mut().set_remote_slot(slot_index, &selected_now);
        }
        if self.player.living.use_item.is_empty() {
            self.stop_using_item(level);
        }
        self.drop(level, removed, true, Prediction::Predicted);
        self.reset_attack_strength_ticker();
    }
}
