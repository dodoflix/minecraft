//! `TestFunctionLoader` / `BuiltinTestFunctions`: the `Consumer<GameTestHelper>` implementations of the `test_function`
//! registry. Loaders register through the [`TEST_FUNCTIONS`] distributed slice (vanilla: `TestFunctionLoader.registerLoader`).

use super::game_test_helper::{GameTestHelper, TestResult};
use mc_core::ident::Identifier;

/// A registered test function (`Consumer<GameTestHelper>`; it may throw).
pub type TestFunction = fn(&mut GameTestHelper<'_>) -> TestResult;

pub struct TestFunctionRegistration {
    /// The `minecraft:test_function` key.
    pub id: &'static str,
    pub run: TestFunction,
}

#[linkme::distributed_slice]
pub static TEST_FUNCTIONS: [TestFunctionRegistration];

/// The implementation registered under `id`.
pub fn find(id: &Identifier) -> Option<TestFunction> {
    let id = id.to_string();
    TEST_FUNCTIONS.iter().find(|f| f.id == id).map(|f| f.run)
}

/// `BuiltinTestFunctions.ALWAYS_PASS_INSTANCE` (`GameTestHelper::succeed`).
#[linkme::distributed_slice(TEST_FUNCTIONS)]
static ALWAYS_PASS: TestFunctionRegistration = TestFunctionRegistration {
    id: "minecraft:always_pass",
    run: |helper| {
        helper.succeed();
        Ok(())
    },
};
