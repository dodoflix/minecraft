//! `net.minecraft.util.worldupdate.UpgradeStatusTranslator`: the stage line the upgrade is in, as the translation key
//! of the `Component` vanilla picks. Vanilla's driver loop compares those components by reference; every key here stands
//! for exactly one component object, so the keys compare the same way.

use mc_datafix::DataFixTypes;
use mc_world::filefix::{Status, UpgradeProgress};

const FAILED: &str = "optimizeWorld.stage.failed";
const COUNTING: &str = "optimizeWorld.stage.counting";
const UPGRADING: &str = "optimizeWorld.stage.upgrading";

/// `UpgradeStatusTranslator.Messages`: `(upgrading, finished)` of the types that have their own lines.
fn messages(ty: DataFixTypes) -> Option<(&'static str, &'static str)> {
    match ty {
        DataFixTypes::CHUNK => Some((
            "optimizeWorld.stage.upgrading.chunks",
            "optimizeWorld.stage.finished.chunks",
        )),
        DataFixTypes::ENTITY_CHUNK => Some((
            "optimizeWorld.stage.upgrading.entities",
            "optimizeWorld.stage.finished.entities",
        )),
        DataFixTypes::POI_CHUNK => Some((
            "optimizeWorld.stage.upgrading.poi",
            "optimizeWorld.stage.finished.poi",
        )),
        _ => None,
    }
}

/// `translate(upgradeProgress)`: the translation key of the current stage.
pub fn translate(progress: &UpgradeProgress) -> &'static str {
    let status = progress.status();
    match status {
        Status::Failed => FAILED,
        Status::Counting => COUNTING,
        Status::Upgrading | Status::Finished => match progress.data_fix_type() {
            None => COUNTING,
            Some(ty) => match messages(ty) {
                None => UPGRADING,
                Some((upgrading, finished)) => {
                    if status == Status::Upgrading {
                        upgrading
                    } else {
                        finished
                    }
                }
            },
        },
    }
}

/// `Component.getString()` of a stage key in the server's `en_us` language.
pub fn text(key: &str) -> &'static str {
    mc_data::lang::translate(key).unwrap_or_else(|| panic!("missing translation {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_lines_follow_the_status_and_type() {
        let progress = UpgradeProgress::default();
        assert_eq!(text(translate(&progress)), "Counting chunks...");
        progress.reset(DataFixTypes::ENTITY_CHUNK);
        assert_eq!(text(translate(&progress)), "Counting chunks...");
        progress.set_status(Status::Upgrading);
        assert_eq!(text(translate(&progress)), "Upgrading all entities...");
        progress.set_status(Status::Finished);
        assert_eq!(
            text(translate(&progress)),
            "Finishing up upgrading entities..."
        );
        progress.reset(DataFixTypes::POI_CHUNK);
        assert_eq!(
            text(translate(&progress)),
            "Finishing up upgrading points of interest..."
        );
        progress.reset(DataFixTypes::STRUCTURE);
        assert_eq!(text(translate(&progress)), "Upgrading all chunks...");
        progress.set_status(Status::Failed);
        assert_eq!(text(translate(&progress)), "Failed! :(");
    }
}
