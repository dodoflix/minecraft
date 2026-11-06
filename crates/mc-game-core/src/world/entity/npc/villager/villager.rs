//! `Villager`: the brain mob that trades, works, sleeps, gossips and breeds (`net.minecraft.world.entity.npc.villager.Villager`).
//!
//! # Seams worth knowing about
//! * `getTradingPlayer()` is an entity id. Vanilla's `updateSpecialPrices(player)` touches the player (effects, menu, packet); while
//!   the player is itself taken out of the level (it is running a menu handler or `mobInteract`) the update cannot reach it, so
//!   callers without the player set `pending_special_prices`, which `customServerAiStep` applies (the player is in the level
//!   then). `startTrading` and every call from the villager's own tick use the exact vanilla path.
//! * The brain's schedule is bound to the level's environment attributes when the brain is made; the constructor has no level, so
//!   the construction-time `updateActivityFromSchedule` of vanilla runs at the first AI step (`schedule_init_pending`).

use super::abstract_villager::{self, AbstractVillagerData, DATA_UNHAPPY_COUNTER};
use super::villager_data::{self, VillagerData, VillagerDataExt};
use super::villager_profession as profession_data;
use super::villager_type;
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::behavior::villager_goal_packages as packages;
use crate::world::entity::ai::brain::{ActivityData, Brain, Provider};
use crate::world::entity::ai::gossip::{GossipContainer, GossipType};
use crate::world::entity::ai::memory::memory_module_type::{
    self as mem, HOME, JOB_SITE, LAST_SLEPT, LAST_WOKEN, MEETING_POINT, POTENTIAL_JOB_SITE,
};
use crate::world::entity::ai::memory::{BrainPacked, MemoryModuleType, MemoryStatus};
use crate::world::entity::ai::navigation::{PathNavigation, StandardPathNavigation};
use crate::world::entity::ai::sensing::sensor_type as sensors;
use crate::world::entity::ai::village::ReputationEventType;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::experience_orb::ExperienceOrb;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::npc::inventory_carrier::{self, InventoryCarrier};
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::schedule::Activity;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::trading::{Merchant, MerchantOffer, MerchantOffers};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, MobEffect, SoundEvent, VillagerProfession};
use mc_nbt::CompoundTag;
use mc_protocol::codec::GlobalPos;
use mc_protocol::components::values::basic::VILLAGER_FOOD;
use uuid::Uuid;

define_accessor!(pub DATA_VILLAGER_DATA: VillagerData = Villager);
define_accessor!(pub DATA_VILLAGER_DATA_FINALIZED: Boolean = Villager);

pub const BREEDING_FOOD_THRESHOLD: i32 = 12;
const MAX_GOSSIP_TOPICS: i32 = 10;
const GOSSIP_COOLDOWN: i64 = 1200;
const GOSSIP_DECAY_INTERVAL: i64 = 24000;
const HOW_FAR_AWAY_TO_TALK_TO_OTHER_VILLAGERS_ABOUT_GOLEMS: f64 = 10.0;
const HOW_MANY_VILLAGERS_NEED_TO_AGREE_TO_SPAWN_A_GOLEM: i32 = 5;
const TIME_SINCE_SLEEPING_FOR_GOLEM_SPAWNING: i64 = 24000;
/// `Villager.SPEED_MODIFIER`.
pub const SPEED_MODIFIER: f32 = 0.5;

/// `Villager`.
pub struct Villager {
    pub mob: MobData,
    pub av: AbstractVillagerData,
    /// `lastTradedPlayer`.
    last_traded_player: Option<EntityId>,
    /// `lastTradedPlayer`'s UUID (the id may be taken out of the level while it is read).
    last_traded_player_uuid: Option<Uuid>,
    /// UUID of `tradingPlayer` (set with it).
    trading_player_uuid: Option<Uuid>,
    food_level: i32,
    gossips: GossipContainer,
    last_gossip_time: i64,
    last_gossip_decay_time: i64,
    villager_xp: i32,
    last_restock_game_time: i64,
    number_of_restocks_today: i32,
    last_restock_check_day: i64,
    pending_special_prices: bool,
    schedule_init_pending: bool,
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.49, 0.98).with_eye_height(0.63)
}

/// `SensorType`s of `BRAIN_PROVIDER`.
static BRAIN_SENSORS: &[&sensors::SensorType] = &[
    &sensors::NEAREST_LIVING_ENTITIES,
    &sensors::NEAREST_PLAYERS,
    &sensors::NEAREST_ITEMS,
    &sensors::NEAREST_BED,
    &sensors::HURT_BY,
    &sensors::VILLAGER_HOSTILES,
    &sensors::VILLAGER_BABIES,
    &sensors::SECONDARY_POIS,
    &sensors::GOLEM_DETECTED,
];

/// `BRAIN_PROVIDER`'s activity supplier.
fn brain_activities(body: &dyn Entity) -> Vec<ActivityData> {
    let profession = body.base().entity_data.get(DATA_VILLAGER_DATA).profession;
    let mut activities = Vec::new();
    if body.is_baby() {
        activities.push(ActivityData::create_pairs(
            Activity::PLAY,
            packages::get_play_package(0.5),
        ));
    } else {
        activities.push(ActivityData::create_pairs_with_conditions(
            Activity::WORK,
            packages::get_work_package(profession, 0.5),
            vec![(JOB_SITE.id(), MemoryStatus::ValuePresent)],
        ));
    }
    activities.push(ActivityData::create_pairs(
        Activity::CORE,
        packages::get_core_package(profession, 0.5),
    ));
    activities.push(ActivityData::create_pairs_with_conditions(
        Activity::MEET,
        packages::get_meet_package(0.5),
        vec![(MEETING_POINT.id(), MemoryStatus::ValuePresent)],
    ));
    activities.push(ActivityData::create_pairs(
        Activity::REST,
        packages::get_rest_package(0.5),
    ));
    activities.push(ActivityData::create_pairs(
        Activity::IDLE,
        packages::get_idle_package(0.5),
    ));
    activities.push(ActivityData::create_pairs(
        Activity::PANIC,
        packages::get_panic_package(0.5),
    ));
    activities.push(ActivityData::create_pairs(
        Activity::PRE_RAID,
        packages::get_pre_raid_package(0.5),
    ));
    activities.push(ActivityData::create_pairs(
        Activity::RAID,
        packages::get_raid_package(0.5),
    ));
    activities.push(ActivityData::create_pairs(
        Activity::HIDE,
        packages::get_hide_package(0.5),
    ));
    activities
}

static BRAIN_PROVIDER: Provider = Provider::with_activities(BRAIN_SENSORS, brain_activities);

/// `POI_MEMORIES` predicate of `memory`: does the POI type `poi_type` belong to this memory for `villager`?
pub fn poi_memory_matches(
    villager_data: &VillagerData,
    memory: MemoryModuleType<GlobalPos>,
    poi_type: crate::world::entity::ai::village::poi::PoiTypeHolder,
) -> bool {
    use crate::world::entity::ai::village::poi::PoiTypes;
    if memory == HOME {
        poi_type.is(PoiTypes::HOME)
    } else if memory == JOB_SITE {
        profession_data::data(villager_data.profession)
            .held_job_site
            .test(poi_type)
    } else if memory == POTENTIAL_JOB_SITE {
        profession_data::is_acquirable_job_site(poi_type)
    } else if memory == MEETING_POINT {
        poi_type.is(PoiTypes::MEETING)
    } else {
        false
    }
}

/// `Villager.createDefaultVillagerData()`.
pub fn create_default_villager_data() -> VillagerData {
    VillagerData::new(villager_data::DEFAULT_TYPE, VillagerProfession::NONE, 1)
}

impl Villager {
    /// `new Villager(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::AgeableMob, |b| {
            b.define(DATA_UNHAPPY_COUNTER, 0);
            b.define(DATA_VILLAGER_DATA, create_default_villager_data());
            b.define(DATA_VILLAGER_DATA_FINALIZED, false);
        });
        let mut villager = Villager {
            mob,
            av: AbstractVillagerData::new(),
            last_traded_player: None,
            last_traded_player_uuid: None,
            trading_player_uuid: None,
            food_level: 0,
            gossips: GossipContainer::new(),
            last_gossip_time: 0,
            last_gossip_decay_time: 0,
            villager_xp: 0,
            last_restock_game_time: 0,
            number_of_restocks_today: 0,
            last_restock_check_day: 0,
            pending_special_prices: false,
            schedule_init_pending: true,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)` (after the yRot roll of `MobData::new`)
        let brain = villager.make_brain(&BrainPacked::EMPTY);
        *villager.brain_mut() = brain;
        abstract_villager::init(&mut villager.mob);
        let mut navigation = StandardPathNavigation::ground(&villager);
        navigation.set_can_open_doors(true);
        navigation.set_can_float(true);
        navigation.set_required_path_length(&villager, 48.0);
        villager.mob.set_navigation(Box::new(navigation));
        villager.mob.can_pick_up_loot = true;
        villager
    }

    /// `getVillagerData()`.
    pub fn get_villager_data(&self) -> VillagerData {
        self.mob.living.entity.entity_data.get(DATA_VILLAGER_DATA)
    }

    /// `setVillagerData(data)`.
    pub fn set_villager_data(&mut self, data: VillagerData) {
        let current = self.get_villager_data();
        if current.profession != data.profession {
            self.av.offers = None;
        }
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VILLAGER_DATA, data);
    }

    pub fn get_villager_data_finalized(&self) -> bool {
        self.mob
            .living
            .entity
            .entity_data
            .get(DATA_VILLAGER_DATA_FINALIZED)
    }

    pub fn set_villager_data_finalized(&mut self, finalized: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VILLAGER_DATA_FINALIZED, finalized);
    }

    /// `VillagerDataHolder.finalizeVillagerType(level, pos)`.
    pub fn finalize_villager_type(&mut self, level: &dyn ServerLevelAccessor, pos: BlockPos) {
        if !self.get_villager_data_finalized() {
            let biome = level
                .get_biome_id(pos)
                .and_then(|id| level.registry_access().registry(keys::BIOME)?.name_of(id.0))
                .map(|n| n.to_string());
            let ty = villager_type::by_biome(biome.as_deref());
            self.set_villager_data(self.get_villager_data().with_type(ty));
            self.set_villager_data_finalized(true);
        }
    }

    pub fn get_gossips(&self) -> &GossipContainer {
        &self.gossips
    }

    /// `setGossips(gossips)` (`putAll`).
    pub fn set_gossips(&mut self, gossips: &GossipContainer) {
        self.gossips.put_all(gossips);
    }

    pub fn get_villager_xp(&self) -> i32 {
        self.villager_xp
    }

    pub fn set_villager_xp(&mut self, value: i32) {
        self.villager_xp = value;
    }

    pub fn set_offers(&mut self, offers: Option<MerchantOffers>) {
        self.av.offers = offers;
    }

    pub fn is_trading(&self) -> bool {
        self.av.trading_player.is_some()
    }

    /// `refreshBrain(level)`.
    pub fn refresh_brain(&mut self, level: &mut ServerLevel) {
        Brain::stop_all(level, self);
        let packed = self.brain().pack();
        let brain = BRAIN_PROVIDER.make_brain(self, &packed);
        *self.brain_mut() = brain;
        self.register_brain_goals(level);
    }

    /// `registerBrainGoals(brain)`.
    fn register_brain_goals(&mut self, level: &ServerLevel) {
        let schedule = if self.is_baby() {
            crate::world::attribute::attributes::BABY_VILLAGER_ACTIVITY
        } else {
            crate::world::attribute::attributes::VILLAGER_ACTIVITY
        };
        let pos = self.position();
        let game_time = level.game_time();
        let brain = self.brain_mut();
        brain.set_schedule(schedule);
        brain.update_activity_from_schedule(game_time, |attribute| {
            Activity::by_id(&level.attribute_activity(attribute, &pos)).unwrap_or(Activity::IDLE)
        });
        self.schedule_init_pending = false;
    }

    /// `setUnhappy()`.
    fn set_unhappy(&mut self, level: &mut ServerLevel) {
        abstract_villager::set_unhappy_counter(self, 40);
        self.make_sound(level, SoundEvent::ENTITY_VILLAGER_NO);
    }

    fn make_sound(&mut self, level: &mut ServerLevel, sound: SoundEvent) {
        crate::world::entity::living_entity::damage::make_sound(self, level, sound);
    }

    /// `stopTrading()`.
    fn stop_trading(&mut self) {
        self.set_trading_player_id(None, None);
    }

    /// `setTradingPlayer(player)`: `shouldStop` re-enters `stopTrading()` (which sets `null` again) like vanilla.
    fn set_trading_player_id(&mut self, player: Option<EntityId>, uuid: Option<Uuid>) {
        let should_stop = self.av.trading_player.is_some() && player.is_none();
        self.av.trading_player = player;
        self.trading_player_uuid = uuid;
        if should_stop {
            self.stop_trading();
        }
    }

    /// `startTrading(player)`.
    fn start_trading(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        self.update_special_prices(level, player);
        self.set_trading_player_id(Some(player.base_id()), Some(player.uuid()));
        let title = crate::world::damagesource::display_name::entity_display_name(level, self);
        let merchant_level = self.get_villager_data().level;
        self.open_trading_screen(level, player, title, merchant_level);
    }

    /// `Merchant.openTradingScreen(player, title, level)` for a villager that is itself out of the level (the stock helper
    /// looks the merchant up in the level).
    fn open_trading_screen(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        title: mc_text::Component,
        merchant_level: i32,
    ) {
        use crate::world::inventory::{AbstractContainerMenu, MerchantMenu, SimpleMenuProvider};
        let me = self.id();
        let provider = SimpleMenuProvider::new(title, move |id, _player| {
            Some(Box::new(MerchantMenu::new(id, me)) as Box<dyn AbstractContainerMenu>)
        });
        let Some(container_id) = player.open_menu(level, &provider) else {
            return;
        };
        let (xp, show, restock) = (
            self.villager_xp,
            self.show_progress_bar(),
            self.can_restock(),
        );
        let offers = self.get_offers(level);
        if !offers.is_empty() {
            let offers = offers.clone();
            player.send_merchant_offers(container_id, &offers, merchant_level, xp, show, restock);
        }
    }

    // ---- restock / demand --------------------------------------------------------------------------------------------

    /// `restock()`.
    pub fn restock(&mut self, level: &mut ServerLevel) {
        self.update_demand(level);
        for offer in self.get_offers(level).0.iter_mut() {
            offer.reset_uses();
        }
        self.request_special_prices_update(level);
        self.last_restock_game_time = level.game_time();
        self.number_of_restocks_today += 1;
    }

    fn needs_to_restock(&mut self, level: &mut ServerLevel) -> bool {
        self.get_offers(level)
            .0
            .iter()
            .any(MerchantOffer::needs_restock)
    }

    fn allowed_to_restock(&self, level: &ServerLevel) -> bool {
        self.number_of_restocks_today == 0
            || self.number_of_restocks_today < 2
                && level.game_time() > self.last_restock_game_time + 2400
    }

    /// `shouldRestock(level)`.
    pub fn should_restock(&mut self, level: &mut ServerLevel) -> bool {
        let half_day_passed_time = self.last_restock_game_time + 12000;
        let game_time = level.game_time();
        let mut is_new_day = game_time > half_day_passed_time;
        let current_day = level.overworld_day_period_count();
        is_new_day |= self.last_restock_check_day > 0 && current_day > self.last_restock_check_day;
        self.last_restock_check_day = current_day;
        if is_new_day {
            self.last_restock_game_time = game_time;
            self.reset_number_of_restocks(level);
        }
        self.allowed_to_restock(level) && self.needs_to_restock(level)
    }

    /// `catchUpDemand()`.
    fn catch_up_demand(&mut self, level: &mut ServerLevel) {
        let missed_updates = 2 - self.number_of_restocks_today;
        if missed_updates > 0 {
            for offer in self.get_offers(level).0.iter_mut() {
                offer.reset_uses();
            }
        }
        for _ in 0..missed_updates {
            self.update_demand(level);
        }
        self.request_special_prices_update(level);
    }

    fn update_demand(&mut self, level: &mut ServerLevel) {
        for offer in self.get_offers(level).0.iter_mut() {
            offer.update_demand();
        }
    }

    fn reset_number_of_restocks(&mut self, level: &mut ServerLevel) {
        self.catch_up_demand(level);
        self.number_of_restocks_today = 0;
    }

    // ---- special prices ----------------------------------------------------------------------------------------------

    /// `getPlayerReputation(player)`.
    pub fn get_player_reputation(&self, player: &Uuid) -> i32 {
        self.gossips.get_reputation(player, |_| true)
    }

    /// `resetSpecialPrices()`.
    fn reset_special_prices(&mut self, level: &mut ServerLevel) {
        for offer in self.get_offers(level).0.iter_mut() {
            offer.reset_special_price_diff();
        }
    }

    /// `updateSpecialPrices(player)`.
    fn update_special_prices(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        self.reset_special_prices(level);
        let reputation = self.get_player_reputation(&player.uuid());
        let hero = player
            .get_effect(MobEffect::HERO_OF_THE_VILLAGE)
            .map(|e| e.amplifier);
        let hero_modifier = match hero {
            None => 0.0,
            Some(amplifier) => f64::from(0.3f32 + 0.0625f32 * amplifier as f32),
        };
        for offer in self.get_offers(level).0.iter_mut() {
            if reputation != 0 {
                offer.add_to_special_price_diff(-mth::floor_f32(
                    reputation as f32 * offer.get_price_multiplier(),
                ));
            }
            if hero_modifier > 0.0 {
                let cost_reduction =
                    (hero_modifier * f64::from(offer.get_base_cost_a().count())).floor() as i32;
                offer.add_to_special_price_diff(-cost_reduction.max(1));
            }
        }
        if self.av.trading_player == Some(player.base_id()) && !self.get_offers(level).is_empty() {
            self.refresh_trading_menu(level, player);
            let offers = self.get_offers(level).clone();
            let container_id = player.player.menus.current_container_id();
            player.send_merchant_offers(
                container_id,
                &offers,
                self.get_villager_data().level,
                self.villager_xp,
                self.show_progress_bar(),
                self.can_restock(),
            );
        }
    }

    /// `if (tradingPlayer.containerMenu instanceof MerchantMenu menu) menu.updateSellItem()`.
    fn refresh_trading_menu(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        use crate::world::entity::player::menu_ctx::PlayerCtx;
        use crate::world::inventory::{MerchantMenu, PlayerMenus};
        let mut offers = self.get_offers(level).clone();
        let notified = {
            let mut ctx = PlayerCtx { player, level };
            PlayerMenus::with_current(
                &mut ctx,
                |c| &mut c.player.player.menus,
                |menu, c| {
                    menu.as_any_mut()
                        .downcast_mut::<MerchantMenu>()
                        .and_then(|m| m.update_sell_item_with_offers(c, &mut offers))
                },
            )
        };
        if let Some(result) = notified {
            self.notify_trade_updated(level, &result);
        }
    }

    /// `updateSpecialPrices(tradingPlayer)` when the trading player is looked up through the level; defers when it cannot be reached.
    fn request_special_prices_update(&mut self, level: &mut ServerLevel) {
        let Some(player_id) = self.av.trading_player else {
            return;
        };
        let reached = level
            .with_entity(player_id, |level, e| {
                match e.as_any_mut().downcast_mut::<ServerPlayer>() {
                    Some(player) => {
                        self.update_special_prices(level, player);
                        true
                    }
                    None => false,
                }
            })
            .unwrap_or(false);
        if !reached {
            self.pending_special_prices = true;
        }
    }

    // ---- trades --------------------------------------------------------------------------------------------------------

    /// `updateTrades(level)`.
    fn update_trades(&mut self, level: &mut ServerLevel) {
        let data = self.get_villager_data();
        if let Some(trades) = profession_data::get_trades(data.profession, data.level) {
            let mut offers = self.av.offers.take().unwrap_or_default();
            abstract_villager::add_offers_from_trade_set(level, &*self, &mut offers, &trades);
            self.av.offers = Some(offers);
            self.request_special_prices_update(level);
        }
    }

    /// `shouldIncreaseLevel()`.
    fn should_increase_level(&self) -> bool {
        let current_level = self.get_villager_data().level;
        villager_data::can_level_up(current_level)
            && self.villager_xp >= villager_data::get_max_xp_per_level(current_level)
    }

    /// `increaseMerchantCareer(level)`.
    fn increase_merchant_career(&mut self, level: &mut ServerLevel) {
        let data = self.get_villager_data();
        self.set_villager_data(data.with_level(data.level + 1));
        self.update_trades(level);
    }

    /// `rewardTradeXp(offer)`.
    fn reward_trade_xp(&mut self, level: &mut ServerLevel, offer_index: usize) {
        let mut pop_xp = 3 + self.base_mut().random.next_int_bounded(4);
        let (xp, reward_exp) = {
            let offer = &self.av.offers.as_ref().expect("offers")[offer_index];
            (offer.get_xp(), offer.should_reward_exp())
        };
        self.villager_xp += xp;
        self.last_traded_player = self.av.trading_player;
        self.last_traded_player_uuid = self.trading_player_uuid;
        if self.should_increase_level() {
            self.increase_merchant_career(level);
            self.add_effect(
                level,
                MobEffectInstance::of(MobEffect::REGENERATION, 200, 0),
                None,
            );
            pop_xp += 5;
        }
        if reward_exp {
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

    // ---- food ----------------------------------------------------------------------------------------------------------

    fn hungry(&self) -> bool {
        self.food_level < BREEDING_FOOD_THRESHOLD
    }

    /// `eatUntilFull()`.
    fn eat_until_full(&mut self) {
        if !self.hungry() {
            return;
        }
        for slot in 0..self.av.inventory.get_container_size() {
            let stack = self.av.inventory.get_item(slot);
            if let Some(food) = stack.get(&VILLAGER_FOOD) {
                let nutrition = food.nutrition;
                let item_count = stack.count();
                let mut to_remove = 0;
                let mut count = item_count;
                while count > 0 {
                    self.food_level += nutrition;
                    to_remove += 1;
                    if !self.hungry() {
                        self.av.inventory.remove_item(slot, to_remove);
                        return;
                    }
                    count -= 1;
                }
                self.av.inventory.remove_item(slot, to_remove);
            }
        }
    }

    fn digest_food(&mut self, amount: i32) {
        self.food_level -= amount;
    }

    /// `eatAndDigestFood()`.
    pub fn eat_and_digest_food(&mut self) {
        self.eat_until_full();
        self.digest_food(12);
    }

    /// `countFoodPointsInInventory()`.
    fn count_food_points_in_inventory(&self) -> i32 {
        let mut points = 0;
        for item in self.av.inventory.get_items() {
            if let Some(food) = item.get(&VILLAGER_FOOD) {
                points += item.count() * food.nutrition;
            }
        }
        points
    }

    pub fn has_excess_food(&self) -> bool {
        self.count_food_points_in_inventory() >= 24
    }

    pub fn wants_more_food(&self) -> bool {
        self.count_food_points_in_inventory() < 12
    }

    /// `hasFarmSeeds()`.
    pub fn has_farm_seeds(&self) -> bool {
        let tag = mc_data::tags::get::<Item>("villager_plantable_seeds");
        self.av
            .inventory
            .get_items()
            .iter()
            .any(|s| tag.is_some_and(|t| t.contains(s.item())) && !s.is_empty())
    }

    // ---- gossip / golems -----------------------------------------------------------------------------------------------

    /// `gossip(level, target, timestamp)`.
    pub fn gossip(&mut self, level: &mut ServerLevel, target: &mut Villager, timestamp: i64) {
        if (timestamp < self.last_gossip_time
            || timestamp >= self.last_gossip_time + GOSSIP_COOLDOWN)
            && (timestamp < target.last_gossip_time
                || timestamp >= target.last_gossip_time + GOSSIP_COOLDOWN)
        {
            let new_gossip = self.gossips.transfer_from(
                &target.gossips,
                &mut self.mob.living.entity.random,
                MAX_GOSSIP_TOPICS,
            );
            self.last_gossip_time = timestamp;
            target.last_gossip_time = timestamp;
            self.spawn_golem_if_needed(
                level,
                timestamp,
                HOW_MANY_VILLAGERS_NEED_TO_AGREE_TO_SPAWN_A_GOLEM,
            );
            if self.av.trading_player.is_some() && new_gossip > 0 {
                self.request_special_prices_update(level);
            }
        }
    }

    /// `maybeDecayGossip()`.
    fn maybe_decay_gossip(&mut self, level: &ServerLevel) {
        let timestamp = level.game_time();
        if self.last_gossip_decay_time == 0 {
            self.last_gossip_decay_time = timestamp;
        } else if timestamp >= self.last_gossip_decay_time + GOSSIP_DECAY_INTERVAL {
            self.gossips.decay();
            self.last_gossip_decay_time = timestamp;
        }
    }

    /// `spawnGolemIfNeeded(level, timestamp, villagersNeededToAgree)`.
    pub fn spawn_golem_if_needed(
        &mut self,
        level: &mut ServerLevel,
        timestamp: i64,
        villagers_needed_to_agree: i32,
    ) {
        if self.wants_to_spawn_golem_at(level, timestamp) {
            let search_box = self.base().bounding_box().inflate(
                HOW_FAR_AWAY_TO_TALK_TO_OTHER_VILLAGERS_ABOUT_GOLEMS,
                HOW_FAR_AWAY_TO_TALK_TO_OTHER_VILLAGERS_ABOUT_GOLEMS,
                HOW_FAR_AWAY_TO_TALK_TO_OTHER_VILLAGERS_ABOUT_GOLEMS,
            );
            let nearby = nearby_villagers(level, &search_box);
            let wanting: Vec<EntityId> = nearby
                .iter()
                .copied()
                .filter(|&v| {
                    level
                        .get_entity(v)
                        .and_then(|e| e.as_any().downcast_ref::<Villager>())
                        .is_some_and(|v| v.wants_to_spawn_golem_at(level, timestamp))
                })
                .take(5)
                .collect();
            // `this` is out of the level: it is part of the box but not found there, so it counts itself
            let me_counts = search_box.intersects_aabb(&self.base().bounding_box())
                && self.wants_to_spawn_golem_at(level, timestamp);
            let agreeing = wanting.len() as i32 + i32::from(me_counts && wanting.len() < 5);
            if agreeing >= villagers_needed_to_agree {
                let pos = self.block_position();
                if crate::world::entity::spawn_util::try_spawn_mob(
                    level,
                    EntityType::IRON_GOLEM,
                    EntitySpawnReason::MobSummoned,
                    pos,
                    10,
                    8,
                    6,
                    crate::world::entity::spawn_util::Strategy::LegacyIronGolem,
                    false,
                )
                .is_some()
                {
                    for v in nearby {
                        level.with_entity(v, |_, e| {
                            crate::world::entity::ai::sensing::golem_sensor::golem_detected(e)
                        });
                    }
                    if me_counts {
                        crate::world::entity::ai::sensing::golem_sensor::golem_detected(self);
                    }
                }
            }
        }
    }

    /// `wantsToSpawnGolem(timestamp)`.
    pub fn wants_to_spawn_golem_at(&self, level: &ServerLevel, _timestamp: i64) -> bool {
        self.golem_spawn_conditions_met(level.game_time())
            && !self
                .brain()
                .has_memory_value(mem::GOLEM_DETECTED_RECENTLY.id())
    }

    /// `golemSpawnConditionsMet(gameTime)`.
    fn golem_spawn_conditions_met(&self, game_time: i64) -> bool {
        self.brain()
            .get_memory(&LAST_SLEPT)
            .is_some_and(|slept| game_time - *slept < TIME_SINCE_SLEEPING_FOR_GOLEM_SPAWNING)
    }

    // ---- POIs ----------------------------------------------------------------------------------------------------------

    /// `releaseAllPois()`.
    pub fn release_all_pois(&mut self, level: &mut ServerLevel) {
        self.release_poi(level, HOME);
        self.release_poi(level, JOB_SITE);
        self.release_poi(level, POTENTIAL_JOB_SITE);
        self.release_poi(level, MEETING_POINT);
    }

    /// `releasePoi(memoryType)`.
    pub fn release_poi(&mut self, level: &mut ServerLevel, memory: MemoryModuleType<GlobalPos>) {
        let Some(global) = self.brain().get_memory(&memory).cloned() else {
            return;
        };
        let data = self.get_villager_data();
        let release = |poi_level: &mut ServerLevel| {
            let manager = poi_level.poi_manager();
            let ty = manager.get_type(global.pos);
            if ty.is_some_and(|t| poi_memory_matches(&data, memory, t)) {
                let _ = manager.release(global.pos);
            }
        };
        // another dimension's POI manager is not reachable from a level
        if global.dimension == level.dimension {
            release(level);
        }
    }

    /// `tellWitnessesThatIWasMurdered(murderer)`.
    fn tell_witnesses_that_i_was_murdered(&mut self, level: &mut ServerLevel, murderer: Uuid) {
        let witnesses: Vec<EntityId> = self
            .brain()
            .get_memory(&mem::NEAREST_VISIBLE_LIVING_ENTITIES)
            .map(|w| {
                w.nearby_entities()
                    .iter()
                    .map(|r| r.0)
                    .filter(|&id| {
                        level
                            .get_entity(id)
                            .is_some_and(|e| e.is_reputation_event_handler())
                    })
                    .collect()
            })
            .unwrap_or_default();
        for witness in witnesses {
            level.with_entity(witness, |level, w| {
                w.on_reputation_event_from(level, ReputationEventType::VILLAGER_KILLED, murderer)
            });
        }
    }
}

/// The villagers in `bb` (`level.getEntitiesOfClass(Villager.class, bb)`).
fn nearby_villagers(level: &ServerLevel, bb: &AABB) -> Vec<EntityId> {
    level.get_entities_of_class(bb, |e| e.get_type() == EntityType::VILLAGER, |_| true)
}

impl InventoryCarrier for Villager {
    fn get_inventory(&self) -> &crate::world::container::SimpleContainer {
        &self.av.inventory
    }
    fn get_inventory_mut(&mut self) -> &mut crate::world::container::SimpleContainer {
        &mut self.av.inventory
    }
}

impl Merchant for Villager {
    fn set_trading_player(&mut self, player: Option<EntityId>) {
        // the UUID is only known to the caller that has the player; ids that are cleared clear both
        let uuid = if player.is_some() {
            self.trading_player_uuid
        } else {
            None
        };
        self.set_trading_player_id(player, uuid);
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
        // CriteriaTriggers::TRADE.trigger(tradingPlayer, this, offer.getResult())
    }

    fn notify_trade_updated(&mut self, level: &mut ServerLevel, stack: &ItemStack) {
        let interval = self.get_ambient_sound_interval();
        if self.mob.ambient_sound_time > -interval + 20 {
            self.mob.ambient_sound_time = -interval;
            self.make_sound(
                level,
                if stack.is_empty() {
                    SoundEvent::ENTITY_VILLAGER_NO
                } else {
                    SoundEvent::ENTITY_VILLAGER_YES
                },
            );
        }
    }

    fn get_villager_xp(&self) -> i32 {
        self.villager_xp
    }

    fn override_xp(&mut self, _xp: i32) {
        // `Villager` does not override `overrideXp` (the base class ignores it; the xp is added by `rewardTradeXp`)
    }

    fn show_progress_bar(&self) -> bool {
        true
    }

    fn get_notify_trade_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_VILLAGER_YES
    }

    fn can_restock(&self) -> bool {
        true
    }

    fn still_valid(&self, player: EntityId) -> bool {
        self.av.trading_player == Some(player) && self.is_alive()
    }
}

impl Entity for Villager {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:villager")
    }

    fn as_merchant(&self) -> Option<&dyn Merchant> {
        Some(self)
    }

    fn as_merchant_mut(&mut self) -> Option<&mut dyn Merchant> {
        Some(self)
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        // `registerBrainGoals` needs the level; it runs at the first AI step (`schedule_init_pending`) or in `refresh_brain`
        self.schedule_init_pending = true;
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn age_boundary_reached(&mut self, level: &mut ServerLevel) {
        mob::ageable::age_boundary_reached(self, level);
        self.refresh_brain(level);
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if self.schedule_init_pending {
            self.register_brain_goals(level);
        }
        if self.pending_special_prices {
            self.pending_special_prices = false;
            self.request_special_prices_update(level);
        }
        Brain::tick(level, self);
        if let Some(_traded) = self.last_traded_player.take() {
            let uuid = self.last_traded_player_uuid.take();
            if let Some(uuid) = uuid {
                self.on_reputation_event_from(level, ReputationEventType::TRADE, uuid);
            }
            level.broadcast_entity_event(self.id(), 14);
        }
        if !self.mob.is_no_ai() && self.base_mut().random.next_int_bounded(100) == 0 {
            let raid_active = level
                .get_raid_at(self.block_position())
                .is_some_and(|r| r.is_active() && !r.is_over());
            if raid_active {
                level.broadcast_entity_event(self.id(), 42);
            }
        }
        if self.av.offers.as_ref().is_none_or(|o| o.is_empty()) && self.is_trading() {
            self.stop_trading();
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if abstract_villager::get_unhappy_counter(self) > 0 {
            let counter = abstract_villager::get_unhappy_counter(self);
            abstract_villager::set_unhappy_counter(self, counter - 1);
        }
        self.maybe_decay_gossip(level);
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
            || self.is_sleeping()
        {
            return mob::interact::mob_interact(self, level, player, hand);
        }
        if self.is_baby() {
            self.set_unhappy(level);
            return InteractionResult::SUCCESS;
        }
        let no_offers = self.get_offers(level).is_empty();
        if hand == InteractionHand::MainHand {
            if no_offers {
                self.set_unhappy(level);
            }
            player.award_stat_once(level, crate::stats::stats::Stats::TALKED_TO_VILLAGER);
        }
        if no_offers {
            return InteractionResult::CONSUME;
        }
        self.start_trading(level, player);
        InteractionResult::SUCCESS
    }

    fn can_breed(&self) -> bool {
        self.food_level + self.count_food_points_in_inventory() >= 12
            && !self.is_sleeping()
            && mob::ageable::get_age(self) == 0
    }

    fn post_data_manipulated(&mut self) {
        // `updateSpecialPrices(tradingPlayer)` (no level here): applied at the next AI step
        if self.av.trading_player.is_some() {
            self.pending_special_prices = true;
        }
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if self.is_sleeping() {
            None
        } else if self.is_trading() {
            Some(SoundEvent::ENTITY_VILLAGER_TRADE)
        } else {
            Some(SoundEvent::ENTITY_VILLAGER_AMBIENT)
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VILLAGER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VILLAGER_DEATH)
    }

    fn is_reputation_event_handler(&self) -> bool {
        true
    }

    fn on_set_last_hurt_by_mob(
        &mut self,
        level: &mut ServerLevel,
        hurt_by: Uuid,
        hurt_by_is_player: bool,
    ) {
        self.on_reputation_event_from(level, ReputationEventType::VILLAGER_HURT, hurt_by);
        if self.is_alive() && hurt_by_is_player {
            level.broadcast_entity_event(self.id(), 13);
        }
    }

    fn die(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        log::info!("Villager {} died, message: '{}'", self.uuid(), "");
        if let Some(murderer) = source.causing_info().map(|c| c.uuid) {
            self.tell_witnesses_that_i_was_murdered(level, murderer);
        }
        self.release_all_pois(level);
        crate::world::entity::living_entity::death::die(self, level, source);
        self.stop_trading();
    }

    fn can_be_leashed(&self) -> bool {
        false
    }

    fn set_heard_bell_time(&mut self, game_time: i64) {
        if self.brain().has_memory_value(mem::HEARD_BELL_TIME.id()) || true {
            self.brain_mut()
                .set_memory(&mem::HEARD_BELL_TIME, game_time);
        }
    }

    fn stop_sleeping(&mut self, level: &mut ServerLevel) {
        crate::world::entity::living_entity::sleeping::stop_sleeping(self, level);
        let now = level.game_time();
        self.brain_mut().set_memory(&LAST_WOKEN, now);
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if reason == EntitySpawnReason::Breeding {
            self.set_villager_data(
                self.get_villager_data()
                    .with_profession(VillagerProfession::NONE),
            );
        }
        let pos = self.block_position();
        self.finalize_villager_type(&*level, pos);
        // `AbstractVillager.finalizeSpawn`: a default group data that never spawns babies
        let group_data = group_data
            .or_else(|| Some(Box::new(mob::spawn::AgeableMobGroupData::with_baby(false))));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let biome_roll = self.base_mut().random.next_double();
        let partner_type = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Villager>())
            .map(|p| p.get_villager_data().r#type);
        let ty = if biome_roll < 0.5 {
            let biome = level
                .get_biome_id(self.block_position())
                .and_then(|id| level.registries.registry(keys::BIOME)?.name_of(id.0))
                .map(|n| n.to_string());
            villager_type::by_biome(biome.as_deref())
        } else if biome_roll < 0.75 {
            self.get_villager_data().r#type
        } else {
            partner_type?
        };
        let mut baby = crate::world::entity::factory::create(
            EntityType::VILLAGER,
            level,
            crate::world::entity::factory::EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let villager = baby.as_any_mut().downcast_mut::<Villager>()?;
        let data = villager
            .get_villager_data()
            .with_type(ty)
            .with_profession(VillagerProfession::NONE);
        villager.set_villager_data(data);
        villager.set_villager_data_finalized(true);
        Some(baby)
    }

    fn thunder_hit(&mut self, level: &mut ServerLevel) {
        use crate::world::entity::mob::convert::{ConversionParams, convert_to};
        if level.difficulty() != mc_world::level_storage::Difficulty::Peaceful {
            log::info!("Villager {} was struck by lightning.", self.uuid());
            let params = ConversionParams::single(self, level, false, false);
            let converted = convert_to(
                self,
                level,
                EntityType::WITCH,
                &params,
                EntitySpawnReason::Conversion,
                |witch, level| {
                    let pos = witch.block_position();
                    let difficulty = ServerLevelAccessor::get_current_difficulty_at(level, pos);
                    witch.finalize_spawn(level, difficulty, EntitySpawnReason::Conversion, None);
                    witch.set_persistence_required();
                },
            );
            if converted.is_some() {
                self.release_all_pois(level);
                return;
            }
        }
        let fire_ticks = self.get_remaining_fire_ticks().wrapping_add(1);
        self.set_remaining_fire_ticks(fire_ticks);
        if self.get_remaining_fire_ticks() == 0 {
            self.ignite_for_seconds(8.0);
        }
        let source = level.damage_sources().lightning_bolt();
        self.hurt_server(level, &source, 5.0);
    }

    fn wants_to_pick_up(&self, _level: &ServerLevel, stack: &ItemStack) -> bool {
        let item = stack.item();
        let picks_up =
            mc_data::tags::get::<Item>("villager_picks_up").is_some_and(|t| t.contains(item));
        (picks_up
            || stack.get(&VILLAGER_FOOD).is_some()
            || profession_data::data(self.get_villager_data().profession)
                .requested_items
                .contains(&item))
            && self.av.inventory.can_add_item(stack)
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item: EntityId) {
        inventory_carrier::pick_up_item(level, self, item);
    }

    fn get_type_name(&self) -> mc_text::Component {
        mc_text::Component::translatable(
            profession_data::name_key(self.get_villager_data().profession),
            Vec::new(),
        )
    }

    fn on_reputation_event_from(
        &mut self,
        level: &mut ServerLevel,
        ty: ReputationEventType,
        source: Uuid,
    ) {
        if ty == ReputationEventType::ZOMBIE_VILLAGER_CURED {
            self.gossips.add(source, GossipType::MajorPositive, 20);
            self.gossips.add(source, GossipType::MinorPositive, 25);
        } else if ty == ReputationEventType::TRADE {
            self.gossips.add(source, GossipType::Trading, 2);
        } else if ty == ReputationEventType::VILLAGER_HURT {
            self.gossips.add(source, GossipType::MinorNegative, 25);
        } else if ty == ReputationEventType::VILLAGER_KILLED {
            self.gossips.add(source, GossipType::MajorNegative, 25);
        }
        if self.trading_player_uuid == Some(source) {
            self.request_special_prices_update(level);
        }
    }

    fn entity_components(&self) -> mc_protocol::components::DataComponentMap {
        let base =
            crate::advancements::predicates::entity::support::base_entity_components(self.base());
        mc_protocol::components::DataComponentMap::builder()
            .add_all(&base)
            .set(
                mc_protocol::components::values::entity::VILLAGER_VARIANT,
                mc_protocol::components::holder::Holder::Reference(
                    self.get_villager_data().r#type.identifier(),
                ),
            )
            .build()
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        abstract_villager::add_additional_save_data(&self.av, out);
        out.put(
            "VillagerData",
            mc_nbt::Tag::Compound(self.get_villager_data().save()),
        );
        out.put_boolean("VillagerDataFinalized", self.get_villager_data_finalized());
        out.put_byte("FoodLevel", self.food_level as i8);
        out.put("Gossips", mc_nbt::Tag::List(self.gossips.to_nbt()));
        out.put_int("Xp", self.villager_xp);
        out.put_long("LastRestock", self.last_restock_game_time);
        out.put_long("LastGossipDecay", self.last_gossip_decay_time);
        out.put_int("RestocksToday", self.number_of_restocks_today);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        abstract_villager::read_additional_save_data(&mut self.av, input);
        let data = input.get_compound("VillagerData").map(VillagerData::load);
        if input.get_boolean_or("VillagerDataFinalized", false) || data.is_some() {
            self.set_villager_data_finalized(true);
            let data = data.unwrap_or_else(create_default_villager_data);
            self.mob
                .living
                .entity
                .entity_data
                .set(DATA_VILLAGER_DATA, data);
        }
        self.food_level = i32::from(input.get_byte_or("FoodLevel", 0));
        self.gossips.clear();
        if let Some(list) = input.get_list("Gossips") {
            self.gossips.put_all(&GossipContainer::from_nbt(list));
        }
        self.villager_xp = input.get_int_or("Xp", 0);
        self.last_restock_game_time = input.get_long_or("LastRestock", 0);
        self.last_gossip_decay_time = input.get_long_or("LastGossipDecay", 0);
        self.number_of_restocks_today = input.get_int_or("RestocksToday", 0);
        // `refreshBrain(level)` needs the level: the brain is rebuilt from its packed memories at the first AI step
        // `refreshBrain(level)`: stop_all is a no-op at load; the rebuilt brain consumes the same RNG
        let packed = self.brain().pack();
        let brain = BRAIN_PROVIDER.make_brain(self, &packed);
        *self.brain_mut() = brain;
        self.schedule_init_pending = true;
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static VILLAGER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::VILLAGER,
    create: |ty, id, registries| Box::new(Villager::new(ty, id, registries)),
};
