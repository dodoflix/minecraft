//! `net.minecraft.world.entity.animal.frog.Tadpole`: a bucketable water mob that grows into a frog after 24000 ticks.
//!
//! `Tadpole extends AbstractFish`: the fish layer ([`abstract_fish`]) is reused piecewise (the `abstract_fish_entity!`
//! macro writes overrides - `aiStep`, `mobInteract`, `FromBucket` - that the tadpole replaces).

use super::tadpole_ai;
use crate::world::entity::animal::fish::abstract_fish;
use crate::world::entity::animal::fish::water_animal::water_creature_entity;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::smooth_swimming_look_control::SmoothSwimmingLookControl;
use mc_game_core::world::entity::ai::control::smooth_swimming_move_control::SmoothSwimmingMoveControl;
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    FROG_TEMPTATIONS, HURT_BY, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::bucketable::{self, Bucketable};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::convert::{self, ConversionParams};
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable};
use mc_game_core::world::entity::movement::fudge_position_after_size_change;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{BUCKET_ENTITY_DATA, CustomData};
use mc_protocol::packets::play::SoundSource;

define_accessor!(pub AGE_LOCKED: Boolean = Tadpole);

/// `Tadpole.ticksToBeFrog` (`Math.abs(-24000)`).
pub const TICKS_TO_BE_FROG: i32 = 24000;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[
        &NEAREST_LIVING_ENTITIES,
        &NEAREST_PLAYERS,
        &HURT_BY,
        &FROG_TEMPTATIONS,
    ],
    tadpole_ai::get_activities,
);

/// `Tadpole`.
pub struct Tadpole {
    pub mob: MobData,
    /// `age`.
    age: i32,
    /// `ageLockParticleTimer`.
    age_lock_particle_timer: i32,
}

impl Tadpole {
    /// `new Tadpole(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            abstract_fish::define_synched_data(b);
            b.define(AGE_LOCKED, false);
        });
        let mut tadpole = Tadpole {
            mob,
            age: 0,
            age_lock_particle_timer: 0,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = tadpole.make_brain(&BrainPacked::EMPTY);
        tadpole.mob.living.brain = brain;
        let navigation = StandardPathNavigation::water_bound(&tadpole);
        abstract_fish::init_controls(&mut tadpole.mob, navigation);
        abstract_fish::register_goals(&mut tadpole);
        tadpole.mob.move_control = Some(Box::new(SmoothSwimmingMoveControl::new(
            85, 10, 0.02, 0.1, true,
        )));
        tadpole.mob.look_control = Some(Box::new(SmoothSwimmingLookControl::new(10)));
        tadpole
    }

    /// `setAgeLocked(locked)`.
    fn set_age_locked(&mut self, locked: bool) {
        self.mob.living.entity.entity_data.set(AGE_LOCKED, locked);
    }

    /// `isAgeLocked()`.
    pub fn is_age_locked(&self) -> bool {
        self.mob.living.entity.entity_data.get(AGE_LOCKED)
    }

    /// `getAge()`.
    pub fn get_age(&self) -> i32 {
        self.age
    }

    /// `setAge(newAge)` without the conversion check (the conversion needs the level mutably, which a load only holds shared;
    /// the next `aiStep` converts a tadpole that is past the threshold).
    fn set_age_raw(&mut self, new_age: i32) {
        self.age = new_age;
    }

    /// `setAge(newAge)`.
    fn set_age(&mut self, level: &mut ServerLevel, new_age: i32) {
        self.age = new_age;
        if self.age >= TICKS_TO_BE_FROG {
            self.age_up_to_frog(level);
        }
    }

    /// `ageUp(ticksToAgeUp)`.
    fn age_up(&mut self, level: &mut ServerLevel, ticks_to_age_up: i32) {
        let new_age = self.age.wrapping_add(ticks_to_age_up.wrapping_mul(20));
        self.set_age(level, new_age);
    }

    /// `ageUp()`: converts into a frog.
    fn age_up_to_frog(&mut self, level: &mut ServerLevel) {
        let old_dimensions = self.get_dimensions(self.base().get_pose());
        let params = ConversionParams::single(self, level, false, false);
        let pos = self.base().position();
        let source = self.get_sound_source();
        let silent = self.base().is_silent();
        convert::convert_to(
            self,
            level,
            EntityType::FROG,
            &params,
            EntitySpawnReason::Conversion,
            move |frog, level| {
                let difficulty =
                    ServerLevelAccessor::get_current_difficulty_at(&*level, frog.block_position());
                frog.finalize_spawn(level, difficulty, EntitySpawnReason::Conversion, None);
                frog.set_persistence_required();
                fudge_position_after_size_change(frog, level, &old_dimensions);
                if !silent {
                    level.play_sound_at(
                        None,
                        pos.x,
                        pos.y,
                        pos.z,
                        SoundEvent::ENTITY_TADPOLE_GROW_UP,
                        source,
                        0.15,
                        1.0,
                    );
                }
            },
        );
    }

    /// `getTicksLeftUntilAdult()`.
    fn get_ticks_left_until_adult(&self) -> i32 {
        0.max(TICKS_TO_BE_FROG.wrapping_sub(self.age))
    }

    /// `feed(player, itemStack)`.
    fn feed(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) {
        consume_one(player, hand);
        let seconds = ageable::get_speed_up_seconds_when_feeding(self.get_ticks_left_until_adult());
        self.age_up(level, seconds);
        // `level.addParticle(HAPPY_VILLAGER, getRandomX(1.0), getRandomY() + 0.5, getRandomZ(1.0), ..)` is a no-op on the
        // server but its arguments draw from the entity RNG
        let b = self.base_mut();
        let _ = b.get_random_x(1.0);
        let _ = b.get_random_y();
        let _ = b.get_random_z(1.0);
    }
}

/// `ItemStack.consume(1, player)`.
fn consume_one(player: &mut ServerPlayer, hand: InteractionHand) {
    let mut stack = player.get_item_in_hand(hand);
    if !player.player.abilities.instabuild {
        stack.shrink(1);
    }
    player.set_item_in_hand(hand, stack);
}

/// `itemStack.is(ItemTags.FROG_FOOD)`.
fn is_frog_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("frog_food").is_some_and(|t| t.contains(stack.item()))
}

impl Bucketable for Tadpole {
    fn from_bucket(&self) -> bool {
        true
    }
    fn set_from_bucket(&mut self, _from_bucket: bool) {}
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        bucketable::save_default_data_to_bucket_tag(self, bucket);
        let mut tag = bucket
            .get(&BUCKET_ENTITY_DATA)
            .map(|d| d.copy_tag())
            .unwrap_or_default();
        tag.put_int("Age", self.get_age());
        tag.put_boolean("AgeLocked", self.is_age_locked());
        bucket.set(BUCKET_ENTITY_DATA, CustomData::of(&tag));
    }
    fn load_from_bucket_tag(&mut self, tag: &CompoundTag) {
        bucketable::load_default_data_from_bucket_tag(self, tag);
        if let Some(age) = tag.get_int("Age") {
            self.set_age_raw(age);
        }
        self.set_age_locked(tag.get_boolean_or("AgeLocked", false));
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::TADPOLE_BUCKET)
    }
    fn get_pickup_sound(&self) -> SoundEvent {
        SoundEvent::ITEM_BUCKET_FILL_TADPOLE
    }
}

impl Entity for Tadpole {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:tadpole")
    }

    water_creature_entity!();

    fn requires_custom_persistence(&self) -> bool {
        abstract_fish::requires_custom_persistence(self) || Bucketable::from_bucket(self)
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !Bucketable::from_bucket(self) && !self.base().has_custom_name()
    }

    fn travel_in_water(
        &mut self,
        level: &mut ServerLevel,
        input: Vec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) {
        abstract_fish::travel_in_water(self, level, input);
    }

    fn get_swim_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_FISH_SWIM
    }

    fn play_step_sound(
        &mut self,
        _level: &mut ServerLevel,
        _pos: mc_core::pos::BlockPos,
        _state: mc_data::block::BlockState,
    ) {
    }

    fn as_bucketable(&self) -> Option<&dyn Bucketable> {
        Some(self)
    }

    fn as_bucketable_mut(&mut self) -> Option<&mut dyn Bucketable> {
        Some(self)
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        tadpole_ai::update_activity(self);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        abstract_fish::ai_step(self, level, SoundEvent::ENTITY_TADPOLE_FLOP);
        if !self.is_age_locked() {
            let age = self.age.wrapping_add(1);
            self.set_age(level, age);
        }
        let locked = self.is_age_locked();
        let timer = self.age_lock_particle_timer;
        self.age_lock_particle_timer =
            ageable::make_age_locked_particle(level, self, timer, locked);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        // `AbstractFish.addAdditionalSaveData`: `fromBucket()` is always true for a tadpole
        out.put_boolean("FromBucket", true);
        out.put_int("Age", self.age);
        out.put_boolean("AgeLocked", self.is_age_locked());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        abstract_fish::set_from_bucket(self, input.get_boolean_or("FromBucket", false));
        self.set_age_raw(input.get_int_or("Age", 0));
        self.set_age_locked(input.get_boolean_or("AgeLocked", false));
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        None
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_TADPOLE_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_TADPOLE_DEATH)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if is_frog_food(&item_stack) && !self.is_age_locked() {
            self.feed(level, player, hand);
            InteractionResult::SUCCESS
        } else if ageable::can_use_golden_dandelion(
            &item_stack,
            true,
            self.age_lock_particle_timer,
            self,
        ) {
            // `AgeableMob.setAgeLocked(this, this::isAgeLocked, player, itemStack, mob -> this.setAgeLockedData())`
            let locked = !self.is_age_locked();
            self.set_age_locked(locked);
            self.set_age_raw(0);
            self.age_lock_particle_timer = 40;
            consume_one(player, hand);
            let now_locked = self.is_age_locked();
            if now_locked {
                self.set_persistence_required();
            }
            let pos = self.block_position();
            let sound = if now_locked {
                SoundEvent::ITEM_GOLDEN_DANDELION_USE
            } else {
                SoundEvent::ITEM_GOLDEN_DANDELION_UNUSE
            };
            level.play_sound(None, pos, sound, SoundSource::Players, 1.0, 1.0);
            InteractionResult::SUCCESS
        } else {
            match bucketable::bucket_mob_pickup_in_place(level, player, hand, self) {
                Some(result) => result,
                None => mob::interact::mob_interact(self, level, player, hand),
            }
        }
    }

    fn should_drop_experience(&self) -> bool {
        false
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static TADPOLE: EntityRegistration = EntityRegistration {
    entity_type: EntityType::TADPOLE,
    create: |ty, id, _registries| Box::new(Tadpole::new(ty, id)),
};

/// The scenario dump fields of a tadpole: `tage locked particle`.
#[distributed_slice(mc_game_core::world::entity::scenario_dump::SCENARIO_DUMP_EXTRA)]
static TADPOLE_DUMP: mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra =
    mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra {
        order: 30,
        applies: |e| e.as_any().is::<Tadpole>(),
        fields: |e, _level, _ctx| {
            let t = e.as_any().downcast_ref::<Tadpole>().expect("applies");
            vec![
                ("tage".into(), t.get_age().to_string()),
                ("locked".into(), u8::from(t.is_age_locked()).to_string()),
                ("particle".into(), t.age_lock_particle_timer.to_string()),
            ]
        },
    };
