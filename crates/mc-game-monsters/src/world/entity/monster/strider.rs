//! `net.minecraft.world.entity.monster.Strider` (+ `StriderGoToLavaGoal`, `StriderPathNavigation`).

use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState, PathComputationType};
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::fluid::Fluid;
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::move_to_block::{self, MoveToBlock, MoveToBlockData};
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, Flag, FollowParentGoal, Goal, GoalFlags, LookAtPlayerGoal, PanicGoal,
    RandomLookAroundGoal, RandomStrollGoal, TemptGoal,
};
use mc_game_core::world::entity::ai::navigation::{NavClass, NavigationData, PathNavigation};
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::dismount_helper;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item_based_steering::ItemBasedSteering;
use mc_game_core::world::entity::living_entity::util::lm;
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::zombie::{ZombieGroupData, get_spawn_as_baby_odds};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::collision::BlockGetterExt as _;
use mc_game_core::world::level::material::FluidStateExt as _;
use mc_game_core::world::level::pathfinder::node_evaluator::{EvaluatorState, NodeEvaluator};
use mc_game_core::world::level::pathfinder::{PathFinder, PathType};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter, LevelReader};
use std::sync::{Arc, LazyLock};

define_accessor!(pub DATA_BOOST_TIME: Int = Strider);
define_accessor!(pub DATA_SUFFOCATING: Boolean = Strider);

static SUFFOCATING_MODIFIER_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::with_default_namespace("suffocating").expect("valid id"));
const SUFFOCATE_STEERING_MODIFIER: f32 = 0.35;
const STEERING_MODIFIER: f32 = 0.55;

fn suffocating_modifier() -> AttributeModifier {
    AttributeModifier {
        id: SUFFOCATING_MODIFIER_ID.clone(),
        amount: f64::from(-0.34f32),
        operation: AttributeModifierOperation::AddMultipliedBase,
    }
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.45, 0.85)
        .with_eye_height(0.4375)
        .with_attachments(
            EntityAttachments::create_default(0.45, 0.85)
                .with_passenger(Vec3::new(0.0, 0.65625, 0.0)),
        )
}

fn is_tag_item(stack: &ItemStack, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

fn is_lava(fluid: Fluid) -> bool {
    mc_data::tags::get::<Fluid>("lava").is_some_and(|t| t.contains(fluid))
}

/// `Strider.checkStriderSpawnRules`.
fn check_strider_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    let mut check_pos = pos;
    loop {
        check_pos = check_pos.relative(Direction::Up);
        if !level.get_fluid_state(check_pos).is_tag("lava") {
            break;
        }
    }
    level.get_block_state(check_pos).is_air()
}

register_spawn_placement!(EntityType::STRIDER, check_strider_spawn_rules);

/// `Strider`.
pub struct Strider {
    pub mob: MobData,
    steering: ItemBasedSteering,
    /// `getControllingPassenger()`: the saddled strider's first passenger when it is a player holding the warped fungus on a
    /// stick (refreshed every tick and when passengers change: the level is not available to the getter).
    controller: Option<EntityId>,
}

impl Strider {
    /// `new Strider(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_BOOST_TIME, 0);
            b.define(DATA_SUFFOCATING, false);
        });
        let mut strider = Strider {
            mob,
            steering: ItemBasedSteering::new(DATA_BOOST_TIME),
            controller: None,
        };
        strider.mob.living.entity.blocks_building = true;
        let navigation = StriderPathNavigation::new(&strider);
        strider.mob.set_navigation(Box::new(navigation));
        strider.register_goals();
        strider.mob.set_pathfinding_malus(PathType::Water, -1.0);
        strider.mob.set_pathfinding_malus(PathType::Lava, 0.0);
        strider
            .mob
            .set_pathfinding_malus(PathType::FireInNeighbor, 0.0);
        strider.mob.set_pathfinding_malus(PathType::Fire, 0.0);
        strider
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let breed = BreedGoal::new(self, 1.0);
        let follow_parent = FollowParentGoal::new(self, 1.0);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let look_at_strider = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| {
                e.get_type() == EntityType::STRIDER
            })),
            8.0,
        );
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(PanicGoal::new(1.65)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(
            3,
            Box::new(TemptGoal::new(
                1.4,
                Arc::new(|i: &ItemStack| is_tag_item(i, "strider_tempt_items")),
                false,
            )),
        );
        goals.add_goal(4, Box::new(StriderGoToLavaGoal::new(1.0)));
        goals.add_goal(5, Box::new(follow_parent));
        goals.add_goal(7, Box::new(RandomStrollGoal::with_interval(1.0, 60)));
        goals.add_goal(8, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        goals.add_goal(9, Box::new(look_at_strider));
    }

    /// `setSuffocating(flag)`.
    pub fn set_suffocating(&mut self, flag: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_SUFFOCATING, flag);
        if let Some(attribute) = lm(self).attributes.get_instance(Attribute::MOVEMENT_SPEED) {
            if flag {
                attribute.add_or_update_transient_modifier(suffocating_modifier());
            } else {
                attribute.remove_modifier(&SUFFOCATING_MODIFIER_ID);
            }
        }
    }

    /// `isSuffocating()`.
    pub fn is_suffocating(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_SUFFOCATING)
    }

    /// `isSaddled()`.
    pub fn is_saddled(&self) -> bool {
        !self.get_item_by_slot(EquipmentSlot::Saddle).is_empty()
    }

    /// `isBeingTempted()`.
    fn is_being_tempted(&self) -> bool {
        self.mob
            .goal_selector
            .find_goal::<TemptGoal>()
            .is_some_and(TemptGoal::is_running)
    }

    fn refresh_controller(&mut self, level: &ServerLevel) {
        let first = self.base().passengers.first().copied();
        self.controller = first
            .filter(|_| self.is_saddled())
            .and_then(|id| level.get_entity(id))
            .filter(|p| holds_warped_fungus_on_a_stick(*p))
            .map(|p| p.id());
    }

    /// `floatStrider()`.
    fn float_strider(&mut self, level: &mut ServerLevel) {
        if self.is_in_lava() {
            let context = mc_game_core::world::entity::movement::collision_context(self);
            let pos = self.block_position();
            if context.is_above(&self.get_liquid_collision_shape(), pos, true)
                && !level.get_fluid_state(pos.above()).is_tag("lava")
            {
                mc_game_core::world::entity::movement::set_on_ground(self, &*level, true);
            } else {
                let m = self.base().delta_movement();
                self.base_mut()
                    .set_delta_movement(m.scale(0.5).add_xyz(0.0, 0.05, 0.0));
            }
        }
    }

    /// `spawnJockey(level, difficulty, jockey, jockeyGroupData)`.
    fn spawn_jockey(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        mut jockey: Box<dyn Entity>,
        jockey_group_data: Option<Box<dyn SpawnGroupData>>,
        jockey_main_hand: Option<ItemStack>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let b = self.base();
        jockey.snap_to(b.x(), b.y(), b.z(), b.y_rot(), 0.0);
        jockey.finalize_spawn(
            level,
            difficulty,
            EntitySpawnReason::Jockey,
            jockey_group_data,
        );
        if let Some(stack) = jockey_main_hand {
            mob::equipment::set_item_slot_spawn(jockey.as_mut(), EquipmentSlot::MainHand, stack);
        }
        if mc_game_core::world::entity::monster::start_riding_unadded(jockey.as_mut(), self) {
            self.mob.pending_passengers.push(jockey);
        }
        Some(Box::new(AgeableMobGroupData::with_chance(0.0)))
    }

    /// `Strider.getDismountLocationForPassenger` up to the fallback to `super`.
    fn find_dismount_location(&self, level: &ServerLevel, passenger: &dyn Entity) -> Option<Vec3> {
        let (bb_width, passenger_width, y_rot) = (
            self.base().get_bb_width(),
            passenger.base().get_bb_width(),
            passenger.base().y_rot(),
        );
        let directions = [
            dismount_helper::get_collision_horizontal_escape_vector(
                bb_width,
                passenger_width,
                y_rot,
            ),
            dismount_helper::get_collision_horizontal_escape_vector(
                bb_width,
                passenger_width,
                y_rot - 22.5,
            ),
            dismount_helper::get_collision_horizontal_escape_vector(
                bb_width,
                passenger_width,
                y_rot + 22.5,
            ),
            dismount_helper::get_collision_horizontal_escape_vector(
                bb_width,
                passenger_width,
                y_rot - 45.0,
            ),
            dismount_helper::get_collision_horizontal_escape_vector(
                bb_width,
                passenger_width,
                y_rot + 45.0,
            ),
        ];
        let mut target_block_positions: Vec<BlockPos> = Vec::new();
        let collider_top = self.base().bounding_box().max_y;
        let collider_bottom = self.base().bounding_box().min_y - 0.5;
        for direction in directions {
            let mut block_pos = BlockPos::containing(
                self.base().x() + direction.x,
                collider_top,
                self.base().z() + direction.z,
            );
            let mut y = collider_top;
            while y > collider_bottom {
                if !target_block_positions.contains(&block_pos) {
                    target_block_positions.push(block_pos);
                }
                block_pos = block_pos.below();
                y -= 1.0;
            }
        }
        for target_block_pos in target_block_positions {
            if !level.get_fluid_state(target_block_pos).is_tag("lava") {
                let block_floor_height = level.get_block_floor_height(target_block_pos);
                if dismount_helper::is_block_floor_valid(block_floor_height) {
                    let location =
                        Vec3::up_from_bottom_center_of(&target_block_pos, block_floor_height);
                    for dismount_pose in dismount_helper::get_dismount_poses(passenger) {
                        let pose_collision_box =
                            dismount_helper::get_local_bounds_for_pose(passenger, *dismount_pose);
                        if dismount_helper::can_dismount_to(
                            level,
                            passenger,
                            &pose_collision_box.move_vec(location),
                        ) {
                            return Some(location);
                        }
                    }
                }
            }
        }
        None
    }

    fn above_dismount_location(&self) -> Vec3 {
        Vec3::new(
            self.base().x(),
            self.base().bounding_box().max_y,
            self.base().z(),
        )
    }
}

/// `player.isHolding(Items.WARPED_FUNGUS_ON_A_STICK)` for a player entity.
fn holds_warped_fungus_on_a_stick(entity: &dyn Entity) -> bool {
    entity.is_player()
        && (entity
            .get_item_by_slot(EquipmentSlot::MainHand)
            .is(Item::WARPED_FUNGUS_ON_A_STICK)
            || entity
                .get_item_by_slot(EquipmentSlot::OffHand)
                .is(Item::WARPED_FUNGUS_ON_A_STICK))
}

impl Entity for Strider {
    mc_game_core::mob_entity_accessors!(mob);

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle
            || mob::equipment::can_dispenser_equip_into_slot(self.as_dyn())
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        if slot != EquipmentSlot::Saddle {
            true
        } else {
            self.is_alive() && !self.is_baby()
        }
    }

    fn get_equip_sound_override(&self, slot: EquipmentSlot) -> Option<SoundEvent> {
        (slot == EquipmentSlot::Saddle).then_some(SoundEvent::ENTITY_STRIDER_SADDLE)
    }

    fn can_stand_on_fluid_fn(
        &self,
    ) -> Option<mc_game_core::world::level::collision::CanStandOnFluid> {
        Some(is_lava)
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        level.is_unobstructed_entity(self)
    }

    fn get_controlling_passenger(&self) -> Option<EntityId> {
        self.controller
    }

    fn add_passenger(&mut self, passenger: &dyn Entity, first_passenger_is_player: bool) {
        let id = passenger.id();
        let is_player = passenger.is_player();
        let passengers = &mut self.mob.living.entity.passengers;
        if passengers.is_empty() || !(is_player && !first_passenger_is_player) {
            passengers.push(id);
        } else {
            passengers.insert(0, id);
        }
        if self.base().passengers.first() == Some(&id) && is_player && self.is_saddled() {
            self.controller = holds_warped_fungus_on_a_stick(passenger).then_some(id);
        }
    }

    fn on_passenger_removed(&mut self, level: &mut ServerLevel, _passenger: EntityId) {
        self.refresh_controller(level);
    }

    fn get_dismount_location_for_passenger(
        &self,
        level: &ServerLevel,
        passenger: &dyn Entity,
    ) -> Vec3 {
        self.find_dismount_location(level, passenger)
            .unwrap_or_else(|| self.above_dismount_location())
    }

    /// The dismount spot of the strider's own search, then `Animal`'s (which also sets the pose), then the spot above it.
    fn get_dismount_location_for_passenger_setting_pose(
        &self,
        level: &ServerLevel,
        passenger: &mut dyn Entity,
    ) -> Vec3 {
        self.find_dismount_location(level, &*passenger)
            .or_else(|| self.animal_dismount_location(level, passenger))
            .unwrap_or_else(|| self.above_dismount_location())
    }

    fn tick_ridden(&mut self, level: &mut ServerLevel, controller: EntityId, _ridden_input: Vec3) {
        let Some((y_rot, x_rot)) = level
            .get_entity(controller)
            .map(|c| (c.base().y_rot(), c.base().x_rot()))
        else {
            return;
        };
        self.set_rot(y_rot, x_rot * 0.5);
        let y = self.base().y_rot();
        self.base_mut().y_rot_o = y;
        self.mob.living.y_body_rot = y;
        self.set_y_head_rot(y);
        self.steering
            .tick_boost(&self.mob.living.entity.entity_data);
    }

    fn get_ridden_input(
        &self,
        _level: &ServerLevel,
        _controller: EntityId,
        _self_input: Vec3,
    ) -> Vec3 {
        Vec3::new(0.0, 0.0, 1.0)
    }

    fn get_ridden_speed(&self, _level: &ServerLevel, _controller: EntityId) -> f32 {
        let steering = if self.is_suffocating() {
            SUFFOCATE_STEERING_MODIFIER
        } else {
            STEERING_MODIFIER
        };
        (mob::attribute_value(self, Attribute::MOVEMENT_SPEED)
            * f64::from(steering)
            * f64::from(
                self.steering
                    .boost_factor(&self.mob.living.entity.entity_data),
            )) as f32
    }

    fn next_step(&self) -> f32 {
        self.base().move_dist + 0.6
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {
        let sound = if self.is_in_lava() {
            SoundEvent::ENTITY_STRIDER_STEP_LAVA
        } else {
            SoundEvent::ENTITY_STRIDER_STEP
        };
        self.play_sound(level, sound, 1.0, 1.0);
    }

    fn boost(&mut self) -> bool {
        let data = &mut self.mob.living.entity.entity_data;
        self.steering
            .boost(data, &mut self.mob.living.entity.random)
    }

    fn check_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        ya: f64,
        on_ground: bool,
        on_state: BlockState,
        pos: BlockPos,
    ) {
        if self.is_in_lava() {
            self.reset_fall_distance();
        } else {
            mc_game_core::world::entity::entity::check_fall_damage_base(
                self, level, ya, on_ground, on_state, pos,
            );
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.refresh_controller(level);
        if self.is_being_tempted() && self.base_mut().random.next_int_bounded(140) == 0 {
            mc_game_core::world::entity::living_entity::damage::make_sound(
                self,
                level,
                SoundEvent::ENTITY_STRIDER_HAPPY,
            );
        } else if mob::pathfinder::is_panicking(self)
            && self.base_mut().random.next_int_bounded(60) == 0
        {
            mc_game_core::world::entity::living_entity::damage::make_sound(
                self,
                level,
                SoundEvent::ENTITY_STRIDER_RETREAT,
            );
        }
        if !self.mob.is_no_ai() {
            let state_inside = level.get_block_state(self.base().block_position());
            let state_on = level.get_block_state(
                mc_game_core::world::entity::movement::get_on_pos_legacy(self, level),
            );
            let in_warm_blocks = state_inside.block().is_in_tag("strider_warm_blocks")
                || state_on.block().is_in_tag("strider_warm_blocks")
                || self.get_fluid_height("lava") > 0.0;
            let on_warm_strider = self
                .base()
                .vehicle
                .and_then(|v| level.get_entity(v))
                .and_then(|v| v.as_any().downcast_ref::<Strider>())
                .is_some_and(|s| !s.is_suffocating());
            self.set_suffocating(!in_warm_blocks && !on_warm_strider);
        }
        mob::tick::tick(self, level);
        self.float_strider(level);
    }

    fn should_passengers_inherit_malus(&self) -> bool {
        true
    }

    fn get_liquid_collision_shape(&self) -> VoxelShape {
        // `Block.column(16.0, 0.0, 8.0)`
        Shapes::box_(0.0, 0.0, 0.0, 1.0, 0.5, 1.0)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if !mob::pathfinder::is_panicking(self) && !self.is_being_tempted() {
            Some(SoundEvent::ENTITY_STRIDER_AMBIENT)
        } else {
            None
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_STRIDER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_STRIDER_DEATH)
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        !self.base().is_vehicle() && !self.is_eye_in_fluid("lava")
    }

    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn is_on_fire(&self) -> bool {
        false
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if level.get_block_state(pos).get_fluid_state().is_tag("lava") {
            10.0
        } else if self.is_in_lava() {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        mc_game_core::world::entity::factory::create(
            EntityType::STRIDER,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_tag_item(stack, "strider_food")
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let has_food = self.is_food(&player.get_item_in_hand(hand));
        if !has_food
            && self.is_saddled()
            && !self.base().is_vehicle()
            && !player.is_shift_key_down()
        {
            level.start_riding_held(player, self, false, true);
            return InteractionResult::SUCCESS;
        }
        let result = mob::interact::mob_interact(self, level, player, hand);
        if !result.consumes_action() {
            let item_stack = player.get_item_in_hand(hand);
            if self.is_equippable_in_slot(&item_stack, EquipmentSlot::Saddle) {
                mob::interact::with_hand_stack(player, hand, |s, p| {
                    mc_game_core::world::item::interact_living_entity(
                        level,
                        s,
                        p,
                        self.as_dyn_mut(),
                        hand,
                    )
                })
            } else {
                InteractionResult::PASS
            }
        } else {
            if has_food && !self.base().is_silent() {
                let p = self.position();
                let source = self.get_sound_source();
                let pitch = 1.0
                    + (self.base_mut().random.next_float() - self.base_mut().random.next_float())
                        * 0.2;
                level.play_sound_at(
                    None,
                    p.x,
                    p.y,
                    p.z,
                    SoundEvent::ENTITY_STRIDER_EAT,
                    source,
                    1.0,
                    pitch,
                );
            }
            result
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if self.is_baby() {
            return mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data);
        }
        let mut group_data = group_data;
        let spawning_monsters = level
            .as_server_level()
            .is_some_and(ServerLevel::is_spawning_monsters);
        if level.random().next_int_bounded(30) == 0 && spawning_monsters {
            if let Some(jockey) = level.create_entity(
                EntityType::ZOMBIFIED_PIGLIN,
                EntitySpawnRequest {
                    reason: EntitySpawnReason::Jockey,
                    ignore_checks: false,
                },
            ) {
                let baby = get_spawn_as_baby_odds(level.random());
                group_data = self.spawn_jockey(
                    level,
                    difficulty,
                    jockey,
                    Some(Box::new(ZombieGroupData::new(baby, false))),
                    Some(ItemStack::new(Item::WARPED_FUNGUS_ON_A_STICK)),
                );
                mob::equipment::set_item_slot_spawn(
                    self,
                    EquipmentSlot::Saddle,
                    ItemStack::new(Item::SADDLE),
                );
                mob::equipment::set_guaranteed_drop(self, EquipmentSlot::Saddle);
            }
        } else if level.random().next_int_bounded(10) == 0 {
            if let Some(mut jockey) = level.create_entity(
                EntityType::STRIDER,
                EntitySpawnRequest {
                    reason: EntitySpawnReason::Jockey,
                    ignore_checks: false,
                },
            ) {
                mob::ageable::set_age(jockey.as_mut(), -24000);
                group_data = self.spawn_jockey(level, difficulty, jockey, None, None);
            }
        } else {
            group_data = Some(Box::new(AgeableMobGroupData::with_chance(0.5)));
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }
}

/// `Strider.StriderGoToLavaGoal`.
struct StriderGoToLavaGoal {
    data: MoveToBlockData,
}

impl StriderGoToLavaGoal {
    fn new(speed_modifier: f64) -> Self {
        Self {
            data: MoveToBlockData::with_vertical(speed_modifier, 8, 2),
        }
    }
}

impl MoveToBlock for StriderGoToLavaGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.data
    }

    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.data
    }

    fn get_move_to_target(&self) -> BlockPos {
        self.data.block_pos
    }

    fn should_recalculate_path(&self) -> bool {
        self.data.try_ticks % 20 == 0
    }

    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        level.get_block_state(pos).block() == Block::LAVA
            && level
                .get_block_state(pos.above())
                .is_pathfindable_by(PathComputationType::Land)
    }
}

impl Goal for StriderGoToLavaGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !mob.is_in_lava() && move_to_block::can_use(self, mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !mob.is_in_lava() && self.is_valid_target(mob, level, self.data.block_pos)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::start(self, mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}

/// `WalkNodeEvaluator` of `Strider.StriderPathNavigation.createPathFinder` (no `setCanPassDoors(true)`).
struct StriderEvaluator(EvaluatorState);

impl NodeEvaluator for StriderEvaluator {
    fn state(&self) -> &EvaluatorState {
        &self.0
    }
    fn state_mut(&mut self) -> &mut EvaluatorState {
        &mut self.0
    }
}

/// `Strider.StriderPathNavigation`.
struct StriderPathNavigation {
    data: NavigationData,
}

impl StriderPathNavigation {
    fn new(mob: &dyn Entity) -> Self {
        let data = NavigationData::with_path_finder(mob, NavClass::Ground, |max_visited_nodes| {
            let mut evaluator = StriderEvaluator(EvaluatorState::walk());
            evaluator.set_can_pass_doors(false);
            PathFinder::new(Box::new(evaluator), max_visited_nodes)
        });
        Self { data }
    }
}

impl PathNavigation for StriderPathNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }

    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }

    fn is_stable_destination(&self, mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        level.get_block_state(pos).block() == Block::LAVA
            || mc_game_core::world::entity::ai::navigation::default_is_stable_destination(
                self, mob, level, pos,
            )
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static STRIDER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::STRIDER,
    create: |ty, id, registries| Box::new(Strider::new(ty, id, registries)),
};
