//! `net.minecraft.world.entity.animal.armadillo.Armadillo`: a brain-driven animal that rolls up into a ball when something
//! scary comes close, sheds scutes over time and gives one to a brush.

use super::armadillo_ai;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::{DamageSource, DamageTypeTags};
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::body_rotation_control::{
    BodyRotationControl, BodyRotationControlBase, base_client_tick,
};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::DANGER_DETECTED_RECENTLY;
use mc_game_core::world::entity::ai::navigation::{PathNavigation, StandardPathNavigation};
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    ARMADILLO_SCARE_DETECTED, FOOD_TEMPTATIONS, HURT_BY, NEAREST_ADULT, NEAREST_LIVING_ENTITIES,
};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::entity::DATA_POSE;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::{self, DATA_BABY_ID, MobData, MobKind, animal, leashable};
use mc_game_core::world::entity::spawn_predicates::is_bright_enough_to_spawn;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::storage::loot::{
    LootContextParamSets, LootParams, loading::get_loot_table,
};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ArmadilloState;

define_accessor!(pub ARMADILLO_STATE: ArmadilloState = Armadillo);

/// `BABY_SCALE`.
const BABY_SCALE: f32 = 0.6;
/// `SCARE_DISTANCE_HORIZONTAL`.
const SCARE_DISTANCE_HORIZONTAL: f64 = 7.0;
/// `SCARE_DISTANCE_VERTICAL`.
const SCARE_DISTANCE_VERTICAL: f64 = 2.0;
/// `ArmadilloState.SCARED.animationDuration()`, `UNROLLING.animationDuration()` and the other two durations.
const ROLLING_DURATION: i64 = 10;
const SCARED_DURATION: i64 = 50;
const UNROLLING_DURATION: i64 = 30;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[
        &NEAREST_LIVING_ENTITIES,
        &HURT_BY,
        &FOOD_TEMPTATIONS,
        &NEAREST_ADULT,
        &ARMADILLO_SCARE_DETECTED,
    ],
    armadillo_ai::get_activities,
);

/// `ArmadilloState.getSerializedName()`.
fn state_name(state: ArmadilloState) -> &'static str {
    match state {
        ArmadilloState::Idle => "idle",
        ArmadilloState::Rolling => "rolling",
        ArmadilloState::Scared => "scared",
        ArmadilloState::Unrolling => "unrolling",
    }
}

/// `ArmadilloState.CODEC` decode.
fn state_by_name(name: &str) -> Option<ArmadilloState> {
    [
        ArmadilloState::Idle,
        ArmadilloState::Rolling,
        ArmadilloState::Scared,
        ArmadilloState::Unrolling,
    ]
    .into_iter()
    .find(|&s| state_name(s) == name)
}

/// `ArmadilloState.isThreatened()`.
pub fn is_threatened(state: ArmadilloState) -> bool {
    state != ArmadilloState::Idle
}

/// `ArmadilloState.animationDuration()`.
pub fn animation_duration(state: ArmadilloState) -> i64 {
    match state {
        ArmadilloState::Idle => 0,
        ArmadilloState::Rolling => ROLLING_DURATION,
        ArmadilloState::Scared => SCARED_DURATION,
        ArmadilloState::Unrolling => UNROLLING_DURATION,
    }
}

/// `ArmadilloState.shouldHideInShell(ticksInState)`.
fn should_hide_in_shell_in(state: ArmadilloState, ticks_in_state: i64) -> bool {
    match state {
        ArmadilloState::Idle => false,
        ArmadilloState::Rolling => ticks_in_state > 5,
        ArmadilloState::Scared => true,
        ArmadilloState::Unrolling => ticks_in_state < 26,
    }
}

/// `isScared()` of any entity that has the armadillo state.
fn is_scared_of(e: &dyn Entity) -> bool {
    e.base().entity_data.get(ARMADILLO_STATE) != ArmadilloState::Idle
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::of_type(EntityType::ARMADILLO)
        .scale(BABY_SCALE)
        .with_eye_height(0.21875)
}

/// `Armadillo`.
pub struct Armadillo {
    pub mob: MobData,
    /// `inStateTicks`.
    in_state_ticks: i64,
    /// `scuteTime`.
    scute_time: i32,
}

impl Armadillo {
    /// `new Armadillo(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(ARMADILLO_STATE, ArmadilloState::Idle);
        });
        let mut armadillo = Armadillo {
            mob,
            in_state_ticks: 0,
            scute_time: 0,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = armadillo.make_brain(&BrainPacked::EMPTY);
        armadillo.mob.living.brain = brain;
        let mut navigation = StandardPathNavigation::ground(&armadillo);
        navigation.set_can_float(true);
        armadillo.mob.set_navigation(Box::new(navigation));
        armadillo.mob.body_rotation_control =
            Some(Box::new(ArmadilloBodyRotationControl::default()));
        armadillo.scute_time = armadillo.pick_next_scute_drop_time();
        armadillo
    }

    /// `isScared()`.
    pub fn is_scared(&self) -> bool {
        is_scared_of(self)
    }

    /// `shouldHideInShell()`.
    pub fn should_hide_in_shell(&self) -> bool {
        should_hide_in_shell_in(self.get_state(), self.in_state_ticks)
    }

    /// `shouldSwitchToScaredState()`.
    pub fn should_switch_to_scared_state(&self) -> bool {
        self.get_state() == ArmadilloState::Rolling && self.in_state_ticks > ROLLING_DURATION
    }

    /// `getState()`.
    pub fn get_state(&self) -> ArmadilloState {
        self.base().entity_data.get(ARMADILLO_STATE)
    }

    /// `switchToState(state)`: a changed value runs `onSyncedDataUpdated`, which restarts the tick counter of the state.
    pub fn switch_to_state(&mut self, state: ArmadilloState) {
        if self.base_mut().entity_data.set(ARMADILLO_STATE, state) {
            self.on_synced_data_updated(ARMADILLO_STATE.id());
        }
    }

    /// `pickNextScuteDropTime()`.
    fn pick_next_scute_drop_time(&mut self) -> i32 {
        self.base_mut().random.next_int_bounded(20 * 60 * 5) + 20 * 60 * 5
    }

    /// `canStayRolledUp()`.
    pub fn can_stay_rolled_up_now(&self) -> bool {
        !self.is_panicking()
            && !self.is_in_liquid()
            && !leashable::is_leashed(self)
            && !self.base().is_passenger()
            && !self.base().is_vehicle()
    }

    /// `rollUp()`.
    pub fn roll_up(&mut self, level: &mut ServerLevel) {
        if !self.is_scared() {
            mob::tick::stop_in_place(self, level);
            animal::reset_love(self);
            self.game_event(level, GameEvent::ENTITY_ACTION);
            make_sound(self, level, SoundEvent::ENTITY_ARMADILLO_ROLL);
            self.switch_to_state(ArmadilloState::Rolling);
        }
    }

    /// `rollOut()`.
    pub fn roll_out(&mut self, level: &mut ServerLevel) {
        if self.is_scared() {
            self.game_event(level, GameEvent::ENTITY_ACTION);
            make_sound(self, level, SoundEvent::ENTITY_ARMADILLO_UNROLL_FINISH);
            self.switch_to_state(ArmadilloState::Idle);
        }
    }

    /// `brushOffScute(interactingEntity, tool)`: the drops of `BuiltInLootTables.ARMADILLO_BRUSH`, the brush sound and the
    /// interact game event; `false` for a baby.
    fn brush_off_scute_by(
        &mut self,
        level: &mut ServerLevel,
        interacting: impl FnOnce(&ServerLevel) -> Option<EntityId>,
        tool: &ItemStack,
    ) -> bool {
        if self.is_baby() {
            return false;
        }
        let interacting = interacting(level);
        let table = Identifier::parse("minecraft:brush/armadillo").expect("valid id");
        let loot = get_loot_table(&level.registries, &table);
        let items = {
            let this: &dyn Entity = &*self;
            let params = LootParams::builder(level)
                .target_entity(this)
                .interacting_entity(interacting.and_then(|id| level.get_entity(id)))
                .tool(tool)
                .create(&LootContextParamSets::ENTITY_INTERACT);
            loot.get_random_items(&params)
        };
        for stack in items {
            mc_game_core::world::entity::item::spawn_at_location(level, self, stack, Vec3::ZERO);
        }
        self.play_sound(level, SoundEvent::ENTITY_ARMADILLO_BRUSH, 1.0, 1.0);
        self.game_event(level, GameEvent::ENTITY_INTERACT);
        true
    }
}

/// `Armadillo.checkArmadilloSpawnRules`.
pub fn check_armadillo_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("armadillo_spawnable_on")
        && is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::ARMADILLO, check_armadillo_spawn_rules);

/// `Armadillo.createBodyControl()`: the body only follows the head while the armadillo is not rolled up.
#[derive(Default)]
struct ArmadilloBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for ArmadilloBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }
    fn client_tick(&mut self, mob: &mut dyn Entity) {
        if !is_scared_of(&*mob) {
            base_client_tick(&mut self.base, mob);
        }
    }
}

/// `itemStack.is(ItemTags.ARMADILLO_FOOD)`.
fn is_armadillo_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("armadillo_food").is_some_and(|t| t.contains(stack.item()))
}

impl Entity for Armadillo {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:armadillo")
    }

    fn get_dimensions(&self, _pose: mc_data::enums::Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn get_age_scale(&self) -> f32 {
        if self.is_baby() { BABY_SCALE } else { 1.0 }
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        factory::create(
            EntityType::ARMADILLO,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == ARMADILLO_STATE.id() {
            self.in_state_ticks = 0;
        }
        if accessor_id == DATA_POSE.id() || accessor_id == DATA_BABY_ID.id() {
            self.refresh_dimensions();
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        armadillo_ai::update_activity(self);
        if self.is_alive()
            && {
                self.scute_time -= 1;
                self.scute_time <= 0
            }
            && self.should_drop_loot(level)
        {
            let table = Identifier::parse("minecraft:gameplay/armadillo_shed").expect("valid id");
            if mob::ext::drop_from_gift_loot_table(level, self, &table, |level, e, stack| {
                mc_game_core::world::entity::item::spawn_at_location(level, e, stack, Vec3::ZERO);
            }) {
                let random = &mut self.base_mut().random;
                let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
                self.play_sound(level, SoundEvent::ENTITY_ARMADILLO_SCUTE_DROP, 1.0, pitch);
                self.game_event(level, GameEvent::ENTITY_PLACE);
            }
            self.scute_time = self.pick_next_scute_drop_time();
        }
        animal::custom_server_ai_step(self);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.is_scared() {
            mob::tick::clamp_head_rotation_to_body(self);
        }
        self.in_state_ticks += 1;
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_armadillo_food(stack)
    }

    fn is_scared_by(&self, entity: &dyn Entity) -> bool {
        if !self
            .base()
            .bounding_box()
            .inflate(
                SCARE_DISTANCE_HORIZONTAL,
                SCARE_DISTANCE_VERTICAL,
                SCARE_DISTANCE_HORIZONTAL,
            )
            .intersects_aabb(&entity.base().bounding_box())
        {
            false
        } else if mc_data::tags::get::<EntityType>("undead")
            .is_some_and(|t| t.contains(entity.get_type()))
            || self.mob.living.last_hurt_by_mob == Some(entity.uuid())
        {
            true
        } else if entity.is_player() {
            !entity.is_spectator() && (entity.is_sprinting() || entity.base().is_passenger())
        } else {
            false
        }
    }

    fn can_stay_rolled_up(&self) -> bool {
        self.can_stay_rolled_up_now()
    }

    fn armadillo_is_scared(&self) -> bool {
        self.is_scared()
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_string("state", state_name(self.get_state()));
        out.put_int("scute_time", self.scute_time);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        let state = input
            .get_string("state")
            .and_then(state_by_name)
            .unwrap_or(ArmadilloState::Idle);
        self.switch_to_state(state);
        if let Some(time) = input.get_int("scute_time") {
            self.scute_time = time;
        }
    }

    fn hurt_server(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        mut damage: f32,
    ) -> bool {
        if self.is_scared() {
            damage = (damage - 1.0) / 2.0;
        }
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }

    fn actually_hurt(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        animal::reset_love(self);
        mc_game_core::world::entity::living_entity::damage::actually_hurt(
            self, level, source, damage,
        );
        if !self.mob.is_no_ai() && !self.is_dead_or_dying() {
            if source.causing_is_living() {
                self.brain_mut()
                    .set_memory_with_expiry(&DANGER_DETECTED_RECENTLY, true, 80);
                if self.can_stay_rolled_up_now() {
                    self.roll_up(level);
                }
            } else if source.is_tag(level, DamageTypeTags::PANIC_ENVIRONMENTAL_CAUSES) {
                self.roll_out(level);
            }
        }
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.is(Item::BRUSH) {
            let player_id = player.id();
            if self.brush_off_scute_by(level, |_| Some(player_id), &item_stack) {
                mob::interact::with_hand_stack(player, hand, |stack, player| {
                    mc_game_core::world::item::hurt_and_break(
                        stack,
                        16,
                        level,
                        player,
                        hand.as_equipment_slot(),
                    );
                });
                return InteractionResult::SUCCESS;
            }
        }
        if self.is_scared() {
            InteractionResult::FAIL
        } else {
            mob::interact::mob_interact(self, level, player, hand)
        }
    }

    fn brush_off_scute(
        &mut self,
        level: &mut ServerLevel,
        interacting: Option<EntityId>,
        tool: &ItemStack,
    ) -> bool {
        self.brush_off_scute_by(level, |_| interacting, tool)
    }

    fn can_fall_in_love(&self) -> bool {
        animal::can_fall_in_love(self) && !self.is_scared()
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if self.is_scared() {
            None
        } else {
            Some(SoundEvent::ENTITY_ARMADILLO_AMBIENT)
        }
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        make_sound(self, level, SoundEvent::ENTITY_ARMADILLO_EAT);
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ARMADILLO_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_scared() {
            SoundEvent::ENTITY_ARMADILLO_HURT_REDUCED
        } else {
            SoundEvent::ENTITY_ARMADILLO_HURT
        })
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_ARMADILLO_STEP, 0.15, 1.0);
    }

    fn get_max_head_y_rot(&self) -> i32 {
        if self.is_scared() { 0 } else { 32 }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ARMADILLO: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ARMADILLO,
    create: |ty, id, _registries| Box::new(Armadillo::new(ty, id)),
};

/// `dump` fields of the armadillo: the state, the ticks in it and the scute timer.
#[distributed_slice(mc_game_core::world::entity::scenario_dump::SCENARIO_DUMP_EXTRA)]
static ARMADILLO_DUMP: mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra =
    mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra {
        order: 20,
        applies: |e| e.get_type() == EntityType::ARMADILLO,
        fields: |e, _level, _ctx| {
            let armadillo = e
                .as_any()
                .downcast_ref::<Armadillo>()
                .expect("armadillo entity");
            vec![
                ("astate".into(), state_name(armadillo.get_state()).into()),
                ("astateticks".into(), armadillo.in_state_ticks.to_string()),
                ("scute".into(), armadillo.scute_time.to_string()),
            ]
        },
    };
