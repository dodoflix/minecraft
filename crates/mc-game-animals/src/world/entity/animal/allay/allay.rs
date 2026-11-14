//! `net.minecraft.world.entity.animal.allay.Allay`: a flying item fetcher. It picks up what its hand holds, brings it to the
//! liked player or note block, dances to a jukebox and duplicates on an amethyst shard while dancing.
//!
//! Not implemented: the client-only holding, dancing and spinning animation counters and the heart particle of entity event
//! 18 (the server never reads them).

use super::allay_ai;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::SimpleContainer;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::FlyingMoveControl;
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ITEM_PICKUP_COOLDOWN_TICKS, LIKED_NOTEBLOCK_COOLDOWN_TICKS, LIKED_NOTEBLOCK_POSITION,
    LIKED_PLAYER,
};
use mc_game_core::world::entity::ai::navigation::{PathNavigation, StandardPathNavigation};
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    HURT_BY, NEAREST_ITEMS, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::npc::inventory_carrier::{self, InventoryCarrier};
use mc_game_core::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::gameevent::vibrations::{self, VibrationData, VibrationSystem};
use mc_game_core::world::level::gameevent::{
    DeliveryMode, DynamicGameEventListener, DynamicListenerAction, GameEventContext,
    GameEventListener, ListenerOwner, PositionSource, SourceEntityInfo,
};
use mc_nbt::CompoundTag;
use mc_protocol::components::{INTERACT_ANIMATION, POTION_CONTENTS};
use mc_protocol::packets::play::SoundSource;

define_accessor!(pub DATA_DANCING: Boolean = Allay);
define_accessor!(pub DATA_CAN_DUPLICATE: Boolean = Allay);

/// `DUPLICATION_COOLDOWN_TICKS`.
const DUPLICATION_COOLDOWN_TICKS: i64 = 6000;
/// `VibrationUser.VIBRATION_EVENT_LISTENER_RANGE`.
const VIBRATION_EVENT_LISTENER_RANGE: i32 = 16;
/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_memories(
    &[
        LIKED_PLAYER.id(),
        LIKED_NOTEBLOCK_POSITION.id(),
        LIKED_NOTEBLOCK_COOLDOWN_TICKS.id(),
    ],
    &[
        &NEAREST_LIVING_ENTITIES,
        &NEAREST_PLAYERS,
        &HURT_BY,
        &NEAREST_ITEMS,
    ],
    allay_ai::get_activities,
);

/// `Allay`.
pub struct Allay {
    pub mob: MobData,
    vibration_data: VibrationData,
    dynamic_vibration_listener: DynamicGameEventListener,
    dynamic_jukebox_listener: DynamicGameEventListener,
    /// `vibrationUser.getPositionSource()`: the entity position source at the eye height of the construction.
    position_source_y_offset: f32,
    inventory: SimpleContainer,
    /// `jukeboxPos`.
    jukebox_pos: Option<BlockPos>,
    /// `duplicationCooldown`.
    duplication_cooldown: i64,
}

impl Allay {
    /// `new Allay(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            b.define(DATA_DANCING, false);
            b.define(DATA_CAN_DUPLICATE, true);
        });
        let eye_height = mob.living.entity.eye_height;
        let mut allay = Allay {
            mob,
            vibration_data: VibrationData::default(),
            dynamic_vibration_listener: DynamicGameEventListener::new(),
            dynamic_jukebox_listener: DynamicGameEventListener::new(),
            position_source_y_offset: eye_height,
            inventory: SimpleContainer::new(1),
            jukebox_pos: None,
            duplication_cooldown: 0,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = allay.make_brain(&BrainPacked::EMPTY);
        allay.mob.living.brain = brain;
        let mut navigation = StandardPathNavigation::flying(&allay);
        navigation.set_can_open_doors(false);
        navigation.set_can_float(true);
        navigation.set_required_path_length(&allay, 48.0);
        allay.mob.set_navigation(Box::new(navigation));
        allay.mob.move_control = Some(Box::new(FlyingMoveControl::new(20, true)));
        allay
    }

    /// `isLikedPlayer(other)`.
    fn is_liked_player(&self, other: &dyn Entity) -> bool {
        other.is_player()
            && self
                .brain()
                .get_memory(&LIKED_PLAYER)
                .is_some_and(|liked| *liked == other.uuid())
    }

    /// `hasItemInHand()`.
    pub fn has_item_in_hand(&self) -> bool {
        !self.get_item_by_slot(EquipmentSlot::MainHand).is_empty()
    }

    /// `isOnPickupCooldown()`.
    fn is_on_pickup_cooldown(&self) -> bool {
        self.brain().has_memory_value(ITEM_PICKUP_COOLDOWN_TICKS)
    }

    /// `isDancing()`.
    pub fn is_dancing(&self) -> bool {
        self.base().entity_data.get(DATA_DANCING)
    }

    /// `setDancing(isDancing)`.
    pub fn set_dancing(&mut self, is_dancing: bool) {
        if self.is_effective_ai() && (!is_dancing || !self.is_panicking()) {
            self.base_mut().entity_data.set(DATA_DANCING, is_dancing);
        }
    }

    /// `setJukeboxPlaying(jukebox, isPlaying)`.
    pub fn set_jukebox_playing(&mut self, jukebox: BlockPos, is_playing: bool) {
        if is_playing {
            if !self.is_dancing() {
                self.jukebox_pos = Some(jukebox);
                self.set_dancing(true);
            }
        } else if self.jukebox_pos == Some(jukebox) || self.jukebox_pos.is_none() {
            self.jukebox_pos = None;
            self.set_dancing(false);
        }
    }

    /// `shouldStopDancing()`.
    fn should_stop_dancing(&self, level: &ServerLevel) -> bool {
        match self.jukebox_pos {
            None => true,
            Some(pos) => {
                !pos.closer_to_center_than(&self.position(), jukebox_listener_radius())
                    || !level.get_block_state(pos).is_block(mc_data::Block::JUKEBOX)
            }
        }
    }

    /// `allayConsidersItemEqual(item1, item2)`.
    fn allay_considers_item_equal(item1: &ItemStack, item2: &ItemStack) -> bool {
        ItemStack::is_same_item(item1, item2) && !Self::has_non_matching_potion(item1, item2)
    }

    /// `hasNonMatchingPotion(itemInHand, pickupItem)`.
    fn has_non_matching_potion(item_in_hand: &ItemStack, pickup_item: &ItemStack) -> bool {
        item_in_hand.get(&POTION_CONTENTS) != pickup_item.get(&POTION_CONTENTS)
    }

    /// `updateDuplicationCooldown()`.
    fn update_duplication_cooldown(&mut self) {
        if self.duplication_cooldown > 0 {
            self.set_duplication_cooldown(self.duplication_cooldown - 1);
        }
    }

    /// `setDuplicationCooldown(duplicationCooldown)`.
    fn set_duplication_cooldown(&mut self, duplication_cooldown: i64) {
        self.duplication_cooldown = duplication_cooldown;
        self.base_mut()
            .entity_data
            .set(DATA_CAN_DUPLICATE, duplication_cooldown == 0);
    }

    /// `resetDuplicationCooldown()`.
    fn reset_duplication_cooldown(&mut self) {
        self.set_duplication_cooldown(DUPLICATION_COOLDOWN_TICKS);
    }

    /// `canDuplicate()`.
    fn can_duplicate(&self) -> bool {
        self.base().entity_data.get(DATA_CAN_DUPLICATE)
    }

    /// `duplicateAllay()`.
    fn duplicate_allay(&mut self, level: &mut ServerLevel) {
        let request = EntitySpawnRequest {
            reason: EntitySpawnReason::Breeding,
            ignore_checks: false,
        };
        let Some(mut allay) = factory::create(EntityType::ALLAY, level, request) else {
            return;
        };
        let position = self.position();
        allay.snap_to_pos(position.x, position.y, position.z);
        allay.set_persistence_required();
        if let Some(copy) = allay.as_any_mut().downcast_mut::<Allay>() {
            copy.reset_duplication_cooldown();
        }
        self.reset_duplication_cooldown();
        level.add_fresh_entity(allay);
    }

    /// `level.playSound(player, this, sound, SoundSource.NEUTRAL, 2.0F, 1.0F)`.
    fn play_sound_to_others(
        &self,
        level: &mut ServerLevel,
        player: &ServerPlayer,
        sound: SoundEvent,
    ) {
        level.play_sound_from_entity_at(
            Some(player.id()),
            self.id(),
            self.position(),
            sound,
            SoundSource::Neutral,
            2.0,
            1.0,
        );
    }

    fn vibration_position_source(&self) -> PositionSource {
        PositionSource::Entity {
            uuid: self.uuid(),
            y_offset: self.position_source_y_offset,
        }
    }

    /// The position of the vibration user's position source (`source.getPosition(level)`).
    fn listener_position(&self) -> Vec3 {
        self.position()
            .add_xyz(0.0, f64::from(self.position_source_y_offset), 0.0)
    }
}

/// `GameEvent.JUKEBOX_PLAY.value().notificationRadius()`.
fn jukebox_listener_radius() -> f64 {
    f64::from(GameEvent::JUKEBOX_PLAY.data().notification_radius)
}

impl InventoryCarrier for Allay {
    fn get_inventory(&self) -> &SimpleContainer {
        &self.inventory
    }
    fn get_inventory_mut(&mut self) -> &mut SimpleContainer {
        &mut self.inventory
    }
}

impl Entity for Allay {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:allay")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        let speed = self.get_speed();
        mc_game_core::world::entity::living_entity::travel::travel_flying(
            self, level, input, speed,
        );
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let liked_attacker = source
            .get_entity()
            .and_then(|id| level.get_entity(id))
            .is_some_and(|attacker| self.is_liked_player(attacker));
        !liked_attacker
            && mc_game_core::world::entity::living_entity::damage::hurt_server(
                self, level, source, damage,
            )
    }

    fn considers_entity_as_ally(&self, level: &ServerLevel, other: &dyn Entity) -> bool {
        self.is_liked_player(other)
            || mob::ext::considers_entity_as_ally_default(level, self.as_dyn(), other)
    }

    fn play_step_sound(
        &mut self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
    }

    fn check_fall_damage(
        &mut self,
        _level: &mut ServerLevel,
        _ya: f64,
        _on_ground: bool,
        _on_state: mc_data::block::BlockState,
        _pos: BlockPos,
    ) {
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.has_item_in_hand() {
            SoundEvent::ENTITY_ALLAY_AMBIENT_WITH_ITEM
        } else {
            SoundEvent::ENTITY_ALLAY_AMBIENT_WITHOUT_ITEM
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ALLAY_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ALLAY_DEATH)
    }

    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        allay_ai::update_activity(self);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if self.is_alive() && self.base().tick_count % 10 == 0 {
            self.heal(level, 1.0);
        }
        if self.is_dancing() && self.should_stop_dancing(level) && self.base().tick_count % 20 == 0
        {
            self.set_dancing(false);
            self.jukebox_pos = None;
        }
        self.update_duplication_cooldown();
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        vibrations::tick(level, self);
        if self.is_panicking() {
            self.set_dancing(false);
        }
    }

    fn can_pick_up_loot(&self) -> bool {
        !self.is_on_pickup_cooldown() && self.has_item_in_hand()
    }

    fn can_dispenser_equip_into_slot(&self, _slot: EquipmentSlot) -> bool {
        false
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let interaction_item = player.get_item_in_hand(hand);
        let item_in_hand = self.get_item_by_slot(EquipmentSlot::MainHand);
        let infinite = player.player.abilities.instabuild;
        let consume = |player: &mut ServerPlayer| {
            mob::interact::with_hand_stack(player, hand, |stack, _| stack.consume(1, infinite));
        };
        if self.is_dancing()
            && mc_data::tags::get::<Item>("duplicates_allays")
                .is_some_and(|t| t.contains(interaction_item.item()))
            && self.can_duplicate()
        {
            self.duplicate_allay(level);
            level.broadcast_entity_event(self.id(), 18);
            self.play_sound_to_others(level, player, SoundEvent::BLOCK_AMETHYST_BLOCK_CHIME);
            consume(player);
            InteractionResult::SUCCESS
        } else if item_in_hand.is_empty() && !interaction_item.is_empty() {
            let item_to_give = interaction_item.copy_with_count(1);
            self.set_item_slot(level, EquipmentSlot::MainHand, item_to_give);
            consume(player);
            self.play_sound_to_others(level, player, SoundEvent::ENTITY_ALLAY_ITEM_GIVEN);
            let uuid = player.uuid();
            self.brain_mut().set_memory(&LIKED_PLAYER, uuid);
            InteractionResult::SUCCESS
        } else if !item_in_hand.is_empty()
            && hand == InteractionHand::MainHand
            && interaction_item.is_empty()
        {
            self.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
            self.play_sound_to_others(level, player, SoundEvent::ENTITY_ALLAY_ITEM_TAKEN);
            let animation = item_in_hand
                .get(&INTERACT_ANIMATION)
                .copied()
                .unwrap_or(SWING_ANIMATION_DEFAULT);
            mob::combat::swing(self, level, InteractionHand::MainHand, animation);
            for stack in self.inventory.remove_all_items() {
                behavior_utils::throw_item(level, &*self, stack, self.position());
            }
            self.brain_mut().erase_memory(&LIKED_PLAYER);
            let mut to_add = item_in_hand;
            player
                .player
                .inventory
                .add(&mut to_add, player.player.abilities.instabuild);
            InteractionResult::SUCCESS
        } else {
            mob::interact::mob_interact(self, level, player, hand)
        }
    }

    fn get_pickup_reach(&self) -> (i32, i32, i32) {
        (1, 1, 1)
    }

    fn wants_to_pick_up(&self, level: &ServerLevel, stack: &ItemStack) -> bool {
        let item_in_hand = self.get_item_by_slot(EquipmentSlot::MainHand);
        !item_in_hand.is_empty()
            && level.mob_griefing()
            && self.inventory.can_add_item(stack)
            && Self::allay_considers_item_equal(&item_in_hand, stack)
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item: EntityId) {
        inventory_carrier::pick_up_item(level, self, item);
    }

    fn is_flapping(&self) -> bool {
        !self.base().on_ground()
    }

    fn update_dynamic_game_event_listener(
        &mut self,
        level: &mut ServerLevel,
        action: DynamicListenerAction,
    ) {
        let source = self.vibration_position_source();
        let position = self.listener_position();
        let id = self.id();
        self.dynamic_vibration_listener.apply(
            action,
            level,
            ListenerOwner::Entity(id),
            source,
            VIBRATION_EVENT_LISTENER_RANGE,
            DeliveryMode::Unspecified,
            position,
        );
        self.dynamic_jukebox_listener.apply(
            action,
            level,
            ListenerOwner::EntitySecondary(id),
            source,
            jukebox_listener_radius() as i32,
            DeliveryMode::Unspecified,
            position,
        );
    }

    fn game_event_listener(&mut self) -> Option<&mut dyn GameEventListener> {
        Some(self)
    }

    /// `Allay.JukeboxListener.handleGameEvent`.
    fn handle_secondary_game_event(
        &mut self,
        _level: &mut ServerLevel,
        event: GameEvent,
        _context: &GameEventContext,
        source_position: Vec3,
    ) -> bool {
        let pos = BlockPos::containing_position(&source_position);
        if event == GameEvent::JUKEBOX_PLAY {
            self.set_jukebox_playing(pos, true);
            true
        } else if event == GameEvent::JUKEBOX_STOP_PLAY {
            self.set_jukebox_playing(pos, false);
            true
        } else {
            false
        }
    }

    fn equipment_has_changed(&self, previous: &ItemStack, current: &ItemStack) -> bool {
        !Self::allay_considers_item_equal(previous, current)
    }

    fn drop_equipment(&mut self, level: &mut ServerLevel) {
        for stack in self.inventory.remove_all_items() {
            mc_game_core::world::entity::item::spawn_at_location(level, self, stack, Vec3::ZERO);
        }
        let item = self.get_item_by_slot(EquipmentSlot::MainHand);
        if !item.is_empty()
            && !mc_game_core::world::item::enchantment_helper::has(
                &level.registries,
                &item,
                mc_game_core::world::item::enchantment::enchantment_effect_components::EffectComponent::prevent_equipment_drop,
            )
        {
            mc_game_core::world::entity::item::spawn_at_location(level, self, item, Vec3::ZERO);
            self.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
        }
    }

    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        inventory_carrier::write_inventory_to_tag(&self.inventory, out);
        if let Ok(listener) =
            mc_dfu::encode_nbt(VibrationData::codec(), &self.vibration_data).into_result()
        {
            out.put("listener", listener);
        }
        out.put_long("DuplicationCooldown", self.duplication_cooldown);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        inventory_carrier::read_inventory_from_tag(&mut self.inventory, input);
        self.vibration_data = mc_game_core::world::block_entity::read_codec(
            input,
            "listener",
            VibrationData::codec(),
        )
        .unwrap_or_default();
        self.set_duplication_cooldown(i64::from(input.get_int_or("DuplicationCooldown", 0)));
    }

    fn should_stay_close_to_leash_holder(&self) -> bool {
        false
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(
            0.0,
            f64::from(self.base().eye_height) * 0.6,
            f64::from(self.base().get_bb_width()) * 0.1,
        )
    }
}

impl GameEventListener for Allay {
    fn get_listener_source(&self) -> PositionSource {
        self.vibration_position_source()
    }

    fn get_listener_radius(&self) -> i32 {
        VIBRATION_EVENT_LISTENER_RANGE
    }

    fn handle_game_event(
        &mut self,
        level: &mut ServerLevel,
        event: GameEvent,
        context: &GameEventContext,
        source_position: Vec3,
    ) -> bool {
        vibrations::handle_game_event(level, self, event, context, source_position)
    }
}

impl VibrationSystem for Allay {
    fn with_vibration_data<R>(
        &mut self,
        _level: &mut ServerLevel,
        f: impl FnOnce(&mut VibrationData) -> R,
    ) -> R {
        f(&mut self.vibration_data)
    }

    fn get_listener_radius(&self) -> i32 {
        VIBRATION_EVENT_LISTENER_RANGE
    }

    fn get_position_source(&self) -> PositionSource {
        self.vibration_position_source()
    }

    fn owner_entity(&self) -> Option<(EntityId, Vec3)> {
        Some((self.id(), self.listener_position()))
    }

    fn get_listenable_events(&self) -> &'static str {
        "allay_can_listen"
    }

    fn can_receive_vibration(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _event: GameEvent,
        _context: &GameEventContext,
    ) -> bool {
        if self.mob.is_no_ai() {
            return false;
        }
        match self.brain().get_memory(&LIKED_NOTEBLOCK_POSITION) {
            None => true,
            Some(global_pos) => {
                allay_ai::is_close_enough(level, global_pos, self.block_position())
                    && global_pos.pos == pos
            }
        }
    }

    fn on_receive_vibration(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        event: GameEvent,
        _source_entity: Option<SourceEntityInfo>,
        _projectile_owner: Option<SourceEntityInfo>,
        _receiving_distance: f32,
    ) {
        if event == GameEvent::NOTE_BLOCK_PLAY {
            allay_ai::hear_noteblock(level, self, pos);
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ALLAY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ALLAY,
    create: |ty, id, _registries| Box::new(Allay::new(ty, id)),
};

/// The scenario dump fields of an allay: `dance candup dupcd jukebox`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static ALLAY_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 25,
    applies: |e| e.as_any().is::<Allay>(),
    fields: |e, _level, _ctx| {
        let allay = e.as_any().downcast_ref::<Allay>().expect("applies");
        vec![
            ("dance".into(), u8::from(allay.is_dancing()).to_string()),
            ("candup".into(), u8::from(allay.can_duplicate()).to_string()),
            ("dupcd".into(), allay.duplication_cooldown.to_string()),
            (
                "jukebox".into(),
                allay.jukebox_pos.map_or_else(
                    || "-".to_string(),
                    |p| format!("{},{},{}", p.x(), p.y(), p.z()),
                ),
            ),
        ]
    },
};
