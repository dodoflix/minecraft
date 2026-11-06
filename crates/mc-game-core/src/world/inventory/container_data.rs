//! `ContainerData`, `SimpleContainerData`, `DataSlot`.
use std::cell::RefCell;
use std::rc::Rc;

pub trait ContainerData {
    fn get(&self, data_id: usize) -> i32;
    fn set(&mut self, data_id: usize, value: i32);
    fn get_count(&self) -> usize;
}

pub struct SimpleContainerData {
    ints: Vec<i32>,
}

impl SimpleContainerData {
    pub fn new(count: usize) -> Self {
        Self {
            ints: vec![0; count],
        }
    }
}

impl ContainerData for SimpleContainerData {
    fn get(&self, data_id: usize) -> i32 {
        self.ints[data_id]
    }
    fn set(&mut self, data_id: usize, value: i32) {
        self.ints[data_id] = value;
    }
    fn get_count(&self) -> usize {
        self.ints.len()
    }
}

pub type SharedContainerData = Rc<RefCell<dyn ContainerData>>;

enum DataSlotKind {
    Container(SharedContainerData, usize),
    Shared(Rc<RefCell<Vec<i32>>>, usize),
    Standalone(i32),
    /// `dataAccess` int `id` of the block entity at `pos`; `cache` is refreshed by the menu before every read
    /// (`DataSlot::refresh`), since reading needs the player's level.
    BlockEntity {
        pos: mc_core::pos::BlockPos,
        id: usize,
        cache: i32,
    },
}

/// `DataSlot`: one synchronised int of a menu.
pub struct DataSlot {
    prev_value: i32,
    kind: DataSlotKind,
}

impl DataSlot {
    pub fn for_container(container: SharedContainerData, data_id: usize) -> Self {
        Self {
            prev_value: 0,
            kind: DataSlotKind::Container(container, data_id),
        }
    }
    /// `shared(int[] storage, index)`.
    pub fn shared(storage: Rc<RefCell<Vec<i32>>>, index: usize) -> Self {
        Self {
            prev_value: 0,
            kind: DataSlotKind::Shared(storage, index),
        }
    }
    pub fn for_block_entity(pos: mc_core::pos::BlockPos, id: usize) -> Self {
        Self {
            prev_value: 0,
            kind: DataSlotKind::BlockEntity { pos, id, cache: 0 },
        }
    }
    /// Re-reads a block-entity backed slot through the player's level (no-op for other kinds).
    pub fn refresh(&mut self, player: &dyn super::MenuPlayer) {
        use crate::world::level::BlockGetter;
        if let DataSlotKind::BlockEntity { pos, id, cache } = &mut self.kind
            && let Some(v) = player
                .level_ref()
                .and_then(|l| l.get_block_entity(*pos))
                .and_then(|be| be.container_data())
                .map(|d| d.get(*id))
        {
            *cache = v;
        }
    }
    pub fn standalone() -> Self {
        Self {
            prev_value: 0,
            kind: DataSlotKind::Standalone(0),
        }
    }
    pub fn get(&self) -> i32 {
        match &self.kind {
            DataSlotKind::Container(c, id) => c.borrow().get(*id),
            DataSlotKind::Shared(s, i) => s.borrow()[*i],
            DataSlotKind::Standalone(v) => *v,
            DataSlotKind::BlockEntity { cache, .. } => *cache,
        }
    }
    pub fn set(&mut self, value: i32) {
        match &mut self.kind {
            DataSlotKind::Container(c, id) => c.borrow_mut().set(*id, value),
            DataSlotKind::Shared(s, i) => s.borrow_mut()[*i] = value,
            DataSlotKind::Standalone(v) => *v = value,
            DataSlotKind::BlockEntity { cache, .. } => *cache = value,
        }
    }
    pub fn check_and_clear_update_flag(&mut self) -> bool {
        let current = self.get();
        let result = current != self.prev_value;
        self.prev_value = current;
        result
    }
}
