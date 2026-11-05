//! `GameTestBatch`, `GameTestBatchListener` and `GameTestBatchFactory`.

use super::game_test_info::GameTestInfo;
use super::game_test_instance::TestHolder;
use super::retry_options::RetryOptions;
use super::test_environment_definition::EnvironmentRef;
use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use mc_core::rotation::Rotation;
use std::rc::Rc;

/// `MAX_TESTS_PER_BATCH`.
const MAX_TESTS_PER_BATCH: usize = 50;

pub struct GameTestBatch {
    pub index: i32,
    pub game_test_infos: Vec<Rc<GameTestInfo>>,
    pub environment: EnvironmentRef,
    pub dimension: DimensionKey,
}

impl GameTestBatch {
    /// `new GameTestBatch(index, infos, environment, dimension)`: a batch needs at least one test.
    pub fn new(index: i32, game_test_infos: Vec<Rc<GameTestInfo>>, environment: EnvironmentRef, dimension: DimensionKey) -> Self {
        assert!(!game_test_infos.is_empty(), "A GameTestBatch must include at least one GameTestInfo!");
        Self { index, game_test_infos, environment, dimension }
    }
}

/// `GameTestBatchListener`.
pub trait GameTestBatchListener {
    fn test_batch_starting(&self, server: &mut MinecraftServer, batch: &GameTestBatch);
    fn test_batch_finished(&self, server: &mut MinecraftServer, batch: &GameTestBatch);
}

/// `GameTestRunner.GameTestBatcher`.
pub type GameTestBatcher = Rc<dyn Fn(&[Rc<GameTestInfo>]) -> Vec<GameTestBatch>>;

/// `GameTestBatchFactory.fromGameTestInfo()`.
pub fn from_game_test_info() -> GameTestBatcher {
    from_game_test_info_with(MAX_TESTS_PER_BATCH)
}

/// `GameTestBatchFactory.fromGameTestInfo(maxTestsPerBatch)`: groups by (environment, dimension) in first-seen order
/// (vanilla groups through a `HashMap`, whose order is unspecified) and splits every group into batches.
pub fn from_game_test_info_with(max_tests_per_batch: usize) -> GameTestBatcher {
    Rc::new(move |infos: &[Rc<GameTestInfo>]| {
        let mut groups: Vec<(EnvironmentRef, DimensionKey, Vec<Rc<GameTestInfo>>)> = Vec::new();
        for info in infos {
            let test = info.get_test();
            let (environment, dimension) = (test.batch().clone(), test.info().dimension.clone());
            match groups.iter_mut().find(|(e, d, _)| *e == environment && *d == dimension) {
                Some((_, _, members)) => members.push(info.clone()),
                None => groups.push((environment, dimension, vec![info.clone()])),
            }
        }
        let mut batches = Vec::new();
        for (environment, dimension, members) in groups {
            for (index, tests) in members.chunks(max_tests_per_batch).enumerate() {
                batches.push(to_game_test_batch(tests.to_vec(), environment.clone(), index as i32, dimension.clone()));
            }
        }
        batches
    })
}

/// `GameTestBatchFactory.toGameTestBatch(tests, batch, counter, dimension)`.
pub fn to_game_test_batch(tests: Vec<Rc<GameTestInfo>>, environment: EnvironmentRef, counter: i32, dimension: DimensionKey) -> GameTestBatch {
    GameTestBatch::new(counter, tests, environment, dimension)
}

/// `new GameTestInfo(test, Rotation.NONE, level, RetryOptions.noRetries())` (`DIRECT` decorator).
pub fn direct(test: TestHolder, dimension: DimensionKey) -> Rc<GameTestInfo> {
    GameTestInfo::new(test, Rotation::None, dimension, RetryOptions::no_retries())
}
