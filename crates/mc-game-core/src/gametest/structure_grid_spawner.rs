//! `StructureGridSpawner`: lays the test structures out in rows.

use super::game_test_info::GameTestInfo;
use super::game_test_runner::StructureSpawner;
use super::structure_utils;
use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `SPACE_BETWEEN_COLUMNS` / `SPACE_BETWEEN_ROWS`.
const SPACE_BETWEEN_COLUMNS: i32 = 5;
const SPACE_BETWEEN_ROWS: i32 = 6;

/// `firstTestNorthWestCorner`: the first corner of the grid of a dimension.
pub type FirstCorner = Rc<dyn Fn(&mut MinecraftServer, &DimensionKey) -> BlockPos>;

struct DimensionGridState {
    first_test_north_west_corner: BlockPos,
    next_corner: BlockPos,
    row_bounds: AABB,
    current_row_count: i32,
    tests_in_last_batch: Vec<Rc<GameTestInfo>>,
}

pub struct StructureGridSpawner {
    tests_per_row: i32,
    first_test_north_west_corner: FirstCorner,
    clear_on_batch: bool,
    grids: RefCell<HashMap<DimensionKey, DimensionGridState>>,
}

impl StructureGridSpawner {
    pub fn new(first_test_north_west_corner: FirstCorner, tests_per_row: i32, clear_on_batch: bool) -> Rc<Self> {
        Rc::new(Self { tests_per_row, first_test_north_west_corner, clear_on_batch, grids: RefCell::new(HashMap::new()) })
    }
}

impl StructureSpawner for StructureGridSpawner {
    fn on_batch_start(&self, server: &mut MinecraftServer) {
        if !self.clear_on_batch {
            return;
        }
        for grid in self.grids.borrow_mut().values_mut() {
            for info in &grid.tests_in_last_batch {
                let Some(view) = info.get_test_instance_view(server) else { continue };
                if let Some(level) = server.level_mut(info.level()) {
                    structure_utils::clear_space_for_structure(level, &view.get_test_bounding_box());
                }
            }
            grid.tests_in_last_batch.clear();
            grid.next_corner = grid.first_test_north_west_corner;
            grid.row_bounds = AABB::from_block_pos(&grid.first_test_north_west_corner);
            grid.current_row_count = 0;
        }
    }

    fn spawn_structure(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>) -> Option<Rc<GameTestInfo>> {
        let dimension = test_info.get_test().info().dimension.clone();
        if !self.grids.borrow().contains_key(&dimension) {
            let start = (self.first_test_north_west_corner)(server, &dimension);
            self.grids.borrow_mut().insert(
                dimension.clone(),
                DimensionGridState {
                    first_test_north_west_corner: start,
                    next_corner: start,
                    row_bounds: AABB::from_block_pos(&start),
                    current_row_count: 0,
                    tests_in_last_batch: Vec::new(),
                },
            );
        }
        let north_west_corner = self.grids.borrow()[&dimension].next_corner;
        test_info.set_test_block_pos(Some(north_west_corner));
        let info_with_structure = test_info.prepare_test_structure(server)?;
        info_with_structure.start_execution(1);
        let structure_bounds = test_info.get_test_instance_view(server)?.get_test_bounds();
        let mut grids = self.grids.borrow_mut();
        let grid = grids.get_mut(&dimension).expect("inserted above");
        grid.row_bounds = grid.row_bounds.minmax(&structure_bounds);
        grid.next_corner = grid.next_corner.offset(structure_bounds.get_xsize() as i32 + SPACE_BETWEEN_COLUMNS, 0, 0);
        grid.current_row_count += 1;
        if grid.current_row_count >= self.tests_per_row {
            grid.current_row_count = 0;
            grid.next_corner = grid.next_corner.offset(0, 0, grid.row_bounds.get_zsize() as i32 + SPACE_BETWEEN_ROWS);
            grid.next_corner = BlockPos::new(grid.first_test_north_west_corner.x(), grid.next_corner.y(), grid.next_corner.z());
            grid.row_bounds = AABB::from_block_pos(&grid.next_corner);
        }
        grid.tests_in_last_batch.push(test_info.clone());
        Some(test_info.clone())
    }
}
