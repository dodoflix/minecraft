//! `net.minecraft.world.entity.animal.sniffer.Sniffer`: a brain-driven animal that sniffs out blocks to dig up seeds, lays an egg
//! instead of a baby and hatches into a slow-growing baby.
//!
//! Not implemented: the client-only animation states, digging particles and the searching/digging local sounds (the server only
//! runs the state, the seed drop and the digging game event).

use super::sniffer_ai;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    SNIFFER_EXPLORED_POSITIONS, TEMPTING_PLAYER,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    FOOD_TEMPTATIONS, HURT_BY, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::entity::DATA_POSE;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::living_entity;
use mc_game_core::world::entity::mob::{
    self, DATA_BABY_ID, MobData, MobKind, animal, ext, leashable, navigation,
};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::pathfinder::PathType;
use mc_protocol::codec::GlobalPos;
use mc_protocol::packets::play::{SnifferState, SoundSource};

define_accessor!(pub DATA_STATE: SnifferState = Sniffer);
define_accessor!(pub DATA_DROP_SEED_AT_TICK: Int = Sniffer);

/// `DIGGING_DROP_SEED_OFFSET_TICKS`.
const DIGGING_DROP_SEED_OFFSET_TICKS: i32 = 120;
/// `SNIFFER_BABY_START_AGE`.
const SNIFFER_BABY_START_AGE: i32 = -48000;
/// `DIGGING_BB_HEIGHT_OFFSET`.
const DIGGING_BB_HEIGHT_OFFSET: f32 = 0.4;
/// The most positions the explored list keeps (`limit(20)` plus the new one).
const EXPLORED_POSITIONS_KEPT: usize = 20;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_memories(
    &[SNIFFER_EXPLORED_POSITIONS.id()],
    &[
        &NEAREST_LIVING_ENTITIES,
        &HURT_BY,
        &NEAREST_PLAYERS,
        &FOOD_TEMPTATIONS,
    ],
    sniffer_ai::get_activities,
);

/// `DIGGING_DIMENSIONS`.
fn digging_dimensions() -> EntityDimensions {
    let full = EntityDimensions::of_type(EntityType::SNIFFER);
    EntityDimensions::scalable(full.width, full.height - DIGGING_BB_HEIGHT_OFFSET)
        .with_eye_height(0.81)
}

/// `Sniffer.State` name as the scenario dumps print it.
fn state_name(state: SnifferState) -> &'static str {
    match state {
        SnifferState::Idling => "idling",
        SnifferState::FeelingHappy => "feeling_happy",
        SnifferState::Scenting => "scenting",
        SnifferState::Sniffing => "sniffing",
        SnifferState::Searching => "searching",
        SnifferState::Digging => "digging",
        SnifferState::Rising => "rising",
    }
}

/// `Sniffer`.
pub struct Sniffer {
    pub mob: MobData,
}

impl Sniffer {
    /// `new Sniffer(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_STATE, SnifferState::Idling);
            b.define(DATA_DROP_SEED_AT_TICK, 0);
        });
        let mut sniffer = Sniffer { mob };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = sniffer.make_brain(&BrainPacked::EMPTY);
        sniffer.mob.living.brain = brain;
        let navigation = StandardPathNavigation::ground(&sniffer);
        sniffer.mob.set_navigation(Box::new(navigation));
        navigation::set_can_float(&mut sniffer, true);
        sniffer.mob.set_pathfinding_malus(PathType::Water, -1.0);
        sniffer
            .mob
            .set_pathfinding_malus(PathType::OnTopOfPowderSnow, -1.0);
        sniffer
            .mob
            .set_pathfinding_malus(PathType::DamageCautious, -1.0);
        sniffer
    }

    /// `getState()`.
    pub fn get_state(&self) -> SnifferState {
        self.base().entity_data.get(DATA_STATE)
    }

    /// `setState(state)`: a changed value runs `onSyncedDataUpdated`.
    fn set_state(&mut self, state: SnifferState) {
        if self.base_mut().entity_data.set(DATA_STATE, state) {
            self.on_synced_data_updated(DATA_STATE.id());
        }
    }

    /// `isSearching()`.
    pub fn is_searching(&self) -> bool {
        self.get_state() == SnifferState::Searching
    }

    /// `isTempted()`.
    pub fn is_tempted(&self) -> bool {
        self.brain().has_memory_value(TEMPTING_PLAYER)
    }

    /// `canSniff()`.
    pub fn can_sniff(&self) -> bool {
        !self.is_tempted()
            && !self.is_panicking()
            && !self.is_in_water()
            && !animal::is_in_love(self)
            && self.base().on_ground()
            && !self.base().is_passenger()
            && !leashable::is_leashed(self)
    }

    /// `canPlayDiggingSound()`.
    pub fn can_play_digging_sound(&self) -> bool {
        matches!(
            self.get_state(),
            SnifferState::Digging | SnifferState::Searching
        )
    }

    /// `getHeadPosition()`.
    fn get_head_position(&self) -> Vec3 {
        let forward = Vec3::direction_from_rotation(self.base().x_rot(), self.base().y_rot());
        self.position().add(forward.scale(2.25))
    }

    /// `getHeadBlock()`.
    fn get_head_block(&self) -> BlockPos {
        let position = self.get_head_position();
        BlockPos::containing(position.x, self.base().y() + f64::from(0.2f32), position.z)
    }

    /// `transitionTo(state)`.
    pub fn transition_to(&mut self, level: &mut ServerLevel, state: SnifferState) {
        match state {
            SnifferState::Idling => self.set_state(SnifferState::Idling),
            SnifferState::FeelingHappy => {
                self.play_sound(level, SoundEvent::ENTITY_SNIFFER_HAPPY, 1.0, 1.0);
                self.set_state(SnifferState::FeelingHappy);
            }
            SnifferState::Scenting => {
                self.set_state(SnifferState::Scenting);
                let pitch = if self.is_baby() { 1.3 } else { 1.0 };
                self.play_sound(level, SoundEvent::ENTITY_SNIFFER_SCENTING, 1.0, pitch);
            }
            SnifferState::Sniffing => {
                self.play_sound(level, SoundEvent::ENTITY_SNIFFER_SNIFFING, 1.0, 1.0);
                self.set_state(SnifferState::Sniffing);
            }
            SnifferState::Searching => self.set_state(SnifferState::Searching),
            SnifferState::Digging => {
                self.set_state(SnifferState::Digging);
                // `onDiggingStart()`
                let drop_at = self.base().tick_count + DIGGING_DROP_SEED_OFFSET_TICKS;
                self.base_mut()
                    .entity_data
                    .set(DATA_DROP_SEED_AT_TICK, drop_at);
                level.broadcast_entity_event(self.id(), 63);
            }
            SnifferState::Rising => {
                self.play_sound(level, SoundEvent::ENTITY_SNIFFER_DIGGING_STOP, 1.0, 1.0);
                self.set_state(SnifferState::Rising);
            }
        }
    }

    /// `onDiggingComplete(success)`.
    pub fn on_digging_complete(&mut self, level: &ServerLevel, success: bool) {
        if success {
            let on_pos = ext_on_pos(level, self);
            self.store_explored_position(level, on_pos);
        }
    }

    /// `getExploredPositions()`.
    fn explored_positions(&self) -> Vec<GlobalPos> {
        self.brain()
            .get_memory(&SNIFFER_EXPLORED_POSITIONS)
            .cloned()
            .unwrap_or_default()
    }

    /// `storeExploredPosition(position)`: the new position first, at most 20 older ones behind it.
    fn store_explored_position(&mut self, level: &ServerLevel, position: BlockPos) {
        let mut updated: Vec<GlobalPos> = self.explored_positions();
        updated.truncate(EXPLORED_POSITIONS_KEPT);
        updated.insert(
            0,
            GlobalPos {
                dimension: level.dimension.clone(),
                pos: position,
            },
        );
        self.brain_mut()
            .set_memory(&SNIFFER_EXPLORED_POSITIONS, updated);
    }

    /// `calculateDigPosition()`: the first of five land positions (10, 12, .. 18 blocks away) whose block below can be dug.
    pub fn calculate_dig_position(&mut self, level: &mut ServerLevel) -> Option<BlockPos> {
        for idx in 0..5 {
            let Some(pos) = land_random_pos::get_pos(self, level, 10 + 2 * idx, 3) else {
                continue;
            };
            let position = BlockPos::containing(pos.x, pos.y, pos.z);
            if !level.world_border().is_within_bounds_block(&position) {
                continue;
            }
            let below = position.below();
            if self.can_dig_at(level, below) {
                return Some(below);
            }
        }
        None
    }

    /// `canDig()`.
    pub fn can_dig(&mut self, level: &mut ServerLevel) -> bool {
        if self.is_panicking()
            || self.is_tempted()
            || self.is_baby()
            || self.is_in_water()
            || !self.base().on_ground()
            || self.base().is_passenger()
        {
            return false;
        }
        let head_below = self.get_head_block().below();
        self.can_dig_at(level, head_below)
    }

    /// `canDig(position)`.
    fn can_dig_at(&mut self, level: &mut ServerLevel, position: BlockPos) -> bool {
        level
            .get_block_state(position)
            .is_tag("sniffer_diggable_block")
            && !self
                .explored_positions()
                .iter()
                .any(|explored| explored.dimension == level.dimension && explored.pos == position)
            && navigation::create_path_pos(self, level, position, 1).is_some_and(|p| p.can_reach())
    }

    /// `dropSeed()`.
    fn drop_seed(&mut self, level: &mut ServerLevel) {
        if self.base().entity_data.get(DATA_DROP_SEED_AT_TICK) != self.base().tick_count {
            return;
        }
        let head = self.get_head_block();
        let table = Identifier::parse("minecraft:gameplay/sniffer_digging").expect("valid id");
        ext::drop_from_gift_loot_table(level, self, &table, |level, _sniffer, stack| {
            let mut entity = ItemEntity::with_stack(
                level,
                f64::from(head.x()),
                f64::from(head.y()),
                f64::from(head.z()),
                stack,
            );
            entity.set_default_pick_up_delay();
            level.add_fresh_entity(Box::new(entity));
        });
        self.play_sound(level, SoundEvent::ENTITY_SNIFFER_DROP_SEED, 1.0, 1.0);
    }

    /// `emitDiggingParticles(state)`: the particles and the local hit sound are client work, the game event is not.
    fn emit_digging_game_event(&self, level: &mut ServerLevel) {
        if self.base().tick_count % 10 == 0 {
            let head = self.get_head_block();
            level.game_event_from_entity_ref(
                self,
                GameEvent::ENTITY_ACTION,
                Vec3::at_center_of(&head),
            );
        }
    }
}

/// `getOnPos()` of the sniffer.
fn ext_on_pos(level: &ServerLevel, sniffer: &Sniffer) -> BlockPos {
    mc_game_core::advancements::predicates::entity::support::get_on_pos(level, sniffer)
}

impl Entity for Sniffer {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:sniffer")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn on_pathfinding_start(&mut self) {
        if self.is_on_fire() || self.is_in_water() {
            self.set_pathfinding_malus(PathType::Water, 0.0);
        }
    }

    fn on_pathfinding_done(&mut self) {
        self.set_pathfinding_malus(PathType::Water, -1.0);
    }

    fn get_baby_start_age(&self) -> i32 {
        SNIFFER_BABY_START_AGE
    }

    fn get_default_dimensions(&self, _pose: Pose) -> EntityDimensions {
        let age_scale = self.get_age_scale();
        if self.get_state() == SnifferState::Digging {
            digging_dimensions().scale(age_scale)
        } else {
            EntityDimensions::of_type(self.base().entity_type).scale(age_scale)
        }
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_STATE.id()
            || accessor_id == DATA_POSE.id()
            || accessor_id == DATA_BABY_ID.id()
        {
            self.refresh_dimensions();
        }
    }

    fn support_quad_leash(&self) -> bool {
        true
    }

    fn jump_from_ground(&mut self, level: &ServerLevel) {
        living_entity::jump::jump_from_ground(self, level);
        let speed_modifier = mob::move_control_speed_modifier(self);
        if speed_modifier > 0.0 {
            let current = self.base().delta_movement().horizontal_distance_sqr();
            if current < 0.01 {
                self.move_relative(0.1, Vec3::new(0.0, 0.0, 1.0));
            }
        }
    }

    fn spawn_child_from_breeding(&mut self, level: &mut ServerLevel, partner: EntityId) {
        let p = self.position();
        let mut egg =
            ItemEntity::with_stack(level, p.x, p.y, p.z, ItemStack::new(Item::SNIFFER_EGG));
        egg.set_default_pick_up_delay();
        animal::finalize_spawn_child_from_breeding(self, level, partner, None);
        let random = &mut self.base_mut().random;
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 0.5;
        self.play_sound(level, SoundEvent::BLOCK_SNIFFER_EGG_PLOP, 1.0, pitch);
        level.add_fresh_entity(Box::new(egg));
    }

    fn die(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        self.transition_to(level, SnifferState::Idling);
        living_entity::damage::die(self, level, source);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        if self.get_state() == SnifferState::Digging {
            self.emit_digging_game_event(level);
            self.drop_seed(level);
        }
        mob::tick::tick(self, level);
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let is_food = self.is_food(&player.get_item_in_hand(hand));
        let result = mob::interact::mob_interact(self, level, player, hand);
        if result.consumes_action() && is_food {
            self.play_eating_sound(level);
        }
        result
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        let pitch = mth::random_between(LevelAccessor::random(level), 0.8, 1.2);
        let (id, position) = (self.id(), self.position());
        level.play_sound_from_entity_at(
            None,
            id,
            position,
            SoundEvent::ENTITY_SNIFFER_EAT,
            SoundSource::Neutral,
            1.0,
            pitch,
        );
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_SNIFFER_STEP, 0.15, 1.0);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if self.can_play_digging_sound() {
            None
        } else {
            Some(SoundEvent::ENTITY_SNIFFER_IDLE)
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SNIFFER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SNIFFER_DEATH)
    }

    fn get_max_head_y_rot(&self) -> i32 {
        50
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        factory::create(
            EntityType::SNIFFER,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn can_mate(&self, partner: &dyn Entity) -> bool {
        let Some(partner_sniffer) = partner.as_any().downcast_ref::<Sniffer>() else {
            return false;
        };
        let mating = |s: SnifferState| {
            matches!(
                s,
                SnifferState::Idling | SnifferState::Scenting | SnifferState::FeelingHappy
            )
        };
        mating(self.get_state())
            && mating(partner_sniffer.get_state())
            && animal::can_mate(self, partner)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        mc_data::tags::get::<Item>("sniffer_food").is_some_and(|t| t.contains(stack.item()))
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        sniffer_ai::update_activity(self);
        animal::custom_server_ai_step(self);
    }
}

/// The scenario dump fields of a sniffer: `state dropat size` (width, height and eye height).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static SNIFFER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 25,
    applies: |e| e.as_any().is::<Sniffer>(),
    fields: |e, _level, _ctx| {
        let sniffer = e.as_any().downcast_ref::<Sniffer>().expect("applies");
        vec![
            ("state".into(), state_name(sniffer.get_state()).into()),
            (
                "dropat".into(),
                sniffer
                    .base()
                    .entity_data
                    .get(DATA_DROP_SEED_AT_TICK)
                    .to_string(),
            ),
            (
                "size".into(),
                format!(
                    "{},{},{}",
                    java_float_to_string(sniffer.base().get_bb_width()),
                    java_float_to_string(sniffer.base().get_bb_height()),
                    java_float_to_string(sniffer.base().eye_height)
                ),
            ),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static SNIFFER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SNIFFER,
    create: |ty, id, _registries| Box::new(Sniffer::new(ty, id)),
};
