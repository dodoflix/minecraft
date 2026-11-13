//! Goals, weapon switching, shooting and shearing of the skeleton family.

use super::{AbstractSkeleton, SkeletonKind};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{Item, MobEffect, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{
    Goal, GoalFlags, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob;
use mc_game_core::world::entity::projectile::arrow::Arrow;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::projectile_weapon_item::{get_mob_arrow, get_weapon_holding_hand};
use mc_game_core::world::item::{InteractionHand, ItemStack};
use mc_protocol::packets::play::SoundSource;
use mc_world::level_storage::Difficulty;

/// `ItemStack.is(tag)` for an item tag.
pub fn item_is_in_tag(stack: &ItemStack, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `registerGoals()`.
pub fn register_goals(skeleton: &mut AbstractSkeleton) {
    use mc_data::EntityType;
    use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, FleeSunGoal, RestrictSunGoal};
    use mc_game_core::world::entity::ai::goal::target::{
        HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
    };
    use std::sync::Arc;
    if let SkeletonKind::WitherSkeleton = skeleton.kind {
        // `WitherSkeleton.registerGoals`: its piglin target goal comes first, then `super.registerGoals()`
        let abstract_piglin = TargetType::Class(Arc::new(|e: &dyn Entity| {
            matches!(e.get_type(), EntityType::PIGLIN | EntityType::PIGLIN_BRUTE)
        }));
        skeleton.mob.target_selector.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(abstract_piglin, true)),
        );
    }
    let look_at = LookAtPlayerGoal::new_player(skeleton, 8.0);
    let sel = &mut skeleton.mob.goal_selector;
    sel.add_goal(2, Box::new(RestrictSunGoal::new()));
    sel.add_goal(3, Box::new(FleeSunGoal::new(1.0)));
    sel.add_goal(
        3,
        Box::new(AvoidEntityGoal::new(
            Arc::new(|e: &dyn Entity| e.get_type() == EntityType::WOLF),
            6.0,
            1.0,
            1.2,
        )),
    );
    sel.add_goal(5, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
    sel.add_goal(6, Box::new(look_at));
    sel.add_goal(6, Box::new(RandomLookAroundGoal::new()));
    let t = &mut skeleton.mob.target_selector;
    t.add_goal(1, Box::new(HurtByTargetGoal::new(Vec::new())));
    t.add_goal(
        2,
        Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
    );
    t.add_goal(
        3,
        Box::new(NearestAttackableTargetGoal::new(
            TargetType::of_type(EntityType::IRON_GOLEM),
            true,
        )),
    );
    let baby_on_land: mc_game_core::world::entity::ai::targeting::TargetSelector =
        Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
            target.is_baby() && !target.is_in_water()
        });
    t.add_goal(
        3,
        Box::new(NearestAttackableTargetGoal::build(
            TargetType::of_type(EntityType::TURTLE),
            10,
            true,
            false,
            Some(baby_on_land),
        )),
    );
}

/// The `bowGoal` / `meleeGoal` field initializers. Not implemented: `bowGoal` (`RangedBowAttackGoal`).
pub fn create_weapon_goals(skeleton: &mut AbstractSkeleton) {
    skeleton.parked_melee_goal = Some(Box::new(SkeletonMeleeAttackGoal {
        melee: MeleeAttackGoal::new(1.2, false),
    }));
}

/// Binary name of the anonymous `MeleeAttackGoal` subclass of `AbstractSkeleton`.
const MELEE_GOAL_NAME: &str = "AbstractSkeleton$1";

/// The anonymous `MeleeAttackGoal` subclass of `AbstractSkeleton` (aggressive flag on start/stop).
struct SkeletonMeleeAttackGoal {
    melee: MeleeAttackGoal,
}

impl Goal for SkeletonMeleeAttackGoal {
    fn name(&self) -> &'static str {
        MELEE_GOAL_NAME
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.start_impl(mob, level);
        mob.mob_mut().expect("mob").set_aggressive(true);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.stop_impl(mob, level);
        mob.mob_mut().expect("mob").set_aggressive(false);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.tick_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.melee.flags()
    }
}

fn is_melee(goal: &dyn Goal) -> bool {
    goal.name() == MELEE_GOAL_NAME
}

fn is_bow(goal: &dyn Goal) -> bool {
    goal.name() == "RangedBowAttackGoal"
}

/// `reassessWeaponGoal()` (the level is not needed: no goal is stopped here).
pub fn reassess_weapon_goal(skeleton: &mut AbstractSkeleton, difficulty: Difficulty) {
    reassess(skeleton, difficulty, None);
}

fn reassess(
    skeleton: &mut AbstractSkeleton,
    difficulty: Difficulty,
    mut level: Option<&mut ServerLevel>,
) {
    // `goalSelector.removeGoal(meleeGoal); goalSelector.removeGoal(bowGoal);`
    for melee in [true, false] {
        let taken = skeleton
            .mob
            .goal_selector
            .take_goal_where(|g| if melee { is_melee(g) } else { is_bow(g) });
        if let Some((mut goal, running)) = taken {
            if running && let Some(level) = level.as_deref_mut() {
                goal.stop(skeleton, level);
            }
            if melee {
                skeleton.parked_melee_goal = Some(goal);
            } else {
                skeleton.parked_bow_goal = Some(goal);
            }
        }
    }
    let hand = get_weapon_holding_hand(skeleton, Item::BOW);
    let slot = if hand == InteractionHand::MainHand {
        EquipmentSlot::MainHand
    } else {
        EquipmentSlot::OffHand
    };
    if skeleton.get_item_by_slot(slot).is(Item::BOW) {
        let mut min_attack_interval = skeleton.get_hard_attack_interval();
        if difficulty != Difficulty::Hard {
            min_attack_interval = skeleton.get_attack_interval();
        }
        if let Some(mut bow) = skeleton.parked_bow_goal.take() {
            set_min_attack_interval(&mut *bow, min_attack_interval);
            skeleton.mob.goal_selector.add_goal(4, bow);
        }
    } else if let Some(melee) = skeleton.parked_melee_goal.take() {
        skeleton.mob.goal_selector.add_goal(4, melee);
    }
}

/// `bowGoal.setMinAttackInterval(interval)`.
fn set_min_attack_interval(_goal: &mut dyn Goal, _interval: i32) {}

/// `performRangedAttack(target, power)`.
pub fn perform_ranged_attack(
    skeleton: &mut AbstractSkeleton,
    level: &mut ServerLevel,
    target: EntityId,
    power: f32,
) {
    let hand = get_weapon_holding_hand(skeleton, Item::BOW);
    let bow_slot = if hand == InteractionHand::MainHand {
        EquipmentSlot::MainHand
    } else {
        EquipmentSlot::OffHand
    };
    let bow_item = skeleton.get_item_by_slot(bow_slot);
    // `getProjectile(bowItem)` of a mob: the held ammunition, else a plain arrow
    let projectile = get_mob_projectile(skeleton);
    let arrow = get_arrow(skeleton, level, &projectile, power, Some(&bow_item));
    let Some((tx, tz, ty_third)) = level.get_entity(target).map(|t| {
        (
            t.base().x(),
            t.base().z(),
            t.base().get_y_progress(0.3333333333333333),
        )
    }) else {
        return;
    };
    let xd = tx - skeleton.base().x();
    let yd = ty_third - arrow.base().y();
    let zd = tz - skeleton.base().z();
    let distance_to_target = (xd * xd + zd * zd).sqrt();
    let uncertainty = skeleton.ranged_attack_uncertainty(level);
    mc_game_core::world::entity::projectile::spawn::spawn_projectile_using_shoot(
        level,
        arrow,
        &projectile,
        xd,
        yd + distance_to_target * f64::from(0.2f32),
        zd,
        1.6,
        uncertainty,
    );
    let pitch = 1.0 / (skeleton.base_mut().random.next_float() * 0.4 + 0.8);
    skeleton.play_sound(level, SoundEvent::ENTITY_SKELETON_SHOOT, 1.0, pitch);
}

/// `getArrow(projectile, power, firingWeapon)`.
fn get_arrow(
    skeleton: &AbstractSkeleton,
    level: &mut ServerLevel,
    projectile: &ItemStack,
    power: f32,
    firing_weapon: Option<&ItemStack>,
) -> Box<dyn Entity> {
    let mut arrow = get_mob_arrow(level, skeleton, projectile, power, firing_weapon);
    match skeleton.kind {
        SkeletonKind::Stray => add_arrow_effect(&mut *arrow, MobEffect::SLOWNESS, 600),
        SkeletonKind::Bogged => add_arrow_effect(&mut *arrow, MobEffect::POISON, 100),
        SkeletonKind::Parched => add_arrow_effect(&mut *arrow, MobEffect::WEAKNESS, 600),
        SkeletonKind::WitherSkeleton => arrow.ignite_for_seconds(100.0),
        SkeletonKind::Skeleton(_) => {}
    }
    arrow
}

fn add_arrow_effect(arrow: &mut dyn Entity, effect: MobEffect, duration: i32) {
    if let Some(arrow) = arrow.as_any_mut().downcast_mut::<Arrow>() {
        arrow.add_effect(mc_protocol::components::MobEffectInstance::new(
            effect, duration, 0,
        ));
    }
}

/// `Bogged.shear(level, soundSource, tool)`.
pub fn shear(
    skeleton: &mut AbstractSkeleton,
    level: &mut ServerLevel,
    sound_source: SoundSource,
    tool: &ItemStack,
) {
    // the entity may be out of the level while it is being sheared: its position goes with the packet
    level.play_sound_from_entity_at(
        None,
        skeleton.id(),
        skeleton.position(),
        SoundEvent::ENTITY_BOGGED_SHEAR,
        sound_source,
        1.0,
        1.0,
    );
    drop_from_shearing_loot_table(skeleton, level, tool);
    skeleton.set_sheared(true);
}

/// `Bogged.mobInteract` around [`shear`].
pub fn shear_bogged(
    skeleton: &mut AbstractSkeleton,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) {
    let tool = player.get_item_in_hand(hand);
    shear(skeleton, level, SoundSource::Players, &tool);
    level.game_event_from_entity_ref(&*player, mc_data::GameEvent::SHEAR, skeleton.position());
    let slot = if hand == InteractionHand::MainHand {
        EquipmentSlot::MainHand
    } else {
        EquipmentSlot::OffHand
    };
    mob::interact::with_hand_stack(player, hand, |stack, player| {
        mc_game_core::world::item::hurt_and_break(stack, 1, level, player, slot);
    });
}

/// `dropFromShearingLootTable(level, BOGGED_SHEAR, tool, (l, drop) -> spawnAtLocation(l, drop, getBbHeight()))`.
fn drop_from_shearing_loot_table(
    skeleton: &mut AbstractSkeleton,
    level: &mut ServerLevel,
    tool: &ItemStack,
) {
    use mc_game_core::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
    let table = Identifier::parse("minecraft:shearing/bogged").expect("valid id");
    let loot = loading::get_loot_table(&level.registries, &table);
    let items = {
        let this: &dyn Entity = &*skeleton;
        let params = LootParams::builder(level)
            .this_entity(this)
            .origin(this.position())
            .tool(tool)
            .create(&LootContextParamSets::SHEARING);
        loot.get_random_items(&params)
    };
    for stack in items {
        let height = skeleton.base().get_bb_height();
        mc_game_core::world::entity::item::spawn_at_location(
            level,
            skeleton,
            stack,
            Vec3::new(0.0, f64::from(height), 0.0),
        );
    }
}

/// `Monster.getProjectile(heldWeapon)` for a bow: the arrow held in a hand (off hand first), else a plain arrow.
fn get_mob_projectile(skeleton: &AbstractSkeleton) -> ItemStack {
    for slot in [EquipmentSlot::OffHand, EquipmentSlot::MainHand] {
        let held = skeleton.get_item_by_slot(slot);
        if mc_game_core::world::item::projectile_weapon_item::is_arrow(&held) {
            return held;
        }
    }
    ItemStack::new(Item::ARROW)
}
