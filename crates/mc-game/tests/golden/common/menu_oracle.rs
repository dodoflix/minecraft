//! Replay side of the in-process menu oracle (producer: `tests/oracle/MenuOracleBase.java`, which drives the REAL vanilla menus of a
//! real `ServerPlayer` on a real `ServerLevel`; scripts in `tests/oracle/menu_scripts/NAME.txt` run through `scripts/menu-oracle.sh`,
//! committed output in `tests/fixtures/menu_oracle/NAME.txt.gz`). The Java header lists the ops and the `OUT` lines.
//!
//! The replay runs the same ops on our menus: a stand-in player ([`TestPlayer`]: inventory, xp, enchantment seed, the entity random,
//! the level) on a `ServerLevel` holding the same blocks. A menu is opened like the player does it: the block at [`TABLE`] provides it
//! (`BlockBehaviour::get_menu_provider`). After every op the state lines and container packets are compared with the recorded ones;
//! on a mismatch the failure names the first diverging op and prints both sides.
//!
//! Adding a menu: one entry in [`MENU_KINDS`] here and in `MenuOracleBase.registerKinds`, then a script.
//!
//! Not implemented: the stand-in player is no level entity, so it does not tick its inventory, pick up item entities, award
//! statistics/advancement triggers (including the recipes the real player unlocks by itself), play its own sounds or count as an
//! opener for scheduled container rechecks.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]

use super::block_scenario::{Env, format_broadcast, parse_state};
use crate::common;

use mc_core::ident::Identifier;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::EntityType;
use mc_data::MobEffect;
use mc_data::block::{Block, BlockState};
use mc_game::registry::RegistryAccess;
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_events::BroadcastRecord;
use mc_game::server::level::level_tick::WeatherState;
use mc_game::stats::Stat;
use mc_game::world::container::Container;
use mc_game::world::entity::EntityId;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::experience_orb::ExperienceOrb;
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::player::inventory::Inventory;
use mc_game::world::inventory::enchantment_menu::BOOKSHELF_OFFSETS;
use mc_game::world::inventory::*;
use mc_game::world::item::ItemStack;
use mc_game::world::item::crafting::{RecipeHolder, RecipeManager, ServerRecipeBook};
use mc_game::world::level::block::behaviour as block_behaviour;
use mc_game::world::level::gamerules::{AnyGameRule, GameRules};
use mc_game::world::level::saveddata::maps::MapItemSavedData;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_nbt::Tag;
use mc_nbt::snbt;
use mc_protocol::components::MapId;
use mc_world::level_storage::{Difficulty, FeatureFlagSet};
use parking_lot::Mutex;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

pub const TABLE: BlockPos = BlockPos::new(0, 70, 0);

/// The map ids `0..MAP_PROBE` the `M` line looks at (twin of `MenuOracleBase.MAP_PROBE`).
const MAP_PROBE: i32 = 64;

/// A menu the oracle can open: the block at [`TABLE`] provides it.
pub struct MenuKind {
    /// The kind name of the `OPEN` op.
    pub name: &'static str,
    pub block: Block,
    /// Blocks the kind places around [`TABLE`] (`shelves`, `blockers` of the `OPEN` op).
    pub setup: fn(&mut Session, usize, usize),
}

fn no_setup(_: &mut Session, _: usize, _: usize) {}

/// Bookshelves at the first `shelves` offsets, obstructions halfway to the first `blockers` ones.
fn enchanting_setup(s: &mut Session, shelves: usize, blockers: usize) {
    for off in BOOKSHELF_OFFSETS.iter().take(shelves) {
        s.set_block(
            TABLE.offset(off.x(), off.y(), off.z()),
            Block::BOOKSHELF.default_state(),
        );
    }
    for off in BOOKSHELF_OFFSETS.iter().take(blockers) {
        s.set_block(
            TABLE.offset(off.x() / 2, off.y(), off.z() / 2),
            Block::STONE.default_state(),
        );
    }
}

/// Iron pyramid of `levels` (0..=4) layers under the beacon.
fn beacon_setup(s: &mut Session, levels: usize, _: usize) {
    for i in 1..=levels.min(4) as i32 {
        for dx in -i..=i {
            for dz in -i..=i {
                s.set_block(TABLE.offset(dx, -i, dz), Block::IRON_BLOCK.default_state());
            }
        }
    }
}

/// A lectern holding a book of `pages` pages (a writable one when `writable != 0`, else a written one).
fn lectern_setup(s: &mut Session, pages: usize, writable: usize) {
    s.set_block(
        TABLE,
        Block::LECTERN
            .default_state()
            .set(mc_data::props::HAS_BOOK, true),
    );
    let list: Vec<String> = (1..=pages).map(|i| format!("\"page {i}\"")).collect();
    let list = list.join(",");
    let snbt = if writable != 0 {
        format!(
            "{{id:\"minecraft:writable_book\",count:1,components:{{\"minecraft:writable_book_content\":{{pages:[{list}]}}}}}}"
        )
    } else {
        format!(
            "{{id:\"minecraft:written_book\",count:1,components:{{\"minecraft:written_book_content\":{{title:\"T\",author:\"A\",resolved:1b,pages:[{list}]}}}}}}"
        )
    };
    // `loadAdditional` of the block entity: page 0 and the page count of the book, like `setBook` (for a book with at least one page)
    let Tag::Compound(tag) = snbt::parse(&format!("{{Book:{snbt},Page:0}}")).expect("book snbt")
    else {
        unreachable!("a compound");
    };
    assert!(
        s.player.level.load_block_entity_in_level(TABLE, &tag),
        "lectern block entity"
    );
}

/// Twin of `MenuOracleBase.registerKinds`.
pub const MENU_KINDS: &[MenuKind] = &[
    MenuKind {
        name: "enchant",
        block: Block::ENCHANTING_TABLE,
        setup: enchanting_setup,
    },
    MenuKind {
        name: "anvil",
        block: Block::ANVIL,
        setup: no_setup,
    },
    MenuKind {
        name: "chipped_anvil",
        block: Block::CHIPPED_ANVIL,
        setup: no_setup,
    },
    MenuKind {
        name: "damaged_anvil",
        block: Block::DAMAGED_ANVIL,
        setup: no_setup,
    },
    MenuKind {
        name: "grindstone",
        block: Block::GRINDSTONE,
        setup: no_setup,
    },
    MenuKind {
        name: "loom",
        block: Block::LOOM,
        setup: no_setup,
    },
    MenuKind {
        name: "crafting",
        block: Block::CRAFTING_TABLE,
        setup: no_setup,
    },
    MenuKind {
        name: "chest",
        block: Block::CHEST,
        setup: no_setup,
    },
    MenuKind {
        name: "cartography",
        block: Block::CARTOGRAPHY_TABLE,
        setup: no_setup,
    },
    MenuKind {
        name: "brewing_stand",
        block: Block::BREWING_STAND,
        setup: no_setup,
    },
    MenuKind {
        name: "beacon",
        block: Block::BEACON,
        setup: beacon_setup,
    },
    MenuKind {
        name: "lectern",
        block: Block::LECTERN,
        setup: lectern_setup,
    },
    MenuKind {
        name: "stonecutter",
        block: Block::STONECUTTER,
        setup: no_setup,
    },
    MenuKind {
        name: "crafter",
        block: Block::CRAFTER,
        setup: no_setup,
    },
    MenuKind {
        name: "smithing",
        block: Block::SMITHING_TABLE,
        setup: no_setup,
    },
];

fn st(s: &ItemStack) -> String {
    if s.is_empty() {
        return "-".into();
    }
    s.to_tag().into_result().expect("stack encodes").to_string()
}

fn parse_stack(text: &str) -> ItemStack {
    if text == "-" {
        return ItemStack::empty();
    }
    let tag = snbt::parse(text).unwrap_or_else(|e| panic!("snbt {text}: {e:?}"));
    ItemStack::from_tag(&tag)
        .into_result()
        .unwrap_or_else(|e| panic!("stack {text}: {e}"))
}

/// The registries and recipes of the default data pack, loaded once per test binary.
fn data() -> (RegistryAccess, RecipeManager) {
    static DATA: LazyLock<(RegistryAccess, RecipeManager)> = LazyLock::new(|| {
        // The server registers it when it starts; the stand-in player has no server (`MapExtendingRecipe.matches`).
        mc_game::world::item::crafting::set_map_scale_lookup(
            mc_game::world::item::map_item::map_scale_lookup,
        );
        let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = common::load_world(
            dir.path(),
            mc_world::level_storage::WorldDataConfiguration::DEFAULT,
            true,
        )
        .expect("default world loads");
        let recipes = stem
            .data_pack_resources
            .listener::<RecipeManager>()
            .expect("recipe manager")
            .clone();
        (stem.registries.composite_access().clone(), recipes)
    });
    DATA.clone()
}

/// Where the player of `VanillaLevel.connect` stands (next to [`TABLE`]).
const START: Vec3 = Vec3::new(0.5, 71.0, 0.5);

/// The entity id of the stand-in player: far from the ids the level hands out, so no entity of the replay shares it.
const PLAYER_ID: i32 = 0x7000_0001;

/// Kinds of the `OPEN` op that open nothing: the menu comes from `SPAWN` and `USE`.
const ENTITY_KINDS: &[&str] = &["merchant", "mount"];

/// `Player` as the menus see it: the inventory, xp, the enchantment seed, the entity random, the recipe book and the level.
pub struct TestPlayer {
    pub inv: Inventory,
    pub creative: bool,
    /// `abilities.mayBuild` (the `MAYBUILD` op, set again by `OPEN`).
    pub may_build: bool,
    pub xp_level: i32,
    pub seed: i32,
    pub rng: LegacyRandomSource,
    pub level: ServerLevel,
    pub recipe_book: ServerRecipeBook,
    /// The `item_crafted` statistics (item id -> count).
    pub crafted: BTreeMap<String, i32>,
    /// The feet (`PLAYER` moves it).
    pub pos: Vec3,
    /// `HEALTH 0`: not alive, no death processing.
    pub dead: bool,
    /// `HEALTH disconnected`.
    pub disconnected: bool,
}

impl TestPlayer {
    /// The eye is 1.62 above the feet.
    fn eye(&self) -> Vec3 {
        Vec3::new(self.pos.x, self.pos.y + f64::from(1.62f32), self.pos.z)
    }
}

impl MenuPlayer for TestPlayer {
    fn entity_id(&self) -> i32 {
        PLAYER_ID
    }
    fn is_alive(&self) -> bool {
        !self.dead
    }
    fn has_disconnected(&self) -> bool {
        self.disconnected
    }
    /// `Player.isWithinEntityInteractionRange`: the `ENTITY_INTERACTION_RANGE` base value (3.0) plus the buffer; the oracle's creative
    /// player never gets the creative bonus (it is added by the player's tick, which scripts do not run for it).
    fn is_within_entity_interaction_range(&self, target: &AABB, buffer: f64) -> bool {
        let range = 3.0 + buffer;
        target.distance_to_sqr(self.eye()) < range * range
    }
    fn registries(&self) -> RegistryAccess {
        self.level.registries.clone()
    }
    fn inventory(&self) -> &Inventory {
        &self.inv
    }
    fn inventory_mut(&mut self) -> &mut Inventory {
        &mut self.inv
    }
    fn has_infinite_materials(&self) -> bool {
        self.creative
    }
    fn is_creative(&self) -> bool {
        self.creative
    }
    fn is_spectator(&self) -> bool {
        false
    }
    fn may_build(&self) -> bool {
        self.may_build
    }
    /// `ServerPlayer.drop` without the swing, thrower and statistics: the item entity at the hand (`createItemStackToDrop`, which draws four
    /// floats from the player's random).
    fn drop(&mut self, stack: ItemStack, _thrown: bool, _prediction: Prediction) {
        if stack.is_empty() {
            return;
        }
        let eye = self.eye();
        let mut entity =
            ItemEntity::with_stack(&self.level, eye.x, eye.y - f64::from(0.3f32), eye.z, stack);
        entity.set_pick_up_delay(40);
        for _ in 0..4 {
            self.rng.next_float();
        }
        self.level.add_fresh_entity(Box::new(entity));
    }
    fn on_equip_item(&mut self, _slot: EquipmentSlot, _old: &ItemStack, _new: &ItemStack) {}
    /// `Player.isWithinBlockInteractionRange`: the eye against the block's box.
    fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool {
        let range = self.container_interaction_range() + buffer;
        AABB::from_block_pos(&pos).distance_to_sqr(self.eye()) < range * range
    }
    fn send_inventory_slot_update(&mut self, _slot: usize, _stack: ItemStack) {}
    fn level(&mut self) -> Option<&mut ServerLevel> {
        Some(&mut self.level)
    }
    fn level_ref(&self) -> Option<&ServerLevel> {
        Some(&self.level)
    }
    fn level_random_next_float(&mut self) -> f32 {
        self.level.random().next_float()
    }
    fn experience_level(&self) -> i32 {
        self.xp_level
    }
    fn give_experience_levels(&mut self, amount: i32) {
        self.xp_level = self.xp_level.saturating_add(amount);
        if self.xp_level < 0 {
            self.xp_level = 0;
        }
    }
    fn enchantment_seed(&self) -> i32 {
        self.seed
    }
    fn on_enchantment_performed(&mut self, _stack: &ItemStack, cost: i32) {
        self.xp_level = self.xp_level.wrapping_sub(cost);
        if self.xp_level < 0 {
            self.xp_level = 0;
        }
        self.seed = self.rng.next_int();
    }
    fn player_random_next_float(&mut self) -> f32 {
        self.rng.next_float()
    }
    fn recipe_book_contains(&self, recipe_id: &Identifier) -> bool {
        self.recipe_book.contains(recipe_id)
    }
    /// `ServerPlayer.awardRecipes` without the packet and the unlock trigger.
    fn award_recipes(&mut self, recipes: &[RecipeHolder]) {
        let manager = self.level.recipe_access();
        let mut entries = Vec::new();
        for recipe in recipes {
            self.recipe_book
                .try_add_recipe(recipe, &manager, &mut entries);
        }
    }
    fn award_stat(&mut self, stat: Stat, count: i32) {
        if let Stat::Crafted(item) = stat {
            *self.crafted.entry(item.name().to_string()).or_default() += count;
        }
    }
}

/// Records what the player's connection would have been sent (`ServerPlayer.containerSynchronizer`).
struct Sync(Rc<RefCell<Vec<String>>>);

impl ContainerSynchronizer for Sync {
    fn send_initial_data(
        &mut self,
        id: i32,
        state_id: i32,
        items: Vec<ItemStack>,
        carried: ItemStack,
        data: &[i32],
    ) {
        let mut b = format!("PKT content {id} {state_id} carried={}", st(&carried));
        for (i, s) in items.iter().enumerate() {
            if !s.is_empty() {
                b += &format!(" {i}={}", st(s));
            }
        }
        self.0.borrow_mut().push(b);
        for (i, v) in data.iter().enumerate() {
            self.0.borrow_mut().push(format!("PKT data {id} {i} {v}"));
        }
    }
    fn send_slot_change(&mut self, id: i32, state_id: i32, slot: usize, stack: ItemStack) {
        self.0
            .borrow_mut()
            .push(format!("PKT slot {id} {state_id} {slot} {}", st(&stack)));
    }
    fn send_carried_change(&mut self, stack: ItemStack) {
        self.0
            .borrow_mut()
            .push(format!("PKT cursor {}", st(&stack)));
    }
    fn send_data_change(&mut self, id: i32, data_id: usize, value: i32) {
        self.0
            .borrow_mut()
            .push(format!("PKT data {id} {data_id} {value}"));
    }
    fn create_slot(&self) -> RemoteSlot {
        RemoteSlot::new_synchronized()
    }
}

struct NoListener;
impl ContainerListener for NoListener {
    fn slot_changed(&mut self, _slot: &Slot, _slot_index: usize, _stack: &ItemStack) {}
}

/// `/gamerule NAME VALUE`.
fn set_rule(rules: &mut GameRules, name: &str, value: &str) {
    let id = Identifier::parse(&format!("minecraft:{name}")).unwrap();
    let rule = AnyGameRule::by_identifier(&id).unwrap_or_else(|| panic!("unknown gamerule {name}"));
    let parsed = rule
        .deserialize(value)
        .unwrap_or_else(|_| panic!("gamerule {name}: bad value {value}"));
    rules.try_set_value(rule, parsed);
}

pub struct Session {
    pub player: TestPlayer,
    menu: Option<Box<dyn AbstractContainerMenu>>,
    packets: Rc<RefCell<Vec<String>>>,
    prev: HashMap<&'static str, String>,
    container_counter: i32,
    placed: Vec<BlockPos>,
    env: Env,
    ticks: i64,
    /// `EVENTS on`: the broadcasts of the level are compared.
    events: bool,
    /// Set by the first `RECIPE` op (see `MenuOracleBase.recipes`).
    recipes: bool,
    /// The entities of `SPAWN` by name.
    ents: Vec<(String, EntityId)>,
    /// The last printed `E` line per entity (see `MenuOracleBase.entityLine`).
    ent_prev: HashMap<String, String>,
    log: Arc<Mutex<Vec<BroadcastRecord>>>,
}

impl Session {
    pub fn new() -> Self {
        let (registries, recipes) = data();
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = registries;
        level.shared.set_recipe_manager(recipes);
        // `VanillaLevel.preload`: the chunks around the origin stay loaded and entity-ticking (forced tickets), so ticks are
        // timing independent and blocks, entities and block entities are not unloaded
        level.tick_state.tick_time = true;
        let r = 6;
        for cx in -r..=r {
            for cz in -r..=r {
                level
                    .chunk_source_mut()
                    .update_chunk_forced(ChunkPos::new(cx, cz), true);
            }
        }
        let start = Instant::now();
        loop {
            level.tick_chunk_source(&|| true, false, true);
            while level.poll_chunk_task() {}
            level.tick_entity_management();
            let ready = (-r..=r).all(|cx| {
                (-r..=r).all(|cz| {
                    level.is_position_ticking_with_entities_loaded(ChunkPos::pack_xz(cx, cz))
                })
            });
            if ready {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(120),
                "chunks did not reach entity ticking"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        // `VanillaLevel.boot`: frozen, quiet world
        let mut game_rules = GameRules::new(FeatureFlagSet::DEFAULT_FLAGS);
        for rule in [
            "advance_time",
            "advance_weather",
            "spawn_mobs",
            "spawn_monsters",
            "spawn_patrols",
            "spawn_phantoms",
            "spawn_wandering_traders",
            "spawn_wardens",
            "raids",
            "log_admin_commands",
            "send_command_feedback",
        ] {
            set_rule(&mut game_rules, rule, "false");
        }
        set_rule(&mut game_rules, "random_tick_speed", "0");
        level.server_state.rules.refresh(&game_rules);
        let log = Arc::new(Mutex::new(Vec::new()));
        level.tick_state.broadcast_log = Some(log.clone());
        Self {
            player: TestPlayer {
                inv: Inventory::new(),
                creative: false,
                may_build: true,
                xp_level: 0,
                seed: 0,
                rng: LegacyRandomSource::new(0),
                level,
                recipe_book: ServerRecipeBook::new(),
                crafted: BTreeMap::new(),
                pos: START,
                dead: false,
                disconnected: false,
            },
            menu: None,
            packets: Rc::default(),
            prev: HashMap::new(),
            container_counter: 0,
            placed: Vec::new(),
            env: Env {
                game_rules,
                weather: WeatherState::default(),
                difficulty: Difficulty::Normal,
                clock: 0,
            },
            ticks: 0,
            events: false,
            recipes: false,
            ents: Vec::new(),
            ent_prev: HashMap::new(),
            log,
        }
    }

    fn ent(&self, name: &str) -> EntityId {
        self.ents
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| panic!("no entity {name}"))
    }

    /// `SPAWN ref type x y z seed [SNBT]`: the entity as the driver's `spawn` makes it, its random seeded with `seed`.
    fn spawn(&mut self, rest: &str) {
        let a: Vec<&str> = rest.splitn(7, ' ').collect();
        let type_name = if a[1].contains(':') {
            a[1].to_owned()
        } else {
            format!("minecraft:{}", a[1])
        };
        let ty =
            EntityType::by_name(&type_name).unwrap_or_else(|| panic!("entity type {type_name}"));
        let level = &mut self.player.level;
        let request = EntitySpawnRequest {
            reason: mc_data::enums::EntitySpawnReason::Command,
            ignore_checks: true,
        };
        let mut e = factory::create(ty, level, request)
            .unwrap_or_else(|| panic!("cannot create {type_name}"));
        let num = |i: usize| a[i].parse::<f64>().unwrap();
        e.snap_to(num(2), num(3), num(4), 0.0, 0.0);
        if let Some(snbt) = a.get(6) {
            let mut tag = mc_nbt::CompoundTag::new();
            e.save_without_id(&mut tag, &|_| None);
            tag.merge(
                &snbt::parse_compound(snbt).unwrap_or_else(|er| panic!("bad SNBT {snbt}: {er}")),
            );
            e.load(&tag)
                .unwrap_or_else(|er| panic!("entity load: {er}"));
        }
        e.base_mut().random = LegacyRandomSource::new(a[5].parse().unwrap());
        let id = e.id();
        level.add_fresh_entity(e);
        self.ents.push((a[0].to_owned(), id));
    }

    /// `ServerPlayer.openMenu` / `openHorseInventory`: an open menu is closed first (with its packet), the counter advances, the menu
    /// is created, announced (`open_packet`: the screen type name of a typed menu) and synchronised.
    fn open_entity_menu(
        &mut self,
        open_packet: Option<&str>,
        create: impl FnOnce(i32, &mut TestPlayer) -> Box<dyn AbstractContainerMenu>,
    ) {
        if let Some(mut old) = self.menu.take() {
            self.packets
                .borrow_mut()
                .push(format!("PKT close {}", old.base().container_id));
            old.removed(&mut self.player);
        }
        self.container_counter = self.container_counter % 100 + 1;
        let id = self.container_counter;
        let mut menu = create(id, &mut self.player);
        if let Some(name) = open_packet {
            self.packets
                .borrow_mut()
                .push(format!("PKT open {id} {name}"));
        }
        menu.add_slot_listener(Box::new(NoListener), &mut self.player);
        menu.base_mut()
            .set_synchronizer(Box::new(Sync(self.packets.clone())), &mut self.player);
        self.menu = Some(menu);
    }

    /// `USE`: a tamed horse without rider opens its inventory (`AbstractHorse.openCustomInventoryScreen`); a villager is talked to
    /// (`Villager.startTrading`: the special prices are reset, the player becomes the trading player, the screen opens).
    fn use_entity(&mut self, name: &str) {
        let id = self.ent(name);
        let kind = self.player.level.get_entity(id).map(|e| {
            let mut tag = mc_nbt::CompoundTag::new();
            e.save_without_id(&mut tag, &|_| None);
            (
                e.as_mount_inventory().is_some(),
                e.base().is_vehicle(),
                tag.get_boolean_or("Tame", false),
            )
        });
        match kind.unwrap_or_else(|| panic!("entity {name} is gone")) {
            (true, vehicle, tamed) => {
                if !vehicle && tamed {
                    self.open_entity_menu(None, |cid, player| {
                        Box::new(HorseInventoryMenu::new(cid, player, id))
                    });
                }
            }
            _ => {
                self.player.level.with_entity(id, |level, e| {
                    let m = e.as_merchant_mut().expect("merchant");
                    for offer in m.get_offers(level).0.iter_mut() {
                        offer.reset_special_price_diff();
                    }
                    m.set_trading_player(Some(PLAYER_ID));
                });
                self.open_entity_menu(Some("merchant"), |cid, _| {
                    Box::new(MerchantMenu::new(cid, id))
                });
            }
        }
    }

    /// The `E` line of `MenuOracleBase.entityLine`.
    fn entity_line(&mut self, name: &str, id: EntityId) -> String {
        let alive = self
            .player
            .level
            .get_entity(id)
            .is_some_and(|e| e.is_alive());
        let mut line = format!("E {name} alive={}", i32::from(alive));
        if !alive {
            return line;
        }
        self.player.level.with_entity(id, |level, e| {
            if let Some(m) = e.as_merchant_mut() {
                let offers: Vec<String> = m
                    .get_offers(level)
                    .iter()
                    .map(|o| {
                        format!(
                            "{}/{}/{}/{}",
                            o.get_uses(),
                            o.get_max_uses(),
                            o.get_demand(),
                            o.get_special_price_diff()
                        )
                    })
                    .collect();
                line += &format!(
                    " xp={} trading={} offers={}",
                    m.get_villager_xp(),
                    i32::from(m.get_trading_player().is_some()),
                    offers.join(";")
                );
            }
            if e.as_mount_inventory().is_some() {
                let mut tag = mc_nbt::CompoundTag::new();
                e.save_without_id(&mut tag, &|_| None);
                line += &format!(
                    " persist={} drops={}",
                    i32::from(tag.get_boolean_or("PersistenceRequired", false)),
                    tag.get("drop_chances")
                        .map_or("-".to_owned(), super::entity_scenario::canon)
                );
            }
        });
        line
    }

    pub fn set_block(&mut self, pos: BlockPos, state: BlockState) {
        self.player.level.set_block_and_update(pos, state);
        self.placed.push(pos);
    }

    fn entities_near_table<T: 'static>(&mut self) -> Vec<EntityId> {
        let area = AABB::new(0.0, 70.0, 0.0, 1.0, 71.0, 1.0).inflate(8.0, 8.0, 8.0);
        self.player
            .level
            .get_entities_of_class(&area, |e| e.as_any().is::<T>(), |_| true)
    }

    /// `OPEN`: resets the world around [`TABLE`], places the block of the kind and opens the menu it provides.
    fn open(&mut self, parts: &[&str]) {
        let kind = parts[0];
        let xp: i32 = parts[1].parse().unwrap();
        let seed: i32 = parts[2].parse().unwrap();
        let creative = parts[3] == "1";
        let pseed: i64 = parts[4].parse().unwrap();
        let lseed: i64 = parts[5].parse().unwrap();
        let shelves: usize = parts[6].parse().unwrap();
        let blockers: usize = parts[7].parse().unwrap();
        let spec = (!ENTITY_KINDS.contains(&kind)).then(|| {
            MENU_KINDS
                .iter()
                .find(|k| k.name == kind)
                .unwrap_or_else(|| panic!("unknown menu kind {kind}"))
        });
        self.prev.clear();
        self.ent_prev.clear();
        self.log.lock().clear();
        if let Some(mut menu) = self.menu.take() {
            menu.removed(&mut self.player);
        }
        for i in 0..self.player.inv.get_container_size() {
            self.player.inv.set_item(i, ItemStack::empty());
        }
        if self.recipes && self.player.recipe_book.known().next().is_some() {
            self.player.recipe_book = ServerRecipeBook::new();
        }
        for (_, id) in std::mem::take(&mut self.ents) {
            self.player
                .level
                .remove_entity(id, mc_game::world::entity::RemovalReason::Discarded);
        }
        for pos in std::mem::take(&mut self.placed) {
            self.player
                .level
                .set_block_and_update(pos, Block::AIR.default_state());
        }
        // removing a container drops its items: after the blocks
        for id in self.entities_near_table::<ExperienceOrb>() {
            self.player
                .level
                .remove_entity(id, mc_game::world::entity::RemovalReason::Discarded);
        }
        for id in self.entities_near_table::<ItemEntity>() {
            self.player
                .level
                .remove_entity(id, mc_game::world::entity::RemovalReason::Discarded);
        }
        self.player.pos = START;
        self.player.dead = false;
        self.player.disconnected = false;
        self.player.xp_level = xp;
        self.player.seed = seed;
        self.player.creative = creative;
        self.player.may_build = true;
        self.player.rng.set_seed(pseed);
        self.player.level.random().set_seed(lseed);
        // an entity kind opens nothing here: the menu comes from SPAWN and USE
        let Some(kind) = spec else {
            self.packets.borrow_mut().clear();
            return;
        };
        self.set_block(TABLE, kind.block.default_state());
        (kind.setup)(self, shelves, blockers);
        let state = self.player.level.get_block_state(TABLE);
        let provider =
            block_behaviour(state.block()).get_menu_provider(state, &self.player.level, TABLE);
        // `ServerPlayer.openMenu`: the counter advances even when the provider declines
        self.container_counter = self.container_counter % 100 + 1;
        let id = self.container_counter;
        self.packets.borrow_mut().clear();
        let Some(mut menu) = provider.and_then(|p| p.create_menu(id, &mut self.player)) else {
            return;
        };
        let type_name = menu.base().get_type().name();
        self.packets.borrow_mut().push(format!(
            "PKT open {id} {}",
            type_name.trim_start_matches("minecraft:")
        ));
        menu.add_slot_listener(Box::new(NoListener), &mut self.player);
        menu.base_mut()
            .set_synchronizer(Box::new(Sync(self.packets.clone())), &mut self.player);
        self.menu = Some(menu);
    }

    /// `TICK`: one server tick; the player's own tick (`ServerPlayer.tick`) is `broadcastChanges` and the `stillValid` check.
    fn tick(&mut self) {
        // `VanillaLevel.tick`: every tick restarts the entity-seed sequence
        mc_core::random::reseed_uniquifier(
            8682522807148012i64 ^ ((self.ticks + 1).wrapping_mul(0x9E3779B97F4A7C15u64 as i64)),
        );
        self.player.level.tick(&mut self.env, &mut || true);
        self.ticks += 1;
        let Session { player, menu, .. } = self;
        let Some(open) = menu.as_mut() else { return };
        open.flush_container_events(player);
        open.broadcast_changes(player);
        if !open.still_valid(player) {
            self.packets
                .borrow_mut()
                .push(format!("PKT close {}", open.base().container_id));
            if let Some(mut m) = self.menu.take() {
                m.removed(&mut self.player);
            }
        }
    }

    fn recipe(&mut self, what: &str, id: &str) {
        self.recipes = true;
        let manager = self.player.level.recipe_access();
        let holders: Vec<_> = if id == "*" {
            manager.recipes().to_vec()
        } else {
            let id = Identifier::parse(id).unwrap();
            vec![
                manager
                    .by_key(&id)
                    .unwrap_or_else(|| panic!("unknown recipe {id}"))
                    .clone(),
            ]
        };
        let book = &mut self.player.recipe_book;
        match what {
            "give" => {
                let mut entries = Vec::new();
                for h in &holders {
                    book.try_add_recipe(h, &manager, &mut entries);
                }
            }
            "take" => {
                book.remove_recipes(&holders, &manager);
            }
            other => panic!("RECIPE give|take, got {other}"),
        }
    }

    fn map(&mut self, parts: &[&str]) {
        let id: i32 = parts[0].parse().unwrap();
        let x: f64 = parts[1].parse().unwrap();
        let z: f64 = parts[2].parse().unwrap();
        let scale: i8 = parts[3].parse().unwrap();
        let level = &mut self.player.level;
        let mut data =
            MapItemSavedData::create_fresh(x, z, scale, false, false, level.dimension.clone());
        if parts[4] == "1" {
            data = data.locked();
        }
        level.set_map_data(MapId(id), data);
    }

    fn state(&mut self, out: &mut Vec<String>, key: &'static str, line: String) {
        if self.prev.insert(key, line.clone()).as_deref() != Some(line.as_str()) {
            out.push(line);
        }
    }

    /// Like [`Session::state`], but the `empty` line counts as already printed.
    fn state_unless_empty(
        &mut self,
        out: &mut Vec<String>,
        key: &'static str,
        line: String,
        empty: &str,
    ) {
        let before = self.prev.get(key).map_or(empty, String::as_str);
        if line != before {
            self.prev.insert(key, line.clone());
            out.push(line);
        }
    }

    /// The `OUT` lines of `MenuOracleBase.dump()`.
    fn dump(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(menu) = self.menu.as_mut() {
            let mut s = String::from("S");
            for i in 0..menu.base().slots.len() {
                let it = menu
                    .base_mut()
                    .with_slot(&mut self.player, i, |sl, e| sl.get_item(e));
                if !it.is_empty() {
                    s += &format!(" {i}={}", st(&it));
                }
            }
            // block entity values are read live like `DataSlot.get()`
            menu.base_mut().refresh_data_slots(&self.player);
            let mut d = String::from("D");
            for v in menu.base().data_slot_values() {
                d += &format!(" {v}");
            }
            let c = format!("C {}", st(menu.base().get_carried()));
            self.state(&mut out, "S", s);
            self.state(&mut out, "D", d);
            self.state(&mut out, "C", c);
        }
        let mut inv = String::from("I");
        for i in 0..self.player.inv.get_container_size() {
            let it = self.player.inv.get_item(i);
            if !it.is_empty() {
                inv += &format!(" {i}={}", st(it));
            }
        }
        self.state(&mut out, "I", inv);
        let x = format!("X {} {} 0 0", self.player.xp_level, self.player.seed);
        self.state(&mut out, "X", x);
        let bs = self.player.level.get_block_state(TABLE);
        let mut b = format!("B {}", bs.block().name().trim_start_matches("minecraft:"));
        if bs.has_property(mc_data::props::HORIZONTAL_FACING) {
            b += &format!(" {}", bs.get(mc_data::props::HORIZONTAL_FACING).get_name());
        }
        for name in [
            "has_bottle_0",
            "has_bottle_1",
            "has_bottle_2",
            "has_book",
            "powered",
        ] {
            if let Some(value) = bs.get_property_str(name) {
                b += &format!(" {name}={value}");
            }
        }
        self.state(&mut out, "B", b);
        let mut total = 0;
        for id in self.entities_near_table::<ExperienceOrb>() {
            self.player.level.with_entity(id, |_, e| {
                if let Some(o) = e.as_any().downcast_ref::<ExperienceOrb>() {
                    total += o.get_value() * o.count;
                }
            });
        }
        let o = format!("ORBS {total}");
        self.state(&mut out, "ORBS", o);
        let mut drops: BTreeMap<String, i32> = BTreeMap::new();
        for id in self.entities_near_table::<ItemEntity>() {
            self.player.level.with_entity(id, |_, e| {
                if let Some(item) = e.as_any().downcast_ref::<ItemEntity>() {
                    let stack = item.get_item();
                    *drops.entry(stack.item().name().to_string()).or_default() += stack.count();
                }
            });
        }
        let drops: String = drops.iter().map(|(k, v)| format!(" {k}={v}")).collect();
        self.state_unless_empty(&mut out, "DROPS", format!("DROPS{drops}"), "DROPS");
        if self.recipes {
            let known: BTreeSet<String> = self
                .player
                .recipe_book
                .known()
                .map(ToString::to_string)
                .collect();
            let known: String = known.iter().map(|k| format!(" {k}")).collect();
            self.state_unless_empty(&mut out, "K", format!("K{known}"), "K");
        }
        let crafted: String = self
            .player
            .crafted
            .iter()
            .filter(|(_, v)| **v > 0)
            .map(|(k, v)| format!(" {k}={v}"))
            .collect();
        self.state_unless_empty(&mut out, "CRAFTED", format!("CRAFTED{crafted}"), "CRAFTED");
        let mut maps = String::from("M");
        for id in 0..MAP_PROBE {
            self.player.level.read_map_data(MapId(id), |d| {
                let hash = d.colors.iter().fold(1i32, |h, &c| {
                    h.wrapping_mul(31).wrapping_add(i32::from(c as i8))
                });
                maps += &format!(
                    " {id}={},{},{},{},{hash}",
                    d.center_x,
                    d.center_z,
                    d.scale,
                    i32::from(d.locked)
                );
            });
        }
        self.state_unless_empty(&mut out, "M", maps, "M");
        // the player's position and life only show once an op moved or killed it
        let pos = self.player.pos;
        let line = format!(
            "P {:?},{:?},{:?} {}",
            pos.x,
            pos.y,
            pos.z,
            i32::from(!self.player.dead)
        );
        self.state_unless_empty(&mut out, "P", line, "P 0.5,71.0,0.5 1");
        for (name, id) in self.ents.clone() {
            let line = self.entity_line(&name, id);
            if self.ent_prev.insert(name, line.clone()).as_deref() != Some(line.as_str()) {
                out.push(line);
            }
        }
        let records = std::mem::take(&mut *self.log.lock());
        if self.events {
            out.extend(records.iter().map(format_broadcast));
        }
        out.append(&mut self.packets.borrow_mut());
        out
    }

    /// Runs one op and returns the `OUT` lines it produces (`R ...` lines first, like the oracle).
    pub fn run_op(&mut self, op: &str) -> Vec<String> {
        let (name, rest) = op.split_once(' ').unwrap_or((op, ""));
        self.log.lock().clear();
        match name {
            "OPEN" => {
                let parts: Vec<&str> = rest.split(' ').collect();
                self.open(&parts);
                self.dump()
            }
            "SET" => {
                let (slot, stack) = rest.split_once(' ').unwrap();
                let stack = parse_stack(stack);
                let slot: usize = slot.parse().unwrap();
                let Session { player, menu, .. } = self;
                let menu = menu.as_mut().unwrap();
                menu.base_mut()
                    .with_slot(player, slot, |sl, e| sl.set(e, stack));
                menu.flush_container_events(player);
                menu.broadcast_changes(player);
                self.dump()
            }
            "GIVE" => {
                let (slot, stack) = rest.split_once(' ').unwrap();
                let slot: usize = slot.parse().unwrap();
                self.player.inv.set_item(slot, parse_stack(stack));
                let Session { player, menu, .. } = self;
                if let Some(menu) = menu.as_mut() {
                    menu.broadcast_changes(player);
                }
                self.dump()
            }
            "CLICK" => {
                let parts: Vec<&str> = rest.split(' ').collect();
                let Session { player, menu, .. } = self;
                let menu = menu.as_mut().unwrap();
                menu.base_mut().suppress_remote_updates();
                menu.clicked(
                    parts[0].parse().unwrap(),
                    parts[1].parse().unwrap(),
                    input(parts[2]),
                    player,
                )
                .expect("click in range");
                menu.base_mut().resume_remote_updates();
                menu.broadcast_changes(player);
                self.dump()
            }
            "BUTTON" => {
                let Session { player, menu, .. } = self;
                let menu = menu.as_mut().unwrap();
                let r = menu.click_menu_button(player, rest.parse().unwrap());
                if r {
                    menu.broadcast_changes(player);
                }
                let mut out = vec![format!("R {r}")];
                out.append(&mut self.dump());
                out
            }
            "NAME" => {
                let Session { player, menu, .. } = self;
                let anvil = menu
                    .as_mut()
                    .unwrap()
                    .as_any_mut()
                    .downcast_mut::<AnvilMenu>()
                    .expect("anvil menu");
                let r = anvil.set_item_name(player, rest);
                let mut out = vec![format!("R {r}")];
                out.append(&mut self.dump());
                out
            }
            "CLOSE" => {
                let Session { player, menu, .. } = self;
                if let Some(mut m) = menu.take() {
                    m.removed(player);
                }
                self.dump()
            }
            "BLOCK" => {
                let parts: Vec<&str> = rest.splitn(4, ' ').collect();
                let pos = BlockPos::new(
                    parts[0].parse().unwrap(),
                    parts[1].parse().unwrap(),
                    parts[2].parse().unwrap(),
                );
                self.set_block(pos, parse_state(parts[3]));
                self.dump()
            }
            "TICK" => {
                for _ in 0..rest.parse::<usize>().unwrap() {
                    self.tick();
                }
                self.dump()
            }
            "RECIPE" => {
                let (what, id) = rest.split_once(' ').unwrap();
                self.recipe(what, id);
                self.dump()
            }
            "GAMERULE" => {
                let (rule, value) = rest.split_once(' ').unwrap();
                set_rule(&mut self.env.game_rules, rule, value);
                self.player
                    .level
                    .server_state
                    .rules
                    .refresh(&self.env.game_rules);
                self.dump()
            }
            "SEED" => {
                let (which, value) = rest.split_once(' ').unwrap();
                let value: i64 = value.parse().unwrap();
                match which {
                    "player" => self.player.rng.set_seed(value),
                    "level" => self.player.level.random().set_seed(value),
                    "enchant" => self.player.seed = value as i32,
                    other => panic!("SEED player|level|enchant, got {other}"),
                }
                self.dump()
            }
            "MAP" => {
                let parts: Vec<&str> = rest.split(' ').collect();
                self.map(&parts);
                self.dump()
            }
            "EVENTS" => {
                self.events = rest == "on";
                self.dump()
            }
            "MAYBUILD" => {
                self.player.may_build = rest == "1";
                self.dump()
            }
            "SLOTSTATE" => {
                let (slot, enabled) = rest.split_once(' ').unwrap();
                let Session { player, menu, .. } = self;
                let menu = menu.as_mut().unwrap();
                let id = menu.base().container_id;
                handle_container_slot_state_changed(
                    menu.as_mut(),
                    player,
                    id,
                    slot.parse().unwrap(),
                    enabled == "1",
                );
                menu.broadcast_changes(player);
                self.dump()
            }
            "SETBEACON" => {
                let (primary, secondary) = rest.split_once(' ').unwrap();
                let effect =
                    |id: &str| (id != "-").then(|| MobEffect::by_name(id).expect("effect"));
                let Session { player, menu, .. } = self;
                let mut out = Vec::new();
                if let Some(beacon) = menu
                    .as_mut()
                    .and_then(|m| m.as_any_mut().downcast_mut::<BeaconMenu>())
                {
                    if beacon.still_valid(player) {
                        let r = beacon.update_effects(player, effect(primary), effect(secondary));
                        beacon.broadcast_changes(player);
                        out.push(match r {
                            Some(r) => format!("R {r}"),
                            None => "R npe".into(),
                        });
                    } else {
                        out.push("R skipped".into());
                    }
                }
                out.append(&mut self.dump());
                out
            }
            "SPAWN" => {
                self.spawn(rest);
                self.dump()
            }
            "USE" => {
                let name = rest.split(' ').next().unwrap();
                self.use_entity(name);
                self.dump()
            }
            "TRADE" => {
                let selection: i32 = rest.parse().unwrap();
                let Session { player, menu, .. } = self;
                if let Some(menu) = menu.as_mut() {
                    // `ServerGamePacketListenerImpl.handleSelectTrade`
                    if let Some(m) = menu.as_any_mut().downcast_mut::<MerchantMenu>()
                        && AbstractContainerMenu::still_valid(m, player)
                    {
                        m.set_selection_hint(player, selection);
                        m.try_move_items(player, selection);
                    }
                    menu.broadcast_changes(player);
                }
                self.dump()
            }
            "NBT" => {
                let (name, snbt) = rest.split_once(' ').unwrap();
                let extra =
                    snbt::parse_compound(snbt).unwrap_or_else(|e| panic!("bad SNBT {snbt}: {e}"));
                let id = self.ent(name);
                self.player.level.with_entity(id, |_, e| {
                    let mut tag = mc_nbt::CompoundTag::new();
                    e.save_without_id(&mut tag, &|_| None);
                    tag.merge(&extra);
                    e.load(&tag)
                        .unwrap_or_else(|er| panic!("entity load: {er}"));
                });
                self.dump()
            }
            "MAPOFFERS" => {
                use mc_game::world::item::trading::{ItemCost, MerchantOffer, MerchantOffers};
                use mc_protocol::components::{MAP_ID, MAP_POST_PROCESSING, MapPostProcessing};
                let id = self.ent(rest);
                self.player.level.with_entity(id, |level, e| {
                    let offers = (0..2)
                        .map(|i| {
                            let mut map = ItemStack::new(mc_data::Item::FILLED_MAP);
                            map.set(MAP_ID, MapId(5 + i));
                            map.set(
                                MAP_POST_PROCESSING,
                                if i == 0 {
                                    MapPostProcessing::Lock
                                } else {
                                    MapPostProcessing::Scale
                                },
                            );
                            MerchantOffer::with_cost_b(
                                ItemCost::of(mc_data::Item::EMERALD, 13 + i),
                                Some(ItemCost::of(mc_data::Item::COMPASS, 1)),
                                map,
                                12,
                                1,
                                0.05,
                            )
                        })
                        .collect();
                    *e.as_merchant_mut().expect("merchant").get_offers(level) =
                        MerchantOffers(offers);
                });
                self.dump()
            }
            "DISCARD" => {
                let id = self.ent(rest);
                self.player
                    .level
                    .remove_entity(id, mc_game::world::entity::RemovalReason::Discarded);
                self.dump()
            }
            "PLAYER" => {
                let c: Vec<f64> = rest.split(' ').map(|v| v.parse().unwrap()).collect();
                self.player.pos = Vec3::new(c[0], c[1], c[2]);
                self.dump()
            }
            "HEALTH" => {
                if rest == "disconnected" {
                    self.player.disconnected = true;
                } else {
                    self.player.dead = rest.parse::<f32>().unwrap() <= 0.0;
                }
                self.dump()
            }
            other => panic!("unknown op {other}"),
        }
    }
}

fn input(name: &str) -> ContainerInput {
    match name {
        "PICKUP" => ContainerInput::Pickup,
        "QUICK_MOVE" => ContainerInput::QuickMove,
        "SWAP" => ContainerInput::Swap,
        "CLONE" => ContainerInput::Clone,
        "THROW" => ContainerInput::Throw,
        "QUICK_CRAFT" => ContainerInput::QuickCraft,
        "PICKUP_ALL" => ContainerInput::PickupAll,
        _ => panic!("input {name}"),
    }
}

fn read_fixture(dir: &str, name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/{dir}/{name}.txt.gz",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("{path}: {e}")),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}

/// Replays `tests/fixtures/DIR/NAME.txt.gz` and compares every op's lines; returns the number of ops. The failure names the first
/// diverging op (number, text, the five ops before it) and prints both sides' lines for it.
pub fn replay(dir: &str, name: &str) -> usize {
    let text = read_fixture(dir, name);
    let mut s = Session::new();
    let mut expected: Vec<&str> = Vec::new();
    let mut actual: Vec<String> = Vec::new();
    let mut op_line = String::new();
    let mut recent: Vec<String> = Vec::new();
    let mut ops = 0usize;
    let finish = |ops: usize, op: &str, actual: &[String], expected: &[&str], recent: &[String]| {
        for i in 0..actual.len().max(expected.len()) {
            assert_eq!(
                actual.get(i).map(String::as_str),
                expected.get(i).copied(),
                "{dir}/{name}: first divergence after op #{ops} `{op}` (line {i}); recent ops {recent:#?}\nactual={actual:#?}\nexpected={expected:#?}"
            );
        }
    };
    for line in text.lines() {
        if let Some(offsets) = line.strip_prefix("OFFSETS ") {
            let ours: Vec<String> = BOOKSHELF_OFFSETS
                .iter()
                .map(|b| format!("{},{},{}", b.x(), b.y(), b.z()))
                .collect();
            assert_eq!(ours.join(";"), offsets, "BOOKSHELF_OFFSETS");
            continue;
        }
        if line.starts_with('#') || line.starts_with("SECTION") {
            continue;
        }
        if let Some(out) = line.strip_prefix("OUT ") {
            expected.push(out);
            continue;
        }
        let op = line
            .strip_prefix("OP ")
            .unwrap_or_else(|| panic!("unexpected line {line}"));
        if !op_line.is_empty() {
            finish(ops, &op_line, &actual, &expected, &recent);
        }
        actual.clear();
        expected.clear();
        op_line = op.to_string();
        recent.push(op_line.chars().take(160).collect());
        if recent.len() > 5 {
            recent.remove(0);
        }
        ops += 1;
        actual = s.run_op(op);
    }
    finish(ops, &op_line, &actual, &expected, &recent);
    ops
}
