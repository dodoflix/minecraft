//! The goals nested in `Panda` (`PandaAttackGoal`, `PandaAvoidGoal`, `PandaBreedGoal`, `PandaHurtByTargetGoal`, `PandaLieOnBackGoal`,
//! `PandaLookAtPlayerGoal`, `PandaPanicGoal`, `PandaRollGoal`, `PandaSitGoal`, `PandaSneezeGoal`). Java subclasses of a generic
//! goal embed it and call its `super` bodies; the struct names are the Java simple class names (the oracle prints them in `goals=`).

use super::panda::{Panda, TOTAL_UNHAPPY_TIME, can_pick_up_and_eat};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, EntityTest};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageTypeTags;
use mc_game_core::world::entity::ai::goal::target::HurtByTargetGoal;
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, Flag, Goal, GoalFlags, LookAtPlayerGoal, LookAtType, MeleeAttackGoal, PanicGoal,
    reduced_tick_delay,
};
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::item::{self, ItemEntity};
use mc_game_core::world::entity::mob::{ext, navigation};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::level_access::BlockGetter;
use std::sync::Arc;

fn panda(mob: &dyn Entity) -> &Panda {
    mob.as_any().downcast_ref::<Panda>().expect("panda")
}

fn panda_mut(mob: &mut dyn Entity) -> &mut Panda {
    mob.as_any_mut().downcast_mut::<Panda>().expect("panda")
}

/// `Panda.PandaAttackGoal`.
pub struct PandaAttackGoal {
    inner: MeleeAttackGoal,
}

impl PandaAttackGoal {
    pub fn new(speed_modifier: f64, track_target: bool) -> Self {
        Self {
            inner: MeleeAttackGoal::new(speed_modifier, track_target),
        }
    }
}

impl Goal for PandaAttackGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaAttackGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        panda(mob).can_perform_action(level) && self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop_impl(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Panda.PandaAvoidGoal<T>`.
pub struct PandaAvoidGoal {
    inner: AvoidEntityGoal,
}

impl PandaAvoidGoal {
    pub fn new(
        avoid_class: EntityTest,
        max_dist: f32,
        walk_speed_modifier: f64,
        sprint_speed_modifier: f64,
    ) -> Self {
        // `EntitySelector.NO_SPECTATORS`
        Self {
            inner: AvoidEntityGoal::with_predicate_on_avoid_entity(
                avoid_class,
                max_dist,
                walk_speed_modifier,
                sprint_speed_modifier,
                Arc::new(|e: &dyn Entity| !e.is_spectator()),
            ),
        }
    }
}

impl Goal for PandaAvoidGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaAvoidGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let p = panda(mob);
        p.is_worried() && p.can_perform_action(level) && self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Panda.PandaBreedGoal`.
pub struct PandaBreedGoal {
    inner: BreedGoal,
    unhappy_cooldown: i32,
    breed_targeting: TargetingConditions,
}

impl PandaBreedGoal {
    pub fn new(panda: &dyn Entity, speed_modifier: f64) -> Self {
        // `Panda.BREED_TARGETING`
        Self {
            inner: BreedGoal::new(panda, speed_modifier),
            unhappy_cooldown: 0,
            breed_targeting: TargetingConditions::for_non_combat().range(8.0),
        }
    }

    /// `canFindBamboo()`.
    fn can_find_bamboo(mob: &dyn Entity, level: &ServerLevel) -> bool {
        let panda_pos = mob.block_position();
        for y_off in 0..3 {
            for r in 0..8 {
                let mut x = 0;
                while x <= r {
                    let mut z = if x < r && x > -r { r } else { 0 };
                    while z <= r {
                        if level.get_block_state(panda_pos.offset(x, y_off, z)).block()
                            == Block::BAMBOO
                        {
                            return true;
                        }
                        z = if z > 0 { -z } else { 1 - z };
                    }
                    x = if x > 0 { -x } else { 1 - x };
                }
            }
        }
        false
    }
}

impl Goal for PandaBreedGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaBreedGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !self.inner.can_use(mob, level) || panda(mob).get_unhappy_counter() != 0 {
            false
        } else if !Self::can_find_bamboo(mob, level) {
            let tick_count = mob.base().tick_count;
            if self.unhappy_cooldown <= tick_count {
                panda_mut(mob).set_unhappy_counter(TOTAL_UNHAPPY_TIME);
                self.unhappy_cooldown = tick_count.wrapping_add(600);
                if mob.is_effective_ai() {
                    let p = mob.position();
                    let player =
                        ext::get_nearest_player(level, &self.breed_targeting, mob, p.x, p.y, p.z);
                    panda_mut(mob).look_at_request = Some(player);
                }
            }
            false
        } else {
            true
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Panda.PandaHurtByTargetGoal`.
pub struct PandaHurtByTargetGoal {
    inner: HurtByTargetGoal,
}

impl PandaHurtByTargetGoal {
    pub fn new() -> Self {
        let mut inner = HurtByTargetGoal::new(Vec::new()).set_alert_others(Vec::new());
        // `alertOther(other, hurtByMob)`: only aggressive pandas join in
        inner.alert_other = Some(Box::new(
            |level: &mut ServerLevel, other: EntityId, hurt_by: EntityId| {
                level.with_entity(other, |level, o| {
                    if o.as_any().is::<Panda>() && o.is_aggressive() {
                        o.set_target(level, Some(hurt_by));
                    }
                });
            },
        ));
        Self { inner }
    }
}

impl Default for PandaHurtByTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for PandaHurtByTargetGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaHurtByTargetGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let p = panda(mob);
        if !p.got_bamboo && !p.did_bite {
            self.inner.can_continue_to_use(mob, level)
        } else {
            mob.set_target(level, None);
            false
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Panda.PandaLieOnBackGoal`.
pub struct PandaLieOnBackGoal {
    cooldown: i32,
}

impl PandaLieOnBackGoal {
    pub fn new() -> Self {
        Self { cooldown: 0 }
    }
}

impl Default for PandaLieOnBackGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for PandaLieOnBackGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaLieOnBackGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.cooldown < mob.base().tick_count
            && panda(mob).is_lazy()
            && panda(mob).can_perform_action(level)
            && mob
                .base_mut()
                .random
                .next_int_bounded(reduced_tick_delay(400))
                == 1
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        if !mob.is_in_water()
            && (panda(mob).is_lazy()
                || mob
                    .base_mut()
                    .random
                    .next_int_bounded(reduced_tick_delay(600))
                    != 1)
        {
            mob.base_mut()
                .random
                .next_int_bounded(reduced_tick_delay(2000))
                != 1
        } else {
            false
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        panda_mut(mob).set_on_back(true);
        self.cooldown = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        panda_mut(mob).set_on_back(false);
        self.cooldown = mob.base().tick_count.wrapping_add(200);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Panda.PandaLookAtPlayerGoal`.
pub struct PandaLookAtPlayerGoal {
    inner: LookAtPlayerGoal,
}

impl PandaLookAtPlayerGoal {
    pub fn new(panda: &dyn Entity, look_distance: f32) -> Self {
        Self {
            inner: LookAtPlayerGoal::new_player(panda, look_distance),
        }
    }

    /// `setTarget(entity)` called by `PandaBreedGoal` on the goal instance: the request is parked on the panda (the goal
    /// selector is out of the mob while the breed goal runs) and applied before the goal's next callback.
    fn sync(&mut self, mob: &mut dyn Entity) {
        if let Some(target) = panda_mut(mob).look_at_request.take() {
            self.inner.look_at = target;
        }
    }
}

impl Goal for PandaLookAtPlayerGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaLookAtPlayerGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.sync(mob);
        if mob.base_mut().random.next_float() >= self.inner.probability {
            return false;
        }
        if self.inner.look_at.is_none() {
            let p = mob.position();
            let eye_y = mob.base().get_eye_y();
            self.inner.look_at = match &self.inner.look_at_type {
                LookAtType::Player => ext::get_nearest_player(
                    level,
                    &self.inner.look_at_context,
                    mob,
                    p.x,
                    eye_y,
                    p.z,
                ),
                LookAtType::Class(test) => {
                    let bb = mob.base().bounding_box().inflate(
                        f64::from(self.inner.look_distance),
                        3.0,
                        f64::from(self.inner.look_distance),
                    );
                    let candidates = level.get_entities_of_class(
                        &bb,
                        |e| e.living().is_some() && test(e),
                        |_| true,
                    );
                    ext::get_nearest_entity(
                        level,
                        &candidates,
                        &self.inner.look_at_context,
                        mob,
                        p.x,
                        eye_y,
                        p.z,
                    )
                }
            };
        }
        panda(mob).can_perform_action(level) && self.inner.look_at.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.sync(mob);
        self.inner.look_at.is_some() && self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.sync(mob);
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.sync(mob);
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.sync(mob);
        if self.inner.look_at.is_some() {
            self.inner.tick(mob, level);
        }
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Panda.PandaPanicGoal`.
pub struct PandaPanicGoal {
    inner: PanicGoal,
}

impl PandaPanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            inner: PanicGoal::with_tag(speed_modifier, DamageTypeTags::PANIC_ENVIRONMENTAL_CAUSES),
        }
    }
}

impl Goal for PandaPanicGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaPanicGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if panda(mob).is_sitting() {
            navigation::stop(mob, level);
            false
        } else {
            self.inner.can_continue_to_use(mob, level)
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }

    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(&self.inner)
    }
}

/// `Panda.PandaRollGoal`.
pub struct PandaRollGoal;

impl Goal for PandaRollGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaRollGoal"
    }

    #[expect(
        clippy::if_same_then_else,
        reason = "mirrors PandaRollGoal.canUse branch structure (the conditions draw from the random source)"
    )]
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if (mob.is_baby() || panda(mob).is_playful()) && mob.base().on_ground() {
            if !panda(mob).can_perform_action(level) {
                return false;
            }
            let angle = mob.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
            let x_dir = -mth::sin(f64::from(angle));
            let z_dir = mth::cos(f64::from(angle));
            let x_step = if f64::from(x_dir.abs()) > 0.5 {
                mth::sign(f64::from(x_dir))
            } else {
                0
            };
            let z_step = if f64::from(z_dir.abs()) > 0.5 {
                mth::sign(f64::from(z_dir))
            } else {
                0
            };
            if level
                .get_block_state(mob.block_position().offset(x_step, -1, z_step))
                .is_air()
            {
                true
            } else if panda(mob).is_playful()
                && mob
                    .base_mut()
                    .random
                    .next_int_bounded(reduced_tick_delay(60))
                    == 1
            {
                true
            } else {
                mob.base_mut()
                    .random
                    .next_int_bounded(reduced_tick_delay(500))
                    == 1
            }
        } else {
            false
        }
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        panda_mut(mob).roll(true);
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look, Flag::Jump])
    }
}

/// `getEntitiesOfClass(ItemEntity.class, panda.getBoundingBox().inflate(range), Panda::canPickUpAndEat)`.
fn edible_items_nearby(mob: &dyn Entity, level: &ServerLevel, range: f64) -> Vec<EntityId> {
    let bb = mob.base().bounding_box().inflate(range, range, range);
    level.get_entities_of_class(
        &bb,
        |e| e.as_any().is::<ItemEntity>(),
        |e| {
            e.as_any()
                .downcast_ref::<ItemEntity>()
                .is_some_and(can_pick_up_and_eat)
        },
    )
}

/// `Panda.PandaSitGoal`.
pub struct PandaSitGoal {
    cooldown: i32,
}

impl PandaSitGoal {
    pub fn new() -> Self {
        Self { cooldown: 0 }
    }
}

impl Default for PandaSitGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for PandaSitGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaSitGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.cooldown > mob.base().tick_count
            || mob.is_baby()
            || mob.is_in_water()
            || !panda(mob).can_perform_action(level)
            || panda(mob).get_unhappy_counter() > 0
        {
            false
        } else if !mob.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            true
        } else {
            !edible_items_nearby(mob, level, 6.0).is_empty()
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        if !mob.is_in_water()
            && (panda(mob).is_lazy()
                || mob
                    .base_mut()
                    .random
                    .next_int_bounded(reduced_tick_delay(600))
                    != 1)
        {
            mob.base_mut()
                .random
                .next_int_bounded(reduced_tick_delay(2000))
                != 1
        } else {
            false
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !panda(mob).is_sitting() && !mob.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            panda_mut(mob).try_to_sit(level);
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if mob.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            let items = edible_items_nearby(mob, level, 8.0);
            if let Some(pos) = items
                .first()
                .and_then(|i| level.get_entity(*i))
                .map(|i| i.block_position())
            {
                navigation::move_to_entity(mob, level, pos, f64::from(1.2f32));
            }
        } else {
            panda_mut(mob).try_to_sit(level);
        }
        self.cooldown = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let stack = mob.get_item_by_slot(EquipmentSlot::MainHand);
        if !stack.is_empty() {
            item::spawn_at_location(level, mob, stack, Vec3::ZERO);
            mob.set_item_slot(
                level,
                EquipmentSlot::MainHand,
                mc_game_core::world::item::ItemStack::empty(),
            );
            let wait_seconds = if panda(mob).is_lazy() {
                mob.base_mut().random.next_int_bounded(50) + 10
            } else {
                mob.base_mut().random.next_int_bounded(150) + 10
            };
            self.cooldown = mob.base().tick_count.wrapping_add(wait_seconds * 20);
        }
        panda_mut(mob).sit(false);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Panda.PandaSneezeGoal`.
pub struct PandaSneezeGoal;

impl Goal for PandaSneezeGoal {
    fn name(&self) -> &'static str {
        "Panda$PandaSneezeGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.is_baby() && panda(mob).can_perform_action(level) {
            if panda(mob).is_weak()
                && mob
                    .base_mut()
                    .random
                    .next_int_bounded(reduced_tick_delay(500))
                    == 1
            {
                true
            } else {
                mob.base_mut()
                    .random
                    .next_int_bounded(reduced_tick_delay(6000))
                    == 1
            }
        } else {
            false
        }
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        panda_mut(mob).sneeze(true);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
