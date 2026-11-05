//! Boats vs the real vanilla classes (`tests/oracle/scenarios/boat_*.txt`, fixtures `tests/fixtures/entity_oracle/boat_*.txt`):
//! status detection (in air / in water / under water / on land), `floatBoat` buoyancy and friction (water, land, ice), bubble columns,
//! rafts and chest boats, per tick. The replay understands the subset of the driver DSL the boat scenarios use.
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource as _};
use mc_data::EntityType;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::vehicle::boat::abstract_boat::{AbstractBoat, Status, boat_spec};
use mc_game::world::entity::vehicle::vehicle_entity;
use mc_game::world::entity::{Entity, EntityId};
use mc_game::world::level::LevelAccessor;

fn registries() -> mc_game::registry::RegistryAccess {
    static ACCESS: std::sync::LazyLock<mc_game::registry::RegistryAccess> = std::sync::LazyLock::new(|| {
        let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (_, stem) = common::load_world(dir.path(), mc_world::level_storage::WorldDataConfiguration::DEFAULT, true).expect("default world loads");
        stem.registries.composite_access().clone()
    });
    ACCESS.clone()
}
fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    block.state_from_properties(props.split(',').filter(|p| !p.is_empty()).map(|p| p.split_once('=').expect("prop=value"))).unwrap_or_else(|| panic!("bad state {text}"))
}

struct Replay {
    level: ServerLevel,
    refs: Vec<(String, EntityId)>,
    watched: Vec<String>,
    out: Vec<String>,
    tick: u64,
}

fn status_name(status: Option<Status>) -> &'static str {
    match status {
        None => "-",
        Some(Status::InWater) => "in_water",
        Some(Status::UnderWater) => "under_water",
        Some(Status::UnderFlowingWater) => "under_flowing_water",
        Some(Status::OnLand) => "on_land",
        Some(Status::InAir) => "in_air",
    }
}

impl Replay {
    fn new() -> Self {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = registries();
        Replay { level, refs: Vec::new(), watched: Vec::new(), out: Vec::new(), tick: 0 }
    }

    fn set(&mut self, x: i32, y: i32, z: i32, state: BlockState) {
        self.level.set_block_and_update(BlockPos::new(x, y, z), state);
    }

    fn fill(&mut self, a: [i32; 3], b: [i32; 3], state: BlockState) {
        for x in a[0].min(b[0])..=a[0].max(b[0]) {
            for y in a[1].min(b[1])..=a[1].max(b[1]) {
                for z in a[2].min(b[2])..=a[2].max(b[2]) {
                    self.set(x, y, z, state);
                }
            }
        }
    }

    fn flat_world(&mut self) {
        let (bedrock, dirt, grass) = (Block::BEDROCK.default_state(), Block::DIRT.default_state(), Block::GRASS_BLOCK.default_state());
        self.fill([-5, -64, -5], [60, -64, 20], bedrock);
        self.fill([-5, -63, -5], [60, -62, 20], dirt);
        self.fill([-5, -61, -5], [60, -61, 20], grass);
    }

    fn id(&self, name: &str) -> EntityId {
        self.refs.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("unknown ref {name}")).1
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let num = |s: &str| s.parse::<f64>().unwrap_or_else(|_| panic!("number {s} in `{line}`"));
        match t[0] {
            "level" => self.flat_world(),
            "difficulty" => {}
            "level_seed" => LevelAccessor::random(&mut self.level).set_seed(num(t[1]) as i64),
            "fill" => self.fill([num(t[1]) as i32, num(t[2]) as i32, num(t[3]) as i32], [num(t[4]) as i32, num(t[5]) as i32, num(t[6]) as i32], parse_state(t[7])),
            "block" => self.set(num(t[1]) as i32, num(t[2]) as i32, num(t[3]) as i32, parse_state(t[4])),
            "spawn" => {
                let ty = EntityType::by_name(t[2]).unwrap_or_else(|| panic!("entity type {}", t[2]));
                let (item, kind, chest) = boat_spec(ty).expect("only boats are replayed");
                let mut boat = AbstractBoat::new(ty, self.level.next_entity_id(), item, kind, chest);
                boat.snap_to(num(t[3]), num(t[4]), num(t[5]), num(t[6]) as f32, num(t[7]) as f32);
                self.refs.push((t[1].to_string(), boat.id()));
                self.level.add_fresh_entity(Box::new(boat));
            }
            "entity_seed" => {
                let id = self.id(t[1]);
                self.level.with_entity(id, |_, e| e.base_mut().random = LegacyRandomSource::new(num(t[2]) as i64));
            }
            "velocity" => {
                let id = self.id(t[1]);
                self.level.with_entity(id, |_, e| e.base_mut().set_delta_movement_xyz(num(t[2]), num(t[3]), num(t[4])));
            }
            "watch" => self.watched = t[1..].iter().filter(|w| **w != "off").map(|w| w.strip_prefix("entity:").expect("entity watch").to_string()).collect(),
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| num(n) as usize) {
                    self.tick_once();
                }
            }
            other => panic!("unsupported scenario command `{other}`"),
        }
    }

    fn tick_once(&mut self) {
        for (_, id) in self.refs.clone() {
            self.level.with_entity(id, |level, e| {
                if !e.is_removed() {
                    mc_game::world::entity::entity::common_tick(e);
                    e.tick(level);
                }
            });
        }
        self.tick += 1;
        self.out.push(format!("T {}", self.tick));
        for name in self.watched.clone() {
            let id = self.id(&name);
            if let Some(e) = self.level.get_entity(id) {
                self.out.push(describe(&name, e));
            }
        }
    }
}

fn describe(name: &str, e: &dyn Entity) -> String {
    let b = e.base();
    let (p, d) = (b.position(), b.delta_movement());
    let boat = e.as_any().downcast_ref::<AbstractBoat>().expect("a boat");
    let mut s = format!(
        "E {name} {} pos={:?},{:?},{:?} delta={:?},{:?},{:?} rot={:?},{:?} og={} fall={:?} fire={} air={} alive={} pose=standing status={} dmg={:?} hurt={}/{} bubble={}/{} paddle={}{} pass={}",
        e.get_type().identifier(),
        p.x,
        p.y,
        p.z,
        d.x,
        d.y,
        d.z,
        b.y_rot(),
        b.x_rot(),
        i32::from(b.on_ground()),
        b.fall_distance,
        b.remaining_fire_ticks,
        b.get_air_supply(),
        i32::from(e.is_alive()),
        status_name(boat.get_status_value()),
        vehicle_entity::get_damage(b),
        vehicle_entity::get_hurt_time(b),
        vehicle_entity::get_hurt_dir(b),
        boat.is_above_bubble_column(),
        boat.get_bubble_time(),
        i32::from(boat.get_paddle_state(0)),
        i32::from(boat.get_paddle_state(1)),
        if b.passengers.is_empty() { "-".to_string() } else { b.passengers.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(";") },
    );
    if let Some(chest) = e.chest_vehicle() {
        use mc_game::world::container::Container;
        let items: Vec<String> = (0..chest.get_container_size()).filter(|&i| !chest.get_item(i).is_empty()).map(|i| format!("{i}:{}*{}", chest.get_item(i).item().name(), chest.get_item(i).count())).collect();
        s.push_str(&format!(" items={}", if items.is_empty() { "-".to_string() } else { items.join(";") }));
    }
    s
}

fn replay(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(scenario_dir().join(format!("{name}.txt"))).unwrap();
    let mut r = Replay::new();
    for line in text.lines() {
        r.run_line(line);
    }
    r.out
}

#[test]
fn boat_physics_matches_vanilla() {
    Fixture::load("boat_physics").assert_matches(&replay("boat_physics"), &Options::default());
}
