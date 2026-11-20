//! `ReenableSpectatorsGenerateChunksInHardcoreWorldsFileFix`: a hardcore world that was ever opened in single player
//! gets the `spectators_generate_chunks` game rule back on.

use mc_datafix::References;

use crate::filefix::{FileFix, FileRelation, MissingSeverity, Type, only_file};

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    fix.add_file_content_fix(|ctx| {
        let level_dat = only_file(ctx.level_dat(&FileRelation::origin().for_file("level.dat")))?;
        let game_rules = ctx.saved_data(
            References::SAVED_DATA_GAME_RULES,
            &FileRelation::data().for_file("minecraft/game_rules.dat"),
            MissingSeverity::Neutral,
        );
        ctx.progress.set_type(Type::Files);
        let Some(data) = level_dat.read(ctx.tree)? else {
            return Ok(());
        };
        let was_ever_loaded_in_singleplayer = data.get("singleplayer_uuid").is_present();
        if was_ever_loaded_in_singleplayer {
            let hardcore = data
                .get("difficulty_settings")
                .or_else_empty_map()
                .get("hardcore")
                .as_boolean_or(false);
            if hardcore {
                let game_rules_file = only_file(game_rules)?;
                if let Some(data_tag) = game_rules_file.read(ctx.tree)? {
                    let fixed = data_tag.update("minecraft:spectators_generate_chunks", |old| {
                        old.create_boolean(true)
                    });
                    game_rules_file.write(ctx.tree, &fixed)?;
                }
            }
        }
        Ok(())
    });
    fix
}
