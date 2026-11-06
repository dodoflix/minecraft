//! `BegGoal` (wolves beg for bones and food).

use crate::world::entity::animal::wolf::Wolf;
use mc_core::random::RandomSource;
use mc_data::Item;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob::ext;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;

pub struct BegGoal {
    player: Option<EntityId>,
    look_distance: f32,
    look_time: i32,
    beg_targeting: TargetingConditions,
}

impl BegGoal {
    pub fn new(look_distance: f32) -> Self {
        Self {
            player: None,
            look_distance,
            look_time: 0,
            beg_targeting: TargetingConditions::for_non_combat().range(f64::from(look_distance)),
        }
    }

    fn player_holding_interesting(wolf: &dyn Entity, player: &dyn Entity) -> bool {
        [EquipmentSlot::MainHand, EquipmentSlot::OffHand]
            .into_iter()
            .any(|slot| {
                let item_stack: ItemStack = player.get_item_by_slot(slot);
                item_stack.is(Item::BONE) || wolf.is_food(&item_stack)
            })
    }
}

impl Goal for BegGoal {
    fn can_use(&mut self, wolf: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let pos = wolf.position();
        self.player =
            ext::get_nearest_player(level, &self.beg_targeting, wolf, pos.x, pos.y, pos.z);
        match self.player.and_then(|p| level.get_entity(p)) {
            None => false,
            Some(player) => Self::player_holding_interesting(wolf, player),
        }
    }

    fn can_continue_to_use(&mut self, wolf: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(player) = self.player.and_then(|p| level.get_entity(p)) else {
            return false;
        };
        if !player.is_alive() {
            return false;
        }
        if wolf.base().distance_to_sqr_vec(player.position())
            > f64::from(self.look_distance * self.look_distance)
        {
            return false;
        }
        self.look_time > 0 && Self::player_holding_interesting(wolf, player)
    }

    fn start(&mut self, wolf: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(w) = wolf.as_any_mut().downcast_mut::<Wolf>() {
            w.set_is_interested(true);
        }
        self.look_time = self.adjusted_tick_delay(40 + wolf.base_mut().random.next_int_bounded(40));
    }

    fn stop(&mut self, wolf: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(w) = wolf.as_any_mut().downcast_mut::<Wolf>() {
            w.set_is_interested(false);
        }
        self.player = None;
    }

    fn tick(&mut self, wolf: &mut dyn Entity, level: &mut ServerLevel) {
        let max_x = wolf.get_max_head_x_rot() as f32;
        if let Some(player) = self.player.and_then(|p| level.get_entity(p)) {
            let (x, y, z) = (
                player.base().x(),
                player.base().get_eye_y(),
                player.base().z(),
            );
            if let Some(m) = wolf.mob_mut() {
                m.look_control_mut().set_look_at_full(x, y, z, 10.0, max_x);
            }
        }
        self.look_time -= 1;
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look])
    }
}
