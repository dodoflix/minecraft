//! The pseudo-random block neighbourhood both block goldens (`block_behaviour_golden`, `block_placement_golden`) share with their
//! oracles (`BlockBehaviourOracle.java`, `BlockPlacementOracle.java`): the mixing function, the state pools and the cell function.
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};

/// The most states one pool block contributes to the global pool.
pub const POOL_CAP: usize = 24;
/// The most states the blocks of one class contribute to the `mates` pool.
pub const CLASS_CAP: usize = 200;

/// SplitMix64 finalizer.
pub fn mix(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// At most `cap` states of the block.
pub fn capped(block: Block, cap: usize) -> Vec<BlockState> {
    block.states().take(cap).collect()
}

/// The states a cell of the neighbourhood can hold: `global` (the shared pool file plus the extras), the `own` states of the block
/// under test and its class `mates`.
pub struct Pools {
    pub world_seed: u64,
    pub global: Vec<BlockState>,
    pub own: Vec<BlockState>,
    pub mates: Vec<BlockState>,
}

impl Pools {
    /// The global pool of `fixtures/block_behaviour_pool.txt` plus the `extra` blocks.
    pub fn new(extra: &[&str]) -> Self {
        let mut global = Vec::new();
        for name in include_str!("../../fixtures/block_behaviour_pool.txt")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .chain(extra.iter().copied())
        {
            global.extend(capped(
                Block::by_name(&format!("minecraft:{name}"))
                    .unwrap_or_else(|| panic!("pool block {name}")),
                POOL_CAP,
            ));
        }
        Self {
            world_seed: 0,
            global,
            own: Vec::new(),
            mates: Vec::new(),
        }
    }

    /// Selects the block under test: its own states and the states of the blocks of its class.
    pub fn select(&mut self, b: Block) {
        self.own = b.states().collect();
        let mut mates = Vec::new();
        for o in Block::iter() {
            if o.data().class == b.data().class && mates.len() < CLASS_CAP {
                let room = CLASS_CAP - mates.len();
                mates.extend(capped(o, room));
            }
        }
        self.mates = mates;
    }

    /// The state of the cell `(x, y, z)` in the world `world_seed`.
    pub fn cell_at(&self, x: i32, y: i32, z: i32) -> BlockState {
        let h = mix(self
            .world_seed
            .wrapping_mul(0x9E3779B97F4A7C15)
            .wrapping_add(
                (i64::from(x).wrapping_mul(73856093)
                    ^ i64::from(y).wrapping_mul(19349663)
                    ^ i64::from(z).wrapping_mul(83492791)) as u64,
            ));
        let r = h % 100;
        let h2 = mix(h);
        let pool = if r < 25 {
            return Block::AIR.default_state();
        } else if r < 55 {
            &self.global
        } else if r < 80 {
            &self.own
        } else {
            &self.mates
        };
        pool[(h2 % pool.len() as u64) as usize]
    }

    pub fn cell(&self, p: BlockPos) -> BlockState {
        self.cell_at(p.x(), p.y(), p.z())
    }
}
