//! `PotentSulfurBlockEntity`: saves `countdown` (`waitingCountdown`, `-1` = not started); the server tickers
//! (nausea cloud, geyser countdown, entity launching). The client-only tickers (particles, local sounds) are not part of
//! a dedicated server.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::effect::MobEffect;
use crate::world::entity::Entity;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::level::collision::{ClipBlock, ClipContext, ClipFluid, CollisionContext};
use crate::world::level::{BlockGetter, BlockGetterExt, LevelAccessor};
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::{PositionalRandomFactory, RandomSource, XoroshiroRandomSource};
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_data::enums::PotentSulfurState;
use mc_data::fluid::Fluid;
use mc_data::{GameEvent, props};
use mc_nbt::CompoundTag;

const EFFECT_APPLICATION_FREQUENCY_TICKS: i64 = 10;
const EFFECT_DURATION_IN_TICKS: i32 = 80;
const GEYSER_SALT: i64 = -904011478;

pub struct PotentSulfurBlockEntity {
    base: BlockEntityData,
    pub waiting_countdown: i32,
    pub eruption_tick: i64,
}

impl PotentSulfurBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::POTENT_SULFUR, pos, state), waiting_countdown: -1, eruption_tick: -1 }
    }

    /// `resetCountdown`.
    pub fn reset_countdown(&mut self) {
        self.waiting_countdown = -1;
    }

    /// `SERVER_WAITING_COUNTDOWN_TICKER`.
    fn waiting_countdown_ticker(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        if level.get_game_time() % 20 != 0 {
            return;
        }
        let Some(source_block) = find_noxious_gas_source_block(level, pos) else { return };
        if self.waiting_countdown <= 0 {
            let water_blocks = source_block.y() - pos.y() - 1;
            let mut geyser_positional = geyser_positional(level, pos);
            if state.get(props::POTENT_SULFUR_STATE) == PotentSulfurState::Dormant {
                self.waiting_countdown = 10 * (water_blocks - 1) + geyser_positional.next_int_between_inclusive(15, 30);
            } else {
                geyser_positional.next_int();
                self.waiting_countdown = water_blocks - 1 + geyser_positional.next_int_between_inclusive(1, 2);
            }
        }
        if self.waiting_countdown > 0 {
            self.waiting_countdown -= 1;
        }
        if self.waiting_countdown == 0 {
            let state_to_set = if state.get(props::POTENT_SULFUR_STATE) == PotentSulfurState::Dormant { PotentSulfurState::Erupting } else { PotentSulfurState::Dormant };
            level.set_block_and_update(pos, state.set(props::POTENT_SULFUR_STATE, state_to_set));
            if state_to_set == PotentSulfurState::Dormant {
                level.game_event_at_block(GameEvent::BLOCK_DEACTIVATE, pos, &crate::world::level::gameevent::GameEventContext::of_state(Some(state)));
            }
        }
    }
}

/// `geyserPositional`.
pub fn geyser_positional(level: &ServerLevel, pos: BlockPos) -> Box<dyn RandomSource> {
    XoroshiroRandomSource::from_seed(level.seed ^ GEYSER_SALT).fork_positional().at(pos.x(), pos.y(), pos.z())
}

/// `SERVER_NAUSEA_EFFECT_TICKER`.
fn nausea_effect_ticker(level: &mut ServerLevel, pos: BlockPos) {
    if level.get_game_time() % EFFECT_APPLICATION_FREQUENCY_TICKS != 0 {
        return;
    }
    let Some(source_block) = find_noxious_gas_source_block(level, pos) else { return };
    let aabb = AABB::from_block_pos(&source_block).inflate(2.5, 0.0, 2.5);
    let nearby = level.get_entities_of_class(&aabb, |e| e.living().is_some(), |e| !e.is_spectator() && e.is_alive());
    for id in nearby {
        let eye = level.with_entity(id, |_, e| e.base().get_eye_position());
        if let Some(eye) = eye
            && can_be_reached_by_noxious_gas(level, source_block, eye)
        {
            level.with_entity(id, |level, e| {
                e.add_effect(level, MobEffectInstance::with_flags(MobEffect::NAUSEA, EFFECT_DURATION_IN_TICKS, 0, true, true), None);
            });
        }
    }
}

/// `LAUNCH_ENTITY_TICKER`.
fn launch_entity_ticker(level: &mut ServerLevel, pos: BlockPos) {
    let Some(source_block) = find_noxious_gas_source_block(level, pos) else { return };
    let water_blocks = source_block.y() - pos.y() - 1;
    let geyser_force_height = get_unobstructed_block_count(level, pos.above(), water_blocks);
    let aabb = AABB::from_block_pos(&pos.above()).expand_towards(0.0, f64::from(geyser_force_height - 1), 0.0);
    let targets = level.get_entities_of_class(&aabb, |_| true, |e| !e.is_spectator() && e.is_alive());
    let limit = f64::from(0.3f32) + f64::from(water_blocks) * 0.1;
    for id in targets {
        level.with_entity(id, |_, e| {
            let velocity = e.base().delta_movement();
            e.check_fall_distance_accumulation();
            let flying_player = e.player().is_some_and(|p| p.abilities.flying);
            if e.can_simulate_movement()
                && !flying_player
                && !e.base().is_passenger()
                && !mc_data::tags::get::<mc_data::EntityType>("not_affected_by_geysers").is_some_and(|t| t.contains(e.get_type()))
                && velocity.y < limit
            {
                e.base_mut().add_delta_movement(Vec3::new(0.0, f64::from(0.2f32), 0.0));
            }
        });
    }
}

fn get_unobstructed_block_count(level: &dyn BlockGetter, pos: BlockPos, water_blocks: i32) -> i32 {
    let geyser_force_height = 6 * water_blocks;
    let context = CollisionContext::position_context(f64::from(pos.below().y()));
    for i in 0..geyser_force_height {
        let current_pos = pos.above_n(i);
        if !is_geyser_passable_block(level.get_block_state(current_pos), level, current_pos, &context) {
            return i;
        }
    }
    geyser_force_height
}

fn is_geyser_passable_block(state: BlockState, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> bool {
    if !state.is_air() && !state.is_block(Block::WATER) { state.get_collision_shape_ctx(level, pos, context).is_empty() } else { true }
}

/// `findNoxiousGasSourceBlock`.
fn find_noxious_gas_source_block(level: &dyn BlockGetter, origin: BlockPos) -> Option<BlockPos> {
    let max_y = origin.y() + 4 + 1;
    let context = CollisionContext::position_context(f64::from(origin.y()));
    let mut pos = origin.above();
    while pos.y() <= max_y {
        let state = level.get_block_state(pos);
        let is_water_logged = level.get_fluid_state(pos).is_source_of_type(Fluid::WATER);
        if !is_water_logged || (!state.is_block(Block::WATER) && !is_geyser_passable_block(state, level, pos, &context)) {
            if state.is_air() || is_geyser_passable_block(state, level, pos, &context) {
                return Some(pos);
            }
            break;
        }
        pos = pos.above();
    }
    None
}

/// `canBeReachedByNoxiousGas`.
pub fn can_be_reached_by_noxious_gas(level: &ServerLevel, source_block: BlockPos, pos: Vec3) -> bool {
    let block_pos = BlockPos::containing_position(&pos);
    let context = CollisionContext::position_context(f64::from(block_pos.below().y()));
    if !is_geyser_passable_block(level.get_block_state(block_pos), level, block_pos, &context) {
        return false;
    }
    if pos.distance_to_sqr(Vec3::at_center_of(&source_block)) > 9.0 {
        return false;
    }
    let below_source = Vec3::at_center_of(&source_block.below());
    let below_pos = pos.with(Axis::Y, pos.y - 1.0);
    is_water(level, below_pos) && have_line_of_sight(level, below_source, below_pos)
}

fn is_water(level: &ServerLevel, pos: Vec3) -> bool {
    level.get_fluid_state(BlockPos::containing_position(&pos)).is_source_of_type(Fluid::WATER)
}

fn have_line_of_sight(level: &ServerLevel, a: Vec3, b: Vec3) -> bool {
    level.clip(&ClipContext::new(a, b, ClipBlock::Collider, ClipFluid::None, CollisionContext::empty())).miss
}

impl BlockEntity for PotentSulfurBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        if let Some(countdown) = tag.get("countdown").and_then(|t| t.as_number()) {
            self.waiting_countdown = countdown.int_value();
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_int("countdown", self.waiting_countdown);
    }

    fn is_ticking(&self) -> bool {
        self.get_block_state().get(props::POTENT_SULFUR_STATE) != PotentSulfurState::Dry
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let state = BlockGetter::get_block_state(level, pos);
        match state.get(props::POTENT_SULFUR_STATE) {
            PotentSulfurState::Dry => {}
            PotentSulfurState::Wet => nausea_effect_ticker(level, pos),
            PotentSulfurState::Dormant => {
                self.waiting_countdown_ticker(level, pos, state);
                nausea_effect_ticker(level, pos);
            }
            PotentSulfurState::Erupting => {
                launch_entity_ticker(level, pos);
                self.waiting_countdown_ticker(level, pos, state);
            }
            PotentSulfurState::Continuous => launch_entity_ticker(level, pos),
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static POTENT_SULFUR: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::POTENT_SULFUR, create: |pos, state| Box::new(PotentSulfurBlockEntity::new(pos, state)) };
