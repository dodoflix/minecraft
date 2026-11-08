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
use mc_game::world::level::{BlockGetter, LevelAccessor};
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
    fn update_shape(
        &self,
        state: BlockState,
        _l: &mut dyn LevelAccessor,
        pos: BlockPos,
        d: Direction,
        _n: BlockPos,
        _ns: BlockState,
    ) -> BlockState {
        log(format!("shape {} {}", pos.to_short_string(), d.get_name()));
        state
    }
    fn neighbor_changed(
        &self,
        _s: BlockState,
        _l: &mut ServerLevel,
        pos: BlockPos,
        b: Block,
        _o: Option<Orientation>,
        _p: bool,
    ) {
        log(format!(
            "neighbor {} {}",
            pos.to_short_string(),
            b.data().name
        ));
    }
    fn on_place(
        &self,
        _s: BlockState,
        _l: &mut ServerLevel,
        pos: BlockPos,
        old: BlockState,
        _p: bool,
    ) {
        log(format!(
            "place {} {}",
            pos.to_short_string(),
            old.block().data().name
        ));
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
        assert!(level.set_block(
            c.relative(d),
            rec,
            UPDATE_CLIENTS | UPDATE_SKIP_ON_PLACE | UPDATE_KNOWN_SHAPE,
            512
        ));
    }
    assert!(take().is_empty());
    assert!(level.set_block(c, stone, UPDATE_ALL, 512));
    let log = take();
    assert_eq!(
        log,
        [
            "neighbor 7, 64, 8 minecraft:air",
            "neighbor 9, 64, 8 minecraft:air",
            "neighbor 8, 63, 8 minecraft:air",
            "neighbor 8, 65, 8 minecraft:air",
            "neighbor 8, 64, 7 minecraft:air",
            "neighbor 8, 64, 9 minecraft:air",
            "shape 7, 64, 8 east",
            "shape 9, 64, 8 west",
            "shape 8, 64, 7 south",
            "shape 8, 64, 9 north",
            "shape 8, 63, 8 up",
            "shape 8, 65, 8 down",
        ]
    );
    assert_eq!(level.get_block_state(c), stone);
    // same state again: nothing changes
    assert!(!level.set_block(c, stone, UPDATE_ALL, 512));
    // clients only: no neighbor / shape updates; on_place still runs unless skipped
    assert!(level.set_block(c, rec, UPDATE_CLIENTS, 512));
    let log = take();
    assert_eq!(log[0], "place 8, 64, 8 minecraft:stone");
    assert!(
        log.iter().all(|l| !l.starts_with("neighbor")),
        "no UPDATE_NEIGHBORS: no neighborChanged"
    );
    assert_eq!(
        log.iter().filter(|l| l.starts_with("shape")).count(),
        6,
        "shape updates run unless UPDATE_KNOWN_SHAPE"
    );
    assert!(level.set_block(c, stone, UPDATE_CLIENTS | UPDATE_SKIP_ON_PLACE, 512));
    assert!(
        take().iter().all(|l| l.starts_with("shape")),
        "only shape updates"
    );
    // known shape: neighbors get `neighborChanged` but no shape updates
    assert!(level.set_block(
        c,
        rec,
        UPDATE_NEIGHBORS | UPDATE_KNOWN_SHAPE | UPDATE_SKIP_ON_PLACE,
        512
    ));
    assert_eq!(take().iter().filter(|l| l.starts_with("shape")).count(), 0);
    level.close();
}

/// A block entity is out of its chunk while its code runs (`with_block_entity`); vanilla keeps it in the chunk, so replacing
/// its block from inside that code must still create the new block's entity, drop the old one, and keep the live one for
/// a same-block state change.
#[test]
fn block_entity_replacing_its_own_block_gets_the_new_entity() {
    use mc_data::BlockEntityType;
    let mut level = level();
    let pos = BlockPos::new(8, 64, 8);
    let entity_type = |level: &ServerLevel| level.get_block_entity(pos).map(|be| be.get_type());
    assert!(level.set_block_and_update(pos, Block::BARREL.default_state()));
    assert_eq!(entity_type(&level), Some(BlockEntityType::BARREL));

    level.with_block_entity(pos, |level, _| {
        level.set_block_and_update(pos, Block::CHEST.default_state());
    });
    assert_eq!(level.get_block_state(pos), Block::CHEST.default_state());
    assert_eq!(entity_type(&level), Some(BlockEntityType::CHEST));

    let chest = Block::CHEST.default_state();
    let other = chest.set(mc_data::props::WATERLOGGED, true);
    // the loot table marks the live entity: a fresh one would not carry it
    let table = mc_core::ident::Identifier::parse("minecraft:chests/simple_dungeon").unwrap();
    level
        .get_block_entity_immediate(pos)
        .unwrap()
        .set_loot_table(table, 1);
    level.with_block_entity(pos, |level, _| {
        level.set_block_and_update(pos, other);
    });
    assert_eq!(entity_type(&level), Some(BlockEntityType::CHEST));
    assert!(level.get_block_entity(pos).unwrap().has_loot_table());

    level.with_block_entity(pos, |level, _| {
        level.set_block_and_update(pos, Block::STONE.default_state());
    });
    assert_eq!(entity_type(&level), None);
    level.close();
}

/// Actions a block entity queues while it is out of its chunk run when the call that took it out ends: the calls nested in
/// it run theirs first.
#[test]
fn deferred_block_entity_actions_run_when_their_call_ends() {
    use mc_game::world::block_entity::deferred;
    let mut level = level();
    let (a, b) = (BlockPos::new(8, 64, 8), BlockPos::new(9, 64, 8));
    level.set_block_and_update(a, Block::BARREL.default_state());
    level.set_block_and_update(b, Block::BARREL.default_state());
    level.with_block_entity(a, |level, _| {
        deferred::push(|_| log("outer".into()));
        level.with_block_entity(b, |_, _| {
            deferred::push(|_| log("inner".into()));
        });
        log("after inner call".into());
        deferred::push(|_| log("outer, later".into()));
    });
    assert_eq!(
        take(),
        ["inner", "after inner call", "outer", "outer, later"]
    );
    level.close();
}
