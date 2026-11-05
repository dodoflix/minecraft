//! The goal registration of the zombie family (`registerGoals()`'s egg goal and `addBehaviourGoals()` of `Zombie`,
//! `Drowned`, `ZombifiedPiglin`).
//!
//! NOT YET WIRED (their goals are not ported yet): `SpearUseGoal`,
//! `ResetUniversalAngerTargetGoal`.

use super::drowned::{self, DrownedGoToWaterGoal, DrownedSwimUpGoal};
use super::{Zombie, ZombieKind};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::move_to_block::{self, MoveToBlock, MoveToBlockData};
use crate::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type};
use crate::world::entity::ai::goal::{BreakDoorGoal, MoveThroughVillageGoal, Flag, Goal, GoalFlags, RandomStrollGoal, RangedAttackGoal, RemoveBlockGoal, RemoveBlockHooks, WaterAvoidingRandomStrollGoal, ZombieAttackGoal};
use crate::world::entity::ai::targeting::TargetSelector;
use crate::world::entity::mob::{self, neutral};
use crate::world::entity::Entity;
use crate::world::level::{BlockGetter, LevelReader};
use crate::world::level::LevelAccessor;
use crate::world::level::block::state::BlockStateExt;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_world::level_storage::Difficulty;
use mc_data::{EntityType, Item, SoundEvent};
use mc_protocol::packets::play::SoundSource;
use std::sync::Arc;

/// `this instanceof BreakDoorGoal`.
pub fn is_break_door_goal(goal: &dyn Goal) -> bool {
    (goal as &dyn std::any::Any).is::<BreakDoorGoal>()
}

/// `breakDoorGoal = new BreakDoorGoal(this, DOOR_BREAKING_PREDICATE)` (a field initializer).
pub fn create_break_door_goal(zombie: &Zombie) -> Box<dyn Goal> {
    Box::new(BreakDoorGoal::new(zombie, Arc::new(|d: Difficulty| d == Difficulty::Hard)))
}

/// `Zombie.ZombieAttackTurtleEggGoal`'s overrides.
struct TurtleEggHooks;

impl RemoveBlockHooks for TurtleEggHooks {
    fn accepted_distance(&self) -> f64 {
        1.14
    }

    fn play_destroy_progress_sound(&self, mob: &mut dyn Entity, level: &mut ServerLevel, pos: BlockPos) {
        let pitch = 0.9 + mob.base_mut().random.next_float() * 0.2;
        level.play_sound(None, pos, SoundEvent::ENTITY_ZOMBIE_DESTROY_EGG, SoundSource::Hostile, 0.5, pitch);
    }

    fn play_break_sound(&self, level: &mut ServerLevel, pos: BlockPos) {
        let pitch = 0.9 + level.random().next_float() * 0.2;
        level.play_sound(None, pos, SoundEvent::ENTITY_TURTLE_EGG_BREAK, SoundSource::Blocks, 0.7, pitch);
    }
}

/// `Turtle.BABY_ON_LAND_SELECTOR`.
fn baby_on_land_selector() -> TargetSelector {
    Arc::new(|target: &dyn Entity, _level: &ServerLevel| target.is_baby() && !target.is_in_water())
}

/// `AbstractVillager.class`.
fn abstract_villager() -> TargetType {
    TargetType::Class(Arc::new(|e: &dyn Entity| matches!(e.get_type(), EntityType::VILLAGER | EntityType::WANDERING_TRADER)))
}

/// `registerGoals()`'s first goal.
pub fn add_egg_goal(zombie: &mut Zombie) {
    zombie.mob.goal_selector.add_goal(4, Box::new(RemoveBlockGoal::with_hooks(Block::TURTLE_EGG, 1.0, 3, Box::new(TurtleEggHooks))));
}

fn alert_class(zombie: &Zombie) -> Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync> {
    match zombie.kind {
        // `getEntitiesOfClass(Zombie.class)` includes every subclass
        ZombieKind::Zombie => Arc::new(|e: &dyn Entity| e.as_zombie().is_some()),
        _ => class_of_type(zombie.get_type()),
    }
}

/// `addBehaviourGoals()`.
pub fn add_behaviour_goals(zombie: &mut Zombie) {
    let zp = class_of_type(EntityType::ZOMBIFIED_PIGLIN);
    let alert = alert_class(zombie);
    match zombie.kind {
        ZombieKind::Drowned(_) => {
            let sel = &mut zombie.mob.goal_selector;
            sel.add_goal(1, Box::new(DrownedGoToWaterGoal::new(1.0)));
            sel.add_goal(2, Box::new(DrownedTridentAttackGoal::new(1.0, 40, 10.0)));
            sel.add_goal(2, Box::new(DrownedAttackGoal { attack: ZombieAttackGoal::new(1.0, false) }));
            sel.add_goal(5, Box::new(DrownedGoToBeachGoal { mtb: MoveToBlockData::new(1.0, 8) }));
            sel.add_goal(6, Box::new(DrownedSwimUpGoal::new(1.0)));
            sel.add_goal(7, Box::new(RandomStrollGoal::new(1.0)));
            let mut hurt_by = HurtByTargetGoal::new(vec![class_of_type(EntityType::DROWNED)]).set_alert_others(vec![zp]);
            hurt_by.alert_class = Some(alert);
            let t = &mut zombie.mob.target_selector;
            t.add_goal(1, Box::new(hurt_by));
            let ok_target: TargetSelector = Arc::new(|target: &dyn Entity, level: &ServerLevel| drowned::ok_target(level, Some(target)));
            t.add_goal(2, Box::new(NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, Some(ok_target))));
            t.add_goal(3, Box::new(NearestAttackableTargetGoal::new(abstract_villager(), false)));
            t.add_goal(3, Box::new(NearestAttackableTargetGoal::new(TargetType::of_type(EntityType::IRON_GOLEM), true)));
            t.add_goal(3, Box::new(NearestAttackableTargetGoal::with_must_reach(TargetType::of_type(EntityType::AXOLOTL), true, false)));
            t.add_goal(5, Box::new(NearestAttackableTargetGoal::build(TargetType::of_type(EntityType::TURTLE), 10, true, false, Some(baby_on_land_selector()))));
        }
        ZombieKind::ZombifiedPiglin(_) => {
            // SpearUseGoal(1) pending
            zombie.mob.goal_selector.add_goal(2, Box::new(ZombieAttackGoal::new(1.0, false)));
            zombie.mob.goal_selector.add_goal(7, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
            let mut hurt_by = HurtByTargetGoal::new(Vec::new()).set_alert_others(Vec::new());
            hurt_by.alert_class = Some(alert);
            let t = &mut zombie.mob.target_selector;
            t.add_goal(1, Box::new(hurt_by));
            let angry = NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None)
                .with_source_selector(Arc::new(|mob: &dyn Entity, target: &dyn Entity, level: &ServerLevel| neutral::is_angry_at(mob, level, target)));
            t.add_goal(2, Box::new(angry));
            // ResetUniversalAngerTargetGoal(3) pending
        }
        _ => {
            // SpearUseGoal(2) pending
            let can_break_doors: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync> = Arc::new(|e: &dyn Entity| e.as_zombie().is_some_and(|z| z.can_break_doors()));
            zombie.mob.goal_selector.add_goal(3, Box::new(ZombieAttackGoal::new(1.0, false)));
            let village = MoveThroughVillageGoal::new(zombie, 1.0, true, 4, can_break_doors);
            zombie.mob.goal_selector.add_goal(6, Box::new(village));
            zombie.mob.goal_selector.add_goal(7, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
            let mut hurt_by = HurtByTargetGoal::new(Vec::new()).set_alert_others(vec![zp]);
            hurt_by.alert_class = Some(alert);
            let t = &mut zombie.mob.target_selector;
            t.add_goal(1, Box::new(hurt_by));
            t.add_goal(2, Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)));
            t.add_goal(3, Box::new(NearestAttackableTargetGoal::new(abstract_villager(), false)));
            t.add_goal(3, Box::new(NearestAttackableTargetGoal::new(TargetType::of_type(EntityType::IRON_GOLEM), true)));
            t.add_goal(5, Box::new(NearestAttackableTargetGoal::build(TargetType::of_type(EntityType::TURTLE), 10, true, false, Some(baby_on_land_selector()))));
        }
    }
}

/// `Drowned.DrownedAttackGoal`.
struct DrownedAttackGoal {
    attack: ZombieAttackGoal,
}

fn ok_target_of_mob(mob: &dyn Entity, level: &ServerLevel) -> bool {
    let target = mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t));
    drowned::ok_target(level, target)
}

impl Goal for DrownedAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.attack.can_use(mob, level) && ok_target_of_mob(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.attack.can_continue_to_use(mob, level) && ok_target_of_mob(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.attack.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.attack.stop(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.attack.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.attack.flags()
    }
}

/// `Drowned.DrownedTridentAttackGoal`.
struct DrownedTridentAttackGoal {
    ranged: RangedAttackGoal,
}

impl DrownedTridentAttackGoal {
    fn new(speed_modifier: f64, attack_interval: i32, attack_radius: f32) -> Self {
        Self { ranged: RangedAttackGoal::new(speed_modifier, attack_interval, attack_radius) }
    }
}

impl Goal for DrownedTridentAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.ranged.can_use(mob, level) && mob.get_item_by_slot(crate::world::entity::equipment::EquipmentSlot::MainHand).is(Item::TRIDENT)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.ranged.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.ranged.start(mob, level);
        mob.mob_mut().expect("mob").set_aggressive(true);
        drowned::start_using_main_hand_item(mob);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.ranged.stop(mob, level);
        crate::world::entity::living_entity::blocking::stop_using_item(mob, level);
        mob.mob_mut().expect("mob").set_aggressive(false);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.ranged.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.ranged.flags()
    }
}

/// `Drowned.DrownedGoToBeachGoal`.
struct DrownedGoToBeachGoal {
    mtb: MoveToBlockData,
}

impl MoveToBlock for DrownedGoToBeachGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.mtb
    }

    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.mtb
    }

    fn is_valid_target(&self, mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        let above = pos.above();
        if level.is_empty_block(above) && level.is_empty_block(above.above()) {
            crate::world::entity::ai::navigation::entity_can_stand_on(level.get_block_state(pos), level, pos, mob)
        } else {
            false
        }
    }
}

impl Goal for DrownedGoToBeachGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_use(self, mob, level) && !level.is_bright_outside() && mob.is_in_water() && mob.base().y() >= f64::from(level.get_sea_level() - 3)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_continue_to_use(self, mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        drowned::set_searching_for_land(mob, false);
        move_to_block::start(self, mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}
