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
use mc_data::block::{Block, BlockState};
use mc_game::registry::RegistryAccess;
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_events::BroadcastRecord;
use mc_game::server::level::level_tick::WeatherState;
use mc_game::world::container::Container;
use mc_game::world::entity::EntityId;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::experience_orb::ExperienceOrb;
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::player::inventory::Inventory;
use mc_game::world::inventory::enchantment_menu::BOOKSHELF_OFFSETS;
use mc_game::world::inventory::*;
use mc_game::world::item::ItemStack;
use mc_game::world::item::crafting::{RecipeManager, ServerRecipeBook};
use mc_game::world::level::block::behaviour as block_behaviour;
use mc_game::world::level::gamerules::{AnyGameRule, GameRules};
use mc_game::world::level::saveddata::maps::MapItemSavedData;
use mc_game::world::level::{BlockGetter, LevelAccessor};
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
        name: "chest",
        block: Block::CHEST,
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

/// The player of `VanillaLevel.connect` (standing next to [`TABLE`]): the eye is 1.62 above the feet.
fn eye() -> Vec3 {
    Vec3::new(0.5, 71.0 + f64::from(1.62f32), 0.5)
}

/// `Player` as the menus see it: the inventory, xp, the enchantment seed, the entity random, the recipe book and the level.
pub struct TestPlayer {
    pub inv: Inventory,
    pub creative: bool,
    pub xp_level: i32,
    pub seed: i32,
    pub rng: LegacyRandomSource,
    pub level: ServerLevel,
    pub recipe_book: ServerRecipeBook,
}

impl MenuPlayer for TestPlayer {
    fn entity_id(&self) -> i32 {
        1
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
    /// `ServerPlayer.drop` without the swing, thrower and statistics: the item entity at the hand (`createItemStackToDrop`, which draws four
    /// floats from the player's random).
    fn drop(&mut self, stack: ItemStack, _thrown: bool, _prediction: Prediction) {
        if stack.is_empty() {
            return;
        }
        let eye = eye();
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
        AABB::from_block_pos(&pos).distance_to_sqr(eye()) < range * range
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
    maps: BTreeSet<i32>,
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
                xp_level: 0,
                seed: 0,
                rng: LegacyRandomSource::new(0),
                level,
                recipe_book: ServerRecipeBook::new(),
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
            maps: BTreeSet::new(),
            log,
        }
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
        let kind = MENU_KINDS
            .iter()
            .find(|k| k.name == kind)
            .unwrap_or_else(|| panic!("unknown menu kind {kind}"));
        self.prev.clear();
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
        self.player.xp_level = xp;
        self.player.seed = seed;
        self.player.creative = creative;
        self.player.rng.set_seed(pseed);
        self.player.level.random().set_seed(lseed);
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
        self.maps.insert(id);
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
        let mut maps = String::from("M");
        for &id in &self.maps {
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
                menu.as_mut().unwrap().broadcast_changes(player);
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
