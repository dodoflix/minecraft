//! `CopperGolem`: a small golem that carries items from copper chests to ordinary chests. It oxidizes over time (waxing and
//! scraping with an axe stop and undo that), turns into a statue block once fully oxidized, and gives its antenna item to
//! shears.
//!
//! Not implemented: the client-only animation states (`setupAnimationStates`, the head spin sound) and the `ClientboundEntityEvent`
//! consumers of the item interactions; the server never reads them.

use super::abstract_golem::AMBIENT_SOUND_INTERVAL;
use super::copper_golem_ai;
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent, Item, SoundEvent, props};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::CopperGolemStatueBlockEntity;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::TRANSPORT_ITEMS_COOLDOWN_TICKS;
use mc_game_core::world::entity::ai::navigation::{PathNavigation, StandardPathNavigation};
use mc_game_core::world::entity::ai::sensing::sensor_type::{HURT_BY, NEAREST_LIVING_ENTITIES};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::lightning_bolt::LightningBolt;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, interact, leashable};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, hurt_and_break};
use mc_game_core::world::level::block::helpers::is_instance_of;
use mc_game_core::world::level::block::impls::chest_block;
use mc_game_core::world::level::block::weathering_copper::WeatherState;
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{CopperGolemState, SoundSource, WeatheringCopperState};
use uuid::Uuid;

define_accessor!(pub DATA_WEATHER_STATE: WeatheringCopperState = CopperGolem);
define_accessor!(pub COPPER_GOLEM_STATE: CopperGolemState = CopperGolem);

/// `IGNORE_WEATHERING_TICK` (waxed).
const IGNORE_WEATHERING_TICK: i64 = -2;
/// `UNSET_WEATHERING_TICK`.
const UNSET_WEATHERING_TICK: i64 = -1;
const WEATHERING_TICK_FROM: i32 = 504_000;
const WEATHERING_TICK_TO: i32 = 552_000;
/// `TURN_TO_STATUE_CHANCE`.
const TURN_TO_STATUE_CHANCE: f32 = 0.0058;
const SPAWN_COOLDOWN_MIN: i32 = 60;
const SPAWN_COOLDOWN_MAX: i32 = 100;
/// `CopperGolem.EQUIPMENT_SLOT_ANTENNA`.
pub const EQUIPMENT_SLOT_ANTENNA: EquipmentSlot = EquipmentSlot::Saddle;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[&NEAREST_LIVING_ENTITIES, &HURT_BY],
    copper_golem_ai::get_activities,
);

fn weather_state_to_data(state: WeatherState) -> WeatheringCopperState {
    match state {
        WeatherState::Unaffected => WeatheringCopperState::Unaffected,
        WeatherState::Exposed => WeatheringCopperState::Exposed,
        WeatherState::Weathered => WeatheringCopperState::Weathered,
        WeatherState::Oxidized => WeatheringCopperState::Oxidized,
    }
}

fn weather_state_from_data(state: WeatheringCopperState) -> WeatherState {
    match state {
        WeatheringCopperState::Unaffected => WeatherState::Unaffected,
        WeatheringCopperState::Exposed => WeatherState::Exposed,
        WeatheringCopperState::Weathered => WeatherState::Weathered,
        WeatheringCopperState::Oxidized => WeatherState::Oxidized,
    }
}

/// `WeatheringCopper.WeatherState.getSerializedName()`.
fn weather_state_name(state: WeatherState) -> &'static str {
    match state {
        WeatherState::Unaffected => "unaffected",
        WeatherState::Exposed => "exposed",
        WeatherState::Weathered => "weathered",
        WeatherState::Oxidized => "oxidized",
    }
}

fn weather_state_by_name(name: &str) -> Option<WeatherState> {
    [
        WeatherState::Unaffected,
        WeatherState::Exposed,
        WeatherState::Weathered,
        WeatherState::Oxidized,
    ]
    .into_iter()
    .find(|s| weather_state_name(*s) == name)
}

/// `CopperGolemState.getSerializedName()`.
pub fn state_name(state: CopperGolemState) -> &'static str {
    match state {
        CopperGolemState::Idle => "idle",
        CopperGolemState::GettingItem => "getting_item",
        CopperGolemState::GettingNoItem => "getting_no_item",
        CopperGolemState::DroppingItem => "dropping_item",
        CopperGolemState::DroppingNoItem => "dropping_no_item",
    }
}

/// `CopperGolemOxidationLevels.getOxidationLevel(state)`: the sounds of a weather state (unaffected and exposed share them).
struct OxidationLevel {
    hurt: SoundEvent,
    death: SoundEvent,
    step: SoundEvent,
}

fn oxidation_level(state: WeatherState) -> OxidationLevel {
    match state {
        WeatherState::Unaffected | WeatherState::Exposed => OxidationLevel {
            hurt: SoundEvent::ENTITY_COPPER_GOLEM_HURT,
            death: SoundEvent::ENTITY_COPPER_GOLEM_DEATH,
            step: SoundEvent::ENTITY_COPPER_GOLEM_STEP,
        },
        WeatherState::Weathered => OxidationLevel {
            hurt: SoundEvent::ENTITY_COPPER_GOLEM_WEATHERED_HURT,
            death: SoundEvent::ENTITY_COPPER_GOLEM_WEATHERED_DEATH,
            step: SoundEvent::ENTITY_COPPER_GOLEM_WEATHERED_STEP,
        },
        WeatherState::Oxidized => OxidationLevel {
            hurt: SoundEvent::ENTITY_COPPER_GOLEM_OXIDIZED_HURT,
            death: SoundEvent::ENTITY_COPPER_GOLEM_OXIDIZED_DEATH,
            step: SoundEvent::ENTITY_COPPER_GOLEM_OXIDIZED_STEP,
        },
    }
}

pub struct CopperGolem {
    pub mob: MobData,
    /// `openedChestPos`.
    opened_chest_pos: Option<BlockPos>,
    /// `lastLightningBoltUUID`.
    last_lightning_bolt_uuid: Option<Uuid>,
    /// `nextWeatheringTick`.
    next_weathering_tick: i64,
}

impl CopperGolem {
    /// `new CopperGolem(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            b.define(DATA_WEATHER_STATE, WeatheringCopperState::Unaffected);
            b.define(COPPER_GOLEM_STATE, CopperGolemState::Idle);
        });
        let mut golem = CopperGolem {
            mob,
            opened_chest_pos: None,
            last_lightning_bolt_uuid: None,
            next_weathering_tick: UNSET_WEATHERING_TICK,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = golem.make_brain(&BrainPacked::EMPTY);
        golem.mob.living.brain = brain;
        let mut navigation = StandardPathNavigation::ground(&golem);
        navigation.set_required_path_length(&golem, 48.0);
        navigation.set_can_open_doors(true);
        golem.mob.set_navigation(Box::new(navigation));
        golem.set_persistence_required();
        golem.set_state(CopperGolemState::Idle);
        golem
            .mob
            .set_pathfinding_malus(PathType::FireInNeighbor, 16.0);
        golem
            .mob
            .set_pathfinding_malus(PathType::DamagingInNeighbor, 16.0);
        golem.mob.set_pathfinding_malus(PathType::Fire, -1.0);
        let cooldown = golem
            .base_mut()
            .random
            .next_int_between(SPAWN_COOLDOWN_MIN, SPAWN_COOLDOWN_MAX);
        golem
            .brain_mut()
            .set_memory(&TRANSPORT_ITEMS_COOLDOWN_TICKS, cooldown);
        golem
    }

    /// `getState()`.
    pub fn get_state(&self) -> CopperGolemState {
        self.base().entity_data.get(COPPER_GOLEM_STATE)
    }

    /// `setState(state)`.
    pub fn set_state(&mut self, state: CopperGolemState) {
        self.base_mut().entity_data.set(COPPER_GOLEM_STATE, state);
    }

    /// `getWeatherState()`.
    pub fn get_weather_state(&self) -> WeatherState {
        weather_state_from_data(self.base().entity_data.get(DATA_WEATHER_STATE))
    }

    /// `setWeatherState(state)`.
    pub fn set_weather_state(&mut self, state: WeatherState) {
        self.base_mut()
            .entity_data
            .set(DATA_WEATHER_STATE, weather_state_to_data(state));
    }

    /// `setOpenedChestPos(pos)`.
    pub fn set_opened_chest_pos(&mut self, pos: BlockPos) {
        self.opened_chest_pos = Some(pos);
    }

    /// `clearOpenedChestPos()`.
    pub fn clear_opened_chest_pos(&mut self) {
        self.opened_chest_pos = None;
    }

    /// `hasContainerOpen(container, blockPos)`.
    fn has_container_open(&self, level: &ServerLevel, block_pos: BlockPos) -> bool {
        let Some(opened) = self.opened_chest_pos else {
            return false;
        };
        let state = level.get_block_state(opened);
        opened == block_pos
            || (is_instance_of(state, "ChestBlock")
                && state.get(props::CHEST_TYPE) != mc_data::enums::ChestType::Single
                && chest_block::get_connected_block_pos(opened, state) == block_pos)
    }

    /// `updateWeathering(level, random, gameTime)`.
    fn update_weathering(&mut self, level: &mut ServerLevel, game_time: i64) {
        if self.next_weathering_tick == IGNORE_WEATHERING_TICK {
            return;
        }
        if self.next_weathering_tick == UNSET_WEATHERING_TICK {
            self.next_weathering_tick = game_time
                + i64::from(
                    level
                        .tick_state
                        .random
                        .next_int_between_inclusive(WEATHERING_TICK_FROM, WEATHERING_TICK_TO),
                );
        } else {
            let weather_state = self.get_weather_state();
            let is_fully_oxidized = weather_state == WeatherState::Oxidized;
            if game_time >= self.next_weathering_tick && !is_fully_oxidized {
                let new_state = weather_state.next();
                let is_new_state_fully_oxidized = new_state == WeatherState::Oxidized;
                self.set_weather_state(new_state);
                self.next_weathering_tick =
                    if is_new_state_fully_oxidized {
                        0
                    } else {
                        self.next_weathering_tick
                            + i64::from(level.tick_state.random.next_int_between_inclusive(
                                WEATHERING_TICK_FROM,
                                WEATHERING_TICK_TO,
                            ))
                    };
            }
            if is_fully_oxidized && self.can_turn_to_statue(level) {
                self.turn_to_statue(level);
            }
        }
    }

    /// `canTurnToStatue(level)`.
    fn can_turn_to_statue(&self, level: &mut ServerLevel) -> bool {
        level.get_block_state(self.block_position()).is_air()
            && level.tick_state.random.next_float() <= TURN_TO_STATUE_CHANCE
    }

    /// `turnToStatue(level)`.
    fn turn_to_statue(&mut self, level: &mut ServerLevel) {
        let pos = self.block_position();
        let pose = mc_data::enums::CopperGolemStatueBlockPose::from_ordinal(
            self.base_mut().random.next_int_between(0, 4) as usize,
        )
        .expect("a pose of the four");
        let state = Block::OXIDIZED_COPPER_GOLEM_STATUE
            .default_state()
            .set(props::COPPER_GOLEM_POSE, pose)
            .set(
                props::HORIZONTAL_FACING,
                Direction::from_y_rot(f64::from(self.base().y_rot())),
            );
        level.set_block_and_update(pos, state);
        let name = self.base().get_custom_name();
        let created = level
            .with_block_entity(pos, |level, be| {
                be.as_any_mut()
                    .downcast_mut::<CopperGolemStatueBlockEntity>()
                    .map(|statue| statue.create_statue(level, name))
                    .is_some()
            })
            .unwrap_or(false);
        if created {
            self.drop_preserved_equipment(level);
            self.discard_in_level(level);
            self.play_sound_default(level, SoundEvent::ENTITY_COPPER_GOLEM_BECOME_STATUE);
            if leashable::is_leashed(self) {
                if level.entity_drops() {
                    leashable::drop_leash(self, level);
                } else {
                    leashable::remove_leash(self, level);
                }
            }
        }
    }

    /// `spawn(weatherState)`.
    pub fn spawn(&mut self, level: &mut ServerLevel, weather_state: WeatherState) {
        self.set_weather_state(weather_state);
        self.play_spawn_sound(level);
    }

    /// `playSpawnSound()`.
    pub fn play_spawn_sound(&self, level: &mut ServerLevel) {
        self.play_sound_default(level, SoundEvent::ENTITY_COPPER_GOLEM_SPAWN);
    }

    /// `level.playSound(null, this, sound, this.getSoundSource(), 1.0F, 1.0F)`.
    fn play_sound_to_all(&self, level: &mut ServerLevel, sound: SoundEvent) {
        level.play_sound_from_entity_at(
            None,
            self.id(),
            self.position(),
            sound,
            self.get_sound_source(),
            1.0,
            1.0,
        );
    }
}

fn is_axe(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("axes").is_some_and(|tag| tag.contains(stack.item()))
}

impl Entity for CopperGolem {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:copper_golem")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_long("next_weather_age", self.next_weathering_tick);
        out.put_string(
            "weather_state",
            weather_state_name(self.get_weather_state()),
        );
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.next_weathering_tick = input.get_long_or("next_weather_age", UNSET_WEATHERING_TICK);
        let state = input
            .get_string("weather_state")
            .and_then(weather_state_by_name)
            .unwrap_or(WeatherState::Unaffected);
        self.set_weather_state(state);
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        copper_golem_ai::update_activity(self);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        let game_time = level.game_time();
        self.update_weathering(level, game_time);
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.is_empty() {
            let equipped_item = self.get_item_by_slot(EquipmentSlot::MainHand);
            if !equipped_item.is_empty() {
                behavior_utils::throw_item(level, &*self, equipped_item, player.position());
                self.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
                return InteractionResult::SUCCESS;
            }
        }
        if item_stack.is(Item::SHEARS) && self.ready_for_shearing() {
            self.shear(level, SoundSource::Players, &item_stack);
            level.game_event_from_entity_ref(&*player, GameEvent::SHEAR, self.position());
            interact::with_hand_stack(player, hand, |s, p| {
                hurt_and_break(s, 1, level, p, hand.as_equipment_slot());
            });
            return InteractionResult::SUCCESS;
        }
        let pos = self.block_position();
        if item_stack.is(Item::HONEYCOMB) && self.next_weathering_tick != IGNORE_WEATHERING_TICK {
            level.level_event_impl(Some(self.id()), 3003, pos, 0);
            level.play_sound(
                None,
                pos,
                SoundEvent::ITEM_HONEYCOMB_WAX_ON,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            self.next_weathering_tick = IGNORE_WEATHERING_TICK;
            interact::use_player_item(level, player, hand);
            return InteractionResult::SUCCESS_SERVER;
        }
        if is_axe(&item_stack) && self.next_weathering_tick == IGNORE_WEATHERING_TICK {
            self.play_sound_to_all(level, SoundEvent::ITEM_AXE_SCRAPE);
            level.level_event_impl(Some(self.id()), 3004, pos, 0);
            self.next_weathering_tick = UNSET_WEATHERING_TICK;
            interact::with_hand_stack(player, hand, |s, p| {
                hurt_and_break(s, 1, level, p, hand.as_equipment_slot());
            });
            return InteractionResult::SUCCESS_SERVER;
        }
        if is_axe(&item_stack) {
            let weather_state = self.get_weather_state();
            if weather_state != WeatherState::Unaffected {
                self.play_sound_to_all(level, SoundEvent::ITEM_AXE_SCRAPE);
                level.level_event_impl(Some(self.id()), 3005, pos, 0);
                self.next_weathering_tick = UNSET_WEATHERING_TICK;
                self.base_mut().entity_data.set_force(
                    DATA_WEATHER_STATE,
                    weather_state_to_data(weather_state.previous()),
                    true,
                );
                interact::with_hand_stack(player, hand, |s, p| {
                    hurt_and_break(s, 1, level, p, hand.as_equipment_slot());
                });
                return InteractionResult::SUCCESS_SERVER;
            }
        }
        mob::interact::mob_interact(self, level, player, hand)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if let Some(server_level) = level.as_server_level_mut() {
            self.play_spawn_sound(server_level);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn copper_golem_play_spawn_sound(&mut self, level: &mut ServerLevel) {
        self.play_spawn_sound(level);
    }

    fn copper_golem_set_weather_state(&mut self, state: WeatherState) {
        self.set_weather_state(state);
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(oxidation_level(self.get_weather_state()).hurt)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(oxidation_level(self.get_weather_state()).death)
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        let sound = oxidation_level(self.get_weather_state()).step;
        self.play_sound(level, sound, 1.0, 1.0);
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        false
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(0.0, f64::from(0.75f32 * self.base().eye_height), 0.0)
    }

    fn container_user_has_open(&self, level: &ServerLevel, pos: BlockPos) -> Option<bool> {
        Some(self.has_container_open(level, pos))
    }

    fn container_user_interaction_range(&self) -> Option<f64> {
        Some(3.0)
    }

    fn is_shearable(&self) -> bool {
        true
    }

    fn ready_for_shearing(&self) -> bool {
        mc_data::tags::get::<Item>("shearable_from_copper_golem")
            .is_some_and(|tag| tag.contains(self.get_item_by_slot(EQUIPMENT_SLOT_ANTENNA).item()))
    }

    fn shear(&mut self, level: &mut ServerLevel, sound_source: SoundSource, _tool: &ItemStack) {
        level.play_sound_from_entity_at(
            None,
            self.id(),
            self.position(),
            SoundEvent::ENTITY_COPPER_GOLEM_SHEAR,
            sound_source,
            1.0,
            1.0,
        );
        let item_stack = self.get_item_by_slot(EQUIPMENT_SLOT_ANTENNA);
        self.set_item_slot(level, EQUIPMENT_SLOT_ANTENNA, ItemStack::empty());
        mc_game_core::world::entity::item::spawn_at_location(
            level,
            self,
            item_stack,
            Vec3::new(0.0, 1.5, 0.0),
        );
    }

    fn drop_equipment(&mut self, level: &mut ServerLevel) {
        self.drop_preserved_equipment(level);
    }

    fn actually_hurt(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        mc_game_core::world::entity::living_entity::damage::actually_hurt(
            self, level, source, damage,
        );
        self.set_state(CopperGolemState::Idle);
    }

    fn thunder_hit_by(&mut self, level: &mut ServerLevel, bolt: &LightningBolt) {
        self.thunder_hit(level);
        let uuid = bolt.uuid();
        if self.last_lightning_bolt_uuid != Some(uuid) {
            self.last_lightning_bolt_uuid = Some(uuid);
            let weather_state = self.get_weather_state();
            if weather_state != WeatherState::Unaffected {
                self.next_weathering_tick = UNSET_WEATHERING_TICK;
                self.base_mut().entity_data.set_force(
                    DATA_WEATHER_STATE,
                    weather_state_to_data(weather_state.previous()),
                    true,
                );
            }
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static COPPER_GOLEM: EntityRegistration = EntityRegistration {
    entity_type: EntityType::COPPER_GOLEM,
    create: |ty, id, _registries| Box::new(CopperGolem::new(ty, id)),
};

/// The scenario dump fields of a copper golem: `weather state wage chest`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static COPPER_GOLEM_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 25,
    applies: |e| e.as_any().is::<CopperGolem>(),
    fields: |e, _level, _ctx| {
        let golem = e.as_any().downcast_ref::<CopperGolem>().expect("applies");
        vec![
            (
                "weather".into(),
                weather_state_name(golem.get_weather_state()).into(),
            ),
            ("state".into(), state_name(golem.get_state()).into()),
            ("wage".into(), golem.next_weathering_tick.to_string()),
            (
                "chest".into(),
                golem.opened_chest_pos.map_or_else(
                    || "-".to_string(),
                    |p| format!("{},{},{}", p.x(), p.y(), p.z()),
                ),
            ),
        ]
    },
};
