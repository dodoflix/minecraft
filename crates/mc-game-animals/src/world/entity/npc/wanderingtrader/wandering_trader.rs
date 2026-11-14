//! `WanderingTrader`: the llama-leading merchant that despawns after `despawnDelay` ticks, drinks an invisibility potion at night
//! and walks to the spot its spawner picked (`net.minecraft.world.entity.npc.wanderingtrader.WanderingTrader`).

use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, Potion, SoundEvent};
use mc_game_ai::world::entity::ai::goal::{
    AvoidEntityGoal, InteractGoal, LookAtTradingPlayerGoal, MoveTowardsRestrictionGoal,
    TradeWithPlayerGoal, UseItemGoal,
};
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::target::{class_of_type, class_of_types};
use mc_game_core::world::entity::ai::goal::{
    Flag, FloatGoal, Goal, GoalFlags, LookAtPlayerGoal, LookAtType, PanicGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::experience_orb::ExperienceOrb;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind, navigation};
use mc_game_core::world::entity::npc::inventory_carrier::{self, InventoryCarrier};
use mc_game_core::world::entity::npc::villager::abstract_villager::{self, AbstractVillagerData};
use mc_game_core::world::entity::player::ServerPlayer;
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::create_potion_item_stack;
use mc_game_core::world::item::trading::{self, Merchant, MerchantOffers, trade_sets};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use std::sync::Arc;

/// `WanderingTrader`.
pub struct WanderingTrader {
    pub mob: MobData,
    pub av: AbstractVillagerData,
    /// `wanderTarget`.
    wander_target: Option<BlockPos>,
    /// `despawnDelay`.
    despawn_delay: i32,
}

/// `new WanderingTrader(type, level)` (`registerGoals` and `createNavigation` run as in `Mob.<init>`).
impl WanderingTrader {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::AgeableMob, |b| {
            b.define(abstract_villager::DATA_UNHAPPY_COUNTER, 0);
        });
        let mut trader = WanderingTrader {
            mob,
            av: AbstractVillagerData::new(),
            wander_target: None,
            despawn_delay: 0,
        };
        abstract_villager::init(&mut trader.mob);
        let navigation = StandardPathNavigation::ground(&trader);
        trader.mob.set_navigation(Box::new(navigation));
        trader.register_goals();
        trader
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_at_trading = LookAtTradingPlayerGoal::new(self);
        let interact = InteractGoal::with_probability(self, LookAtType::Player, 3.0, 1.0);
        let look_at_mob = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())),
            8.0,
        );
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(
            0,
            Box::new(UseItemGoal::new(
                create_potion_item_stack(Item::POTION, Potion::INVISIBILITY),
                Some(SoundEvent::ENTITY_WANDERING_TRADER_DISAPPEARED),
                Arc::new(|e, level| level.is_dark_outside() && !e.is_invisible()),
            )),
        );
        goals.add_goal(
            0,
            Box::new(UseItemGoal::new(
                ItemStack::new(Item::MILK_BUCKET),
                Some(SoundEvent::ENTITY_WANDERING_TRADER_REAPPEARED),
                Arc::new(|e, level| level.is_bright_outside() && e.is_invisible()),
            )),
        );
        goals.add_goal(1, Box::new(TradeWithPlayerGoal::new()));
        let avoid = |class, distance| {
            Box::new(AvoidEntityGoal::new(class, distance, 0.5, 0.5)) as Box<dyn Goal>
        };
        goals.add_goal(
            1,
            avoid(Arc::new(|e: &dyn Entity| e.as_zombie().is_some()), 8.0),
        );
        goals.add_goal(1, avoid(class_of_type(EntityType::EVOKER), 12.0));
        goals.add_goal(1, avoid(class_of_type(EntityType::VINDICATOR), 8.0));
        goals.add_goal(1, avoid(class_of_type(EntityType::VEX), 8.0));
        goals.add_goal(1, avoid(class_of_type(EntityType::PILLAGER), 15.0));
        goals.add_goal(1, avoid(class_of_type(EntityType::ILLUSIONER), 12.0));
        goals.add_goal(1, avoid(class_of_types(&[EntityType::ZOGLIN]), 10.0));
        goals.add_goal(1, Box::new(PanicGoal::new(0.5)));
        goals.add_goal(1, Box::new(look_at_trading));
        goals.add_goal(
            2,
            Box::new(WanderToPositionGoal {
                stop_distance: 2.0,
                speed_modifier: 0.35,
            }),
        );
        goals.add_goal(4, Box::new(MoveTowardsRestrictionGoal::new(0.35)));
        goals.add_goal(8, Box::new(WaterAvoidingRandomStrollGoal::new(0.35)));
        goals.add_goal(9, Box::new(interact));
        goals.add_goal(10, Box::new(look_at_mob));
    }

    /// `isTrading()`.
    pub fn is_trading(&self) -> bool {
        self.av.trading_player.is_some()
    }

    /// `setDespawnDelay(despawnDelay)`.
    pub fn set_despawn_delay(&mut self, despawn_delay: i32) {
        self.despawn_delay = despawn_delay;
    }

    /// `setWanderTarget(pos)`.
    pub fn set_wander_target(&mut self, pos: Option<BlockPos>) {
        self.wander_target = pos;
    }

    /// `maybeDespawn()`.
    fn maybe_despawn(&mut self, level: &mut ServerLevel) {
        if self.despawn_delay > 0 && !self.is_trading() {
            self.despawn_delay -= 1;
            if self.despawn_delay == 0 {
                self.discard_in_level(level);
            }
        }
    }

    /// `updateTrades(level)`.
    fn update_trades(&mut self, level: &mut ServerLevel) {
        let mut offers = self.av.offers.take().unwrap_or_default();
        for set in [
            trade_sets::WANDERING_TRADER_BUYING,
            trade_sets::WANDERING_TRADER_UNCOMMON,
            trade_sets::WANDERING_TRADER_COMMON,
        ] {
            abstract_villager::add_offers_from_trade_set(
                level,
                &*self,
                &mut offers,
                &trade_sets::trade_set_key(set),
            );
        }
        self.av.offers = Some(offers);
    }

    /// `rewardTradeXp(offer)`.
    fn reward_trade_xp(&mut self, level: &mut ServerLevel, offer_index: usize) {
        if self
            .av
            .offers
            .as_ref()
            .is_some_and(|o| o[offer_index].should_reward_exp())
        {
            let pop_xp = 3 + self.base_mut().random.next_int_bounded(4);
            let pos = self.position();
            level.add_fresh_entity(Box::new(ExperienceOrb::at(
                level,
                pos.x,
                pos.y + 0.5,
                pos.z,
                pop_xp,
            )));
        }
    }
}

impl InventoryCarrier for WanderingTrader {
    fn get_inventory(&self) -> &mc_game_core::world::container::SimpleContainer {
        &self.av.inventory
    }
    fn get_inventory_mut(&mut self) -> &mut mc_game_core::world::container::SimpleContainer {
        &mut self.av.inventory
    }
}

impl Merchant for WanderingTrader {
    fn set_trading_player(&mut self, player: Option<EntityId>) {
        self.av.trading_player = player;
    }

    fn get_trading_player(&self) -> Option<EntityId> {
        self.av.trading_player
    }

    fn get_offers(&mut self, level: &mut ServerLevel) -> &mut MerchantOffers {
        if self.av.offers.is_none() {
            self.av.offers = Some(MerchantOffers::new());
            self.update_trades(level);
        }
        self.av.offers.as_mut().expect("offers")
    }

    fn override_offers(&mut self, _offers: MerchantOffers) {}

    fn notify_trade(&mut self, level: &mut ServerLevel, offer_index: usize) {
        if let Some(offer) = self
            .av
            .offers
            .as_mut()
            .and_then(|o| o.0.get_mut(offer_index))
        {
            offer.increase_uses();
        }
        abstract_villager::reset_ambient_sound_time(self);
        self.reward_trade_xp(level, offer_index);
        // the `CriteriaTriggers.TRADE` trigger of `AbstractVillager.notifyTrade` runs in `trading::notify_trade` (it needs the player)
    }

    fn notify_trade_updated(&mut self, level: &mut ServerLevel, stack: &ItemStack) {
        let interval = self.get_ambient_sound_interval();
        if self.mob.ambient_sound_time > -interval + 20 {
            self.mob.ambient_sound_time = -interval;
            let sound = if stack.is_empty() {
                SoundEvent::ENTITY_WANDERING_TRADER_NO
            } else {
                SoundEvent::ENTITY_WANDERING_TRADER_YES
            };
            mc_game_core::world::entity::living_entity::damage::make_sound(self, level, sound);
        }
    }

    fn get_villager_xp(&self) -> i32 {
        0
    }

    fn override_xp(&mut self, _xp: i32) {}

    fn show_progress_bar(&self) -> bool {
        false
    }

    fn get_notify_trade_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_WANDERING_TRADER_YES
    }
}

impl Entity for WanderingTrader {
    fn get_slot(&self, slot: i32) -> Option<ItemStack> {
        inventory_carrier::get_slot(self, slot)
    }

    fn set_slot(&mut self, level: &mut ServerLevel, slot: i32, stack: ItemStack) {
        inventory_carrier::set_slot(self, level, slot, stack);
    }

    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:wandering_trader")
    }

    fn as_merchant(&self) -> Option<&dyn Merchant> {
        Some(self)
    }

    fn as_merchant_mut(&mut self) -> Option<&mut dyn Merchant> {
        Some(self)
    }

    fn wandering_trader_despawn_delay(&self) -> Option<i32> {
        Some(self.despawn_delay)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        // `AbstractVillager.finalizeSpawn`: a default group data that never spawns babies
        let group_data = group_data
            .or_else(|| Some(Box::new(mob::spawn::AgeableMobGroupData::with_baby(false))));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.item() == Item::VILLAGER_SPAWN_EGG
            || !self.is_alive()
            || self.is_trading()
            || self.is_baby()
        {
            return mob::interact::mob_interact(self, level, player, hand);
        }
        if hand == InteractionHand::MainHand {
            player.award_stat_once(level, mc_game_core::stats::stats::Stats::TALKED_TO_VILLAGER);
        }
        if self.get_offers(level).is_empty() {
            return InteractionResult::CONSUME;
        }
        self.av.trading_player = Some(player.base_id());
        let title =
            mc_game_core::world::damagesource::display_name::entity_display_name(level, self);
        let me = self.id();
        trading::open_trading_screen(player, level, me, self, title, 1);
        InteractionResult::SUCCESS_SERVER
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        abstract_villager::add_additional_save_data(&self.av, out);
        out.put_int("DespawnDelay", self.despawn_delay);
        if let Some(pos) = self.wander_target {
            out.put_int_array("wander_target", vec![pos.x(), pos.y(), pos.z()]);
        }
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        abstract_villager::read_additional_save_data(&mut self.av, level, input);
        self.despawn_delay = input.get_int_or("DespawnDelay", 0);
        self.wander_target =
            mc_game_core::world::entity::living::read_block_pos(input.get("wander_target"));
        let age = mob::ageable::get_age(self);
        mob::ageable::set_age(self, age.max(0));
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    fn can_be_leashed(&self) -> bool {
        false
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_trading() {
            SoundEvent::ENTITY_WANDERING_TRADER_TRADE
        } else {
            SoundEvent::ENTITY_WANDERING_TRADER_AMBIENT
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WANDERING_TRADER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WANDERING_TRADER_DEATH)
    }

    fn override_consume_sound(&self, stack: &ItemStack) -> Option<SoundEvent> {
        Some(if stack.item() == Item::MILK_BUCKET {
            SoundEvent::ENTITY_WANDERING_TRADER_DRINK_MILK
        } else {
            SoundEvent::ENTITY_WANDERING_TRADER_DRINK_POTION
        })
    }

    fn die(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        mc_game_core::world::entity::living_entity::death::die(self, level, source);
        self.av.trading_player = None;
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        self.maybe_despawn(level);
    }
}

/// `WanderingTrader.WanderToPositionGoal`.
struct WanderToPositionGoal {
    stop_distance: f64,
    speed_modifier: f64,
}

fn trader(mob: &mut dyn Entity) -> &mut WanderingTrader {
    mob.as_any_mut()
        .downcast_mut::<WanderingTrader>()
        .expect("wandering trader")
}

/// `!pos.closerToCenterThan(trader.position(), distance)`.
fn is_too_far_away(mob: &dyn Entity, pos: BlockPos, distance: f64) -> bool {
    !pos.closer_to_center_than(&mob.position(), distance)
}

impl Goal for WanderToPositionGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        let wander_position = trader(mob).wander_target;
        wander_position.is_some_and(|p| is_too_far_away(mob, p, self.stop_distance))
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        trader(mob).wander_target = None;
        navigation::stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(wander_position) = trader(mob).wander_target else {
            return;
        };
        if !navigation::is_done(mob, level) {
            return;
        }
        if is_too_far_away(mob, wander_position, 10.0) {
            let p = mob.position();
            let dir = Vec3::new(
                f64::from(wander_position.x()) - p.x,
                f64::from(wander_position.y()) - p.y,
                f64::from(wander_position.z()) - p.z,
            )
            .normalize();
            let target = dir.scale(10.0).add(p);
            navigation::move_to(
                mob,
                level,
                target.x,
                target.y,
                target.z,
                self.speed_modifier,
            );
        } else {
            navigation::move_to(
                mob,
                level,
                f64::from(wander_position.x()),
                f64::from(wander_position.y()),
                f64::from(wander_position.z()),
                self.speed_modifier,
            );
        }
    }

    fn name(&self) -> &'static str {
        "WanderingTrader$WanderToPositionGoal"
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static WANDERING_TRADER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::WANDERING_TRADER,
    create: |ty, id, registries| Box::new(WanderingTrader::new(ty, id, registries)),
};
