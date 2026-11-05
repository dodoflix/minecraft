//! `Brain` vs vanilla (`oracle/BrainOracle.java`, fixture `fixtures/brain/brain.txt`): a brain with fake behaviors (start/tick/stop log)
//! ticked 70 times on a seeded level random — behavior start/tick order across priorities and activities (HashMap order), `Behavior`
//! duration rolls, `RunOne` shuffling, `DoNothing`, `SetWalkTargetFromLookTarget`, activity switching with requirements and
//! erase-on-stop memories, memory expiry, `stopAll`, the sensors' random initial delay — and the sensor scan pattern.

use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_game::entry_condition;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::ai::behavior::position_tracker::{BlockPosTracker, PositionTracker};
use mc_game::world::entity::ai::behavior::{set_walk_target_from_look_target, Behavior, BehaviorBase, BehaviorControl, DoNothing, GateBehavior, OrderPolicy, RunningPolicy, ShufflingList};
use mc_game::world::entity::ai::brain::{ActivityData, Brain};
use mc_game::world::entity::ai::memory::memory_module_type::*;
use mc_game::world::entity::ai::memory::{MemoryMap, MemoryStatus, MemoryTypeId};
use mc_game::world::entity::ai::sensing::sensor::{Sensor, SensorInstance};
use mc_game::world::entity::ai::sensing::sensor_type;
use mc_game::world::entity::entity::Entity;
use mc_game::world::entity::schedule::Activity;
use mc_game::world::entity::ServerPlayer;
use mc_protocol::codec::GameProfile;
use std::cell::{Cell, RefCell};

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static NOW: Cell<i64> = const { Cell::new(0) };
}

fn log(name: &str, what: &str) {
    LOG.with(|l| l.borrow_mut().push(format!("{} {} {}", NOW.get(), name, what)));
}

struct Fake {
    name: &'static str,
    run_for: i32,
    period: i32,
    calls: i32,
    ran: i32,
    set_on_start: bool,
    erase_on_stop: Option<MemoryTypeId>,
}

impl Behavior for Fake {
    fn check_extra_start_conditions(&mut self, _: &mut ServerLevel, _: &mut dyn Entity) -> bool {
        let ok = self.calls % self.period == 0;
        self.calls += 1;
        ok
    }
    fn start(&mut self, _: &mut ServerLevel, body: &mut dyn Entity, _: i64) {
        self.ran = 0;
        log(self.name, "start");
        if self.set_on_start {
            body.brain_mut().set_memory_with_expiry(&ATTACK_COOLING_DOWN, true, 4);
        }
    }
    fn can_still_use(&mut self, _: &mut ServerLevel, _: &mut dyn Entity, _: i64) -> bool {
        self.ran < self.run_for
    }
    fn tick(&mut self, _: &mut ServerLevel, _: &mut dyn Entity, _: i64) {
        self.ran += 1;
        log(self.name, "tick");
    }
    fn stop(&mut self, _: &mut ServerLevel, body: &mut dyn Entity, _: i64) {
        log(self.name, "stop");
        if let Some(m) = self.erase_on_stop {
            body.brain_mut().erase_memory_id(m);
        }
    }
}

fn fake(name: &'static str, cond: Vec<(MemoryTypeId, MemoryStatus)>, min: i32, max: i32, run_for: i32, period: i32) -> Box<dyn BehaviorControl> {
    fake_full(name, cond, min, max, run_for, period, false, None)
}

#[allow(clippy::too_many_arguments)]
fn fake_full(name: &'static str, cond: Vec<(MemoryTypeId, MemoryStatus)>, min: i32, max: i32, run_for: i32, period: i32, set_on_start: bool, erase_on_stop: Option<MemoryTypeId>) -> Box<dyn BehaviorControl> {
    Box::new(BehaviorBase::with_duration(cond, min, max, Fake { name, run_for, period, calls: 0, ran: 0, set_on_start, erase_on_stop }))
}

fn pairs(v: Vec<(i32, Box<dyn BehaviorControl>)>) -> Vec<(i32, Box<dyn BehaviorControl>)> {
    v
}

struct CountingSensor(i32);

impl Sensor for CountingSensor {
    fn scan_rate(&self) -> i32 {
        self.0
    }
    fn do_tick(&mut self, _: &mut ServerLevel, _: &mut dyn Entity) {}
    fn requires(&self) -> Vec<MemoryTypeId> {
        Vec::new()
    }
}

#[test]
fn brain_matches_vanilla() {
    let golden = include_str!("fixtures/brain/brain.txt");
    let mut expected = golden.lines();
    let mut next = |what: &str| -> String { expected.next().unwrap_or_else(|| panic!("fixture ended at {what}")).to_string() };

    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.tick_state.random = LegacyRandomSource::new(424242);
    let profile = GameProfile { id: uuid::Uuid::from_u128(1), name: "brain".into(), properties: vec![] };
    let mut body = ServerPlayer::new(1, Identifier::parse("minecraft:overworld").unwrap(), profile, Default::default());

    let run_one = GateBehavior::with_list(
        vec![],
        vec![],
        OrderPolicy::Shuffled,
        RunningPolicy::RunOne,
        vec![
            (fake("gate_a", vec![], 2, 4, 2, 1), 1),
            (fake("gate_b", vec![], 2, 6, 3, 2), 2),
            (fake("gate_c", vec![], 1, 3, 1, 1), 3),
        ],
        ShufflingList::with_random(LegacyRandomSource::new(777)),
        "RunOne",
    );
    let activities = vec![
        ActivityData::create_pairs(
            Activity::CORE,
            pairs(vec![
                (0, fake("core_a", vec![], 3, 5, 4, 1)),
                (1, Box::new(DoNothing::new(2, 6))),
                (2, Box::new(set_walk_target_from_look_target::create(1.0, 2))),
                (3, Box::new(run_one)),
            ]),
        ),
        ActivityData::create_pairs(Activity::IDLE, pairs(vec![(10, fake("idle_a", vec![], 4, 4, 5, 1)), (10, fake("idle_b", entry_condition![LOOK_TARGET => ValuePresent], 2, 7, 3, 3))])),
        ActivityData::create_pairs_with_conditions(
            Activity::WORK,
            pairs(vec![(10, fake_full("work_a", entry_condition![ATTACK_COOLING_DOWN => ValueAbsent], 3, 9, 6, 1, true, None))]),
            entry_condition![HEARD_BELL_TIME => ValueAbsent],
        ),
        ActivityData::create_full(
            Activity::PANIC,
            pairs(vec![
                (4, fake_full("panic_a", vec![], 2, 2, 8, 1, false, Some(IS_PANICKING.id()))),
                (10, fake("panic_b", entry_condition![IS_PANICKING => ValuePresent], 1, 5, 2, 1)),
            ]),
            entry_condition![IS_PANICKING => ValuePresent],
            vec![IS_PANICKING.id()],
        ),
        ActivityData::create_pairs(Activity::MEET, pairs(vec![(10, fake("meet_a", vec![], 3, 3, 3, 1))])),
    ];
    let mut brain_random = LegacyRandomSource::new(99);
    let memory_types = [WALK_TARGET.id(), LOOK_TARGET.id(), ATTACK_COOLING_DOWN.id(), LONG_JUMP_COOLDOWN_TICKS.id(), HEARD_BELL_TIME.id(), IS_PANICKING.id()];
    body.brain_mut().replace_with(Brain::construct(&memory_types, &[&sensor_type::DUMMY, &sensor_type::DUMMY], activities, &MemoryMap::EMPTY, &mut brain_random));
    assert_eq!(format!("brain_random_after_construct {}", brain_random.next_int_bounded(1000000)), next("construct"));

    for t in 1..=70i64 {
        let now = 1000 + t;
        NOW.set(now);
        level.set_game_time(now);
        LOG.with(|l| l.borrow_mut().clear());
        let b = body.brain_mut();
        match t {
            3 => b.set_memory(&LOOK_TARGET, PositionTracker::Block(BlockPosTracker::new(BlockPos::new(1, 2, 3)))),
            5 => b.set_memory_with_expiry(&ATTACK_COOLING_DOWN, true, 6),
            9 => {
                b.set_memory(&IS_PANICKING, true);
                b.set_active_activity_if_possible(Activity::PANIC);
            }
            15 => b.set_active_activity_if_possible(Activity::WORK),
            20 => b.use_default_activity(),
            25 => b.set_active_activity_to_first_valid(&[Activity::PANIC, Activity::MEET, Activity::WORK]),
            30 => b.set_memory_with_expiry(&LONG_JUMP_COOLDOWN_TICKS, 7, 3),
            33 => b.set_memory(&HEARD_BELL_TIME, 5i64),
            34 => b.set_active_activity_if_possible(Activity::WORK),
            40 => {
                b.set_default_activity(Activity::MEET);
                b.use_default_activity();
            }
            50 => b.erase_memory(&LOOK_TARGET),
            55 => Brain::stop_all(&mut level, &mut body),
            _ => {}
        }
        Brain::tick(&mut level, &mut body);
        let events: Vec<String> = LOG.with(|l| l.borrow().clone());
        for e in events {
            assert_eq!(format!("ev {e}"), next("event"), "tick {t}");
        }
        let b = body.brain();
        let mut active: Vec<&str> = b.get_active_activities().iter().map(|a| a.get_name()).collect();
        active.sort_unstable();
        let cool = if b.has_memory_value(ATTACK_COOLING_DOWN) { format!("true/{}", b.get_time_until_expiry(&ATTACK_COOLING_DOWN)) } else { "false".to_string() };
        let jump = b.get_memory(&LONG_JUMP_COOLDOWN_TICKS).map_or("-".to_string(), |v| v.to_string());
        let line = format!(
            "tick {t} active=[{}] walk={} look={} cool={cool} jump={jump} panic={} bell={} rand={}",
            active.join(", "),
            b.has_memory_value(WALK_TARGET),
            b.has_memory_value(LOOK_TARGET),
            b.has_memory_value(IS_PANICKING),
            b.has_memory_value(HEARD_BELL_TIME),
            level.tick_state.random.next_int_bounded(1000000)
        );
        assert_eq!(line, next("tick line"));
    }

    for rate in [1, 2, 5, 20, 40, 200] {
        for seed in [1i64, 42, 987654321] {
            let mut instance = SensorInstance::new(Box::new(CountingSensor(rate)));
            let mut r = LegacyRandomSource::new(seed);
            instance.randomly_delay_start(&mut r);
            let mut ticks = Vec::new();
            for t in 1..=500 {
                if instance.due() {
                    ticks.push(t);
                }
            }
            let shown: Vec<String> = ticks.iter().take(12).map(|t| t.to_string()).collect();
            assert_eq!(format!("sensor {rate} {seed} next={} [{}]", r.next_int_bounded(1000), shown.join(", ")), next("sensor"));
        }
    }
    assert!(expected.next().is_none(), "fixture has more lines");
}

#[test]
fn memory_expiry_and_empty_collections() {
    use mc_game::world::entity::ai::memory::EntityRef;
    let mut r = LegacyRandomSource::new(1);
    let mut brain = Brain::construct(&[LONG_JUMP_COOLDOWN_TICKS.id(), NEAREST_LIVING_ENTITIES.id(), HOME.id()], &[], vec![], &MemoryMap::EMPTY, &mut r);
    brain.set_memory_with_expiry(&LONG_JUMP_COOLDOWN_TICKS, 5, 2);
    assert_eq!(brain.get_time_until_expiry(&LONG_JUMP_COOLDOWN_TICKS), 2);
    brain.forget_outdated_memories();
    brain.forget_outdated_memories();
    assert!(brain.has_memory_value(LONG_JUMP_COOLDOWN_TICKS));
    assert_eq!(brain.get_time_until_expiry(&LONG_JUMP_COOLDOWN_TICKS), 0);
    brain.forget_outdated_memories();
    assert!(!brain.has_memory_value(LONG_JUMP_COOLDOWN_TICKS));
    // an empty collection clears the slot instead of being stored
    brain.set_memory(&NEAREST_LIVING_ENTITIES, vec![EntityRef(3)]);
    assert!(brain.has_memory_value(NEAREST_LIVING_ENTITIES));
    brain.set_memory(&NEAREST_LIVING_ENTITIES, Vec::new());
    assert!(!brain.has_memory_value(NEAREST_LIVING_ENTITIES));
    // unregistered memories are silently ignored by setters and fail every check
    brain.set_memory(&WALK_TARGET, mc_game::world::entity::ai::memory::WalkTarget::from_block_pos(BlockPos::new(0, 0, 0), 1.0, 1));
    assert!(!brain.check_memory(WALK_TARGET.id(), MemoryStatus::Registered));
    assert!(brain.check_memory(HOME.id(), MemoryStatus::ValueAbsent));
}

#[test]
fn pack_and_load_persistent_memories() {
    use mc_game::world::entity::ai::memory::{BrainPacked, Unit};
    use mc_nbt::Tag;
    use mc_protocol::codec::GlobalPos;
    let mut r = LegacyRandomSource::new(1);
    let types = [HOME.id(), LAST_SLEPT.id(), IS_IN_WATER.id(), LIKED_PLAYER.id(), WALK_TARGET.id(), VISITED_BLOCK_POSITIONS.id(), GOLEM_DETECTED_RECENTLY.id(), HEARD_BELL_TIME.id()];
    let mut brain = Brain::construct(&types, &[], vec![], &MemoryMap::EMPTY, &mut r);
    let home = GlobalPos { dimension: Identifier::parse("minecraft:overworld").unwrap(), pos: BlockPos::new(1, -2, 3) };
    brain.set_memory(&HOME, home.clone());
    brain.set_memory_with_expiry(&LAST_SLEPT, 123_456_789_012i64, 40);
    brain.set_memory(&IS_IN_WATER, Unit);
    brain.set_memory_with_expiry(&GOLEM_DETECTED_RECENTLY, true, 599);
    brain.set_memory(&LIKED_PLAYER, uuid::Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef));
    brain.set_memory(&HEARD_BELL_TIME, 7i64); // no codec: not persisted
    let mut set = mc_game::world::entity::ai::memory::GlobalPosSet::new();
    set.insert(home.clone());
    brain.set_memory(&VISITED_BLOCK_POSITIONS, set);
    let nbt = brain.pack().to_nbt();
    let memories = nbt.get_compound("memories").expect("memories");
    assert_eq!(memories.len(), 6);
    assert!(memories.get("minecraft:heard_bell_time").is_none());
    let home_tag = memories.get_compound("minecraft:home").unwrap().get_compound("value").unwrap();
    assert_eq!(home_tag.get_string("dimension"), Some("minecraft:overworld"));
    assert_eq!(home_tag.get("pos"), Some(&Tag::IntArray(vec![1, -2, 3])));
    let slept = memories.get_compound("minecraft:last_slept").unwrap();
    assert_eq!((slept.get_long("value"), slept.get_long("ttl")), (Some(123_456_789_012), Some(40)));
    assert_eq!(memories.get_compound("minecraft:is_in_water").unwrap().get_compound("value").map(|c| c.len()), Some(0));
    assert_eq!(memories.get_compound("minecraft:golem_detected_recently").unwrap().get("value"), Some(&Tag::Byte(1)));
    assert!(matches!(memories.get_compound("minecraft:liked_player").unwrap().get("value"), Some(Tag::IntArray(a)) if a.len() == 4));

    let packed = BrainPacked::from_nbt(&nbt).expect("decodes");
    let mut loaded = Brain::construct(&types, &[], vec![], &packed.memories, &mut r);
    assert_eq!(loaded.get_memory(&HOME), Some(&home));
    assert_eq!(loaded.get_time_until_expiry(&LAST_SLEPT), 40);
    assert_eq!(loaded.get_memory(&LAST_SLEPT), Some(&123_456_789_012i64));
    assert!(loaded.has_memory_value(IS_IN_WATER) && !loaded.has_memory_value(HEARD_BELL_TIME));
    assert!(loaded.get_memory(&VISITED_BLOCK_POSITIONS).is_some_and(|s| s.contains(&home)));
    // malformed entries are dropped, the rest loads
    let mut broken = nbt.clone();
    broken.get_compound_mut("memories").unwrap().put("minecraft:home", mc_nbt::CompoundTag::new());
    broken.get_compound_mut("memories").unwrap().put("minecraft:path", mc_nbt::CompoundTag::new());
    let partial = BrainPacked::from_nbt(&broken).unwrap();
    let loaded = Brain::construct(&types, &[], vec![], &partial.memories, &mut r);
    assert!(!loaded.has_memory_value(HOME) && loaded.has_memory_value(LAST_SLEPT));
}
