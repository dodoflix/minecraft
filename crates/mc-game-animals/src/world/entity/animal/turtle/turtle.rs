//! `Turtle`: egg-laying amphibious animal with a home beach.

use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::props;
use mc_data::{Attribute, EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::{MoveControl, MoveControlBase, Operation};
use mc_game_core::world::entity::ai::goal::move_to_block;
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, Flag, Goal, GoalFlags, LookAtPlayerGoal, MoveToBlock, MoveToBlockData, PanicGoal,
    RandomStrollGoal, TemptGoal,
};
use mc_game_core::world::entity::ai::navigation::{NavClass, NavigationData, PathNavigation};
use mc_game_core::world::entity::ai::util::default_random_pos;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::experience_orb::ExperienceOrb;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::lightning_bolt::LightningBolt;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable, animal, ext, navigation};
use mc_game_core::world::entity::movement::MovementLevel as _;
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId, MoverType};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::BlockGetter as _;
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::block::impls::turtle_egg_block;
use mc_game_core::world::level::level_access::LevelAccessor as _;
use mc_game_core::world::level::material::FluidStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::SoundSource;
use std::sync::Arc;

define_accessor!(pub HAS_EGG: Boolean = Turtle);
define_accessor!(pub LAYING_EGG: Boolean = Turtle);

pub const BABY_SCALE: f32 = 0.3;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let base = EntityDimensions::of_type(EntityType::TURTLE);
    let attachments = EntityAttachments::create_default(base.width, base.height)
        .with_passenger(Vec3::new(0.0, f64::from(base.height), -0.25));
    base.with_attachments(attachments).scale(BABY_SCALE)
}

fn is_turtle_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("turtle_food").is_some_and(|t| t.contains(stack.item()))
}

fn turtle(e: &dyn Entity) -> &Turtle {
    e.as_any().downcast_ref::<Turtle>().expect("Turtle")
}

fn turtle_mut(e: &mut dyn Entity) -> &mut Turtle {
    e.as_any_mut().downcast_mut::<Turtle>().expect("Turtle")
}

/// `Turtle`.
pub struct Turtle {
    pub mob: MobData,
    lay_egg_counter: i32,
    home_pos: BlockPos,
    travel_pos: Option<BlockPos>,
    going_home: bool,
}

impl Turtle {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(HAS_EGG, false);
            b.define(LAYING_EGG, false);
        });
        let mut t = Turtle {
            mob,
            lay_egg_counter: 0,
            home_pos: BlockPos::ZERO,
            travel_pos: None,
            going_home: false,
        };
        t.mob.set_navigation(Box::new(TurtleNavigation {
            data: NavigationData::new(&t, NavClass::Amphibious),
        }));
        t.register_goals();
        use mc_game_core::world::level::pathfinder::PathType;
        t.mob.set_pathfinding_malus(PathType::Water, 0.0);
        t.mob.set_pathfinding_malus(PathType::DoorIronClosed, -1.0);
        t.mob.set_pathfinding_malus(PathType::DoorWoodClosed, -1.0);
        t.mob.set_pathfinding_malus(PathType::DoorOpen, -1.0);
        t.mob.move_control = Some(Box::new(TurtleMoveControl::default()));
        t
    }

    fn register_goals(&mut self) {
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let breed = BreedGoal::new(self, 1.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(TurtlePanicGoal::new(1.2)));
        goals.add_goal(1, Box::new(TurtleBreedGoal { breed }));
        goals.add_goal(1, Box::new(TurtleLayEggGoal::new(1.0)));
        goals.add_goal(
            2,
            Box::new(TemptGoal::new(
                1.1,
                Arc::new(|s: &ItemStack| is_turtle_food(s)),
                false,
            )),
        );
        goals.add_goal(3, Box::new(TurtleGoToWaterGoal::new(1.0)));
        goals.add_goal(4, Box::new(TurtleGoHomeGoal::new(1.0)));
        goals.add_goal(7, Box::new(TurtleTravelGoal::new(1.0)));
        goals.add_goal(8, Box::new(look_at));
        goals.add_goal(9, Box::new(TurtleRandomStrollGoal::new(1.0, 100)));
    }

    pub fn set_home_pos(&mut self, pos: BlockPos) {
        self.home_pos = pos;
    }

    pub fn get_home_pos(&self) -> BlockPos {
        self.home_pos
    }

    pub fn has_egg(&self) -> bool {
        self.mob.living.entity.entity_data.get(HAS_EGG)
    }

    fn set_has_egg(&mut self, on_off: bool) {
        self.mob.living.entity.entity_data.set(HAS_EGG, on_off);
    }

    pub fn is_laying_egg(&self) -> bool {
        self.mob.living.entity.entity_data.get(LAYING_EGG)
    }

    fn set_laying_egg(&mut self, on: bool) {
        self.lay_egg_counter = i32::from(on);
        self.mob.living.entity.entity_data.set(LAYING_EGG, on);
    }
}

/// `Turtle.TurtlePathNavigation` (`AmphibiousPathNavigation`).
pub struct TurtleNavigation {
    data: NavigationData,
}

impl PathNavigation for TurtleNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }
    fn is_stable_destination(&self, mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        if mob
            .as_any()
            .downcast_ref::<Turtle>()
            .is_some_and(|t| t.travel_pos.is_some())
        {
            level.get_block_state(pos).block() == Block::WATER
        } else {
            !level.get_block_state(pos.below()).is_air()
        }
    }
}

/// `Turtle.TurtleMoveControl`.
#[derive(Default)]
pub struct TurtleMoveControl {
    base: MoveControlBase,
}

impl TurtleMoveControl {
    fn update_speed(mob: &mut dyn Entity) {
        if mob.is_in_water() {
            let dm = mob.base().delta_movement().add(Vec3::new(0.0, 0.005, 0.0));
            mob.base_mut().set_delta_movement(dm);
            if !turtle(mob)
                .get_home_pos()
                .closer_to_center_than(&mob.position(), 16.0)
            {
                let s = (mob.get_speed() / 2.0).max(0.08);
                mob.set_speed(s);
            }
            if mob.is_baby() {
                let s = (mob.get_speed() / 3.0).max(0.06);
                mob.set_speed(s);
            }
        } else if mob.base().on_ground() {
            let s = (mob.get_speed() / 2.0).max(0.06);
            mob.set_speed(s);
        }
    }
}

impl MoveControl for TurtleMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        Self::update_speed(mob);
        if self.base.operation == Operation::MoveTo && !navigation::is_done(mob, level) {
            let xd = self.base.wanted_x - mob.base().x();
            let mut yd = self.base.wanted_y - mob.base().y();
            let zd = self.base.wanted_z - mob.base().z();
            let dd = (xd * xd + yd * yd + zd * zd).sqrt();
            if dd < f64::from(1.0E-5f32) {
                mob.set_speed(0.0);
            } else {
                yd /= dd;
                let y_rot_d =
                    (mth::atan2(zd, xd) * f64::from(mc_core::math::mth::RAD_TO_DEG)) as f32 - 90.0;
                let new_y_rot = self.base.rotlerp(mob.base().y_rot(), y_rot_d, 90.0);
                mob.set_y_rot(new_y_rot);
                l_mut(mob).y_body_rot = new_y_rot;
                let target_speed = (self.base.speed_modifier
                    * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED))
                    as f32;
                let speed = mth::lerp_f32(0.125, mob.get_speed(), target_speed);
                mob.set_speed(speed);
                let dm = mob.base().delta_movement().add(Vec3::new(
                    0.0,
                    f64::from(mob.get_speed()) * yd * 0.1,
                    0.0,
                ));
                mob.base_mut().set_delta_movement(dm);
            }
        } else {
            mob.set_speed(0.0);
        }
    }
}

fn l_mut(e: &mut dyn Entity) -> &mut mc_game_core::world::entity::living::LivingEntityData {
    e.living_mut().expect("living mob")
}

/// `Turtle.TurtlePanicGoal`.
pub struct TurtlePanicGoal {
    panic: PanicGoal,
}

impl TurtlePanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            panic: PanicGoal::new(speed_modifier),
        }
    }
}

impl Goal for TurtlePanicGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !self.panic.should_panic(mob, level) {
            return false;
        }
        match PanicGoal::look_for_water(level, mob, 7) {
            Some(pos) => {
                self.panic.pos_x = f64::from(pos.x());
                self.panic.pos_y = f64::from(pos.y());
                self.panic.pos_z = f64::from(pos.z());
                true
            }
            None => self.panic.find_random_position(mob, level),
        }
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.panic.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.panic.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.panic.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.panic.flags()
    }
    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(&self.panic)
    }
}

/// `TurtleBreedGoal.breed()`.
fn turtle_breed(animal_e: &mut dyn Entity, level: &mut ServerLevel, partner: EntityId) {
    let mut love_cause = animal::get_love_cause(animal_e, level);
    if love_cause.is_none() {
        love_cause = level
            .get_entity(partner)
            .and_then(|p| animal::get_love_cause(p, level));
    }
    if let Some(cause) = love_cause {
        level.with_player(cause, |level, p| {
            p.award_stat(level, Stats::ANIMALS_BRED, 1)
        });
        ext::trigger_bred_animals(level, cause, animal_e, partner, None);
    }
    turtle_mut(animal_e).set_has_egg(true);
    ageable::set_age_in_level(animal_e, level, 6000);
    level.with_entity(partner, |level, p| {
        ageable::set_age_in_level(p, level, 6000)
    });
    animal::reset_love(animal_e);
    level.with_entity(partner, |_, p| animal::reset_love(p));
    if level.mob_drops() {
        let p = animal_e.position();
        let value = animal_e.base_mut().random.next_int_bounded(7) + 1;
        let orb = ExperienceOrb::at(level, p.x, p.y, p.z, value);
        level.add_fresh_entity(Box::new(orb));
    }
}

/// `Turtle.TurtleBreedGoal`.
pub struct TurtleBreedGoal {
    breed: BreedGoal,
}

impl Goal for TurtleBreedGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.breed.can_use(mob, level) && !turtle(mob).has_egg()
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.breed.can_continue_to_use(mob, level)
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.breed.stop(mob, level);
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.breed
            .tick_with(mob, level, &mut |a, l, partner| turtle_breed(a, l, partner));
    }
    fn flags(&self) -> GoalFlags {
        self.breed.flags()
    }
}

/// `Turtle.TurtleGoHomeGoal`.
pub struct TurtleGoHomeGoal {
    speed_modifier: f64,
    stuck: bool,
    close_to_home_try_ticks: i32,
}

impl TurtleGoHomeGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
            close_to_home_try_ticks: 0,
        }
    }
}

impl Goal for TurtleGoHomeGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        if mob.is_baby() {
            return false;
        }
        if turtle(mob).has_egg() {
            return true;
        }
        if mob.base_mut().random.next_int_bounded(
            mc_game_core::world::entity::ai::goal::reduced_tick_delay(700),
        ) != 0
        {
            return false;
        }
        !turtle(mob)
            .home_pos
            .closer_to_center_than(&mob.position(), 64.0)
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        turtle_mut(mob).going_home = true;
        self.stuck = false;
        self.close_to_home_try_ticks = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        turtle_mut(mob).going_home = false;
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !turtle(mob)
            .home_pos
            .closer_to_center_than(&mob.position(), 7.0)
            && !self.stuck
            && self.close_to_home_try_ticks <= self.adjusted_tick_delay(600)
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let home_pos = turtle(mob).home_pos;
        let close_to_home = home_pos.closer_to_center_than(&mob.position(), 16.0);
        if close_to_home {
            self.close_to_home_try_ticks += 1;
        }
        if navigation::is_done(mob, level) {
            let home_vec = Vec3::at_bottom_center_of(&home_pos);
            let pi10 = f64::from((std::f64::consts::PI / 10.0) as f32);
            let pi2 = f64::from((std::f64::consts::PI / 2.0) as f32);
            let mut next_pos =
                default_random_pos::get_pos_towards(mob, level, 16, 3, home_vec, pi10);
            if next_pos.is_none() {
                next_pos = default_random_pos::get_pos_towards(mob, level, 8, 7, home_vec, pi2);
            }
            if let Some(p) = next_pos
                && !close_to_home
                && level
                    .get_block_state(BlockPos::containing(p.x, p.y, p.z))
                    .block()
                    != Block::WATER
            {
                next_pos = default_random_pos::get_pos_towards(mob, level, 16, 5, home_vec, pi2);
            }
            match next_pos {
                None => self.stuck = true,
                Some(p) => {
                    navigation::move_to(mob, level, p.x, p.y, p.z, self.speed_modifier);
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Turtle.TurtleGoToWaterGoal`.
pub struct TurtleGoToWaterGoal {
    mtb: MoveToBlockData,
}

impl TurtleGoToWaterGoal {
    pub fn new(speed_modifier: f64) -> Self {
        // `turtle.isBaby()` is false while the goals are registered (`Mob.<init>`)
        let mut mtb = MoveToBlockData::new(speed_modifier, 24);
        mtb.vertical_search_start = -1;
        Self { mtb }
    }
}

impl MoveToBlock for TurtleGoToWaterGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.mtb
    }
    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.mtb
    }
    fn should_recalculate_path(&self) -> bool {
        self.mtb.try_ticks % 160 == 0
    }
    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        level.get_block_state(pos).block() == Block::WATER
    }
}

impl Goal for TurtleGoToWaterGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if (mob.is_baby() && !mob.is_in_water())
            || (!turtle(mob).going_home && !mob.is_in_water() && !turtle(mob).has_egg())
        {
            move_to_block::can_use(self, mob, level)
        } else {
            false
        }
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !mob.is_in_water()
            && self.mtb.try_ticks <= 1200
            && self.is_valid_target(mob, level, self.mtb.block_pos)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        // `super(turtle, turtle.isBaby() ? 2.0 : speedModifier, 24)` is fixed at construction
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

/// `Turtle.TurtleLayEggGoal`.
pub struct TurtleLayEggGoal {
    mtb: MoveToBlockData,
}

impl TurtleLayEggGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            mtb: MoveToBlockData::new(speed_modifier, 16),
        }
    }
}

impl MoveToBlock for TurtleLayEggGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.mtb
    }
    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.mtb
    }
    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        if !level.is_empty_block(pos.above()) {
            false
        } else {
            turtle_egg_block::is_sand(level, pos)
        }
    }
}

impl Goal for TurtleLayEggGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if turtle(mob).has_egg()
            && turtle(mob)
                .home_pos
                .closer_to_center_than(&mob.position(), 9.0)
        {
            move_to_block::can_use(self, mob, level)
        } else {
            false
        }
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_continue_to_use(self, mob, level)
            && turtle(mob).has_egg()
            && turtle(mob)
                .home_pos
                .closer_to_center_than(&mob.position(), 9.0)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::start(self, mob, level);
    }
    fn requires_update_every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, mob, level);
        let turtle_pos = mob.block_position();
        if !mob.is_in_water() && self.is_reached_target() {
            if turtle(mob).lay_egg_counter < 1 {
                turtle_mut(mob).set_laying_egg(true);
            } else if turtle(mob).lay_egg_counter > self.adjusted_tick_delay(200) {
                let pitch = 0.9 + level.random().next_float() * 0.2;
                level.play_sound(
                    None,
                    turtle_pos,
                    SoundEvent::ENTITY_TURTLE_LAY_EGG,
                    SoundSource::Blocks,
                    0.3,
                    pitch,
                );
                let egg_pos = self.mtb.block_pos.above();
                let eggs = mob.base_mut().random.next_int_bounded(4) + 1;
                let egg_state = Block::TURTLE_EGG.default_state().set(props::EGGS, eggs);
                level.set_block_and_update(egg_pos, egg_state);
                level.game_event_from_entity_ref_in_state(
                    &*mob,
                    Some(egg_state),
                    GameEvent::BLOCK_PLACE,
                    mc_core::phys::Vec3::at_center_of(&egg_pos),
                );
                let t = turtle_mut(mob);
                t.set_has_egg(false);
                t.set_laying_egg(false);
                animal::set_in_love_time(mob, 600);
            }
            if turtle(mob).is_laying_egg() {
                turtle_mut(mob).lay_egg_counter += 1;
            }
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}

/// `Turtle.TurtleRandomStrollGoal`.
pub struct TurtleRandomStrollGoal {
    stroll: RandomStrollGoal,
}

impl TurtleRandomStrollGoal {
    pub fn new(speed_modifier: f64, interval: i32) -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(speed_modifier, interval),
        }
    }
}

impl Goal for TurtleRandomStrollGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !mob.is_in_water() && !turtle(mob).going_home && !turtle(mob).has_egg() {
            self.stroll.can_use(mob, level)
        } else {
            false
        }
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.stroll.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stroll.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stroll.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.stroll.flags()
    }
}

/// `Turtle.TurtleTravelGoal`.
pub struct TurtleTravelGoal {
    speed_modifier: f64,
    stuck: bool,
}

impl TurtleTravelGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
        }
    }
}

impl Goal for TurtleTravelGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !turtle(mob).going_home && !turtle(mob).has_egg() && mob.is_in_water()
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let random = &mut mob.base_mut().random;
        let xt = random.next_int_bounded(1025) - 512;
        let mut yt = random.next_int_bounded(9) - 4;
        let zt = random.next_int_bounded(1025) - 512;
        if f64::from(yt) + mob.base().y() > f64::from(level.get_sea_level() - 1) {
            yt = 0;
        }
        let p = mob.position();
        turtle_mut(mob).travel_pos = Some(BlockPos::containing(
            f64::from(xt) + p.x,
            f64::from(yt) + p.y,
            f64::from(zt) + p.z,
        ));
        self.stuck = false;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(travel_pos) = turtle(mob).travel_pos else {
            self.stuck = true;
            return;
        };
        if navigation::is_done(mob, level) {
            let target_pos = Vec3::at_bottom_center_of(&travel_pos);
            let pi10 = f64::from((std::f64::consts::PI / 10.0) as f32);
            let pi2 = f64::from((std::f64::consts::PI / 2.0) as f32);
            let mut next_pos =
                default_random_pos::get_pos_towards(mob, level, 16, 3, target_pos, pi10);
            if next_pos.is_none() {
                next_pos = default_random_pos::get_pos_towards(mob, level, 8, 7, target_pos, pi2);
            }
            if let Some(p) = next_pos {
                let xc = mth::floor_f64(p.x);
                let zc = mth::floor_f64(p.z);
                if !level.has_chunks_at(xc - 34, zc - 34, xc + 34, zc + 34) {
                    next_pos = None;
                }
            }
            match next_pos {
                None => self.stuck = true,
                Some(p) => {
                    navigation::move_to(mob, level, p.x, p.y, p.z, self.speed_modifier);
                }
            }
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !navigation::is_done(mob, level)
            && !self.stuck
            && !turtle(mob).going_home
            && !animal::is_in_love(mob)
            && !turtle(mob).has_egg()
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        turtle_mut(mob).travel_pos = None;
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

impl Entity for Turtle {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:turtle")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int_array(
            "home_pos",
            vec![self.home_pos.x(), self.home_pos.y(), self.home_pos.z()],
        );
        out.put_boolean("has_egg", self.has_egg());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        let home = input
            .get_int_array("home_pos")
            .filter(|a| a.len() == 3)
            .map(|a| BlockPos::new(a[0], a[1], a[2]))
            .unwrap_or_else(|| self.block_position());
        self.set_home_pos(home);
        mob::nbt::read_additional_save_data(self, input);
        self.set_has_egg(input.get_boolean_or("has_egg", false));
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let pos = self.block_position();
        self.set_home_pos(pos);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        200
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if !self.is_in_water() && self.base().on_ground() && !self.is_baby() {
            Some(SoundEvent::ENTITY_TURTLE_AMBIENT_LAND)
        } else {
            None
        }
    }

    fn play_swim_sound(&mut self, level: &mut ServerLevel, volume: f32) {
        let a = self.base_mut().random.next_float();
        let b = self.base_mut().random.next_float();
        let sound = self.get_swim_sound();
        self.play_sound(level, sound, volume * 1.5, 1.0 + (a - b) * 0.4);
    }

    fn get_swim_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_TURTLE_SWIM
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_baby() {
            SoundEvent::ENTITY_TURTLE_HURT_BABY
        } else {
            SoundEvent::ENTITY_TURTLE_HURT
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() {
            SoundEvent::ENTITY_TURTLE_DEATH_BABY
        } else {
            SoundEvent::ENTITY_TURTLE_DEATH
        })
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        let sound = if self.is_baby() {
            SoundEvent::ENTITY_TURTLE_SHAMBLE_BABY
        } else {
            SoundEvent::ENTITY_TURTLE_SHAMBLE
        };
        self.play_sound(level, sound, 0.15, 1.0);
    }

    fn can_fall_in_love(&self) -> bool {
        mob::animal::can_fall_in_love(self) && !self.has_egg()
    }

    fn next_step(&self) -> f32 {
        self.base().move_dist + 0.15
    }

    fn get_age_scale(&self) -> f32 {
        if self.is_baby() { BABY_SCALE } else { 1.0 }
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        mc_game_core::world::entity::factory::create(
            EntityType::TURTLE,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_turtle_food(stack)
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if (!self.going_home && level.get_fluid_state(pos).is_tag("water"))
            || turtle_egg_block::on_sand(level, pos)
        {
            10.0
        } else {
            level.get_pathfinding_cost_from_light_levels(pos)
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if self.is_alive()
            && self.is_laying_egg()
            && self.lay_egg_counter >= 1
            && self.lay_egg_counter % 5 == 0
        {
            let pos = self.block_position();
            if turtle_egg_block::on_sand(&*level, pos) {
                let below = level.get_block_state(pos.below());
                level.level_event_impl(None, 2001, pos, below.id() as i32);
                self.game_event(level, GameEvent::ENTITY_ACTION);
            }
        }
    }

    fn age_boundary_reached(&mut self, level: &mut ServerLevel) {
        ageable::age_boundary_reached(self, level);
        if !self.is_baby() && level.mob_drops() {
            ext::drop_from_gift_loot_table(
                level,
                self,
                &Identifier::parse("minecraft:gameplay/turtle_grow").expect("valid id"),
                |level, e, stack| {
                    mc_game_core::world::entity::item::spawn_at_location(
                        level,
                        e,
                        stack,
                        Vec3::ZERO,
                    );
                },
            );
        }
    }

    fn travel_in_water(
        &mut self,
        level: &mut ServerLevel,
        input: Vec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) {
        self.move_relative(0.1, input);
        let delta = self.base().delta_movement();
        self.r#move(level, MoverType::SelfMover, delta);
        let scaled = self.base().delta_movement().scale(0.9);
        self.base_mut().set_delta_movement(scaled);
        if self.get_target().is_none()
            && (!self.going_home || !self.home_pos.closer_to_center_than(&self.position(), 20.0))
        {
            let dm = self
                .base()
                .delta_movement()
                .add(Vec3::new(0.0, -0.005, 0.0));
            self.base_mut().set_delta_movement(dm);
        }
    }

    fn can_be_leashed(&self) -> bool {
        false
    }

    fn thunder_hit_by(&mut self, level: &mut ServerLevel, _bolt: &LightningBolt) {
        let source = level.damage_sources().lightning_bolt();
        self.hurt_server(level, &source, f32::MAX);
    }
}

/// `Turtle.checkTurtleSpawnRules`.
fn check_turtle_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    pos.y() < level.get_sea_level() + 4
        && turtle_egg_block::on_sand(level, pos)
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::TURTLE, check_turtle_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static TURTLE: EntityRegistration = EntityRegistration {
    entity_type: EntityType::TURTLE,
    create: |ty, id, registries| Box::new(Turtle::new(ty, id, registries)),
};
