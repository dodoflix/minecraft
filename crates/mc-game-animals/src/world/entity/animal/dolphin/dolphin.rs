//! `net.minecraft.world.entity.animal.dolphin.Dolphin` with its private goals (`DolphinSwimToTreasureGoal`,
//! `DolphinSwimWithPlayerGoal`, `MoveToItemGoal`, `PlayWithItemsGoal`).

use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::PathComputationType;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_game_ai::world::entity::ai::goal::{
    AvoidEntityGoal, BreathAirGoal, DolphinJumpGoal, FollowPlayerRiddenEntityGoal,
    RandomSwimmingGoal, TryFindLiquidGoal,
};
use mc_game_core::define_accessor;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::smooth_swimming_look_control::SmoothSwimmingLookControl;
use mc_game_core::world::entity::ai::control::smooth_swimming_move_control::SmoothSwimmingMoveControl;
use mc_game_core::world::entity::ai::goal::target::{HurtByTargetGoal, class_of_type};
use mc_game_core::world::entity::ai::goal::{
    Flag, Goal, GoalFlags, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::ai::util::default_random_pos;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable, navigation};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, MoverType, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::level_access::BlockGetter as _;
use mc_game_core::world::level::material::FluidStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use std::sync::LazyLock;

define_accessor!(pub GOT_FISH: Boolean = Dolphin);
define_accessor!(pub MOISTNESS_LEVEL: Int = Dolphin);

/// `SWIM_WITH_PLAYER_TARGETING`.
static SWIM_WITH_PLAYER_TARGETING: LazyLock<TargetingConditions> = LazyLock::new(|| {
    TargetingConditions::for_non_combat()
        .range(10.0)
        .ignore_line_of_sight()
});

pub const TOTAL_AIR_SUPPLY: i32 = 4800;
const TOTAL_MOISTNESS_LEVEL: i32 = 2400;
pub const BABY_SCALE: f32 = 0.65;

const DEG_TO_RAD_F: f32 = (std::f64::consts::PI / 180.0) as f32;
const TWO_PI_F: f32 = (std::f64::consts::PI * 2.0) as f32;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::of_type(EntityType::DOLPHIN)
        .scale(BABY_SCALE)
        .with_eye_height(0.09375);
    dims.with_attachments(
        EntityAttachments::create_default(dims.width, dims.height)
            .with_passenger(Vec3::new(0.0, 0.3125, 0.0)),
    )
}

/// `ALLOWED_ITEMS`: `!e.hasPickUpDelay() && e.isAlive() && e.isInWater()`.
fn allowed_item(e: &dyn Entity) -> bool {
    e.as_any()
        .downcast_ref::<ItemEntity>()
        .is_some_and(|i| !i.has_pick_up_delay() && i.is_alive() && i.is_in_water())
}

pub struct Dolphin {
    pub mob: MobData,
    pub treasure_pos: Option<BlockPos>,
}

impl Dolphin {
    /// `new Dolphin(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob =
            MobData::new_with_air_supply(ty, id, MobKind::AgeableMob, TOTAL_AIR_SUPPLY, |b| {
                b.define(GOT_FISH, false);
                b.define(MOISTNESS_LEVEL, TOTAL_MOISTNESS_LEVEL);
            });
        let mut dolphin = Dolphin {
            mob,
            treasure_pos: None,
        };
        let navigation = StandardPathNavigation::water_bound(&dolphin);
        dolphin
            .mob
            .set_pathfinding_malus(mc_game_core::world::level::pathfinder::PathType::Water, 0.0);
        dolphin.mob.set_navigation(Box::new(navigation));
        dolphin.register_goals();
        dolphin.mob.move_control = Some(Box::new(SmoothSwimmingMoveControl::new(
            85, 10, 0.02, 0.1, true,
        )));
        dolphin.mob.look_control = Some(Box::new(SmoothSwimmingLookControl::new(10)));
        dolphin.mob.can_pick_up_loot = true;
        dolphin
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let breath = BreathAirGoal::new();
        let find_liquid = TryFindLiquidGoal::new("dolphin_tries_to_find");
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let avoid_guardian = AvoidEntityGoal::new(
            std::sync::Arc::new(|e| e.get_type() == EntityType::GUARDIAN),
            8.0,
            1.0,
            1.0,
        );
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(breath));
        goals.add_goal(0, Box::new(find_liquid));
        goals.add_goal(1, Box::new(DolphinSwimToTreasureGoal { stuck: false }));
        goals.add_goal(
            2,
            Box::new(DolphinSwimWithPlayerGoal {
                speed_modifier: 4.0,
                player: None,
            }),
        );
        goals.add_goal(4, Box::new(RandomSwimmingGoal::new(1.0, 10)));
        goals.add_goal(4, Box::new(RandomLookAroundGoal::new()));
        goals.add_goal(5, Box::new(look_at));
        goals.add_goal(5, Box::new(DolphinJumpGoal::new(10)));
        goals.add_goal(6, Box::new(MeleeAttackGoal::new(f64::from(1.2f32), true)));
        goals.add_goal(7, Box::new(MoveToItemGoal { cooldown: 0 }));
        goals.add_goal(8, Box::new(PlayWithItemsGoal));
        goals.add_goal(
            8,
            Box::new(FollowPlayerRiddenEntityGoal::new(is_abstract_boat)),
        );
        goals.add_goal(
            8,
            Box::new(FollowPlayerRiddenEntityGoal::new(is_abstract_nautilus)),
        );
        goals.add_goal(9, Box::new(avoid_guardian));
        self.mob.target_selector.add_goal(
            1,
            Box::new(
                HurtByTargetGoal::new(vec![class_of_type(EntityType::GUARDIAN)])
                    .set_alert_others(vec![]),
            ),
        );
    }

    /// `gotFish()`.
    pub fn got_fish(&self) -> bool {
        self.mob.living.entity.entity_data.get(GOT_FISH)
    }

    /// `setGotFish(gotFish)`.
    pub fn set_got_fish(&mut self, got_fish: bool) {
        self.mob.living.entity.entity_data.set(GOT_FISH, got_fish);
    }

    /// `getMoistnessLevel()`.
    pub fn get_moistness_level(&self) -> i32 {
        self.mob.living.entity.entity_data.get(MOISTNESS_LEVEL)
    }

    /// `setMoisntessLevel(level)`.
    pub fn set_moistness_level(&mut self, level: i32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(MOISTNESS_LEVEL, level);
    }

    /// `closeToNextPos()`.
    fn close_to_next_pos(&self, level: &ServerLevel) -> bool {
        match navigation::with_navigation_ref(self, level, |n| n.get_target_pos()).flatten() {
            Some(target) => target.closer_to_center_than(&self.position(), 12.0),
            None => false,
        }
    }
}

fn is_abstract_boat(e: &dyn Entity) -> bool {
    let name = e.get_type().data().name;
    name.ends_with("_boat") || name.ends_with("_raft")
}

fn is_abstract_nautilus(e: &dyn Entity) -> bool {
    matches!(
        e.get_type(),
        EntityType::NAUTILUS | EntityType::ZOMBIE_NAUTILUS
    )
}

impl Entity for Dolphin {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:dolphin")
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let max = self.get_max_air_supply();
        self.base_mut().set_air_supply(max);
        self.set_x_rot(0.0);
        let group_data =
            group_data.unwrap_or_else(|| Box::new(AgeableMobGroupData::with_chance(0.1)));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        factory::create(
            EntityType::DOLPHIN,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    // `AgeableWaterCreature`: no fluid push, no leash... but `canBeLeashed()` is true again for the dolphin, and
    // `handleAirSupply` is empty (the dolphin has its own long air supply and moistness).
    fn base_tick(&mut self, level: &mut ServerLevel) {
        mob::tick::base_tick(self, level);
    }
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }
    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::MainHand && self.can_pick_up_loot()
    }
    fn can_be_leashed(&self) -> bool {
        true
    }
    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        level.is_unobstructed_entity(self)
    }
    fn get_ambient_sound_interval(&self) -> i32 {
        crate::world::entity::animal::fish::water_animal::AMBIENT_SOUND_INTERVAL
    }
    fn get_base_experience_reward(&mut self, _level: &ServerLevel) -> i32 {
        crate::world::entity::animal::fish::water_animal::get_base_experience_reward(self)
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.get_type())
        }
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        // `!isBaby() && super.canAttack(target)`: the `Mob` / `LivingEntity` chain
        !self.is_baby()
            && target.get_type() != EntityType::GHAST
            && !(target.is_player()
                && level.difficulty() == mc_world::level_storage::Difficulty::Peaceful)
            && target.can_be_seen_as_enemy()
    }

    fn get_max_air_supply(&self) -> i32 {
        TOTAL_AIR_SUPPLY
    }

    fn increase_air_supply(&mut self, _current_supply: i32) -> i32 {
        self.get_max_air_supply()
    }

    fn get_max_head_x_rot(&self) -> i32 {
        1
    }

    fn get_max_head_y_rot(&self) -> i32 {
        1
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item_id: EntityId) {
        if !self.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            return;
        }
        let Some(item_stack) = level
            .get_entity(item_id)
            .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
            .map(|i| i.get_item())
        else {
            return;
        };
        if self.can_hold_item(&item_stack) {
            mc_game_core::world::entity::player::pickup::on_item_pickup_by_id(
                level, &*self, item_id,
            );
            self.set_item_slot(level, EquipmentSlot::MainHand, item_stack.clone());
            mob::equipment::set_guaranteed_drop(self, EquipmentSlot::MainHand);
            // `take(entity, itemStack.getCount())`
            if level.get_entity(item_id).is_some_and(|e| !e.is_removed()) {
                let packet = mc_protocol::packets::play::ClientboundTakeItemEntityPacket {
                    item_id,
                    player_id: self.id(),
                    amount: item_stack.count(),
                };
                level.entity_tracker.send_to_tracking_players(
                    item_id,
                    &mc_game_core::server::level::server_entity::encode_payload(&packet),
                );
            }
            level.with_entity(item_id, |level, e| e.discard_in_level(level));
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.mob.is_no_ai() {
            let max = self.get_max_air_supply();
            self.base_mut().set_air_supply(max);
        } else {
            if self.is_in_water_or_rain(level) {
                self.set_moistness_level(TOTAL_MOISTNESS_LEVEL);
            } else {
                let moistness = self.get_moistness_level() - 1;
                self.set_moistness_level(moistness);
                if self.get_moistness_level() <= 0 {
                    mc_game_core::world::entity::living_entity::util::hurt(
                        level,
                        self,
                        |sources| sources.dry_out(),
                        1.0,
                    );
                }
                if self.base().on_ground() {
                    let dx = (self.base_mut().random.next_float() * 2.0 - 1.0) * 0.2;
                    let dz = (self.base_mut().random.next_float() * 2.0 - 1.0) * 0.2;
                    let dm = self.base().delta_movement().add(Vec3::new(
                        f64::from(dx),
                        0.5,
                        f64::from(dz),
                    ));
                    self.base_mut().set_delta_movement(dm);
                    let y_rot = self.base_mut().random.next_float() * 360.0;
                    self.set_y_rot(y_rot);
                    mc_game_core::world::entity::movement::set_on_ground(self, &*level, false);
                    self.base_mut().needs_sync = true;
                }
            }
            // `level().isClientSide() && ...`: client-side dolphin particles
        }
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if !item_stack.is_empty()
            && mc_data::tags::get::<Item>("fishes").is_some_and(|t| t.contains(item_stack.item()))
        {
            self.play_sound(level, SoundEvent::ENTITY_DOLPHIN_EAT, 1.0, 1.0);
            if ageable::can_age_up(self) {
                mob::interact::use_player_item(level, player, hand);
                let seconds = ageable::get_speed_up_seconds_when_feeding(-ageable::get_age(self));
                ageable::age_up(self, level, seconds, true);
            } else {
                self.set_got_fish(true);
                mob::interact::use_player_item(level, player, hand);
            }
            InteractionResult::SUCCESS
        } else {
            mob::interact::mob_interact(self, level, player, hand)
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DOLPHIN_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DOLPHIN_DEATH)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_in_water() {
            SoundEvent::ENTITY_DOLPHIN_AMBIENT_WATER
        } else {
            SoundEvent::ENTITY_DOLPHIN_AMBIENT
        })
    }

    fn get_swim_splash_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_DOLPHIN_SPLASH
    }

    fn get_swim_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_DOLPHIN_SWIM
    }

    fn travel_in_water(
        &mut self,
        level: &mut ServerLevel,
        input: Vec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) {
        let speed = self.get_speed();
        self.move_relative(speed, input);
        let dm = self.base().delta_movement();
        self.r#move(level, MoverType::SelfMover, dm);
        let scaled = self.base().delta_movement().scale(0.9);
        self.base_mut().set_delta_movement(scaled);
        if self.get_target().is_none() {
            let dm = self
                .base()
                .delta_movement()
                .add(Vec3::new(0.0, -0.005, 0.0));
            self.base_mut().set_delta_movement(dm);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("GotFish", self.got_fish());
        out.put_int("Moistness", self.get_moistness_level());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_got_fish(input.get_boolean_or("GotFish", false));
        self.set_moistness_level(input.get_int_or("Moistness", TOTAL_MOISTNESS_LEVEL));
    }
}

/// `level.findNearestMapStructure(DOLPHIN_LOCATED, pos, 50, false)`. Not implemented: always `None`, so the dolphin finds
/// no structure and gives up like vanilla's "no structure" branch.
fn find_nearest_map_structure(
    _level: &mut ServerLevel,
    _wanted: &[Identifier],
    _pos: BlockPos,
    _radius: i32,
) -> Option<BlockPos> {
    None
}

/// `Dolphin.DolphinSwimToTreasureGoal`.
struct DolphinSwimToTreasureGoal {
    stuck: bool,
}

impl DolphinSwimToTreasureGoal {
    /// `BlockPos.containing(treasurePos.getX(), dolphin.getY(), treasurePos.getZ()).closerToCenterThan(dolphin.position(), 4.0)`.
    fn reached(treasure_pos: BlockPos, dolphin: &dyn Entity) -> bool {
        BlockPos::containing(
            f64::from(treasure_pos.x()),
            dolphin.base().y(),
            f64::from(treasure_pos.z()),
        )
        .closer_to_center_than(&dolphin.position(), 4.0)
    }
}

fn dolphin(mob: &mut dyn Entity) -> &mut Dolphin {
    mob.as_any_mut()
        .downcast_mut::<Dolphin>()
        .expect("dolphin goal on a dolphin")
}

impl Goal for DolphinSwimToTreasureGoal {
    fn is_interruptable(&self) -> bool {
        false
    }

    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        let d = dolphin(mob);
        d.got_fish() && d.base().get_air_supply() >= 100
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        let stuck = self.stuck;
        let d = dolphin(mob);
        match d.treasure_pos {
            None => false,
            Some(treasure_pos) => {
                !Self::reached(treasure_pos, d) && !stuck && d.base().get_air_supply() >= 100
            }
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stuck = false;
        navigation::stop(mob, level);
        let dolphin_pos = mob.block_position();
        let wanted: Vec<Identifier> = {
            let registries = &level.registries;
            let tag = Identifier::new("minecraft", "dolphin_located").expect("valid id");
            match (
                registries.registry(keys::STRUCTURE),
                registries.tag(keys::STRUCTURE, &tag),
            ) {
                (Some(registry), Some(ids)) => {
                    ids.iter().filter_map(|id| registry.name_of(*id)).collect()
                }
                _ => Vec::new(),
            }
        };
        let treasure_pos = find_nearest_map_structure(level, &wanted, dolphin_pos, 50);
        match treasure_pos {
            Some(pos) => {
                dolphin(mob).treasure_pos = Some(pos);
                broadcast_entity_event(level, mob.id(), 38);
            }
            None => self.stuck = true,
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let stuck = self.stuck;
        let d = dolphin(mob);
        if d.treasure_pos
            .is_none_or(|treasure_pos| Self::reached(treasure_pos, d))
            || stuck
        {
            d.set_got_fish(false);
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(treasure_block) = dolphin(mob).treasure_pos else {
            return;
        };
        let close = dolphin(mob).close_to_next_pos(level);
        if close || navigation::is_done(mob, level) {
            let treasure_pos = Vec3::at_center_of(&treasure_block);
            let mut next_pos = default_random_pos::get_pos_towards(
                mob,
                level,
                16,
                1,
                treasure_pos,
                f64::from((std::f64::consts::PI / 8.0) as f32),
            );
            if next_pos.is_none() {
                next_pos = default_random_pos::get_pos_towards(
                    mob,
                    level,
                    8,
                    4,
                    treasure_pos,
                    f64::from((std::f64::consts::PI / 2.0) as f32),
                );
            }
            if let Some(candidate) = next_pos {
                let next = BlockPos::containing_position(&candidate);
                if !level.get_fluid_state(next).is_tag("water")
                    || !level
                        .get_block_state(next)
                        .is_pathfindable_by(PathComputationType::Water)
                {
                    next_pos = default_random_pos::get_pos_towards(
                        mob,
                        level,
                        8,
                        5,
                        treasure_pos,
                        f64::from((std::f64::consts::PI / 2.0) as f32),
                    );
                }
            }
            let Some(next_pos) = next_pos else {
                self.stuck = true;
                return;
            };
            let y_rot_speed = (mob.get_max_head_y_rot() + 20) as f32;
            let x_rot_angle = mob.get_max_head_x_rot() as f32;
            mob.mob_mut()
                .expect("mob")
                .look_control_mut()
                .set_look_at_full(next_pos.x, next_pos.y, next_pos.z, y_rot_speed, x_rot_angle);
            navigation::move_to(mob, level, next_pos.x, next_pos.y, next_pos.z, 1.3);
            let bound = self.adjusted_tick_delay(80);
            if level.random().next_int_bounded(bound) == 0 {
                broadcast_entity_event(level, mob.id(), 38);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

/// `Dolphin.DolphinSwimWithPlayerGoal`.
struct DolphinSwimWithPlayerGoal {
    speed_modifier: f64,
    player: Option<EntityId>,
}

impl DolphinSwimWithPlayerGoal {
    fn give_grace(&self, level: &mut ServerLevel, dolphin: EntityId) {
        if let Some(player) = self.player {
            level.with_entity(player, |level, p| {
                p.add_effect(
                    level,
                    MobEffectInstance::of(MobEffect::DOLPHINS_GRACE, 100, 0),
                    Some(dolphin),
                );
            });
        }
    }
}

impl Goal for DolphinSwimWithPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let (x, y, z) = (mob.base().x(), mob.base().y(), mob.base().z());
        self.player =
            mob::ext::get_nearest_player(level, &SWIM_WITH_PLAYER_TARGETING, mob, x, y, z);
        match self.player {
            None => false,
            Some(player) => {
                level.get_entity(player).is_some_and(|p| p.is_swimming())
                    && mob.get_target() != Some(player)
            }
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.player
            .and_then(|id| level.get_entity(id))
            .is_some_and(|p| {
                p.is_swimming() && mob.base().distance_to_sqr_vec(p.position()) < 256.0
            })
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.give_grace(level, mob.id());
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.player = None;
        navigation::stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(player) = self.player else { return };
        let y_rot_speed = (mob.get_max_head_y_rot() + 20) as f32;
        let x_rot_angle = mob.get_max_head_x_rot() as f32;
        let Some(target) = level.get_entity(player) else {
            return;
        };
        let (target_pos, target_block, swimming) = (
            target.position(),
            target.block_position(),
            target.is_swimming(),
        );
        mob::look_at_entity_full(mob, target, y_rot_speed, x_rot_angle);
        if mob.base().distance_to_sqr_vec(target_pos) < 6.25 {
            navigation::stop(mob, level);
        } else {
            navigation::move_to_entity(mob, level, target_block, self.speed_modifier);
        }
        if swimming && level.random().next_int_bounded(6) == 0 {
            self.give_grace(level, mob.id());
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

/// `Dolphin.ItemGoal.drop(itemStack)`.
fn drop_stack(dolphin: &mut dyn Entity, level: &mut ServerLevel, item_stack: ItemStack) {
    if item_stack.is_empty() {
        return;
    }
    let y_hand_pos = dolphin.base().get_eye_y() - f64::from(0.3f32);
    let (x, z) = (dolphin.base().x(), dolphin.base().z());
    let mut thrown_item = ItemEntity::with_stack(level, x, y_hand_pos, z, item_stack);
    thrown_item.set_pick_up_delay(40);
    thrown_item.set_thrower(dolphin.uuid());
    let pow = 0.3f32;
    let dir = dolphin.base_mut().random.next_float() * TWO_PI_F;
    let pow2 = 0.02f32 * dolphin.base_mut().random.next_float();
    let y_rot = dolphin.base().y_rot() * DEG_TO_RAD_F;
    let x_rot = dolphin.base().x_rot() * DEG_TO_RAD_F;
    let dx = pow * -mth::sin(f64::from(y_rot)) * mth::cos(f64::from(x_rot))
        + mth::cos(f64::from(dir)) * pow2;
    let dy = pow * mth::sin(f64::from(x_rot)) * 1.5;
    let dz = pow * mth::cos(f64::from(y_rot)) * mth::cos(f64::from(x_rot))
        + mth::sin(f64::from(dir)) * pow2;
    thrown_item.base_mut().set_delta_movement(Vec3::new(
        f64::from(dx),
        f64::from(dy),
        f64::from(dz),
    ));
    level.add_fresh_entity(Box::new(thrown_item));
}

/// `Dolphin.ItemGoal.dropItem()`.
fn drop_item(dolphin: &mut dyn Entity, level: &mut ServerLevel) -> bool {
    let item_stack = dolphin.get_item_by_slot(EquipmentSlot::MainHand);
    if item_stack.is_empty() {
        false
    } else {
        drop_stack(dolphin, level, item_stack);
        dolphin.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
        true
    }
}

/// The ids of the allowed items within 8 blocks, in entity iteration order.
fn nearby_items(dolphin: &dyn Entity, level: &ServerLevel) -> Vec<EntityId> {
    let bb = dolphin.base().bounding_box().inflate(8.0, 8.0, 8.0);
    level.get_entities_of_class(&bb, |e| e.as_any().is::<ItemEntity>(), allowed_item)
}

/// `Dolphin.MoveToItemGoal`.
struct MoveToItemGoal {
    cooldown: i32,
}

impl Goal for MoveToItemGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.cooldown > mob.base().tick_count {
            return false;
        }
        !nearby_items(mob, level).is_empty()
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if drop_item(mob, level) {
            self.cooldown = mob
                .base()
                .tick_count
                .wrapping_add(mob.base_mut().random.next_int_bounded(100));
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let items = nearby_items(mob, level);
        if let Some(&first) = items.first() {
            if let Some(pos) = level.get_entity(first).map(|i| i.block_position()) {
                navigation::move_to_entity(mob, level, pos, f64::from(1.2f32));
            }
            mob.play_sound(level, SoundEvent::ENTITY_DOLPHIN_PLAY, 1.0, 1.0);
        }
        self.cooldown = 0;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let items = nearby_items(mob, level);
        if let Some(&first) = items.first()
            && let Some(pos) = level.get_entity(first).map(|i| i.block_position())
        {
            navigation::move_to_entity(mob, level, pos, f64::from(1.2f32));
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Dolphin.PlayWithItemsGoal`.
struct PlayWithItemsGoal;

impl Goal for PlayWithItemsGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !mob.get_item_by_slot(EquipmentSlot::MainHand).is_empty()
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        drop_item(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        drop_item(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static DOLPHIN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::DOLPHIN,
    create: |ty, id, _registries| Box::new(Dolphin::new(ty, id)),
};
