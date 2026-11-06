//! Pathfinder golden: `tests/oracle/pathfinder/NAME.txt` is replayed on real vanilla by `tests/oracle/PathfinderOracle.java`
//! (`scripts/entity-oracle.sh .../PathfinderOracle.java /abs/path/NAME.txt > tests/fixtures/pathfinder/NAME.txt`: the real
//! `mob.getNavigation().createPath(..)` of real vanilla mobs on terrains built with `setBlock`; per query every node with
//! g/h/f/type/walkedDistance/costMalus/closed, `canReach`, `getDistToTarget`, node count; plus the `PathType` of positions
//! through `getPathTypeStatic`, the level's `PathTypeCache` and the mob's evaluator) and, line by line, through our
//! `PathNavigationRegion` + `PathFinder` + node evaluators. The fixture's `MOB` lines carry the vanilla mob profile (box,
//! maluses, step height, fall distance, evaluator flags, ...), which [`TestMob`] replays; they are inputs, not compared.
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::EntityId;
use mc_game::world::entity::ai::navigation::{
    NavClass, NavigationData, create_path_with_finder, find_surface_position,
};
use mc_game::world::level::BlockGetter;
use mc_game::world::level::collision::CollisionContext;
use mc_game::world::level::pathfinder::NodeEvaluator;
use mc_game::world::level::pathfinder::pathfinding_context::PathfindingContext;
use mc_game::world::level::pathfinder::walk_node_evaluator::get_path_type_static;
use mc_game::world::level::pathfinder::{Path, PathEnv, PathType, PathfindingMob};

/// A vanilla mob as the fixture's `MOB` line describes it.
struct TestMob {
    id: EntityId,
    pos: Vec3,
    bb: AABB,
    width: f32,
    height: f32,
    ground: bool,
    water: bool,
    liquid: bool,
    floatable: bool,
    step: f32,
    fall: i32,
    passenger: bool,
    powder: bool,
    malus: [f32; PathType::COUNT],
    random: LegacyRandomSource,
}

impl PathfindingMob for TestMob {
    fn id(&self) -> EntityId {
        self.id
    }
    fn position(&self) -> Vec3 {
        self.pos
    }
    fn block_position(&self) -> BlockPos {
        BlockPos::containing(self.pos.x, self.pos.y, self.pos.z)
    }
    fn bounding_box(&self) -> AABB {
        self.bb
    }
    fn bb_width(&self) -> f32 {
        self.width
    }
    fn bb_height(&self) -> f32 {
        self.height
    }
    fn on_ground(&self) -> bool {
        self.ground
    }
    fn is_in_water(&self) -> bool {
        self.water
    }
    fn is_in_liquid(&self) -> bool {
        self.liquid
    }
    fn is_in_floatable_fluid(&self) -> bool {
        self.floatable
    }
    fn can_stand_on_fluid(&self, _fluid: FluidState) -> bool {
        false
    }
    fn get_pathfinding_malus(&self, ty: PathType) -> f32 {
        self.malus[ty.ordinal()]
    }
    fn set_pathfinding_malus(&mut self, ty: PathType, cost: f32) {
        self.malus[ty.ordinal()] = cost;
    }
    fn max_up_step(&self) -> f32 {
        self.step
    }
    fn get_max_fall_distance(&self) -> i32 {
        self.fall
    }
    fn on_pathfinding_start(&mut self) {}
    fn on_pathfinding_done(&mut self) {}
    fn random(&mut self) -> &mut dyn RandomSource {
        &mut self.random
    }
    fn collision_context(&self) -> CollisionContext {
        CollisionContext::of_entity(self.id, false, self.pos.y, None, None, false)
            .with_entity_state(0.0, false, self.powder)
    }
}

struct Mob {
    mob: TestMob,
    nav: NavigationData,
    class: NavClass,
    /// `getAttributeValue(FOLLOW_RANGE)`
    follow_value: f64,
}

fn mob_from_line(line: &str) -> Mob {
    let l = Line::parse(line);
    let pos = l.vec("pos");
    let bb = l.vec("bb");
    let malus: Vec<f32> = l
        .str("malus")
        .split(',')
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(malus.len(), PathType::COUNT);
    let class = match l.str("nav") {
        "GroundPathNavigation" => NavClass::Ground,
        "WallClimberNavigation" => NavClass::WallClimber,
        "WaterBoundPathNavigation" => NavClass::Water,
        "FlyingPathNavigation" => NavClass::Flying,
        "AmphibiousPathNavigation" => NavClass::Amphibious,
        other => panic!("navigation {other} is not a standard one"),
    };
    let mut nav = NavigationData::for_class(class, l.f64("follow"), l.str("type") == "dolphin");
    let e: Vec<bool> = l.str("eval").split(',').map(|v| v == "true").collect();
    let ev = nav.node_evaluator_mut();
    ev.set_can_pass_doors(e[0]);
    ev.set_can_open_doors(e[1]);
    ev.set_can_float(e[2]);
    ev.set_can_walk_over_fences(e[3]);
    let mob = TestMob {
        id: 1000,
        pos: Vec3::new(pos[0], pos[1], pos[2]),
        bb: AABB::new(bb[0], bb[1], bb[2], bb[3], bb[4], bb[5]),
        width: l.str("w").parse().unwrap(),
        height: l.str("h").parse().unwrap(),
        ground: l.str("ground") == "true",
        water: l.str("water") == "true",
        liquid: l.str("liquid") == "true",
        floatable: l.str("floatable") == "true",
        step: l.str("step").parse().unwrap(),
        fall: l.str("fall").parse().unwrap(),
        passenger: l.str("passenger") == "true",
        powder: l.fields.get("powder").is_some_and(|v| v == "true"),
        malus: malus.try_into().unwrap(),
        random: LegacyRandomSource::new(l.str("seed").parse().unwrap()),
    };
    nav.set_required_path_length(l.f64("followv"), l.str("reqlen").parse().unwrap());
    Mob {
        mob,
        nav,
        class,
        follow_value: l.f64("followv"),
    }
}

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    block
        .state_from_properties(
            props
                .split(',')
                .filter(|p| !p.is_empty())
                .map(|p| p.split_once('=').expect("prop=value")),
        )
        .unwrap_or_else(|| panic!("bad state {text}"))
}

struct Replay {
    level: ServerLevel,
    mobs: Vec<(String, Mob)>,
    mob_lines: Vec<String>,
    out: Vec<String>,
}

fn f32s(v: f32) -> String {
    format!("{v:?}")
}

impl Replay {
    fn new(fixture: &Fixture) -> Self {
        let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = common::load_world(
            dir.path(),
            mc_world::level_storage::WorldDataConfiguration::DEFAULT,
            true,
        )
        .expect("default world loads");
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = stem.registries.composite_access().clone();
        let mob_lines = fixture
            .lines
            .iter()
            .filter(|l| l.starts_with("MOB "))
            .cloned()
            .collect();
        Replay {
            level,
            mobs: Vec::new(),
            mob_lines,
            out: Vec::new(),
        }
    }

    fn set(&mut self, x: i32, y: i32, z: i32, state: BlockState) {
        use mc_game::world::level::LevelAccessor;
        self.level.set_block(
            BlockPos::new(x, y, z),
            state,
            mc_game::world::level::flags::UPDATE_CLIENTS
                | mc_game::world::level::flags::UPDATE_KNOWN_SHAPE,
            512,
        );
    }

    fn mob(&mut self, name: &str) -> &mut Mob {
        &mut self
            .mobs
            .iter_mut()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no mob {name}"))
            .1
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let int = |s: &str| {
            s.parse::<i32>()
                .unwrap_or_else(|_| panic!("int {s} in `{line}`"))
        };
        let num = |s: &str| {
            s.parse::<f64>()
                .unwrap_or_else(|_| panic!("number {s} in `{line}`"))
        };
        match t[0] {
            "fill" => {
                let state = parse_state(t[7]);
                let (a, b) = (
                    [int(t[1]), int(t[2]), int(t[3])],
                    [int(t[4]), int(t[5]), int(t[6])],
                );
                for x in a[0].min(b[0])..=a[0].max(b[0]) {
                    for y in a[1].min(b[1])..=a[1].max(b[1]) {
                        for z in a[2].min(b[2])..=a[2].max(b[2]) {
                            self.set(x, y, z, state);
                        }
                    }
                }
            }
            "block" => self.set(int(t[1]), int(t[2]), int(t[3]), parse_state(t[4])),
            "mob" => {
                let prefix = format!("MOB {} ", t[1]);
                let fixture_line = self
                    .mob_lines
                    .iter()
                    .find(|l| l.starts_with(&prefix))
                    .unwrap_or_else(|| panic!("fixture has no `{prefix}` line"))
                    .clone();
                self.mobs
                    .push((t[1].to_string(), mob_from_line(&fixture_line)));
            }
            "path" => {
                let (x, y, z, reach) = (num(t[2]), num(t[3]), num(t[4]), int(t[5]));
                let level = &self.level;
                let m = self
                    .mobs
                    .iter_mut()
                    .find(|(n, _)| n == t[1])
                    .map(|(_, m)| m)
                    .expect("mob");
                let path = create_path(level, m, x, y, z, reach);
                dump_path(&mut self.out, t[1], path, m);
            }
            "pathset" => {
                let reach = int(t[2]);
                let targets: Vec<BlockPos> = t[3..]
                    .chunks(3)
                    .map(|c| BlockPos::new(int(c[0]), int(c[1]), int(c[2])))
                    .collect();
                let level = &self.level;
                let m = self
                    .mobs
                    .iter_mut()
                    .find(|(n, _)| n == t[1])
                    .map(|(_, m)| m)
                    .expect("mob");
                let path = create_path_targets(level, m, &targets, reach);
                dump_path(&mut self.out, t[1], path, m);
            }
            "malus" => {
                let line = format!(
                    "MALUS {} {}",
                    t[1],
                    self.mob(t[1])
                        .mob
                        .malus
                        .iter()
                        .map(|v| f32s(*v))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                self.out.push(line);
            }
            "pathtype" => {
                let pos = BlockPos::new(int(t[2]), int(t[3]), int(t[4]));
                let level = &self.level;
                let m = self
                    .mobs
                    .iter_mut()
                    .find(|(n, _)| n == t[1])
                    .map(|(_, m)| m)
                    .expect("mob");
                let context = PathfindingContext::new(level, Some(level.path_type_cache()), &m.mob);
                let s = get_path_type_static(&context, pos);
                let c = level.path_type_cache().lock().get_or_compute(level, pos);
                let c2 = level.path_type_cache().lock().get_or_compute(level, pos);
                let mut env = PathEnv {
                    context,
                    mob: &mut m.mob,
                };
                let ev =
                    m.nav
                        .node_evaluator_mut()
                        .get_path_type(&mut env, pos.x(), pos.y(), pos.z());
                self.out.push(format!(
                    "PT {} {},{},{} static={} cache={} cache2={} evaluator={}",
                    t[1],
                    pos.x(),
                    pos.y(),
                    pos.z(),
                    s.name(),
                    c.name(),
                    c2.name(),
                    ev.name()
                ));
            }
            other => panic!("unknown scenario command {other}"),
        }
    }
}

/// `nav.createPath(x, y, z, reach)` of the standard navigation classes (the guards of `PathNavigation.createPath` + the
/// ground navigation's surface search), driving the real `PathFinder`.
fn create_path(
    level: &ServerLevel,
    m: &mut Mob,
    x: f64,
    y: f64,
    z: f64,
    reach: i32,
) -> Option<Path> {
    let mut pos = BlockPos::containing(x, y, z);
    if m.class.is_ground() {
        level.chunk_now(pos.x() >> 4, pos.z() >> 4)?;
        if !m.nav.can_path_to_targets_below_surface {
            pos = find_surface_position(level, pos);
        }
    }
    create_path_targets(level, m, &[pos], reach)
}

/// `PathNavigation.createPath(targets, 8, false, reachRange)`.
fn create_path_targets(
    level: &ServerLevel,
    m: &mut Mob,
    targets: &[BlockPos],
    reach: i32,
) -> Option<Path> {
    if targets.is_empty() || m.mob.pos.y < f64::from(level.get_min_y()) {
        return None;
    }
    let can_update = match m.class {
        NavClass::Ground | NavClass::WallClimber => m.mob.ground || m.mob.liquid || m.mob.passenger,
        NavClass::Water => m.nav.allow_breaching || m.mob.liquid,
        NavClass::Flying => m.nav.node_evaluator().can_float() && m.mob.liquid || !m.mob.passenger,
        NavClass::Amphibious => true,
    };
    if !can_update {
        return None;
    }
    if let Some(path) = &m.nav.path
        && !path.is_done()
        && m.nav.target_pos().is_some_and(|t| targets.contains(&t))
    {
        return Some(path.clone());
    }
    // `getMaxPathLength()`
    let max_path_length = m.nav.max_path_length(m.follow_value);
    create_path_with_finder(
        &mut m.nav,
        &mut m.mob,
        level,
        targets,
        8,
        false,
        reach,
        max_path_length,
        1.0,
    )
}

fn dump_path(out: &mut Vec<String>, name: &str, path: Option<Path>, m: &Mob) {
    let Some(p) = path else {
        out.push(format!("PATH {name} null"));
        return;
    };
    let t = p.get_target();
    let nt = m.nav.target_pos().map_or("null".to_string(), |t| {
        format!("{},{},{}", t.x(), t.y(), t.z())
    });
    out.push(format!(
        "PATH {name} reached={} count={} dist={} target={},{},{} next={} navtarget={nt}",
        p.can_reach(),
        p.get_node_count(),
        f32s(p.get_dist_to_target()),
        t.x(),
        t.y(),
        t.z(),
        p.get_next_node_index()
    ));
    for (i, n) in p.nodes().iter().enumerate() {
        out.push(format!(
            "N {i} {} {} {} g={} h={} f={} type={} walked={} malus={} closed={}",
            n.x,
            n.y,
            n.z,
            f32s(n.g),
            f32s(n.h),
            f32s(n.f),
            n.ty.name(),
            f32s(n.walked_distance),
            f32s(n.cost_malus),
            n.closed
        ));
    }
}

fn check(name: &str) {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/pathfinder/{name}.txt"));
    let fixture = Fixture::from_text(
        name,
        &std::fs::read_to_string(&fixture_path)
            .unwrap_or_else(|e| panic!("fixture {}: {e}", fixture_path.display())),
    );
    let text = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/oracle/pathfinder/{name}.txt")),
    )
    .unwrap();
    let mut r = Replay::new(&fixture);
    for line in text.lines() {
        r.run_line(line);
    }
    let opts = Options {
        ignore_kinds: vec!["MOB", "#!"],
        ..Default::default()
    };
    fixture.assert_matches(&r.out, &opts);
}

#[test]
fn basic() {
    check("basic");
}

#[test]
fn terrain_corridors() {
    check("terrain");
}

#[test]
fn mob_families() {
    check("mobs");
}
