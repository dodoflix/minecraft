//! `setBlock` flag semantics and neighbor/shape update order on a real (void) level.
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_game::register_block;
use mc_game::server::level::ServerLevel;
use mc_game::world::level::block::BlockBehaviour;
use mc_game::world::level::flags::*;
use mc_game::world::level::redstone::Orientation;
use mc_game::world::level::{LevelAccessor, BlockGetter};
use std::cell::RefCell;

thread_local! { static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
fn log(s: String) {
    LOG.with(|l| l.borrow_mut().push(s));
}
fn take() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

struct Recorder;
impl BlockBehaviour for Recorder {
    fn update_shape(&self, state: BlockState, _l: &mut dyn LevelAccessor, pos: BlockPos, d: Direction, _n: BlockPos, _ns: BlockState) -> BlockState {
        log(format!("shape {} {}", pos.to_short_string(), d.get_name()));
        state
    }
    fn neighbor_changed(&self, _s: BlockState, _l: &mut ServerLevel, pos: BlockPos, b: Block, _o: Option<Orientation>, _p: bool) {
        log(format!("neighbor {} {}", pos.to_short_string(), b.data().name));
    }
    fn on_place(&self, _s: BlockState, _l: &mut ServerLevel, pos: BlockPos, old: BlockState, _p: bool) {
        log(format!("place {} {}", pos.to_short_string(), old.block().data().name));
    }
}
register_block!("minecraft:glowstone", |_| Box::new(Recorder));

fn level() -> ServerLevel {
    ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap())
}

#[test]
fn set_block_flags_and_update_order() {
    let mut level = level();
    let c = BlockPos::new(8, 64, 8);
    let rec = Block::GLOWSTONE.default_state();
    let stone = Block::STONE.default_state();
    for d in Direction::ALL {
        assert!(level.set_block(c.relative(d), rec, UPDATE_CLIENTS | UPDATE_SKIP_ON_PLACE | UPDATE_KNOWN_SHAPE, 512));
    }
    assert!(take().is_empty());
    assert!(level.set_block(c, stone, UPDATE_ALL, 512));
    let log = take();
    assert_eq!(
        log,
        [
            "neighbor 7, 64, 8 minecraft:air", "neighbor 9, 64, 8 minecraft:air", "neighbor 8, 63, 8 minecraft:air", "neighbor 8, 65, 8 minecraft:air",
            "neighbor 8, 64, 7 minecraft:air", "neighbor 8, 64, 9 minecraft:air",
            "shape 7, 64, 8 east", "shape 9, 64, 8 west", "shape 8, 64, 7 south", "shape 8, 64, 9 north", "shape 8, 63, 8 up", "shape 8, 65, 8 down",
        ]
    );
    assert_eq!(level.get_block_state(c), stone);
    // same state again: nothing changes
    assert!(!level.set_block(c, stone, UPDATE_ALL, 512));
    // clients only: no neighbor / shape updates; on_place still runs unless skipped
    assert!(level.set_block(c, rec, UPDATE_CLIENTS, 512));
    let log = take();
    assert_eq!(log[0], "place 8, 64, 8 minecraft:stone");
    assert!(log.iter().all(|l| !l.starts_with("neighbor")), "no UPDATE_NEIGHBORS: no neighborChanged");
    assert_eq!(log.iter().filter(|l| l.starts_with("shape")).count(), 6, "shape updates run unless UPDATE_KNOWN_SHAPE");
    assert!(level.set_block(c, stone, UPDATE_CLIENTS | UPDATE_SKIP_ON_PLACE, 512));
    assert!(take().iter().all(|l| l.starts_with("shape")), "only shape updates");
    // known shape: neighbors get `neighborChanged` but no shape updates
    assert!(level.set_block(c, rec, UPDATE_NEIGHBORS | UPDATE_KNOWN_SHAPE | UPDATE_SKIP_ON_PLACE, 512));
    assert_eq!(take().iter().filter(|l| l.starts_with("shape")).count(), 0);
    level.close();
}
