//! `Zombie` itself: the behavior every class of the family shares (the "super" functions of the subclass overrides).

use super::{
    DATA_BABY_ID, REINFORCEMENT_ATTEMPTS, REINFORCEMENT_RANGE_MAX, REINFORCEMENT_RANGE_MIN, Zombie,
    ZombieKind,
};
use crate::impl_spawn_group_data;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource};
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::{LookAtPlayerGoal, RandomLookAroundGoal};
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::living_entity::util::lm;
use crate::world::entity::mob;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, Item};
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use std::sync::LazyLock;

fn id(path: &str) -> Identifier {
    Identifier::new("minecraft", path).expect("valid id")
}

/// `SPEED_MODIFIER_BABY_ID`.
static SPEED_MODIFIER_BABY_ID: LazyLock<Identifier> = LazyLock::new(|| id("baby"));
/// `REINFORCEMENT_CALLER_CHARGE_ID`.
static REINFORCEMENT_CALLER_CHARGE_ID: LazyLock<Identifier> =
    LazyLock::new(|| id("reinforcement_caller_charge"));
static REINFORCEMENT_CALLEE_CHARGE_ID: LazyLock<Identifier> =
    LazyLock::new(|| id("reinforcement_callee_charge"));
static LEADER_ZOMBIE_BONUS_ID: LazyLock<Identifier> = LazyLock::new(|| id("leader_zombie_bonus"));
static ZOMBIE_RANDOM_SPAWN_BONUS_ID: LazyLock<Identifier> =
    LazyLock::new(|| id("zombie_random_spawn_bonus"));
/// `Mob.RANDOM_SPAWN_BONUS_ID`.
static RANDOM_SPAWN_BONUS_ID: LazyLock<Identifier> = LazyLock::new(|| id("random_spawn_bonus"));

/// `Zombie.ZombieGroupData`.
pub struct ZombieGroupData {
    pub is_baby: bool,
    pub can_spawn_jockey: bool,
}

impl ZombieGroupData {
    pub fn new(baby: bool, can_spawn_jockey: bool) -> Self {
        Self {
            is_baby: baby,
            can_spawn_jockey,
        }
    }
}

impl_spawn_group_data!(ZombieGroupData);

/// `Zombie.getSpawnAsBabyOdds(random)`.
pub fn get_spawn_as_baby_odds(random: &mut dyn RandomSource) -> bool {
    random.next_float() < 0.05
}

/// `groupData instanceof Zombie.ZombieGroupData`: the group data of a zombie or of a class extending the group data
/// (`HuskGroupData`).
pub fn as_zombie_group(group: &mut dyn SpawnGroupData) -> Option<&mut ZombieGroupData> {
    if group.as_any().is::<ZombieGroupData>() {
        return group.as_any_mut().downcast_mut::<ZombieGroupData>();
    }
    group
        .as_any_mut()
        .downcast_mut::<super::husk::HuskGroupData>()
        .map(|h| &mut h.base)
}

pub fn item_is_in_tag(stack: &ItemStack, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `registerGoals()` + the virtual `addBehaviourGoals()`.
pub fn register_goals(zombie: &mut Zombie) {
    // `ZombieAttackTurtleEggGoal` (priority 4) is added with the behaviour goals below, in vanilla's order
    super::goals::add_egg_goal(zombie);
    let look_at = LookAtPlayerGoal::new_player(zombie, 8.0);
    zombie.mob.goal_selector.add_goal(8, Box::new(look_at));
    zombie
        .mob
        .goal_selector
        .add_goal(8, Box::new(RandomLookAroundGoal::new()));
    super::goals::add_behaviour_goals(zombie);
}

/// `setCanBreakDoors(canBreakDoors)`.
pub fn set_can_break_doors(zombie: &mut Zombie, can_break_doors: bool) {
    let can_navigate_ground = zombie
        .mob
        .navigation
        .as_ref()
        .is_some_and(|n| n.can_navigate_ground());
    if can_navigate_ground {
        if zombie.can_break_doors != can_break_doors {
            zombie.can_break_doors = can_break_doors;
            if let Some(navigation) = zombie.mob.navigation.as_mut() {
                navigation.set_can_open_doors(can_break_doors);
            }
            if can_break_doors {
                if let Some(goal) = zombie.parked_break_door_goal.take() {
                    zombie.mob.goal_selector.add_goal(1, goal);
                }
            } else {
                remove_break_door_goal(zombie);
            }
        }
    } else if zombie.can_break_doors {
        remove_break_door_goal(zombie);
        zombie.can_break_doors = false;
    }
}

/// `goalSelector.removeGoal(breakDoorGoal)`: the goal object is kept for the next `addGoal`; a running goal is stopped
/// by the next tick (the removal outside a tick has no level at hand).
fn remove_break_door_goal(zombie: &mut Zombie) {
    if let Some((goal, was_running)) = zombie
        .mob
        .goal_selector
        .take_goal_where(super::goals::is_break_door_goal)
    {
        if was_running {
            zombie.goals_to_stop.push(goal);
        } else {
            zombie.parked_break_door_goal = Some(goal);
        }
    }
}

/// `setBaby(baby)`.
pub fn set_baby(zombie: &mut Zombie, baby: bool) {
    if zombie.mob.living.entity.entity_data.set(DATA_BABY_ID, baby) {
        zombie.on_synced_data_updated(DATA_BABY_ID.id());
    }
    if let Some(speed) = lm(zombie)
        .attributes
        .get_instance(Attribute::MOVEMENT_SPEED)
    {
        speed.remove_modifier(&SPEED_MODIFIER_BABY_ID);
        if baby {
            speed.add_transient_modifier(AttributeModifier {
                id: SPEED_MODIFIER_BABY_ID.clone(),
                amount: 0.5,
                operation: AttributeModifierOperation::AddMultipliedBase,
            });
        }
    }
}

/// `getBaseExperienceReward(level)`.
pub fn get_base_experience_reward(zombie: &mut Zombie, _level: &ServerLevel) -> i32 {
    if zombie.is_baby() {
        zombie.mob.xp_reward = (f64::from(zombie.mob.xp_reward) * 2.5) as i32;
    }
    mob::equipment::get_base_experience_reward(zombie)
}

/// `tick()`.
pub fn tick(zombie: &mut Zombie, level: &mut ServerLevel) {
    if !zombie.goals_to_stop.is_empty() {
        let goals = std::mem::take(&mut zombie.goals_to_stop);
        for mut goal in goals {
            goal.stop(zombie, level);
            zombie.parked_break_door_goal = Some(goal);
        }
    }
    if let ZombieKind::ZombieVillager(_) = zombie.kind {
        super::zombie_villager::pre_tick(zombie, level);
    }
    mob::tick::tick(zombie, level);
    if zombie.converts_in_water() {
        tick_drowning_tracker(zombie, level);
    }
}

fn tick_drowning_tracker(zombie: &mut Zombie, level: &mut ServerLevel) {
    let ready = zombie.is_alive() && !zombie.mob.is_no_ai();
    let afflicted = ready && zombie.is_eye_in_fluid("water");
    let convert =
        zombie
            .drowning_tracker
            .tick(&mut zombie.mob.living.entity.entity_data, ready, afflicted);
    if convert {
        do_drowning_conversion(zombie, level);
    }
}

/// `drowningTracker.doConversion(level)`.
fn do_drowning_conversion(zombie: &mut Zombie, level: &mut ServerLevel) {
    let to = zombie.converts_to_when_drowning();
    let sound = zombie.get_conversion_sound();
    let params = mob::ConversionParams::single(zombie, level, true, true);
    mob::convert::convert_to(
        zombie,
        level,
        to,
        &params,
        EntitySpawnReason::Conversion,
        |converted, level| {
            let pos = converted.block_position();
            let special = ServerLevelAccessor::get_current_difficulty_at(&*level, pos)
                .get_special_multiplier();
            if let Some(converted_zombie) = converted.as_zombie_mut() {
                converted_zombie.handle_attributes(special, EntitySpawnReason::Conversion);
            }
            if !converted.base().is_silent() {
                level.level_event_impl(None, sound, converted.block_position(), 0);
            }
        },
    );
}

/// `hasNearbyAlivePlayer(x, y, z, distance)`.
fn has_nearby_alive_player(level: &ServerLevel, x: f64, y: f64, z: f64, distance: f64) -> bool {
    level.players().iter().any(|&player_id| {
        level.get_entity(player_id).is_some_and(|player| {
            if player.is_spectator() || !player.is_alive() {
                return false;
            }
            let dist = player.base().distance_to_sqr_xyz(x, y, z);
            distance < 0.0 || dist < distance * distance
        })
    })
}

/// `noCollision(entity)`.
fn no_collision_entity(level: &ServerLevel, entity: &dyn Entity) -> bool {
    use crate::world::level::collision::CollisionGetter;
    let context = crate::world::entity::movement::collision_context(entity);
    CollisionGetter::no_collision(
        level,
        Some(entity.id()),
        Some(&context),
        &entity.base().bounding_box(),
    )
}

/// `hurtServer(level, source, damage)`.
pub fn hurt_server(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) -> bool {
    if !crate::world::entity::living_entity::damage::hurt_server(zombie, level, source, damage) {
        return false;
    }
    let mut target = mob::tick::get_target_checked(zombie, level);
    if target.is_none() && source.causing_is_living() {
        target = source.get_entity();
    }
    if let Some(target) = target
        && level.difficulty() == Difficulty::Hard
        && f64::from(zombie.base_mut().random.next_float())
            < mob::attribute_value(zombie, Attribute::SPAWN_REINFORCEMENTS)
        && level.is_spawning_monsters()
    {
        call_reinforcements(zombie, level, target);
    }
    true
}

/// The reinforcement part of `hurtServer`.
fn call_reinforcements(zombie: &mut Zombie, level: &mut ServerLevel, target: EntityId) {
    let x = mth::floor_f64(zombie.base().x());
    let y = mth::floor_f64(zombie.base().y());
    let z = mth::floor_f64(zombie.base().z());
    let ty = zombie.get_type();
    let Some(mut reinforcement) = factory::create(
        ty,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Reinforcement,
            ignore_checks: false,
        },
    ) else {
        return;
    };
    for _ in 0..REINFORCEMENT_ATTEMPTS {
        let random = &mut zombie.base_mut().random;
        let xt = x.wrapping_add(
            mth::next_int(random, REINFORCEMENT_RANGE_MIN, REINFORCEMENT_RANGE_MAX)
                .wrapping_mul(mth::next_int(random, -1, 1)),
        );
        let yt = y.wrapping_add(
            mth::next_int(random, REINFORCEMENT_RANGE_MIN, REINFORCEMENT_RANGE_MAX)
                .wrapping_mul(mth::next_int(random, -1, 1)),
        );
        let zt = z.wrapping_add(
            mth::next_int(random, REINFORCEMENT_RANGE_MIN, REINFORCEMENT_RANGE_MAX)
                .wrapping_mul(mth::next_int(random, -1, 1)),
        );
        let spawn_pos = BlockPos::new(xt, yt, zt);
        if crate::world::entity::spawn_placements::is_spawn_position_ok(ty, &*level, spawn_pos)
            && crate::world::entity::spawn_placements::check_spawn_rules_with_level_random(
                level,
                ty,
                EntitySpawnReason::Reinforcement,
                spawn_pos,
            )
        {
            reinforcement.set_pos(f64::from(xt), f64::from(yt), f64::from(zt));
            let can_spawn_in_liquids = reinforcement
                .as_zombie()
                .is_some_and(|z| z.can_spawn_in_liquids());
            if !has_nearby_alive_player(level, f64::from(xt), f64::from(yt), f64::from(zt), 7.0)
                && ServerLevelAccessor::is_unobstructed_entity(&*level, &*reinforcement)
                && no_collision_entity(level, &*reinforcement)
                && (can_spawn_in_liquids
                    || !ServerLevelAccessor::contains_any_liquid(
                        &*level,
                        &reinforcement.base().bounding_box(),
                    ))
            {
                reinforcement.set_target(level, Some(target));
                let difficulty = ServerLevelAccessor::get_current_difficulty_at(
                    &*level,
                    reinforcement.block_position(),
                );
                reinforcement.finalize_spawn(
                    level,
                    difficulty,
                    EntitySpawnReason::Reinforcement,
                    None,
                );
                if let Some(attribute) = lm(&mut *reinforcement)
                    .attributes
                    .get_instance(Attribute::SPAWN_REINFORCEMENTS)
                {
                    attribute.add_permanent_modifier(AttributeModifier {
                        id: REINFORCEMENT_CALLEE_CHARGE_ID.clone(),
                        amount: f64::from(-0.05f32),
                        operation: AttributeModifierOperation::AddValue,
                    });
                }
                level.add_fresh_entity_with_passengers(vec![reinforcement]);
                if let Some(attribute) = lm(zombie)
                    .attributes
                    .get_instance(Attribute::SPAWN_REINFORCEMENTS)
                {
                    let existing_amount = attribute
                        .get_modifier(&REINFORCEMENT_CALLER_CHARGE_ID)
                        .map_or(0.0, |m| m.amount);
                    attribute.remove_modifier(&REINFORCEMENT_CALLER_CHARGE_ID);
                    attribute.add_permanent_modifier(AttributeModifier {
                        id: REINFORCEMENT_CALLER_CHARGE_ID.clone(),
                        amount: existing_amount - 0.05,
                        operation: AttributeModifierOperation::AddValue,
                    });
                }
                return;
            }
        }
    }
}

/// `doHurtTarget(level, target)`.
pub fn do_hurt_target(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    target: &mut dyn Entity,
) -> bool {
    let result =
        crate::world::entity::living_entity::combat::mob_do_hurt_target(zombie, level, target);
    if result {
        let difficulty =
            ServerLevelAccessor::get_current_difficulty_at(&*level, zombie.block_position())
                .get_effective_difficulty();
        if zombie.get_item_by_slot(EquipmentSlot::MainHand).is_empty()
            && zombie.is_on_fire()
            && zombie.base_mut().random.next_float() < difficulty * 0.3
        {
            target.ignite_for_seconds((2 * difficulty as i32) as f32);
        }
    }
    if let ZombieKind::Husk = zombie.kind {
        return super::husk::do_hurt_target_after(zombie, level, target, result);
    }
    result
}

/// `killedEntity(level, entity, source)`.
pub fn killed_entity(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    victim: &DamageEntity,
    _source: &DamageSource,
) -> bool {
    let mut perished = true;
    let difficulty = level.difficulty();
    if (difficulty == Difficulty::Normal || difficulty == Difficulty::Hard)
        && victim.entity_type == EntityType::VILLAGER
    {
        if difficulty != Difficulty::Hard && zombie.base_mut().random.next_boolean() {
            return perished;
        }
        let converted = level
            .with_entity(victim.id, |level, villager| {
                zombie.convert_villager_to_zombie_villager(level, villager)
            })
            .unwrap_or(false);
        if converted {
            perished = false;
        }
    }
    perished
}

/// `populateDefaultEquipmentSlots(random, difficulty)`.
pub fn populate_default_equipment_slots(
    zombie: &mut Zombie,
    random: &mut dyn RandomSource,
    difficulty: &DifficultyInstance,
) {
    mob::equipment::populate_default_equipment_slots(zombie, random, difficulty);
    if random.next_float()
        < (if difficulty.get_difficulty() == Difficulty::Hard {
            0.05
        } else {
            0.01
        })
    {
        let rand = random.next_int_bounded(6);
        let item = match rand {
            0 => Item::IRON_SWORD,
            1 => Item::IRON_SPEAR,
            _ => Item::IRON_SHOVEL,
        };
        mob::equipment::set_item_slot_spawn(zombie, EquipmentSlot::MainHand, ItemStack::new(item));
    }
}

/// `canHoldItem(itemStack)`.
pub fn can_hold_item(zombie: &Zombie, stack: &ItemStack) -> bool {
    !(item_is_in_tag(stack, "eggs") && zombie.is_baby() && zombie.base().is_passenger())
}

/// `wantsToPickUp(level, itemStack)`.
pub fn wants_to_pick_up(zombie: &Zombie, _level: &ServerLevel, stack: &ItemStack) -> bool {
    if stack.is(Item::GLOW_INK_SAC) {
        false
    } else {
        zombie.can_hold_item(stack)
    }
}

/// `finalizeSpawn(level, difficulty, spawnReason, groupData)`.
pub fn finalize_spawn(
    zombie: &mut Zombie,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let mut group_data = mob::spawn::finalize_spawn(zombie, level, difficulty, reason, group_data);
    let difficulty_modifier = difficulty.get_special_multiplier();
    if reason != EntitySpawnReason::Conversion {
        let can_pick_up = level.random().next_float() < 0.55 * difficulty_modifier;
        zombie.set_can_pick_up_loot(can_pick_up);
    }
    if group_data.is_none() {
        group_data = Some(Box::new(ZombieGroupData::new(
            get_spawn_as_baby_odds(level.random()),
            true,
        )));
    }
    let zombie_data = group_data
        .as_deref_mut()
        .and_then(as_zombie_group)
        .map(|g| (g.is_baby, g.can_spawn_jockey));
    if let Some((is_baby, can_spawn_jockey)) = zombie_data {
        if is_baby {
            zombie.set_baby(true);
            if can_spawn_jockey {
                spawn_chicken_jockey(zombie, level, difficulty);
            }
        }
        let can_break_doors = level.random().next_float() < difficulty_modifier * 0.1;
        zombie.set_can_break_doors(can_break_doors);
        if reason != EntitySpawnReason::Conversion {
            zombie.populate_default_equipment_slots(level.random(), &difficulty);
            let registries = level.registry_access().clone();
            zombie.populate_default_equipment_enchantments(
                &registries,
                level.random(),
                &difficulty,
            );
        }
    }
    if zombie.get_item_by_slot(EquipmentSlot::Head).is_empty()
        && crate::util::special_dates::is_halloween()
        && level.random().next_float() < 0.25
    {
        let pumpkin = if level.random().next_float() < 0.1 {
            Item::JACK_O_LANTERN
        } else {
            Item::CARVED_PUMPKIN
        };
        mob::equipment::set_item_slot_spawn(zombie, EquipmentSlot::Head, ItemStack::new(pumpkin));
        zombie.set_drop_chance(EquipmentSlot::Head, 0.0);
    }
    zombie.handle_attributes(difficulty_modifier, reason);
    group_data
}

/// The chicken jockey part of `finalizeSpawn` (a baby that may spawn as a jockey).
fn spawn_chicken_jockey(
    zombie: &mut Zombie,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
) {
    if f64::from(level.random().next_float()) < 0.05 {
        let inflated = zombie.base().bounding_box().inflate(5.0, 3.0, 5.0);
        // `getEntitiesOfClass(Chicken.class, ..., ENTITY_NOT_BEING_RIDDEN)`; a generation region has no entities
        let chickens: Vec<EntityId> = level.as_server_level().map_or_else(Vec::new, |sl| {
            sl.get_entities_of_class(
                &inflated,
                |e| e.get_type() == EntityType::CHICKEN,
                |e| e.is_alive() && !e.base().is_vehicle() && !e.base().is_passenger(),
            )
        });
        if let Some(&chicken_id) = chickens.first()
            && let Some(sl) = level.as_server_level_mut()
        {
            sl.with_entity(chicken_id, |_, chicken| set_chicken_jockey(chicken));
            sl.start_riding(zombie, chicken_id, false, false);
        }
    } else if f64::from(level.random().next_float()) < 0.05 {
        let Some(mut chicken) = level.create_entity(
            EntityType::CHICKEN,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Jockey,
                ignore_checks: false,
            },
        ) else {
            return;
        };
        let (x, y, z, y_rot) = (
            zombie.base().x(),
            zombie.base().y(),
            zombie.base().z(),
            zombie.base().y_rot(),
        );
        chicken.snap_to(x, y, z, y_rot, 0.0);
        chicken.finalize_spawn(level, difficulty, EntitySpawnReason::Jockey, None);
        set_chicken_jockey(&mut *chicken);
        start_riding_fresh_vehicle(zombie, &mut *chicken, false, false, level);
        level.add_fresh_entity_with_passengers(chicken);
    }
}

/// `chicken.setChickenJockey(true)`.
fn set_chicken_jockey(chicken: &mut dyn Entity) {
    if let Some(chicken) = chicken
        .as_any_mut()
        .downcast_mut::<crate::world::entity::animal::chicken::Chicken>()
    {
        chicken.set_chicken_jockey(true);
    }
}

/// `rider.startRiding(vehicle, force, sendEventAndTriggers)` for a vehicle that is not in the level yet (a freshly
/// created jockey mount).
pub fn start_riding_fresh_vehicle(
    rider: &mut dyn Entity,
    vehicle: &mut dyn Entity,
    force: bool,
    send_event_and_triggers: bool,
    level: &mut dyn ServerLevelAccessor,
) -> bool {
    use mc_data::enums::Pose;
    if rider.base().vehicle == Some(vehicle.id()) {
        return false;
    }
    if !vehicle.could_accept_passenger() || !vehicle.get_type().data().serializable {
        return false;
    }
    if !(force || (rider.can_ride(&*vehicle) && vehicle.can_add_passenger(rider.as_dyn()))) {
        return false;
    }
    rider.set_pose(Pose::Standing);
    rider.base_mut().vehicle = Some(vehicle.id());
    vehicle.add_passenger(&*rider, false);
    if send_event_and_triggers {
        level.game_event(
            mc_data::GameEvent::ENTITY_MOUNT,
            vehicle.position(),
            &crate::world::level::gameevent::GameEventContext::of_entity(Some(rider.id())),
        );
    }
    true
}

/// `onOffspringSpawnedFromEgg(spawner, offspring)`.
pub fn on_offspring_spawned_from_egg(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    offspring: EntityId,
) {
    let Some(pos) = level.get_entity(offspring).map(|o| o.block_position()) else {
        return;
    };
    let difficulty_modifier =
        ServerLevelAccessor::get_current_difficulty_at(&*level, pos).get_special_multiplier();
    let can_pick_up = zombie.base_mut().random.next_float() < 0.55 * difficulty_modifier;
    level.with_entity(offspring, |_, o| o.set_can_pick_up_loot(can_pick_up));
}

/// `handleAttributes(difficultyModifier, spawnReason)`.
pub fn handle_attributes(
    zombie: &mut Zombie,
    difficulty_modifier: f32,
    spawn_reason: EntitySpawnReason,
) {
    randomize_reinforcements_chance(zombie);
    let bonus = zombie.base_mut().random.next_double() * f64::from(0.05f32);
    if let Some(a) = lm(zombie)
        .attributes
        .get_instance(Attribute::KNOCKBACK_RESISTANCE)
    {
        a.add_or_replace_permanent_modifier(AttributeModifier {
            id: RANDOM_SPAWN_BONUS_ID.clone(),
            amount: bonus,
            operation: AttributeModifierOperation::AddValue,
        });
    }
    let follow_range_modifier =
        zombie.base_mut().random.next_double() * 1.5 * f64::from(difficulty_modifier);
    if follow_range_modifier > 1.0
        && let Some(a) = lm(zombie).attributes.get_instance(Attribute::FOLLOW_RANGE)
    {
        a.add_or_replace_permanent_modifier(AttributeModifier {
            id: ZOMBIE_RANDOM_SPAWN_BONUS_ID.clone(),
            amount: follow_range_modifier,
            operation: AttributeModifierOperation::AddMultipliedTotal,
        });
    }
    if zombie.base_mut().random.next_float() < difficulty_modifier * 0.05 {
        let reinforcements = zombie.base_mut().random.next_double() * 0.25 + 0.5;
        if let Some(a) = lm(zombie)
            .attributes
            .get_instance(Attribute::SPAWN_REINFORCEMENTS)
        {
            a.add_or_replace_permanent_modifier(AttributeModifier {
                id: LEADER_ZOMBIE_BONUS_ID.clone(),
                amount: reinforcements,
                operation: AttributeModifierOperation::AddValue,
            });
        }
        let health = zombie.base_mut().random.next_double() * 3.0 + 1.0;
        if let Some(a) = lm(zombie).attributes.get_instance(Attribute::MAX_HEALTH) {
            a.add_or_replace_permanent_modifier(AttributeModifier {
                id: LEADER_ZOMBIE_BONUS_ID.clone(),
                amount: health,
                operation: AttributeModifierOperation::AddMultipliedTotal,
            });
        }
        if spawn_reason != EntitySpawnReason::Conversion
            && spawn_reason != EntitySpawnReason::Load
            && spawn_reason != EntitySpawnReason::DimensionTravel
        {
            let max_health = lm(zombie).get_max_health();
            lm(zombie).set_health(max_health);
        }
        zombie.set_can_break_doors(true);
    }
}

/// `randomizeReinforcementsChance()`.
fn randomize_reinforcements_chance(zombie: &mut Zombie) {
    let value = if let ZombieKind::ZombifiedPiglin(_) = zombie.kind {
        0.0
    } else {
        zombie.base_mut().random.next_double() * f64::from(0.1f32)
    };
    if let Some(a) = lm(zombie)
        .attributes
        .get_instance(Attribute::SPAWN_REINFORCEMENTS)
    {
        a.set_base_value(value);
    }
}

/// `addAdditionalSaveData(output)`.
pub fn add_additional_save_data(zombie: &Zombie, out: &mut CompoundTag) {
    mob::nbt::add_additional_save_data(zombie, out);
    out.put_boolean("IsBaby", zombie.is_baby());
    out.put_boolean("CanBreakDoors", zombie.can_break_doors());
    let afflicted = zombie.is_eye_in_fluid("water");
    zombie.drowning_tracker.add_additional_save_data(
        &zombie.mob.living.entity.entity_data,
        afflicted,
        out,
    );
    match &zombie.kind {
        ZombieKind::ZombieVillager(_) => {
            super::zombie_villager::add_additional_save_data(zombie, out)
        }
        ZombieKind::ZombifiedPiglin(_) => mob::neutral::add_persistent_anger_save_data(zombie, out),
        _ => {}
    }
}

/// `readAdditionalSaveData(input)`.
pub fn read_additional_save_data(zombie: &mut Zombie, input: &CompoundTag) {
    mob::nbt::read_additional_save_data(zombie, input);
    zombie.set_baby(input.get_boolean_or("IsBaby", false));
    zombie.set_can_break_doors(input.get_boolean_or("CanBreakDoors", false));
    zombie
        .drowning_tracker
        .read_additional_save_data(&mut zombie.mob.living.entity.entity_data, input);
    match &zombie.kind {
        ZombieKind::ZombieVillager(_) => {
            super::zombie_villager::read_additional_save_data(zombie, input)
        }
        ZombieKind::ZombifiedPiglin(_) => {
            mob::neutral::read_persistent_anger_save_data(zombie, None, input)
        }
        _ => {}
    }
}

/// The default body of `Entity.updateSwimming()` (the classes of the family other than `Drowned` keep it).
pub fn entity_update_swimming(zombie: &mut Zombie, level: &ServerLevel) {
    use crate::world::level::BlockGetter;
    use crate::world::level::material::FluidStateExt;
    if zombie.is_swimming() {
        let swimming =
            zombie.is_sprinting() && zombie.is_in_water() && !zombie.base().is_passenger();
        zombie.set_swimming(swimming);
    } else {
        let swimming = zombie.is_sprinting()
            && zombie.is_under_water()
            && !zombie.base().is_passenger()
            && BlockGetter::get_fluid_state(level, zombie.base().block_position()).is_tag("water");
        zombie.set_swimming(swimming);
    }
}
