//! `WoodlandMansionPieces`: the woodland mansion layout (`MansionGrid`: three floors of corridors and rooms on an 11x11
//! grid, from the structure's random), its placement as template pieces (`MansionPiecePlacer`: entrance, outer walls, roofs,
//! carpets, indoor walls/doors, rooms) and the data markers of `WoodlandMansionPiece` (chests, evoker/vindicator/allay).

use super::structure_entities;
use crate::world::level::levelgen::structure::template_structure_piece::{
    TemplateStructurePieceData, mirror_legacy_name, read_mirror, read_rotation,
    rotation_legacy_name,
};
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::STRUCTURE_BLOCK;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_data::props;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext,
    StructurePieceType, create_chest_at,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::get_zero_position_with_transform;
use mc_game_core::world::level::levelgen::structure::templatesystem::{
    StructurePlaceSettings, StructureTemplateManager,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;

const WOODLAND_MANSION_LOOT: &str = "minecraft:chests/woodland_mansion";

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static WOODLAND_MANSION_PIECE: StructurePieceType = StructurePieceType {
    id: "wmp",
    load: load_piece,
};

fn load_piece(
    context: &StructurePieceSerializationContext<'_>,
    tag: &CompoundTag,
) -> Result<Box<dyn StructurePiece>, String> {
    Ok(Box::new(WoodlandMansionPiece::load(
        &context.env.structure_template_manager,
        tag,
    )?))
}

/// `generateMansion(manager, origin, rotation, pieces, random)`.
pub fn generate_mansion(
    structure_template_manager: &StructureTemplateManager,
    origin: BlockPos,
    rotation: Rotation,
    pieces: &mut Vec<WoodlandMansionPiece>,
    random: &mut dyn RandomSource,
) {
    let grid = MansionGrid::new(random);
    let mut placer = MansionPiecePlacer {
        structure_template_manager,
        start_x: 0,
        start_y: 0,
    };
    placer.create_mansion(origin, rotation, pieces, &grid, random);
}

/// `FloorRoomCollection`: the template names of the three floors (`ThirdFloorRoomCollection` = the second's).
#[derive(Clone, Copy)]
enum FloorRoomCollection {
    First,
    Second,
}

impl FloorRoomCollection {
    fn get_1x1(self, random: &mut dyn RandomSource) -> String {
        match self {
            FloorRoomCollection::First => format!("1x1_a{}", random.next_int_bounded(5) + 1),
            FloorRoomCollection::Second => format!("1x1_b{}", random.next_int_bounded(5) + 1),
        }
    }

    fn get_1x1_secret(self, random: &mut dyn RandomSource) -> String {
        format!("1x1_as{}", random.next_int_bounded(4) + 1)
    }

    fn get_1x2_side_entrance(self, random: &mut dyn RandomSource, is_stairs_room: bool) -> String {
        match self {
            FloorRoomCollection::First => format!("1x2_a{}", random.next_int_bounded(9) + 1),
            FloorRoomCollection::Second if is_stairs_room => "1x2_c_stairs".to_owned(),
            FloorRoomCollection::Second => format!("1x2_c{}", random.next_int_bounded(4) + 1),
        }
    }

    fn get_1x2_front_entrance(self, random: &mut dyn RandomSource, is_stairs_room: bool) -> String {
        match self {
            FloorRoomCollection::First => format!("1x2_b{}", random.next_int_bounded(5) + 1),
            FloorRoomCollection::Second if is_stairs_room => "1x2_d_stairs".to_owned(),
            FloorRoomCollection::Second => format!("1x2_d{}", random.next_int_bounded(5) + 1),
        }
    }

    fn get_1x2_secret(self, random: &mut dyn RandomSource) -> String {
        match self {
            FloorRoomCollection::First => format!("1x2_s{}", random.next_int_bounded(2) + 1),
            FloorRoomCollection::Second => format!("1x2_se{}", random.next_int_bounded(1) + 1),
        }
    }

    fn get_2x2(self, random: &mut dyn RandomSource) -> String {
        match self {
            FloorRoomCollection::First => format!("2x2_a{}", random.next_int_bounded(4) + 1),
            FloorRoomCollection::Second => format!("2x2_b{}", random.next_int_bounded(5) + 1),
        }
    }

    fn get_2x2_secret(self, _random: &mut dyn RandomSource) -> String {
        "2x2_s1".to_owned()
    }
}

/// `SimpleGrid`: `grid[x][y]`, `valueIfOutside` for reads outside.
struct SimpleGrid {
    grid: Vec<Vec<i32>>,
    width: i32,
    height: i32,
    value_if_outside: i32,
}

impl SimpleGrid {
    fn new(width: i32, height: i32, value_if_outside: i32) -> SimpleGrid {
        SimpleGrid {
            grid: vec![vec![0; height as usize]; width as usize],
            width,
            height,
            value_if_outside,
        }
    }

    fn set(&mut self, x: i32, y: i32, value: i32) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.grid[x as usize][y as usize] = value;
        }
    }

    fn set_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, value: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, value);
            }
        }
    }

    fn get(&self, x: i32, y: i32) -> i32 {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.grid[x as usize][y as usize]
        } else {
            self.value_if_outside
        }
    }

    fn setif(&mut self, x: i32, y: i32, if_value: i32, value: i32) {
        if self.get(x, y) == if_value {
            self.set(x, y, value);
        }
    }

    fn edges_to(&self, x: i32, y: i32, if_value: i32) -> bool {
        self.get(x - 1, y) == if_value
            || self.get(x + 1, y) == if_value
            || self.get(x, y + 1) == if_value
            || self.get(x, y - 1) == if_value
    }
}

const ROOM_1X1: i32 = 65536;
const ROOM_1X2: i32 = 131072;
const ROOM_2X2: i32 = 262144;
const ROOM_ORIGIN_FLAG: i32 = 1048576;
const ROOM_DOOR_FLAG: i32 = 2097152;
const ROOM_STAIRS_FLAG: i32 = 4194304;
const ROOM_CORRIDOR_FLAG: i32 = 8388608;
const ROOM_TYPE_MASK: i32 = 983040;
const ROOM_ID_MASK: i32 = 65535;

/// `MansionGrid`.
struct MansionGrid {
    base_grid: SimpleGrid,
    third_floor_grid: SimpleGrid,
    floor_rooms: [SimpleGrid; 3],
    entrance_x: i32,
    entrance_y: i32,
}

/// `Direction.getStepX()/getStepZ()` of a horizontal direction.
fn step(direction: Direction) -> (i32, i32) {
    (direction.get_step_x(), direction.get_step_z())
}

impl MansionGrid {
    fn new(random: &mut dyn RandomSource) -> MansionGrid {
        let (entrance_x, entrance_y) = (7, 4);
        let mut base_grid = SimpleGrid::new(11, 11, 5);
        base_grid.set_rect(entrance_x, entrance_y, entrance_x + 1, entrance_y + 1, 3);
        base_grid.set_rect(
            entrance_x - 1,
            entrance_y,
            entrance_x - 1,
            entrance_y + 1,
            2,
        );
        base_grid.set_rect(
            entrance_x + 2,
            entrance_y - 2,
            entrance_x + 3,
            entrance_y + 3,
            5,
        );
        base_grid.set_rect(
            entrance_x + 1,
            entrance_y - 2,
            entrance_x + 1,
            entrance_y - 1,
            1,
        );
        base_grid.set_rect(
            entrance_x + 1,
            entrance_y + 2,
            entrance_x + 1,
            entrance_y + 3,
            1,
        );
        base_grid.set(entrance_x - 1, entrance_y - 1, 1);
        base_grid.set(entrance_x - 1, entrance_y + 2, 1);
        base_grid.set_rect(0, 0, 11, 1, 5);
        base_grid.set_rect(0, 9, 11, 11, 5);
        recursive_corridor(
            random,
            &mut base_grid,
            entrance_x,
            entrance_y - 2,
            Direction::West,
            6,
        );
        recursive_corridor(
            random,
            &mut base_grid,
            entrance_x,
            entrance_y + 3,
            Direction::West,
            6,
        );
        recursive_corridor(
            random,
            &mut base_grid,
            entrance_x - 2,
            entrance_y - 1,
            Direction::West,
            3,
        );
        recursive_corridor(
            random,
            &mut base_grid,
            entrance_x - 2,
            entrance_y + 2,
            Direction::West,
            3,
        );
        while clean_edges(&mut base_grid) {}
        let mut floor_rooms = [
            SimpleGrid::new(11, 11, 5),
            SimpleGrid::new(11, 11, 5),
            SimpleGrid::new(11, 11, 5),
        ];
        identify_rooms(random, &base_grid, &mut floor_rooms[0]);
        identify_rooms(random, &base_grid, &mut floor_rooms[1]);
        floor_rooms[0].set_rect(
            entrance_x + 1,
            entrance_y,
            entrance_x + 1,
            entrance_y + 1,
            ROOM_CORRIDOR_FLAG,
        );
        floor_rooms[1].set_rect(
            entrance_x + 1,
            entrance_y,
            entrance_x + 1,
            entrance_y + 1,
            ROOM_CORRIDOR_FLAG,
        );
        let third_floor_grid = SimpleGrid::new(base_grid.width, base_grid.height, 5);
        let mut grid = MansionGrid {
            base_grid,
            third_floor_grid,
            floor_rooms,
            entrance_x,
            entrance_y,
        };
        grid.setup_third_floor(random);
        let MansionGrid {
            third_floor_grid,
            floor_rooms,
            ..
        } = &mut grid;
        identify_rooms(random, third_floor_grid, &mut floor_rooms[2]);
        grid
    }

    /// `isHouse(grid, x, y)`.
    fn is_house(grid: &SimpleGrid, x: i32, y: i32) -> bool {
        matches!(grid.get(x, y), 1..=4)
    }

    /// `isRoomId(grid, x, y, floor, roomId)`.
    fn is_room_id(&self, x: i32, y: i32, floor: usize, room_id: i32) -> bool {
        (self.floor_rooms[floor].get(x, y) & ROOM_ID_MASK) == room_id
    }

    /// `get1x2RoomDirection(grid, x, y, floorNum, roomId)`.
    fn get_1x2_room_direction(
        &self,
        x: i32,
        y: i32,
        floor_num: usize,
        room_id: i32,
    ) -> Option<Direction> {
        Direction::HORIZONTAL.into_iter().find(|&direction| {
            let (sx, sz) = step(direction);
            self.is_room_id(x + sx, y + sz, floor_num, room_id)
        })
    }

    /// `setupThirdFloor()`.
    fn setup_third_floor(&mut self, random: &mut dyn RandomSource) {
        let mut potential_rooms: Vec<(i32, i32)> = Vec::new();
        for y in 0..self.third_floor_grid.height {
            for x in 0..self.third_floor_grid.width {
                let room_data = self.floor_rooms[1].get(x, y);
                let room_type = room_data & ROOM_TYPE_MASK;
                if room_type == ROOM_1X2 && (room_data & ROOM_DOOR_FLAG) == ROOM_DOOR_FLAG {
                    potential_rooms.push((x, y));
                }
            }
        }
        if potential_rooms.is_empty() {
            let (w, h) = (self.third_floor_grid.width, self.third_floor_grid.height);
            self.third_floor_grid.set_rect(0, 0, w, h, 5);
            return;
        }
        let (room_x, room_y) =
            potential_rooms[random.next_int_bounded(potential_rooms.len() as i32) as usize];
        let room_data = self.floor_rooms[1].get(room_x, room_y);
        self.floor_rooms[1].set(room_x, room_y, room_data | ROOM_STAIRS_FLAG);
        let room_dir = self
            .get_1x2_room_direction(room_x, room_y, 1, room_data & ROOM_ID_MASK)
            .expect("a 1x2 room has a second half");
        let room_end_x = room_x + room_dir.get_step_x();
        let room_end_y = room_y + room_dir.get_step_z();
        for y in 0..self.third_floor_grid.height {
            for x in 0..self.third_floor_grid.width {
                if !Self::is_house(&self.base_grid, x, y) {
                    self.third_floor_grid.set(x, y, 5);
                } else if x == room_x && y == room_y {
                    self.third_floor_grid.set(x, y, 3);
                } else if x == room_end_x && y == room_end_y {
                    self.third_floor_grid.set(x, y, 3);
                    self.floor_rooms[2].set(x, y, ROOM_CORRIDOR_FLAG);
                }
            }
        }
        let mut potential_corridors: Vec<Direction> = Vec::new();
        for direction in Direction::HORIZONTAL {
            let (sx, sz) = step(direction);
            if self.third_floor_grid.get(room_end_x + sx, room_end_y + sz) == 0 {
                potential_corridors.push(direction);
            }
        }
        if potential_corridors.is_empty() {
            let (w, h) = (self.third_floor_grid.width, self.third_floor_grid.height);
            self.third_floor_grid.set_rect(0, 0, w, h, 5);
            self.floor_rooms[1].set(room_x, room_y, room_data);
        } else {
            let corridor_dir = potential_corridors
                [random.next_int_bounded(potential_corridors.len() as i32) as usize];
            let (sx, sz) = step(corridor_dir);
            recursive_corridor(
                random,
                &mut self.third_floor_grid,
                room_end_x + sx,
                room_end_y + sz,
                corridor_dir,
                4,
            );
            while clean_edges(&mut self.third_floor_grid) {}
        }
    }
}

/// `recursiveCorridor(grid, x, y, heading, depth)`.
fn recursive_corridor(
    random: &mut dyn RandomSource,
    grid: &mut SimpleGrid,
    x: i32,
    y: i32,
    heading: Direction,
    depth: i32,
) {
    if depth <= 0 {
        return;
    }
    let (hx, hz) = step(heading);
    grid.set(x, y, 1);
    grid.setif(x + hx, y + hz, 0, 1);
    for _attempt in 0..8 {
        let next_dir = Direction::from_2d_data_value(random.next_int_bounded(4));
        if next_dir != heading.get_opposite()
            && (next_dir != Direction::East || !random.next_boolean())
        {
            let (nx, nz) = (x + hx, y + hz);
            let (dx, dz) = step(next_dir);
            if grid.get(nx + dx, nz + dz) == 0 && grid.get(nx + dx * 2, nz + dz * 2) == 0 {
                recursive_corridor(random, grid, x + hx + dx, y + hz + dz, next_dir, depth - 1);
                break;
            }
        }
    }
    let cw = heading.get_clockwise();
    let ccw = heading.get_counter_clockwise();
    let ((cx, cz), (ax, az)) = (step(cw), step(ccw));
    grid.setif(x + cx, y + cz, 0, 2);
    grid.setif(x + ax, y + az, 0, 2);
    grid.setif(x + hx + cx, y + hz + cz, 0, 2);
    grid.setif(x + hx + ax, y + hz + az, 0, 2);
    grid.setif(x + hx * 2, y + hz * 2, 0, 2);
    grid.setif(x + cx * 2, y + cz * 2, 0, 2);
    grid.setif(x + ax * 2, y + az * 2, 0, 2);
}

/// `cleanEdges(grid)`.
fn clean_edges(grid: &mut SimpleGrid) -> bool {
    let mut touched = false;
    for y in 0..grid.height {
        for x in 0..grid.width {
            if grid.get(x, y) == 0 {
                let house =
                    |dx: i32, dy: i32| i32::from(MansionGrid::is_house(grid, x + dx, y + dy));
                let direct_neighbors = house(1, 0) + house(-1, 0) + house(0, 1) + house(0, -1);
                if direct_neighbors >= 3 {
                    grid.set(x, y, 2);
                    touched = true;
                } else if direct_neighbors == 2 {
                    let diagonal_neighbors =
                        house(1, 1) + house(-1, 1) + house(1, -1) + house(-1, -1);
                    if diagonal_neighbors <= 1 {
                        grid.set(x, y, 2);
                        touched = true;
                    }
                }
            }
        }
    }
    touched
}

/// `identifyRooms(fromGrid, roomGrid)`.
fn identify_rooms(
    random: &mut dyn RandomSource,
    from_grid: &SimpleGrid,
    room_grid: &mut SimpleGrid,
) {
    let mut room_pos: Vec<(i32, i32)> = Vec::new();
    for y in 0..from_grid.height {
        for x in 0..from_grid.width {
            if from_grid.get(x, y) == 2 {
                room_pos.push((x, y));
            }
        }
    }
    shuffle(&mut room_pos, random);
    let mut room_id = 10;
    for (xx, y) in room_pos {
        if room_grid.get(xx, y) != 0 {
            continue;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (xx, xx, y, y);
        let mut room_type = ROOM_1X1;
        if room_grid.get(xx + 1, y) == 0
            && room_grid.get(xx, y + 1) == 0
            && room_grid.get(xx + 1, y + 1) == 0
            && from_grid.get(xx + 1, y) == 2
            && from_grid.get(xx, y + 1) == 2
            && from_grid.get(xx + 1, y + 1) == 2
        {
            x1 = xx + 1;
            y1 = y + 1;
            room_type = ROOM_2X2;
        } else if room_grid.get(xx - 1, y) == 0
            && room_grid.get(xx, y + 1) == 0
            && room_grid.get(xx - 1, y + 1) == 0
            && from_grid.get(xx - 1, y) == 2
            && from_grid.get(xx, y + 1) == 2
            && from_grid.get(xx - 1, y + 1) == 2
        {
            x0 = xx - 1;
            y1 = y + 1;
            room_type = ROOM_2X2;
        } else if room_grid.get(xx - 1, y) == 0
            && room_grid.get(xx, y - 1) == 0
            && room_grid.get(xx - 1, y - 1) == 0
            && from_grid.get(xx - 1, y) == 2
            && from_grid.get(xx, y - 1) == 2
            && from_grid.get(xx - 1, y - 1) == 2
        {
            x0 = xx - 1;
            y0 = y - 1;
            room_type = ROOM_2X2;
        } else if room_grid.get(xx + 1, y) == 0 && from_grid.get(xx + 1, y) == 2 {
            x1 = xx + 1;
            room_type = ROOM_1X2;
        } else if room_grid.get(xx, y + 1) == 0 && from_grid.get(xx, y + 1) == 2 {
            y1 = y + 1;
            room_type = ROOM_1X2;
        } else if room_grid.get(xx - 1, y) == 0 && from_grid.get(xx - 1, y) == 2 {
            x0 = xx - 1;
            room_type = ROOM_1X2;
        } else if room_grid.get(xx, y - 1) == 0 && from_grid.get(xx, y - 1) == 2 {
            y0 = y - 1;
            room_type = ROOM_1X2;
        }
        let mut door_x = if random.next_boolean() { x0 } else { x1 };
        let mut door_y = if random.next_boolean() { y0 } else { y1 };
        let mut door_flag = ROOM_DOOR_FLAG;
        if !from_grid.edges_to(door_x, door_y, 1) {
            door_x = if door_x == x0 { x1 } else { x0 };
            door_y = if door_y == y0 { y1 } else { y0 };
            if !from_grid.edges_to(door_x, door_y, 1) {
                door_y = if door_y == y0 { y1 } else { y0 };
                if !from_grid.edges_to(door_x, door_y, 1) {
                    door_x = if door_x == x0 { x1 } else { x0 };
                    door_y = if door_y == y0 { y1 } else { y0 };
                    if !from_grid.edges_to(door_x, door_y, 1) {
                        door_flag = 0;
                        door_x = x0;
                        door_y = y0;
                    }
                }
            }
        }
        for ry in y0..=y1 {
            for rx in x0..=x1 {
                if rx == door_x && ry == door_y {
                    room_grid.set(rx, ry, ROOM_ORIGIN_FLAG | door_flag | room_type | room_id);
                } else {
                    room_grid.set(rx, ry, room_type | room_id);
                }
            }
        }
        room_id += 1;
    }
}

/// `PlacementData`.
struct PlacementData {
    rotation: Rotation,
    position: BlockPos,
    wall_type: &'static str,
}

/// `MansionPiecePlacer`.
struct MansionPiecePlacer<'a> {
    structure_template_manager: &'a StructureTemplateManager,
    start_x: i32,
    start_y: i32,
}

impl MansionPiecePlacer<'_> {
    fn piece(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        template_name: &str,
        position: BlockPos,
        rotation: Rotation,
    ) {
        pieces.push(WoodlandMansionPiece::new(
            self.structure_template_manager,
            template_name,
            position,
            rotation,
            Mirror::None,
        ));
    }

    fn piece_mirrored(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        template_name: &str,
        position: BlockPos,
        rotation: Rotation,
        mirror: Mirror,
    ) {
        pieces.push(WoodlandMansionPiece::new(
            self.structure_template_manager,
            template_name,
            position,
            rotation,
            mirror,
        ));
    }

    /// `createMansion(origin, rotation, pieces, mansion)`.
    fn create_mansion(
        &mut self,
        origin: BlockPos,
        rotation: Rotation,
        pieces: &mut Vec<WoodlandMansionPiece>,
        mansion: &MansionGrid,
        random: &mut dyn RandomSource,
    ) {
        let mut data = PlacementData {
            rotation,
            position: origin,
            wall_type: "wall_flat",
        };
        self.entrance(pieces, &mut data);
        let mut second_data = PlacementData {
            rotation: data.rotation,
            position: data.position.above_n(8),
            wall_type: "wall_window",
        };
        let base_grid = &mansion.base_grid;
        let third_grid = &mansion.third_floor_grid;
        self.start_x = mansion.entrance_x + 1;
        self.start_y = mansion.entrance_y + 1;
        let end_x = mansion.entrance_x + 1;
        let end_y = mansion.entrance_y;
        self.traverse_outer_walls(
            pieces,
            &mut data,
            base_grid,
            Direction::South,
            self.start_x,
            self.start_y,
            end_x,
            end_y,
        );
        self.traverse_outer_walls(
            pieces,
            &mut second_data,
            base_grid,
            Direction::South,
            self.start_x,
            self.start_y,
            end_x,
            end_y,
        );
        let mut third_data = PlacementData {
            rotation: data.rotation,
            position: data.position.above_n(19),
            wall_type: "wall_window",
        };
        let mut done = false;
        let mut y = 0;
        while y < third_grid.height && !done {
            let mut x = third_grid.width - 1;
            while x >= 0 && !done {
                if MansionGrid::is_house(third_grid, x, y) {
                    third_data.position = third_data.position.relative_n(
                        rotation.rotate(Direction::South),
                        8 + (y - self.start_y) * 8,
                    );
                    third_data.position = third_data
                        .position
                        .relative_n(rotation.rotate(Direction::East), (x - self.start_x) * 8);
                    self.traverse_wall_piece(pieces, &mut third_data);
                    self.traverse_outer_walls(
                        pieces,
                        &mut third_data,
                        third_grid,
                        Direction::South,
                        x,
                        y,
                        x,
                        y,
                    );
                    done = true;
                }
                x -= 1;
            }
            y += 1;
        }
        self.create_roof(
            pieces,
            origin.above_n(16),
            rotation,
            base_grid,
            Some(third_grid),
        );
        self.create_roof(pieces, origin.above_n(27), rotation, third_grid, None);
        let room_collections = [
            FloorRoomCollection::First,
            FloorRoomCollection::Second,
            FloorRoomCollection::Second,
        ];
        for (floor_num, &room_collection) in room_collections.iter().enumerate() {
            let floor_origin =
                origin.above_n(8 * floor_num as i32 + if floor_num == 2 { 3 } else { 0 });
            let rooms = &mansion.floor_rooms[floor_num];
            let grid = if floor_num == 2 {
                third_grid
            } else {
                base_grid
            };
            let south_piece = if floor_num == 0 {
                "carpet_south_1"
            } else {
                "carpet_south_2"
            };
            let west_piece = if floor_num == 0 {
                "carpet_west_1"
            } else {
                "carpet_west_2"
            };
            for y in 0..grid.height {
                for xx in 0..grid.width {
                    if grid.get(xx, y) == 1 {
                        let mut pos = floor_origin.relative_n(
                            rotation.rotate(Direction::South),
                            8 + (y - self.start_y) * 8,
                        );
                        pos = pos
                            .relative_n(rotation.rotate(Direction::East), (xx - self.start_x) * 8);
                        self.piece(pieces, "corridor_floor", pos, rotation);
                        if grid.get(xx, y - 1) == 1
                            || (rooms.get(xx, y - 1) & ROOM_CORRIDOR_FLAG) == ROOM_CORRIDOR_FLAG
                        {
                            self.piece(
                                pieces,
                                "carpet_north",
                                pos.relative_n(rotation.rotate(Direction::East), 1).above(),
                                rotation,
                            );
                        }
                        if grid.get(xx + 1, y) == 1
                            || (rooms.get(xx + 1, y) & ROOM_CORRIDOR_FLAG) == ROOM_CORRIDOR_FLAG
                        {
                            self.piece(
                                pieces,
                                "carpet_east",
                                pos.relative_n(rotation.rotate(Direction::South), 1)
                                    .relative_n(rotation.rotate(Direction::East), 5)
                                    .above(),
                                rotation,
                            );
                        }
                        if grid.get(xx, y + 1) == 1
                            || (rooms.get(xx, y + 1) & ROOM_CORRIDOR_FLAG) == ROOM_CORRIDOR_FLAG
                        {
                            self.piece(
                                pieces,
                                south_piece,
                                pos.relative_n(rotation.rotate(Direction::South), 5)
                                    .relative_n(rotation.rotate(Direction::West), 1),
                                rotation,
                            );
                        }
                        if grid.get(xx - 1, y) == 1
                            || (rooms.get(xx - 1, y) & ROOM_CORRIDOR_FLAG) == ROOM_CORRIDOR_FLAG
                        {
                            self.piece(
                                pieces,
                                west_piece,
                                pos.relative_n(rotation.rotate(Direction::West), 1)
                                    .relative_n(rotation.rotate(Direction::North), 1),
                                rotation,
                            );
                        }
                    }
                }
            }
            let wall_piece = if floor_num == 0 {
                "indoors_wall_1"
            } else {
                "indoors_wall_2"
            };
            let door_piece = if floor_num == 0 {
                "indoors_door_1"
            } else {
                "indoors_door_2"
            };
            let mut door_dirs: Vec<Direction> = Vec::new();
            for y in 0..grid.height {
                for xxx in 0..grid.width {
                    let mut third_floor_start_room = floor_num == 2 && grid.get(xxx, y) == 3;
                    if grid.get(xxx, y) == 2 || third_floor_start_room {
                        let room_data = rooms.get(xxx, y);
                        let room_type = room_data & ROOM_TYPE_MASK;
                        let room_id = room_data & ROOM_ID_MASK;
                        third_floor_start_room = third_floor_start_room
                            && (room_data & ROOM_CORRIDOR_FLAG) == ROOM_CORRIDOR_FLAG;
                        door_dirs.clear();
                        if (room_data & ROOM_DOOR_FLAG) == ROOM_DOOR_FLAG {
                            for direction in Direction::HORIZONTAL {
                                let (sx, sz) = step(direction);
                                if grid.get(xxx + sx, y + sz) == 1 {
                                    door_dirs.push(direction);
                                }
                            }
                        }
                        let door_dir: Option<Direction> = if !door_dirs.is_empty() {
                            Some(
                                door_dirs[random.next_int_bounded(door_dirs.len() as i32) as usize],
                            )
                        } else if (room_data & ROOM_ORIGIN_FLAG) == ROOM_ORIGIN_FLAG {
                            Some(Direction::Up)
                        } else {
                            None
                        };
                        let mut room_pos = floor_origin.relative_n(
                            rotation.rotate(Direction::South),
                            8 + (y - self.start_y) * 8,
                        );
                        room_pos = room_pos.relative_n(
                            rotation.rotate(Direction::East),
                            -1 + (xxx - self.start_x) * 8,
                        );
                        let pick = |direction: Direction| {
                            if door_dir == Some(direction) {
                                door_piece
                            } else {
                                wall_piece
                            }
                        };
                        if MansionGrid::is_house(grid, xxx - 1, y)
                            && !mansion.is_room_id(xxx - 1, y, floor_num, room_id)
                        {
                            self.piece(pieces, pick(Direction::West), room_pos, rotation);
                        }
                        if grid.get(xxx + 1, y) == 1 && !third_floor_start_room {
                            let pos = room_pos.relative_n(rotation.rotate(Direction::East), 8);
                            self.piece(pieces, pick(Direction::East), pos, rotation);
                        }
                        if MansionGrid::is_house(grid, xxx, y + 1)
                            && !mansion.is_room_id(xxx, y + 1, floor_num, room_id)
                        {
                            let mut pos = room_pos.relative_n(rotation.rotate(Direction::South), 7);
                            pos = pos.relative_n(rotation.rotate(Direction::East), 7);
                            self.piece(
                                pieces,
                                pick(Direction::South),
                                pos,
                                rotation.get_rotated(Rotation::Clockwise90),
                            );
                        }
                        if grid.get(xxx, y - 1) == 1 && !third_floor_start_room {
                            let mut pos = room_pos.relative_n(rotation.rotate(Direction::North), 1);
                            pos = pos.relative_n(rotation.rotate(Direction::East), 7);
                            self.piece(
                                pieces,
                                pick(Direction::North),
                                pos,
                                rotation.get_rotated(Rotation::Clockwise90),
                            );
                        }
                        if room_type == ROOM_1X1 {
                            self.add_room_1x1(
                                pieces,
                                room_pos,
                                rotation,
                                door_dir,
                                room_collection,
                                random,
                            );
                        } else if room_type == ROOM_1X2 && door_dir.is_some() {
                            let room_dir =
                                mansion.get_1x2_room_direction(xxx, y, floor_num, room_id);
                            let is_stairs_room = (room_data & ROOM_STAIRS_FLAG) == ROOM_STAIRS_FLAG;
                            self.add_room_1x2(
                                pieces,
                                room_pos,
                                rotation,
                                room_dir,
                                door_dir.expect("checked"),
                                room_collection,
                                is_stairs_room,
                                random,
                            );
                        } else if room_type == ROOM_2X2
                            && door_dir.is_some_and(|d| d != Direction::Up)
                        {
                            let door_dir = door_dir.expect("checked");
                            let mut room_dir = door_dir.get_clockwise();
                            let (sx, sz) = step(room_dir);
                            if !mansion.is_room_id(xxx + sx, y + sz, floor_num, room_id) {
                                room_dir = room_dir.get_opposite();
                            }
                            self.add_room_2x2(
                                pieces,
                                room_pos,
                                rotation,
                                room_dir,
                                door_dir,
                                room_collection,
                                random,
                            );
                        } else if room_type == ROOM_2X2 && door_dir == Some(Direction::Up) {
                            self.add_room_2x2_secret(
                                pieces,
                                room_pos,
                                rotation,
                                room_collection,
                                random,
                            );
                        }
                    }
                }
            }
        }
    }

    /// `traverseOuterWalls(pieces, data, grid, gridDirection, startX, startY, endX, endY)`.
    fn traverse_outer_walls(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        data: &mut PlacementData,
        grid: &SimpleGrid,
        mut grid_direction: Direction,
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
    ) {
        let (mut grid_x, mut grid_y) = (start_x, start_y);
        let start_direction = grid_direction;
        loop {
            let (sx, sz) = step(grid_direction);
            if !MansionGrid::is_house(grid, grid_x + sx, grid_y + sz) {
                self.traverse_turn(pieces, data);
                grid_direction = grid_direction.get_clockwise();
                if grid_x != end_x || grid_y != end_y || start_direction != grid_direction {
                    self.traverse_wall_piece(pieces, data);
                }
            } else {
                let (cx, cz) = step(grid_direction.get_counter_clockwise());
                if MansionGrid::is_house(grid, grid_x + sx, grid_y + sz)
                    && MansionGrid::is_house(grid, grid_x + sx + cx, grid_y + sz + cz)
                {
                    self.traverse_inner_turn(data);
                    grid_x += sx;
                    grid_y += sz;
                    grid_direction = grid_direction.get_counter_clockwise();
                } else {
                    grid_x += sx;
                    grid_y += sz;
                    if grid_x != end_x || grid_y != end_y || start_direction != grid_direction {
                        self.traverse_wall_piece(pieces, data);
                    }
                }
            }
            if grid_x == end_x && grid_y == end_y && start_direction == grid_direction {
                break;
            }
        }
    }

    /// `createRoof(pieces, roofOrigin, rotation, grid, aboveGrid)`.
    fn create_roof(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        roof_origin: BlockPos,
        rotation: Rotation,
        grid: &SimpleGrid,
        above_grid: Option<&SimpleGrid>,
    ) {
        let south = rotation.rotate(Direction::South);
        let east = rotation.rotate(Direction::East);
        let west = rotation.rotate(Direction::West);
        let north = rotation.rotate(Direction::North);
        let origin_of = |x: i32, y: i32| {
            roof_origin
                .relative_n(south, 8 + (y - self.start_y) * 8)
                .relative_n(east, (x - self.start_x) * 8)
        };
        let is_house = |x: i32, y: i32| MansionGrid::is_house(grid, x, y);
        for y in 0..grid.height {
            for x in 0..grid.width {
                let position = origin_of(x, y);
                let is_above = above_grid.is_some_and(|a| MansionGrid::is_house(a, x, y));
                if is_house(x, y) && !is_above {
                    self.piece(pieces, "roof", position.above_n(3), rotation);
                    if !is_house(x + 1, y) {
                        let p2 = position.relative_n(east, 6);
                        self.piece(pieces, "roof_front", p2, rotation);
                    }
                    if !is_house(x - 1, y) {
                        let mut p2 = position.relative_n(east, 0);
                        p2 = p2.relative_n(south, 7);
                        self.piece(
                            pieces,
                            "roof_front",
                            p2,
                            rotation.get_rotated(Rotation::Clockwise180),
                        );
                    }
                    if !is_house(x, y - 1) {
                        let p2 = position.relative_n(west, 1);
                        self.piece(
                            pieces,
                            "roof_front",
                            p2,
                            rotation.get_rotated(Rotation::Counterclockwise90),
                        );
                    }
                    if !is_house(x, y + 1) {
                        let mut p2 = position.relative_n(east, 6);
                        p2 = p2.relative_n(south, 6);
                        self.piece(
                            pieces,
                            "roof_front",
                            p2,
                            rotation.get_rotated(Rotation::Clockwise90),
                        );
                    }
                }
            }
        }
        if let Some(above_grid) = above_grid {
            for y in 0..grid.height {
                for xx in 0..grid.width {
                    let pos = origin_of(xx, y);
                    let is_above = MansionGrid::is_house(above_grid, xx, y);
                    if is_house(xx, y) && is_above {
                        if !is_house(xx + 1, y) {
                            let p2 = pos.relative_n(east, 7);
                            self.piece(pieces, "small_wall", p2, rotation);
                        }
                        if !is_house(xx - 1, y) {
                            let mut p2 = pos.relative_n(west, 1);
                            p2 = p2.relative_n(south, 6);
                            self.piece(
                                pieces,
                                "small_wall",
                                p2,
                                rotation.get_rotated(Rotation::Clockwise180),
                            );
                        }
                        if !is_house(xx, y - 1) {
                            let mut p2 = pos.relative_n(west, 0);
                            p2 = p2.relative_n(north, 1);
                            self.piece(
                                pieces,
                                "small_wall",
                                p2,
                                rotation.get_rotated(Rotation::Counterclockwise90),
                            );
                        }
                        if !is_house(xx, y + 1) {
                            let mut p2 = pos.relative_n(east, 6);
                            p2 = p2.relative_n(south, 7);
                            self.piece(
                                pieces,
                                "small_wall",
                                p2,
                                rotation.get_rotated(Rotation::Clockwise90),
                            );
                        }
                        if !is_house(xx + 1, y) {
                            if !is_house(xx, y - 1) {
                                let mut p2 = pos.relative_n(east, 7);
                                p2 = p2.relative_n(north, 2);
                                self.piece(pieces, "small_wall_corner", p2, rotation);
                            }
                            if !is_house(xx, y + 1) {
                                let mut p2 = pos.relative_n(east, 8);
                                p2 = p2.relative_n(south, 7);
                                self.piece(
                                    pieces,
                                    "small_wall_corner",
                                    p2,
                                    rotation.get_rotated(Rotation::Clockwise90),
                                );
                            }
                        }
                        if !is_house(xx - 1, y) {
                            if !is_house(xx, y - 1) {
                                let mut p2 = pos.relative_n(west, 2);
                                p2 = p2.relative_n(north, 1);
                                self.piece(
                                    pieces,
                                    "small_wall_corner",
                                    p2,
                                    rotation.get_rotated(Rotation::Counterclockwise90),
                                );
                            }
                            if !is_house(xx, y + 1) {
                                let mut p2 = pos.relative_n(west, 1);
                                p2 = p2.relative_n(south, 8);
                                self.piece(
                                    pieces,
                                    "small_wall_corner",
                                    p2,
                                    rotation.get_rotated(Rotation::Clockwise180),
                                );
                            }
                        }
                    }
                }
            }
        }
        for y in 0..grid.height {
            for xxx in 0..grid.width {
                let pos = origin_of(xxx, y);
                let is_above = above_grid.is_some_and(|a| MansionGrid::is_house(a, xxx, y));
                if is_house(xxx, y) && !is_above {
                    if !is_house(xxx + 1, y) {
                        let p2 = pos.relative_n(east, 6);
                        if !is_house(xxx, y + 1) {
                            let p3 = p2.relative_n(south, 6);
                            self.piece(pieces, "roof_corner", p3, rotation);
                        } else if is_house(xxx + 1, y + 1) {
                            let p3 = p2.relative_n(south, 5);
                            self.piece(pieces, "roof_inner_corner", p3, rotation);
                        }
                        if !is_house(xxx, y - 1) {
                            self.piece(
                                pieces,
                                "roof_corner",
                                p2,
                                rotation.get_rotated(Rotation::Counterclockwise90),
                            );
                        } else if is_house(xxx + 1, y - 1) {
                            let mut p3 = pos.relative_n(east, 9);
                            p3 = p3.relative_n(north, 2);
                            self.piece(
                                pieces,
                                "roof_inner_corner",
                                p3,
                                rotation.get_rotated(Rotation::Clockwise90),
                            );
                        }
                    }
                    if !is_house(xxx - 1, y) {
                        let mut p2 = pos.relative_n(east, 0);
                        p2 = p2.relative_n(south, 0);
                        if !is_house(xxx, y + 1) {
                            let p3 = p2.relative_n(south, 6);
                            self.piece(
                                pieces,
                                "roof_corner",
                                p3,
                                rotation.get_rotated(Rotation::Clockwise90),
                            );
                        } else if is_house(xxx - 1, y + 1) {
                            let mut p3 = p2.relative_n(south, 8);
                            p3 = p3.relative_n(west, 3);
                            self.piece(
                                pieces,
                                "roof_inner_corner",
                                p3,
                                rotation.get_rotated(Rotation::Counterclockwise90),
                            );
                        }
                        if !is_house(xxx, y - 1) {
                            self.piece(
                                pieces,
                                "roof_corner",
                                p2,
                                rotation.get_rotated(Rotation::Clockwise180),
                            );
                        } else if is_house(xxx - 1, y - 1) {
                            let p3 = p2.relative_n(south, 1);
                            self.piece(
                                pieces,
                                "roof_inner_corner",
                                p3,
                                rotation.get_rotated(Rotation::Clockwise180),
                            );
                        }
                    }
                }
            }
        }
    }

    /// `entrance(pieces, data)`.
    fn entrance(&self, pieces: &mut Vec<WoodlandMansionPiece>, data: &mut PlacementData) {
        let west = data.rotation.rotate(Direction::West);
        self.piece(
            pieces,
            "entrance",
            data.position.relative_n(west, 9),
            data.rotation,
        );
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::South), 16);
    }

    /// `traverseWallPiece(pieces, data)`.
    fn traverse_wall_piece(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        data: &mut PlacementData,
    ) {
        self.piece(
            pieces,
            data.wall_type,
            data.position
                .relative_n(data.rotation.rotate(Direction::East), 7),
            data.rotation,
        );
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::South), 8);
    }

    /// `traverseTurn(pieces, data)`.
    fn traverse_turn(&self, pieces: &mut Vec<WoodlandMansionPiece>, data: &mut PlacementData) {
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::South), -1);
        self.piece(pieces, "wall_corner", data.position, data.rotation);
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::South), -7);
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::West), -6);
        data.rotation = data.rotation.get_rotated(Rotation::Clockwise90);
    }

    /// `traverseInnerTurn(pieces, data)`.
    fn traverse_inner_turn(&self, data: &mut PlacementData) {
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::South), 6);
        data.position = data
            .position
            .relative_n(data.rotation.rotate(Direction::East), 8);
        data.rotation = data.rotation.get_rotated(Rotation::Counterclockwise90);
    }

    /// `addRoom1x1(pieces, roomPos, rotation, doorDir, rooms)`.
    fn add_room_1x1(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        room_pos: BlockPos,
        rotation: Rotation,
        door_dir: Option<Direction>,
        rooms: FloorRoomCollection,
        random: &mut dyn RandomSource,
    ) {
        let mut piece_rot = Rotation::None;
        let mut room_type = rooms.get_1x1(random);
        if door_dir != Some(Direction::East) {
            match door_dir {
                Some(Direction::North) => {
                    piece_rot = piece_rot.get_rotated(Rotation::Counterclockwise90)
                }
                Some(Direction::West) => piece_rot = piece_rot.get_rotated(Rotation::Clockwise180),
                Some(Direction::South) => piece_rot = piece_rot.get_rotated(Rotation::Clockwise90),
                _ => room_type = rooms.get_1x1_secret(random),
            }
        }
        let mut orientation =
            get_zero_position_with_transform(BlockPos::new(1, 0, 0), Mirror::None, piece_rot, 7, 7);
        piece_rot = piece_rot.get_rotated(rotation);
        orientation = orientation.rotate(rotation);
        let pos = room_pos.offset(orientation.x(), 0, orientation.z());
        self.piece(pieces, &room_type, pos, piece_rot);
    }

    /// `addRoom1x2(pieces, roomPos, rotation, roomDir, doorDir, rooms, isStairsRoom)`.
    fn add_room_1x2(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        room_pos: BlockPos,
        rotation: Rotation,
        room_dir: Option<Direction>,
        door_dir: Direction,
        rooms: FloorRoomCollection,
        is_stairs_room: bool,
        random: &mut dyn RandomSource,
    ) {
        use Direction::{East, North, South, Up, West};
        let (east, south, west, north) = (
            rotation.rotate(East),
            rotation.rotate(South),
            rotation.rotate(West),
            rotation.rotate(North),
        );
        let Some(room_dir) = room_dir else { return };
        match (door_dir, room_dir) {
            (East, South) => {
                let pos = room_pos.relative_n(east, 1);
                self.piece(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation,
                );
            }
            (East, North) => {
                let mut pos = room_pos.relative_n(east, 1);
                pos = pos.relative_n(south, 6);
                self.piece_mirrored(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation,
                    Mirror::LeftRight,
                );
            }
            (West, North) => {
                let mut pos = room_pos.relative_n(east, 7);
                pos = pos.relative_n(south, 6);
                self.piece(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise180),
                );
            }
            (West, South) => {
                let pos = room_pos.relative_n(east, 7);
                self.piece_mirrored(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation,
                    Mirror::FrontBack,
                );
            }
            (South, East) => {
                let pos = room_pos.relative_n(east, 1);
                self.piece_mirrored(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise90),
                    Mirror::LeftRight,
                );
            }
            (South, West) => {
                let pos = room_pos.relative_n(east, 7);
                self.piece(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise90),
                );
            }
            (North, West) => {
                let mut pos = room_pos.relative_n(east, 7);
                pos = pos.relative_n(south, 6);
                self.piece_mirrored(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise90),
                    Mirror::FrontBack,
                );
            }
            (North, East) => {
                let mut pos = room_pos.relative_n(east, 1);
                pos = pos.relative_n(south, 6);
                self.piece(
                    pieces,
                    &rooms.get_1x2_side_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Counterclockwise90),
                );
            }
            (South, North) => {
                let mut pos = room_pos.relative_n(east, 1);
                pos = pos.relative_n(north, 8);
                self.piece(
                    pieces,
                    &rooms.get_1x2_front_entrance(random, is_stairs_room),
                    pos,
                    rotation,
                );
            }
            (North, South) => {
                let mut pos = room_pos.relative_n(east, 7);
                pos = pos.relative_n(south, 14);
                self.piece(
                    pieces,
                    &rooms.get_1x2_front_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise180),
                );
            }
            (West, East) => {
                let pos = room_pos.relative_n(east, 15);
                self.piece(
                    pieces,
                    &rooms.get_1x2_front_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise90),
                );
            }
            (East, West) => {
                let mut pos = room_pos.relative_n(west, 7);
                pos = pos.relative_n(south, 6);
                self.piece(
                    pieces,
                    &rooms.get_1x2_front_entrance(random, is_stairs_room),
                    pos,
                    rotation.get_rotated(Rotation::Counterclockwise90),
                );
            }
            (Up, East) => {
                let pos = room_pos.relative_n(east, 15);
                self.piece(
                    pieces,
                    &rooms.get_1x2_secret(random),
                    pos,
                    rotation.get_rotated(Rotation::Clockwise90),
                );
            }
            (Up, South) => {
                let mut pos = room_pos.relative_n(east, 1);
                pos = pos.relative_n(north, 0);
                self.piece(pieces, &rooms.get_1x2_secret(random), pos, rotation);
            }
            _ => {}
        }
    }

    /// `addRoom2x2(pieces, roomPos, rotation, roomDir, doorDir, rooms)`.
    fn add_room_2x2(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        room_pos: BlockPos,
        rotation: Rotation,
        room_dir: Direction,
        door_dir: Direction,
        rooms: FloorRoomCollection,
        random: &mut dyn RandomSource,
    ) {
        use Direction::{East, North, South, West};
        let (mut east, mut south) = (0, 0);
        let mut rot = rotation;
        let mut mirror = Mirror::None;
        match (door_dir, room_dir) {
            (East, South) => east = -7,
            (East, North) => {
                east = -7;
                south = 6;
                mirror = Mirror::LeftRight;
            }
            (North, East) => {
                east = 1;
                south = 14;
                rot = rotation.get_rotated(Rotation::Counterclockwise90);
            }
            (North, West) => {
                east = 7;
                south = 14;
                rot = rotation.get_rotated(Rotation::Counterclockwise90);
                mirror = Mirror::LeftRight;
            }
            (South, West) => {
                east = 7;
                south = -8;
                rot = rotation.get_rotated(Rotation::Clockwise90);
            }
            (South, East) => {
                east = 1;
                south = -8;
                rot = rotation.get_rotated(Rotation::Clockwise90);
                mirror = Mirror::LeftRight;
            }
            (West, North) => {
                east = 15;
                south = 6;
                rot = rotation.get_rotated(Rotation::Clockwise180);
            }
            (West, South) => {
                east = 15;
                mirror = Mirror::FrontBack;
            }
            _ => {}
        }
        let mut pos = room_pos.relative_n(rotation.rotate(East), east);
        pos = pos.relative_n(rotation.rotate(South), south);
        self.piece_mirrored(pieces, &rooms.get_2x2(random), pos, rot, mirror);
    }

    /// `addRoom2x2Secret(pieces, roomPos, rotation, rooms)`.
    fn add_room_2x2_secret(
        &self,
        pieces: &mut Vec<WoodlandMansionPiece>,
        room_pos: BlockPos,
        rotation: Rotation,
        rooms: FloorRoomCollection,
        random: &mut dyn RandomSource,
    ) {
        let pos = room_pos.relative_n(rotation.rotate(Direction::East), 1);
        self.piece_mirrored(
            pieces,
            &rooms.get_2x2_secret(random),
            pos,
            rotation,
            Mirror::None,
        );
    }
}

/// `WoodlandMansionPieces.WoodlandMansionPiece`.
pub struct WoodlandMansionPiece {
    data: TemplateStructurePieceData,
}

impl WoodlandMansionPiece {
    /// `WoodlandMansionPiece(manager, templateName, position, rotation, mirror)`.
    pub fn new(
        structure_template_manager: &StructureTemplateManager,
        template_name: &str,
        position: BlockPos,
        rotation: Rotation,
        mirror: Mirror,
    ) -> WoodlandMansionPiece {
        WoodlandMansionPiece {
            data: TemplateStructurePieceData::new(
                &WOODLAND_MANSION_PIECE,
                0,
                structure_template_manager,
                &make_location(template_name),
                template_name.to_owned(),
                make_settings(mirror, rotation),
                position,
            ),
        }
    }

    /// `WoodlandMansionPiece(manager, tag)`.
    pub fn load(
        structure_template_manager: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<WoodlandMansionPiece, String> {
        let data = TemplateStructurePieceData::load_with_location(
            &WOODLAND_MANSION_PIECE,
            tag,
            structure_template_manager,
            |name| Ok(make_location(name)),
            |_| {
                Ok(make_settings(
                    read_mirror(tag, "Mi")?,
                    read_rotation(tag, "Rot")?,
                ))
            },
        )?;
        Ok(WoodlandMansionPiece { data })
    }
}

/// `makeLocation(templateName)`.
fn make_location(template_name: &str) -> Identifier {
    Identifier::with_default_namespace(&format!("woodland_mansion/{template_name}"))
        .expect("valid woodland mansion template id")
}

/// `makeSettings(mirror, rotation)`.
fn make_settings(mirror: Mirror, rotation: Rotation) -> StructurePlaceSettings {
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_ignore_entities(true)
        .set_rotation(rotation)
        .set_mirror(mirror)
        .add_processor(STRUCTURE_BLOCK.clone());
    settings
}

impl StructurePiece for WoodlandMansionPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(
        &self,
        context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.data.add_additional_save_data(context, tag);
        tag.put_string(
            "Rot",
            rotation_legacy_name(self.data.place_settings.get_rotation()),
        );
        tag.put_string(
            "Mi",
            mirror_legacy_name(self.data.place_settings.get_mirror()),
        );
    }

    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.data.move_by(dx, dy, dz);
    }

    fn get_rotation(&self) -> Rotation {
        self.data.get_rotation()
    }

    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        reference_pos: BlockPos,
    ) {
        let rotation = self.data.place_settings.get_rotation();
        self.data.post_process(
            level,
            random,
            chunk_bb,
            reference_pos,
            &mut |marker_id, position, level, random, chunk_bb| {
                handle_data_marker(rotation, marker_id, position, level, random, chunk_bb)
            },
        );
    }
}

/// `WoodlandMansionPiece.handleDataMarker(markerId, position, level, random, chunkBB)`.
fn handle_data_marker(
    rotation: Rotation,
    marker_id: &str,
    position: BlockPos,
    level: &mut dyn WorldGenLevel,
    random: &mut dyn RandomSource,
    chunk_bb: &BoundingBox,
) {
    if marker_id.starts_with("Chest") {
        let mut chest_state = Block::CHEST.default_state();
        let facing = match marker_id {
            "ChestWest" => Some(Direction::West),
            "ChestEast" => Some(Direction::East),
            "ChestSouth" => Some(Direction::South),
            "ChestNorth" => Some(Direction::North),
            _ => None,
        };
        if let Some(facing) = facing {
            chest_state = chest_state.set(props::HORIZONTAL_FACING, rotation.rotate(facing));
        }
        create_chest_at(
            level,
            chunk_bb,
            random,
            position,
            WOODLAND_MANSION_LOOT,
            Some(chest_state),
        );
        return;
    }
    // `mobs`: the entity kind and how many (`Group of Allays` draws the count from the level's random first)
    let count = match marker_id {
        "Mage" | "Warrior" => 1,
        "Group of Allays" => level.random().next_int_bounded(3) + 1,
        _ => return,
    };
    for _ in 0..count {
        let tag = match marker_id {
            "Mage" => structure_entities::evoker(level.random(), position),
            "Warrior" => {
                let special_multiplier = level
                    .get_current_difficulty_at(position)
                    .get_special_multiplier();
                structure_entities::vindicator(level.random(), position, special_multiplier)
            }
            _ => structure_entities::allay(level.random(), position),
        };
        level.add_fresh_entity_nbt(tag);
        level.set_block_flags(position, Block::AIR.default_state(), 2);
    }
}
