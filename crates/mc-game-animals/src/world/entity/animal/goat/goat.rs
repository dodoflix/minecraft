//! `net.minecraft.world.entity.animal.goat.Goat`: a brain-driven animal that long-jumps, rams, drops its horns and gives
//! milk.
//!
//! Not implemented: the client-only head lowering animation (`isLoweringHead`/`lowerHeadTick`, entity events 58 and 59,
//! `getRammingXHeadRot`); the server never runs it.

use super::goat_ai;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::{RandomSource, SingleThreadedRandomSource};
use mc_core::uuid_util::java_hash_code;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    FOOD_TEMPTATIONS, HURT_BY, NEAREST_ADULT, NEAREST_ITEMS, NEAREST_LIVING_ENTITIES,
    NEAREST_PLAYERS,
};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, animal};
use mc_game_core::world::entity::player::menu_ctx::PlayerCtx;
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{
    InteractionHand, InteractionResult, ItemStack, create_filled_result,
};
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::level_access::LevelAccessor;
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{Holder, INSTRUMENT, InstrumentComponent};
use mc_protocol::packets::play::SoundSource;

define_accessor!(pub DATA_IS_SCREAMING_GOAT: Boolean = Goat);
define_accessor!(pub DATA_HAS_LEFT_HORN: Boolean = Goat);
define_accessor!(pub DATA_HAS_RIGHT_HORN: Boolean = Goat);

/// `LONG_JUMPING_DIMENSION_SCALE_FACTOR`.
pub const LONG_JUMPING_DIMENSION_SCALE_FACTOR: f32 = 0.7;
/// `GOAT_FALL_DAMAGE_REDUCTION`.
pub const GOAT_FALL_DAMAGE_REDUCTION: i32 = 10;
/// `GOAT_SCREAMING_CHANCE`.
pub const GOAT_SCREAMING_CHANCE: f64 = 0.02;
/// `UNIHORN_CHANCE` (`0.1F` widened to a double, compared against `nextFloat()` as a float).
pub const UNIHORN_CHANCE: f32 = 0.1;
const BABY_SCALE: f32 = 0.55;
const ADULT_ATTACK_DAMAGE: f64 = 2.0;
const BABY_ATTACK_DAMAGE: f64 = 1.0;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[
        &NEAREST_LIVING_ENTITIES,
        &NEAREST_PLAYERS,
        &NEAREST_ITEMS,
        &NEAREST_ADULT,
        &HURT_BY,
        &FOOD_TEMPTATIONS,
    ],
    goat_ai::get_activities,
);

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.45, 0.65)
        .with_eye_height(0.59375)
        .with_attachments(
            EntityAttachments::create_default(0.45, 0.65)
                .with_passenger(Vec3::new(0.0, 0.53125, 0.0)),
        )
}

/// `Goat`.
pub struct Goat {
    pub mob: MobData,
}

impl Goat {
    /// `new Goat(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_IS_SCREAMING_GOAT, false);
            b.define(DATA_HAS_LEFT_HORN, true);
            b.define(DATA_HAS_RIGHT_HORN, true);
        });
        let mut goat = Goat { mob };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = goat.make_brain(&BrainPacked::EMPTY);
        goat.mob.living.brain = brain;
        let navigation = StandardPathNavigation::ground(&goat);
        goat.mob.set_navigation(Box::new(navigation));
        mob::navigation::set_can_float(&mut goat, true);
        goat.mob.set_pathfinding_malus(PathType::PowderSnow, -1.0);
        goat.mob
            .set_pathfinding_malus(PathType::OnTopOfPowderSnow, -1.0);
        goat
    }

    /// `createHorn()`: the instrument is drawn from a random seeded with the goat's UUID hash.
    pub fn create_horn(&self, level: &ServerLevel) -> ItemStack {
        let mut random =
            SingleThreadedRandomSource::new(i64::from(java_hash_code(&self.base().uuid)));
        let tag = if self.is_screaming_goat() {
            "minecraft:screaming_goat_horns"
        } else {
            "minecraft:regular_goat_horns"
        };
        let mut stack = ItemStack::new(Item::GOAT_HORN);
        let registry = level.registry_access().registry_or_throw(keys::INSTRUMENT);
        let tag = mc_core::ident::Identifier::parse(tag).expect("valid tag id");
        let ids = registry.tag(&tag).unwrap_or(&[]);
        if !ids.is_empty()
            && let Some(name) =
                registry.name_of(ids[random.next_int_bounded(ids.len() as i32) as usize])
        {
            stack.set(
                INSTRUMENT,
                InstrumentComponent {
                    instrument: Holder::Reference(name),
                },
            );
        }
        stack
    }

    /// `hasLeftHorn()`.
    pub fn has_left_horn(&self) -> bool {
        self.base().entity_data.get(DATA_HAS_LEFT_HORN)
    }

    /// `hasRightHorn()`.
    pub fn has_right_horn(&self) -> bool {
        self.base().entity_data.get(DATA_HAS_RIGHT_HORN)
    }

    /// `dropHorn()`.
    pub fn drop_horn(&mut self, level: &mut ServerLevel) -> bool {
        if self.is_baby() {
            return false;
        }
        let (has_left, has_right) = (self.has_left_horn(), self.has_right_horn());
        if !has_left && !has_right {
            return false;
        }
        let horn_to_drop = if !has_left {
            DATA_HAS_RIGHT_HORN
        } else if !has_right || self.base_mut().random.next_boolean() {
            // only both horns draw the side
            DATA_HAS_LEFT_HORN
        } else {
            DATA_HAS_RIGHT_HORN
        };
        self.base_mut().entity_data.set(horn_to_drop, false);
        let position = self.position();
        let item = self.create_horn(level);
        let random = &mut self.base_mut().random;
        let delta_x = f64::from(mth::random_between(random, -0.2, 0.2));
        let delta_y = f64::from(mth::random_between(random, 0.3, 0.7));
        let delta_z = f64::from(mth::random_between(random, -0.2, 0.2));
        let item_entity = ItemEntity::with_stack_and_motion(
            level, position.x, position.y, position.z, item, delta_x, delta_y, delta_z,
        );
        level.add_fresh_entity(Box::new(item_entity));
        true
    }

    /// `isScreamingGoat()`.
    pub fn is_screaming_goat(&self) -> bool {
        self.base().entity_data.get(DATA_IS_SCREAMING_GOAT)
    }

    /// `setScreamingGoat(isScreamingGoat)`.
    pub fn set_screaming_goat(&mut self, is_screaming_goat: bool) {
        self.base_mut()
            .entity_data
            .set(DATA_IS_SCREAMING_GOAT, is_screaming_goat);
    }

    /// `getMilkingSound()`.
    fn milking_sound(&self) -> SoundEvent {
        if self.is_screaming_goat() {
            SoundEvent::ENTITY_GOAT_SCREAMING_MILK
        } else {
            SoundEvent::ENTITY_GOAT_MILK
        }
    }

    /// `ageBoundaryReached()` (it does not call `AgeableMob.ageBoundaryReached`).
    fn update_attack_damage(&mut self) {
        let damage = if self.is_baby() {
            BABY_ATTACK_DAMAGE
        } else {
            ADULT_ATTACK_DAMAGE
        };
        if let Some(instance) = self
            .mob
            .living
            .attributes
            .get_instance(Attribute::ATTACK_DAMAGE)
        {
            instance.set_base_value(damage);
        }
    }
}

impl Entity for Goat {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:goat")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn age_boundary_reached(&mut self, _level: &mut ServerLevel) {
        self.update_attack_damage();
    }

    fn calculate_fall_damage(&self, fall_distance: f64, damage_modifier: f32) -> i32 {
        mc_game_core::world::entity::living_entity::damage::calculate_fall_damage(
            self,
            fall_distance,
            damage_modifier,
        ) - GOAT_FALL_DAMAGE_REDUCTION
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_screaming_goat() {
            SoundEvent::ENTITY_GOAT_SCREAMING_AMBIENT
        } else {
            SoundEvent::ENTITY_GOAT_AMBIENT
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_screaming_goat() {
            SoundEvent::ENTITY_GOAT_SCREAMING_HURT
        } else {
            SoundEvent::ENTITY_GOAT_HURT
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_screaming_goat() {
            SoundEvent::ENTITY_GOAT_SCREAMING_DEATH
        } else {
            SoundEvent::ENTITY_GOAT_DEATH
        })
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_GOAT_STEP, 0.15, 1.0);
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = factory::create(
            EntityType::GOAT,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        goat_ai::init_memories(&mut *baby, LevelAccessor::random(level));
        let selected_screaming = if LevelAccessor::random(level).next_boolean() {
            Some(self.is_screaming_goat())
        } else {
            level
                .get_entity(partner)
                .and_then(|p| p.as_any().downcast_ref::<Goat>())
                .map(Goat::is_screaming_goat)
        };
        let baby_is_screaming = selected_screaming == Some(true)
            || LevelAccessor::random(level).next_double() < GOAT_SCREAMING_CHANCE;
        baby.as_any_mut()
            .downcast_mut::<Goat>()
            .expect("goat")
            .set_screaming_goat(baby_is_screaming);
        Some(baby)
    }

    fn get_age_scale(&self) -> f32 {
        if self.is_baby() { BABY_SCALE } else { 1.0 }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        goat_ai::update_activity(self);
        animal::custom_server_ai_step(self);
    }

    fn get_max_head_y_rot(&self) -> i32 {
        15
    }

    fn set_y_head_rot(&mut self, y_head_rot: f32) {
        let max_head_y_rot = self.get_max_head_y_rot() as f32;
        let y_body_rot = self.mob.living.y_body_rot;
        let delta_from_body = mth::degrees_difference(y_body_rot, y_head_rot);
        let delta_from_body_clamped =
            mth::clamp_f32(delta_from_body, -max_head_y_rot, max_head_y_rot);
        self.mob.living.y_head_rot = y_body_rot + delta_from_body_clamped;
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        let sound = if self.is_screaming_goat() {
            SoundEvent::ENTITY_GOAT_SCREAMING_EAT
        } else {
            SoundEvent::ENTITY_GOAT_EAT
        };
        let pitch = mth::random_between(LevelAccessor::random(level), 0.8, 1.2);
        let (id, position) = (self.id(), self.position());
        level.play_sound_from_entity_at(
            None,
            id,
            position,
            sound,
            SoundSource::Neutral,
            1.0,
            pitch,
        );
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        mc_data::tags::get::<Item>("goat_food").is_some_and(|t| t.contains(stack.item()))
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let held_item = player.get_item_in_hand(hand);
        if held_item.is(Item::BUCKET) && !self.is_baby() {
            player.play_sound(level, self.milking_sound(), 1.0, 1.0);
            let result = create_filled_result(
                held_item,
                &mut PlayerCtx { player, level },
                ItemStack::new(Item::MILK_BUCKET),
                true,
            );
            player.set_item_in_hand(hand, result);
            InteractionResult::SUCCESS
        } else {
            let result = mob::interact::mob_interact(self, level, player, hand);
            // `heldItem` is the live stack: feeding the last item leaves it empty
            if result.consumes_action() && self.is_food(&player.get_item_in_hand(hand)) {
                self.play_eating_sound(level);
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
        goat_ai::init_memories(self, level.random());
        let screaming = level.random().next_double() < GOAT_SCREAMING_CHANCE;
        self.set_screaming_goat(screaming);
        self.update_attack_damage();
        if !self.is_baby() && level.random().next_float() < UNIHORN_CHANCE {
            let horn_to_remove = if level.random().next_boolean() {
                DATA_HAS_LEFT_HORN
            } else {
                DATA_HAS_RIGHT_HORN
            };
            self.base_mut().entity_data.set(horn_to_remove, false);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        let default_dimensions = if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.base().entity_type)
        };
        let default_dimensions = if pose == Pose::LongJumping {
            default_dimensions.scale(LONG_JUMPING_DIMENSION_SCALE_FACTOR)
        } else {
            default_dimensions
        };
        mob::ext::living_dimensions(self, pose, default_dimensions)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("IsScreamingGoat", self.is_screaming_goat());
        out.put_boolean("HasLeftHorn", self.has_left_horn());
        out.put_boolean("HasRightHorn", self.has_right_horn());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.set_screaming_goat(input.get_boolean_or("IsScreamingGoat", false));
        let data = &mut self.base_mut().entity_data;
        data.set(
            DATA_HAS_LEFT_HORN,
            input.get_boolean_or("HasLeftHorn", true),
        );
        data.set(
            DATA_HAS_RIGHT_HORN,
            input.get_boolean_or("HasRightHorn", true),
        );
    }
}

/// `Goat.checkGoatSpawnRules`.
pub fn check_goat_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("goats_spawnable_on")
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::GOAT, check_goat_spawn_rules);

/// The scenario dump fields of a goat: `scream lhorn rhorn`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static GOAT_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 25,
    applies: |e| e.as_any().is::<Goat>(),
    fields: |e, _level, _ctx| {
        let goat = e.as_any().downcast_ref::<Goat>().expect("applies");
        vec![
            (
                "scream".into(),
                u8::from(goat.is_screaming_goat()).to_string(),
            ),
            ("lhorn".into(), u8::from(goat.has_left_horn()).to_string()),
            ("rhorn".into(), u8::from(goat.has_right_horn()).to_string()),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static GOAT: EntityRegistration = EntityRegistration {
    entity_type: EntityType::GOAT,
    create: |ty, id, _registries| Box::new(Goat::new(ty, id)),
};
