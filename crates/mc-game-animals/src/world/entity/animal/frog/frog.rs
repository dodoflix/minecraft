//! `net.minecraft.world.entity.animal.frog.Frog`: a brain-driven animal that long-jumps, swims, eats small mobs with its
//! tongue and lays frogspawn.

use super::frog_ai;
use super::frog_variant::FrogVariant;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::look_control::{self, LookControl, LookControlBase};
use mc_game_core::world::entity::ai::control::smooth_swimming_move_control::SmoothSwimmingMoveControl;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, IS_PREGNANT};
use mc_game_core::world::entity::ai::memory::{BrainPacked, Unit};
use mc_game_core::world::entity::ai::navigation::{NavClass, NavigationData, PathNavigation};
use mc_game_core::world::entity::ai::sensing::mob_sensors::frog_can_eat;
use mc_game_core::world::entity::ai::sensing::sensor_type::{FROG_ATTACKABLES, FROG_TEMPTATIONS, HURT_BY, IS_IN_WATER, NEAREST_LIVING_ENTITIES};
use mc_game_core::world::entity::factory::{self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::mob::{self, MobData, MobKind, animal};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::variant::{self, SpawnContext};
use mc_game_core::world::entity::{Entity, EntityId, MoverType};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::level_access::LevelAccessor;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::pathfinder::node_evaluator::{EvaluatorState, NodeEvaluator, PathEnv, default_get_path_type, default_get_start};
use mc_game_core::world::level::pathfinder::{NodeId, PathFinder, PathType};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_VARIANT_ID: FrogVariant = Frog);
define_accessor!(pub DATA_TONGUE_TARGET_ID: OptionalUnsignedInt = Frog);

/// `FROG_FALL_DAMAGE_REDUCTION`.
const FROG_FALL_DAMAGE_REDUCTION: i32 = 5;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(&[&NEAREST_LIVING_ENTITIES, &HURT_BY, &FROG_ATTACKABLES, &FROG_TEMPTATIONS, &IS_IN_WATER], frog_ai::get_activities);

/// `Frog`.
pub struct Frog {
    pub mob: MobData,
    /// Identifier of the variant entry (what `VariantUtils.writeVariant` saves).
    variant_key: Identifier,
    /// `variant` read from NBT, resolved to a registry id by [`Entity::resolve_registry_refs`].
    pending_variant: Option<Identifier>,
}

impl Frog {
    /// `new Frog(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(registries, keys::FROG_VARIANT, &Identifier::parse(mc_game_core::world::entity::animal::temperature_variants::TEMPERATE).expect("valid id"));
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_VARIANT_ID, default_variant as i32);
            b.define(DATA_TONGUE_TARGET_ID, None);
        });
        let variant_key = registries.registry_or_throw(keys::FROG_VARIANT).name_of(default_variant).expect("variant entry");
        let mut frog = Frog { mob, variant_key, pending_variant: None };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = frog.make_brain(&BrainPacked::EMPTY);
        frog.mob.living.brain = brain;
        let navigation = FrogPathNavigation::new(&frog);
        frog.mob.set_navigation(Box::new(navigation));
        frog.mob.look_control = Some(Box::<FrogLookControl>::default());
        frog.mob.set_pathfinding_malus(PathType::Water, 4.0);
        frog.mob.set_pathfinding_malus(PathType::Trapdoor, -1.0);
        frog.mob.move_control = Some(Box::new(SmoothSwimmingMoveControl::new(85, 10, 0.02, 0.1, true)));
        frog
    }

    /// `getVariant()` (registry id).
    pub fn get_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_VARIANT_ID)
    }

    /// `setVariant(variant)`.
    fn set_variant(&mut self, registries: &RegistryAccess, id: u32) {
        self.mob.living.entity.entity_data.set(DATA_VARIANT_ID, id as i32);
        if let Some(name) = registries.registry(keys::FROG_VARIANT).and_then(|r| r.name_of(id)) {
            self.variant_key = name;
        }
    }
}

/// `eraseTongueTarget()`.
pub fn erase_tongue_target(e: &mut dyn Entity) {
    e.base_mut().entity_data.set(DATA_TONGUE_TARGET_ID, None);
}

/// `getTongueTarget()`: the entity the synched id resolves to in the level.
pub fn get_tongue_target(e: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    e.base().entity_data.get(DATA_TONGUE_TARGET_ID).filter(|&id| level.get_entity(id).is_some())
}

/// `setTongueTarget(target)`.
pub fn set_tongue_target(e: &mut dyn Entity, target: EntityId) {
    e.base_mut().entity_data.set(DATA_TONGUE_TARGET_ID, Some(target));
}

/// `Frog.checkFrogSpawnRules`.
pub fn check_frog_spawn_rules(_ty: EntityType, level: &dyn ServerLevelAccessor, _reason: EntitySpawnReason, pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
    level.get_block_state(pos.below()).is_tag("frogs_spawnable_on") && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::FROG, check_frog_spawn_rules);

/// `Frog.FrogLookControl`: the head pitch is only reset while no tongue target is set.
#[derive(Default)]
struct FrogLookControl {
    base: LookControlBase,
    /// `getTongueTarget().isEmpty()`, read from the frog at the start of every tick (the control is taken out of the frog
    /// while it ticks).
    reset_x_rot: bool,
}

impl LookControl for FrogLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.reset_x_rot = get_tongue_target(&*mob, level).is_none();
        look_control::default_tick(self, mob, level);
    }

    fn reset_x_rot_on_tick(&self) -> bool {
        self.reset_x_rot
    }
}

/// `Frog.FrogNodeEvaluator extends AmphibiousNodeEvaluator` (`prefersShallowSwimming`).
struct FrogNodeEvaluator {
    state: EvaluatorState,
}

impl NodeEvaluator for FrogNodeEvaluator {
    fn state(&self) -> &EvaluatorState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut EvaluatorState {
        &mut self.state
    }

    fn get_start(&mut self, env: &mut PathEnv<'_>) -> NodeId {
        if !env.mob.is_in_water() {
            default_get_start(self, env)
        } else {
            let bb = env.mob.bounding_box();
            self.get_start_node(env, BlockPos::new(mth::floor_f64(bb.min_x), mth::floor_f64(bb.min_y), mth::floor_f64(bb.min_z)))
        }
    }

    fn get_path_type(&mut self, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
        let below_state = env.context.get_block_state(BlockPos::new(x, y - 1, z));
        if below_state.is_tag("frog_prefer_jump_to") { PathType::Open } else { default_get_path_type(self, env, x, y, z) }
    }
}

/// `Frog.FrogPathNavigation extends AmphibiousPathNavigation`.
struct FrogPathNavigation {
    data: NavigationData,
}

impl FrogPathNavigation {
    fn new(frog: &Frog) -> Self {
        let data = NavigationData::with_path_finder(frog, NavClass::Amphibious, |max_visited_nodes| {
            PathFinder::new(Box::new(FrogNodeEvaluator { state: EvaluatorState::amphibious(true) }), max_visited_nodes)
        });
        FrogPathNavigation { data }
    }
}

impl PathNavigation for FrogPathNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }

    fn can_cut_corner(&self, path_type: PathType) -> bool {
        path_type != PathType::WaterBorder && mc_game_core::world::entity::ai::navigation::default_can_cut_corner(self, path_type)
    }
}

/// `itemStack.is(ItemTags.FROG_FOOD)`.
fn is_frog_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("frog_food").is_some_and(|t| t.contains(stack.item()))
}

impl Entity for Frog {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:frog")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn get_head_rot_speed(&self) -> i32 {
        35
    }

    fn get_max_head_y_rot(&self) -> i32 {
        5
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        frog_ai::update_activity(self);
        animal::custom_server_ai_step(self);
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        let (id, position) = (self.id(), self.base().position());
        level.play_sound_from_entity_at(None, id, position, SoundEvent::ENTITY_FROG_EAT, mc_protocol::packets::play::SoundSource::Neutral, 2.0, 1.0);
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        let mut frog = factory::create(EntityType::FROG, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })?;
        frog_ai::init_memories(&mut *frog, LevelAccessor::random(level));
        Some(frog)
    }

    fn can_be_a_baby(&self) -> bool {
        false
    }

    fn spawn_child_from_breeding(&mut self, level: &mut ServerLevel, partner: EntityId) {
        animal::finalize_spawn_child_from_breeding(self, level, partner, None);
        self.brain_mut().set_memory(&IS_PREGNANT, Unit);
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let registries = level.registry_access().clone();
        let selected = {
            let pos = self.block_position();
            let context = SpawnContext::create(&*level, pos);
            let registry = registries.registry_or_throw(keys::FROG_VARIANT);
            let entries: Vec<(u32, _)> = (0..registry.len() as u32).filter_map(|i| registry.get_by_id::<FrogVariant>(i).map(|v| (i, v.spawn_conditions.clone()))).collect();
            let winners = variant::select(entries.into_iter(), &context);
            variant::get_random_safe(&winners, level.random())
        };
        if let Some(id) = selected {
            self.set_variant(&registries, id);
        }
        frog_ai::init_memories(self, level.random());
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_FROG_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_FROG_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_FROG_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_FROG_STEP, 0.15, 1.0);
    }

    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    fn calculate_fall_damage(&self, fall_distance: f64, damage_modifier: f32) -> i32 {
        mc_game_core::world::entity::living_entity::damage::calculate_fall_damage(self, fall_distance, damage_modifier) - FROG_FALL_DAMAGE_REDUCTION
    }

    fn travel_in_water(&mut self, level: &mut ServerLevel, input: Vec3, _base_gravity: f64, _is_falling: bool, _old_y: f64) {
        let speed = self.get_speed();
        self.move_relative(speed, input);
        let delta = self.base().delta_movement();
        self.r#move(level, MoverType::SelfMover, delta);
        let scaled = self.base().delta_movement().scale(0.9);
        self.base_mut().set_delta_movement(scaled);
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_frog_food(stack)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        variant::write_variant(out, &self.variant_key);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.pending_variant = variant::read_variant_key(input);
    }

    fn resolve_registry_refs(&mut self, registries: &RegistryAccess) {
        if let Some(key) = self.pending_variant.take() {
            if let Some(id) = registries.registry(keys::FROG_VARIANT).and_then(|r| r.id_of(&key)) {
                self.set_variant(registries, id);
            }
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static FROG: EntityRegistration = EntityRegistration { entity_type: EntityType::FROG, create: |ty, id, registries| Box::new(Frog::new(ty, id, registries)) };

/// `Frog.canEat(entity)`.
pub fn can_eat(entity: &dyn Entity) -> bool {
    frog_can_eat(entity)
}
