//! `GameTestListener`.

use super::game_test_info::GameTestInfo;
use super::game_test_runner::GameTestRunner;
use crate::server::MinecraftServer;
use std::rc::Rc;

pub trait GameTestListener {
    fn test_structure_loaded(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>);

    fn test_passed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, runner: &Rc<GameTestRunner>);

    fn test_failed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, runner: &Rc<GameTestRunner>);

    /// `listener` is this listener's own `Rc` (`ReportGameListener` adds itself to the copy).
    fn test_added_for_rerun(
        &self,
        server: &mut MinecraftServer,
        listener: &Rc<dyn GameTestListener>,
        original: &Rc<GameTestInfo>,
        copy: &Rc<GameTestInfo>,
        runner: &Rc<GameTestRunner>,
    );
}
