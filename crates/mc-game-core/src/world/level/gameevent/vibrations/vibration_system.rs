//! `VibrationSystem`: the persistent [`VibrationData`], the user contract ([`VibrationSystem`]: vanilla's
//! `VibrationSystem` + `VibrationSystem.User`), the listener (`Listener.handleGameEvent`, occlusion) and the ticker
//! (`Ticker.tick`) as functions over any implementor.
//!
//! Block entities (sculk sensor, calibrated sensor, shrieker) implement the trait through a small `Copy` handle that
//! looks the block entity up in the level for every access (see `block_entity::vibration`), because their vibration
//! logic changes their own block and the neighbours then read the block entity again; entities (warden) implement it on
//! the taken-out entity itself.

use super::vibration_info::{self, VibrationInfo};
use super::vibration_selector::{self, VibrationSelector};
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::level::collision::{BlockGetterExt, ClipBlockStateContext};
use crate::world::level::gameevent::{GameEventContext, PositionSource, SourceEntityInfo};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::GameEvent;
use mc_dfu::extra::{NON_NEGATIVE_INT, optional_always_present_field_of};
use mc_dfu::{Codec, record};
use mc_protocol::packets::play::ParticleOptions;
use std::sync::LazyLock;

/// `NO_VIBRATION_FREQUENCY`.
pub const NO_VIBRATION_FREQUENCY: i32 = 0;

/// `RESONANCE_EVENTS`.
pub const RESONANCE_EVENTS: [GameEvent; 15] = [
    GameEvent::RESONATE_1,
    GameEvent::RESONATE_2,
    GameEvent::RESONATE_3,
    GameEvent::RESONATE_4,
    GameEvent::RESONATE_5,
    GameEvent::RESONATE_6,
    GameEvent::RESONATE_7,
    GameEvent::RESONATE_8,
    GameEvent::RESONATE_9,
    GameEvent::RESONATE_10,
    GameEvent::RESONATE_11,
    GameEvent::RESONATE_12,
    GameEvent::RESONATE_13,
    GameEvent::RESONATE_14,
    GameEvent::RESONATE_15,
];

/// `getGameEventFrequency(event)` (`VIBRATION_FREQUENCY_FOR_EVENT`).
pub fn get_game_event_frequency(event: GameEvent) -> i32 {
    if let Some(i) = RESONANCE_EVENTS.iter().position(|e| *e == event) {
        return i as i32 + 1;
    }
    match event {
        GameEvent::STEP | GameEvent::SWIM | GameEvent::FLAP => 1,
        GameEvent::PROJECTILE_LAND
        | GameEvent::HIT_GROUND
        | GameEvent::SPLASH
        | GameEvent::BOUNCE => 2,
        GameEvent::ITEM_INTERACT_FINISH
        | GameEvent::PROJECTILE_SHOOT
        | GameEvent::INSTRUMENT_PLAY => 3,
        GameEvent::ENTITY_ACTION | GameEvent::ELYTRA_GLIDE | GameEvent::UNEQUIP => 4,
        GameEvent::ENTITY_DISMOUNT | GameEvent::EQUIP => 5,
        GameEvent::ENTITY_INTERACT | GameEvent::SHEAR | GameEvent::ENTITY_MOUNT => 6,
        GameEvent::ENTITY_DAMAGE => 7,
        GameEvent::DRINK | GameEvent::EAT => 8,
        GameEvent::CONTAINER_CLOSE
        | GameEvent::BLOCK_CLOSE
        | GameEvent::BLOCK_DEACTIVATE
        | GameEvent::BLOCK_DETACH => 9,
        GameEvent::CONTAINER_OPEN
        | GameEvent::BLOCK_OPEN
        | GameEvent::BLOCK_ACTIVATE
        | GameEvent::BLOCK_ATTACH
        | GameEvent::PRIME_FUSE
        | GameEvent::NOTE_BLOCK_PLAY => 10,
        GameEvent::BLOCK_CHANGE => 11,
        GameEvent::BLOCK_DESTROY | GameEvent::FLUID_PICKUP => 12,
        GameEvent::BLOCK_PLACE | GameEvent::FLUID_PLACE => 13,
        GameEvent::ENTITY_PLACE | GameEvent::LIGHTNING_STRIKE | GameEvent::TELEPORT => 14,
        GameEvent::ENTITY_DIE | GameEvent::EXPLODE => 15,
        _ => NO_VIBRATION_FREQUENCY,
    }
}

/// `getResonanceEventByFrequency(frequency)`.
pub fn get_resonance_event_by_frequency(vibration_frequency: i32) -> GameEvent {
    RESONANCE_EVENTS[(vibration_frequency - 1) as usize]
}

/// `getRedstoneStrengthForDistance(distance, listenerRadius)`.
pub fn get_redstone_strength_for_distance(distance: f32, listener_radius: i32) -> i32 {
    let power_scale = 15.0 / f64::from(listener_radius);
    1.max(15 - mth::floor_f64(power_scale * f64::from(distance)))
}

/// `VibrationSystem.Data`.
#[derive(Clone, Debug, Default)]
pub struct VibrationData {
    pub current_vibration: Option<VibrationInfo>,
    pub travel_time_in_ticks: i32,
    pub selection_strategy: VibrationSelector,
    pub reload_vibration_particle: bool,
}

struct Wire {
    event: Option<VibrationInfo>,
    selector: VibrationSelector,
    event_delay: i32,
}

impl VibrationData {
    /// `decrementTravelTime()`.
    pub fn decrement_travel_time(&mut self) {
        self.travel_time_in_ticks = 0.max(self.travel_time_in_ticks - 1);
    }

    /// `VibrationSystem.Data.CODEC`.
    pub fn codec() -> &'static Codec<VibrationData> {
        static CODEC: LazyLock<Codec<VibrationData>> = LazyLock::new(|| {
            record!(Wire {
                event: vibration_info::CODEC.lenient_optional_field_of("event"),
                selector: vibration_selector::CODEC.field_of("selector"),
                event_delay: optional_always_present_field_of(&NON_NEGATIVE_INT, "event_delay", 0),
            })
            .codec()
            .xmap(
                |w: Wire| VibrationData {
                    current_vibration: w.event,
                    travel_time_in_ticks: w.event_delay,
                    selection_strategy: w.selector,
                    reload_vibration_particle: true,
                },
                |d: &VibrationData| Wire {
                    event: d.current_vibration.clone(),
                    selector: d.selection_strategy.clone(),
                    event_delay: d.travel_time_in_ticks,
                },
            )
        });
        &CODEC
    }
}

/// `VibrationSystem` + `VibrationSystem.User`.
pub trait VibrationSystem {
    /// `getVibrationData()`: run `f` on the data (the implementor decides where it lives).
    fn with_vibration_data<R>(
        &mut self,
        level: &mut ServerLevel,
        f: impl FnOnce(&mut VibrationData) -> R,
    ) -> R;

    /// `User.getListenerRadius()`.
    fn get_listener_radius(&self) -> i32;
    /// `User.getPositionSource()`.
    fn get_position_source(&self) -> PositionSource;
    /// The entity that owns this system and the current position of its source, for owners that the level cannot look up
    /// because they are taken out of their slot while they act (`PositionSource.getPosition(level)` of an entity source).
    fn owner_entity(&self) -> Option<(EntityId, Vec3)> {
        None
    }
    /// `User.canReceiveVibration(level, pos, event, context)`.
    fn can_receive_vibration(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        event: GameEvent,
        context: &GameEventContext,
    ) -> bool;
    /// `User.onReceiveVibration(level, pos, event, sourceEntity, projectileOwner, receivingDistance)`.
    fn on_receive_vibration(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        event: GameEvent,
        source_entity: Option<SourceEntityInfo>,
        projectile_owner: Option<SourceEntityInfo>,
        receiving_distance: f32,
    );
    /// `User.getListenableEvents()`: the game event tag (path).
    fn get_listenable_events(&self) -> &'static str {
        "vibrations"
    }
    /// `User.canTriggerAvoidVibration()`.
    fn can_trigger_avoid_vibration(&self) -> bool {
        false
    }
    /// `User.requiresAdjacentChunksToBeTicking()`.
    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        false
    }
    /// `User.calculateTravelTimeInTicks(distanceToDestination)`.
    fn calculate_travel_time_in_ticks(&self, distance_to_destination: f32) -> i32 {
        mth::floor_f32(distance_to_destination)
    }
    /// `User.isValidVibration(event, context)`.
    fn is_valid_vibration(
        &mut self,
        level: &mut ServerLevel,
        event: GameEvent,
        context: &GameEventContext,
    ) -> bool {
        if !event_is_in_tag(event, self.get_listenable_events()) {
            return false;
        }
        if let Some(source) = resolve_source_entity(level, context) {
            if source.spectator {
                return false;
            }
            if source.stepping_carefully && event_is_in_tag(event, "ignore_vibrations_sneaking") {
                if self.can_trigger_avoid_vibration() && source.is_server_player {
                    super::trigger_avoid_vibration(level, source.id);
                }
                return false;
            }
            if source.dampens_vibrations {
                return false;
            }
        }
        context
            .affected_state
            .is_none_or(|state| !state_is_tag(state, "dampens_vibrations"))
    }
    /// `User.onDataChanged()` (`setChanged()` for block entities).
    fn on_data_changed(&mut self, _level: &mut ServerLevel) {}
}

fn event_is_in_tag(event: GameEvent, tag: &str) -> bool {
    mc_data::tags::get::<GameEvent>(tag).is_some_and(|t| t.contains(event))
}

fn state_is_tag(state: mc_data::block::BlockState, tag: &str) -> bool {
    state.block().is_in_tag(tag)
}

/// `context.sourceEntity()`: the facts a delayed vibration carries, else the entity as the level has it right now
/// (stored or lent by the code that acts).
pub fn resolve_source_entity(
    level: &ServerLevel,
    context: &GameEventContext,
) -> Option<SourceEntityInfo> {
    context.source_info.or_else(|| {
        level
            .get_entity(context.source_entity?)
            .map(|entity| super::source_entity_info(level, entity))
    })
}

/// `PositionSource.getPosition(level)`.
pub fn get_position(level: &ServerLevel, source: &PositionSource) -> Option<Vec3> {
    source.get_position(&|uuid| level.get_entity_by_uuid(uuid).map(|e| e.position()))
}

/// `user.getPositionSource().getPosition(level)`.
fn listener_position<S: VibrationSystem>(level: &ServerLevel, system: &S) -> Option<Vec3> {
    system
        .owner_entity()
        .map(|(_, position)| position)
        .or_else(|| get_position(level, &system.get_position_source()))
}

/// `VibrationSystem.Listener.handleGameEvent`.
pub fn handle_game_event<S: VibrationSystem>(
    level: &mut ServerLevel,
    system: &mut S,
    event: GameEvent,
    context: &GameEventContext,
    source_position: Vec3,
) -> bool {
    if system.with_vibration_data(level, |data| data.current_vibration.is_some()) {
        return false;
    }
    if !system.is_valid_vibration(level, event, context) {
        return false;
    }
    let Some(destination) = listener_position(level, system) else {
        return false;
    };
    if !system.can_receive_vibration(
        level,
        BlockPos::containing_position(&source_position),
        event,
        context,
    ) {
        return false;
    }
    if is_occluded(level, source_position, destination) {
        return false;
    }
    schedule_vibration(level, system, event, context, source_position, destination);
    true
}

/// `VibrationSystem.Listener.forceScheduleVibration`.
pub fn force_schedule_vibration<S: VibrationSystem>(
    level: &mut ServerLevel,
    system: &mut S,
    event: GameEvent,
    context: &GameEventContext,
    origin: Vec3,
) {
    if let Some(destination) = listener_position(level, system) {
        schedule_vibration(level, system, event, context, origin, destination);
    }
}

fn schedule_vibration<S: VibrationSystem>(
    level: &mut ServerLevel,
    system: &mut S,
    event: GameEvent,
    context: &GameEventContext,
    origin: Vec3,
    dest: Vec3,
) {
    let game_time = level.game_time();
    let info = VibrationInfo::new(
        event,
        origin.distance_to(dest) as f32,
        origin,
        resolve_source_entity(level, context),
    );
    system.with_vibration_data(level, |data| {
        data.selection_strategy.add_candidate(info, game_time)
    });
}

/// `VibrationSystem.Listener.distanceBetweenInBlocks`.
pub fn distance_between_in_blocks(origin: BlockPos, dest: BlockPos) -> f32 {
    origin.dist_sqr(&dest).sqrt() as f32
}

/// `VibrationSystem.Listener.isOccluded`.
fn is_occluded(level: &ServerLevel, origin: Vec3, dest: Vec3) -> bool {
    let from = Vec3::new(
        f64::from(mth::floor_f64(origin.x)) + 0.5,
        f64::from(mth::floor_f64(origin.y)) + 0.5,
        f64::from(mth::floor_f64(origin.z)) + 0.5,
    );
    let to = Vec3::new(
        f64::from(mth::floor_f64(dest.x)) + 0.5,
        f64::from(mth::floor_f64(dest.y)) + 0.5,
        f64::from(mth::floor_f64(dest.z)) + 0.5,
    );
    let occluding =
        |state: mc_data::block::BlockState| state_is_tag(state, "occludes_vibration_signals");
    for direction in Direction::VALUES {
        let nudged_source = from.relative(direction, f64::from(1.0e-5f32));
        if level
            .is_block_in_line(&ClipBlockStateContext {
                from: nudged_source,
                to,
                is_target_block: &occluding,
            })
            .miss
        {
            return false;
        }
    }
    true
}

/// `VibrationSystem.Ticker.tick(level, data, user)`.
pub fn tick<S: VibrationSystem>(level: &mut ServerLevel, system: &mut S) {
    if system.with_vibration_data(level, |data| data.current_vibration.is_none()) {
        try_select_and_schedule_vibration(level, system);
    }
    let Some(current_vibration) =
        system.with_vibration_data(level, |data| data.current_vibration.clone())
    else {
        return;
    };
    let mut has_changed = system.with_vibration_data(level, |data| data.travel_time_in_ticks > 0);
    try_reload_vibration_particle(level, system);
    let travel_time = system.with_vibration_data(level, |data| {
        data.decrement_travel_time();
        data.travel_time_in_ticks
    });
    if travel_time <= 0 {
        has_changed = receive_vibration(level, system, &current_vibration);
    }
    if has_changed {
        system.on_data_changed(level);
    }
}

/// The wire form of a vibration particle destination (`VibrationParticleOption`).
fn vibration_particle(
    level: &ServerLevel,
    source: &PositionSource,
    owner: Option<EntityId>,
    arrival_in_ticks: i32,
) -> Option<ParticleOptions> {
    let destination = match source {
        PositionSource::Block(pos) => mc_protocol::packets::play::PositionSource::Block(*pos),
        PositionSource::Entity { uuid, y_offset } => {
            mc_protocol::packets::play::PositionSource::Entity {
                source_entity_id: match owner {
                    Some(id) => id,
                    None => level.get_entity_by_uuid(*uuid)?.id(),
                },
                y_offset: *y_offset,
            }
        }
    };
    Some(ParticleOptions::Vibration {
        destination,
        arrival_in_ticks,
    })
}

fn try_select_and_schedule_vibration<S: VibrationSystem>(level: &mut ServerLevel, system: &mut S) {
    let game_time = level.game_time();
    let Some(context) = system.with_vibration_data(level, |data| {
        data.selection_strategy.chosen_candidate(game_time)
    }) else {
        return;
    };
    let origin = context.pos;
    let travel_time = system.calculate_travel_time_in_ticks(context.distance);
    system.with_vibration_data(level, |data| {
        data.current_vibration = Some(context);
        data.travel_time_in_ticks = travel_time;
    });
    let owner = system.owner_entity().map(|(id, _)| id);
    if let Some(particle) =
        vibration_particle(level, &system.get_position_source(), owner, travel_time)
    {
        level.send_particles(
            particle, origin.x, origin.y, origin.z, 1, 0.0, 0.0, 0.0, 0.0,
        );
    }
    system.on_data_changed(level);
    system.with_vibration_data(level, |data| data.selection_strategy.start_over());
}

fn try_reload_vibration_particle<S: VibrationSystem>(level: &mut ServerLevel, system: &mut S) {
    let (reload, current, travel_time) = system.with_vibration_data(level, |data| {
        (
            data.reload_vibration_particle,
            data.current_vibration.clone(),
            data.travel_time_in_ticks,
        )
    });
    if !reload {
        return;
    }
    let Some(current) = current else {
        system.with_vibration_data(level, |data| data.reload_vibration_particle = false);
        return;
    };
    let origin = current.pos;
    let position_source = system.get_position_source();
    let destination = listener_position(level, system).unwrap_or(origin);
    let owner = system.owner_entity().map(|(id, _)| id);
    let initial_travel_time = system.calculate_travel_time_in_ticks(current.distance);
    let alpha = 1.0 - f64::from(travel_time) / f64::from(initial_travel_time);
    let new_initial_x = mth::lerp_f64(alpha, origin.x, destination.x);
    let new_initial_y = mth::lerp_f64(alpha, origin.y, destination.y);
    let new_initial_z = mth::lerp_f64(alpha, origin.z, destination.z);
    let particle_was_sent = vibration_particle(level, &position_source, owner, travel_time)
        .is_some_and(|particle| {
            level.send_particles(
                particle,
                new_initial_x,
                new_initial_y,
                new_initial_z,
                1,
                0.0,
                0.0,
                0.0,
                0.0,
            ) > 0
        });
    if particle_was_sent {
        system.with_vibration_data(level, |data| data.reload_vibration_particle = false);
    }
}

fn receive_vibration<S: VibrationSystem>(
    level: &mut ServerLevel,
    system: &mut S,
    current_vibration: &VibrationInfo,
) -> bool {
    let origin = BlockPos::containing_position(&current_vibration.pos);
    let destination =
        listener_position(level, system).map_or(origin, |p| BlockPos::containing_position(&p));
    if system.requires_adjacent_chunks_to_be_ticking()
        && !are_adjacent_chunks_ticking(level, destination)
    {
        return false;
    }
    let source_entity = current_vibration.get_entity(level);
    let projectile_owner = current_vibration.get_projectile_owner(level);
    system.on_receive_vibration(
        level,
        origin,
        current_vibration.game_event,
        source_entity,
        projectile_owner,
        distance_between_in_blocks(origin, destination),
    );
    system.with_vibration_data(level, |data| data.current_vibration = None);
    true
}

fn are_adjacent_chunks_ticking(level: &ServerLevel, listener_pos: BlockPos) -> bool {
    let (listener_x, listener_z) = (listener_pos.x() >> 4, listener_pos.z() >> 4);
    for x in listener_x - 1..=listener_x + 1 {
        for z in listener_z - 1..=listener_z + 1 {
            if !level.should_tick_blocks_at(ChunkPos::pack_xz(x, z))
                || level.chunk_now(x, z).is_none()
            {
                return false;
            }
        }
    }
    true
}
