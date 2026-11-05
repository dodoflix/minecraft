//! `TestFinder` (+ `TestInstanceFinder`, `TestPosFinder`): which tests / test blocks a `/test` subcommand works on.

use super::failed_test_tracker;
use super::game_test_instance::{TestHolder, lookup_test};
use super::player_view::PlayerView;
use super::structure_utils;
use crate::commands::CommandSourceStack;
use crate::commands::arguments::resource_arguments::ResourceKey;
use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use mc_core::pos::BlockPos;

/// `GlobalPos`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GlobalPos {
    pub dimension: DimensionKey,
    pub pos: BlockPos,
}

/// `TestPosFinder`, evaluated on every [`TestFinder::find_test_pos`] call like vanilla's lazy supplier.
type PosFinder = Box<dyn Fn(&mut MinecraftServer) -> Vec<GlobalPos>>;

pub struct TestFinder {
    source: CommandSourceStack,
    tests: Vec<TestHolder>,
    test_pos_finder: PosFinder,
}

impl TestFinder {
    pub fn source(&self) -> &CommandSourceStack {
        &self.source
    }

    /// `findTestPos()`.
    pub fn find_test_pos(&self, server: &mut MinecraftServer) -> Vec<GlobalPos> {
        (self.test_pos_finder)(server)
    }

    /// `findTests()`.
    pub fn find_tests(&self) -> Vec<TestHolder> {
        self.tests.clone()
    }

    pub fn builder() -> Builder {
        Builder { copies: None }
    }
}

/// `TestFinder.Builder`.
pub struct Builder {
    /// `createMultipleCopies(amount)`.
    copies: Option<i32>,
}

/// `createCopies(amount)`.
fn repeat<Q: Clone>(source: Vec<Q>, amount: i32) -> Vec<Q> {
    let mut copy_list = Vec::new();
    for _ in 0..amount {
        copy_list.extend(source.iter().cloned());
    }
    copy_list
}

impl Builder {
    /// `createMultipleCopies(amount)`.
    pub fn create_multiple_copies(self, amount: i32) -> Builder {
        Builder { copies: Some(amount) }
    }

    fn build(&self, source: &CommandSourceStack, tests: Vec<TestHolder>, test_pos_finder: PosFinder) -> TestFinder {
        match self.copies {
            None => TestFinder { source: source.clone(), tests, test_pos_finder },
            Some(amount) => TestFinder {
                source: source.clone(),
                tests: repeat(tests, amount),
                test_pos_finder: Box::new(move |server| repeat(test_pos_finder(server), amount)),
            },
        }
    }

    /// `radius(sourceStack, radius)`.
    pub fn radius(&self, source: &CommandSourceStack, radius: i32) -> TestFinder {
        let pos = BlockPos::containing_position(&source.get_position());
        let dimension = source.get_level().clone();
        self.build(
            source,
            Vec::new(),
            Box::new(move |server| {
                structure_utils::find_test_blocks(server, &dimension, pos, radius).into_iter().map(|p| GlobalPos { dimension: dimension.clone(), pos: p }).collect()
            }),
        )
    }

    /// `nearest(sourceStack)`.
    pub fn nearest(&self, source: &CommandSourceStack) -> TestFinder {
        let pos = BlockPos::containing_position(&source.get_position());
        let dimension = source.get_level().clone();
        self.build(
            source,
            Vec::new(),
            Box::new(move |server| {
                structure_utils::find_nearest_test(server, &dimension, pos, 15).map(|p| GlobalPos { dimension: dimension.clone(), pos: p }).into_iter().collect()
            }),
        )
    }

    /// `allNearby(sourceStack)`.
    pub fn all_nearby(&self, source: &CommandSourceStack) -> TestFinder {
        self.radius(source, 250)
    }

    /// `lookedAt(sourceStack)`: `source.getPlayer().getCamera()` is a null dereference for a source that is no player.
    pub fn looked_at(&self, source: &CommandSourceStack) -> TestFinder {
        let pos = BlockPos::containing_position(&source.get_position());
        let dimension = source.get_level().clone();
        let source_for_view = source.clone();
        self.build(
            source,
            Vec::new(),
            Box::new(move |server| {
                let view = PlayerView::of(server, &source_for_view).expect("the source of /test runthat is a player");
                structure_utils::looked_at_test_pos(server, &dimension, pos, view.eye, view.look)
                    .into_iter()
                    .map(|p| GlobalPos { dimension: dimension.clone(), pos: p })
                    .collect()
            }),
        )
    }

    /// `failedTests(sourceStack, onlyRequiredTests)`.
    pub fn failed_tests(&self, source: &CommandSourceStack, only_required_tests: bool) -> TestFinder {
        let tests = failed_test_tracker::get_last_failed_tests().into_iter().filter(|test| !only_required_tests || test.value.required()).collect();
        self.build(source, tests, Box::new(|_| Vec::new()))
    }

    /// `byResourceSelection(sourceStack, holders)`: the selected `test_instance` registry keys.
    pub fn by_resource_selection(&self, source: &CommandSourceStack, keys: &[ResourceKey]) -> TestFinder {
        let tests = source.with_server(|server| {
            let registries = server.registries.composite_access();
            keys.iter().filter_map(|key| lookup_test(registries, &key.id)).collect()
        });
        self.build(source, tests, Box::new(|_| Vec::new()))
    }
}
