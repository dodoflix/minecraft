//! `LevelDatToSavedDataFileFix`: the parts of level.dat that are saved data files from now on move out of it.

use mc_datafix::{DataFixTypes, References, TypeReference, data_fixer};
use mc_dfu::Dynamic;

use crate::filefix::{
    ContentContext, FileFix, FileFixError, FileRelation, MissingSeverity, Type, only_file,
};

/// `FALLBACK_SINGLE_PLAYER_UUID`: `Util.NIL_UUID`.
const FALLBACK_SINGLE_PLAYER_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// A saved data file at `relation`, read and written at the version of the fix.
fn saved_data(
    ctx: &ContentContext<'_>,
    ty: TypeReference,
    relation: FileRelation,
) -> Result<crate::filefix::SavedDataNbt, FileFixError> {
    only_file(ctx.saved_data(ty, &relation, MissingSeverity::Neutral))
}

/// `extractToFile`: the value of `key` becomes the content of `target`.
fn extract_to_file(
    ctx: &mut ContentContext<'_>,
    target: &crate::filefix::SavedDataNbt,
    content: Dynamic,
    key: &str,
) -> Result<Dynamic, FileFixError> {
    match content.get(key).result() {
        None => Ok(content),
        Some(tag) => {
            target.write(ctx.tree, &tag)?;
            Ok(content.remove(key))
        }
    }
}

/// `extractWorldBorderToFile`.
fn extract_world_border_to_file(
    ctx: &mut ContentContext<'_>,
    target: &crate::filefix::SavedDataNbt,
    content: &Dynamic,
    divider: f64,
) -> Result<(), FileFixError> {
    if let Some(world_border_tag) = content.get("world_border").result() {
        let world_border_tag = world_border_tag
            .update("center_x", |x| {
                x.create_double(x.as_double_or(0.0) / divider)
            })
            .update("center_z", |z| {
                z.create_double(z.as_double_or(0.0) / divider)
            });
        target.write(ctx.tree, &world_border_tag)?;
    }
    Ok(())
}

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    fix.add_file_content_fix(|ctx| {
        let level_dat = only_file(ctx.level_dat(&FileRelation::origin().for_file("level.dat")))?;
        let dragon_fight = saved_data(
            ctx,
            References::SAVED_DATA_ENDER_DRAGON_FIGHT,
            FileRelation::for_data_file_in_dimension("the_end", "minecraft/ender_dragon_fight.dat"),
        )?;
        let fallback_player_data = only_file(ctx.player_data(
            &FileRelation::player_data().for_file(&format!("{FALLBACK_SINGLE_PLAYER_UUID}.dat")),
        ))?;
        let data = |ty, file: &str| saved_data(ctx, ty, FileRelation::data().for_file(file));
        let wandering_trader = data(
            References::SAVED_DATA_WANDERING_TRADER,
            "minecraft/wandering_trader.dat",
        )?;
        let custom_boss_events = data(
            References::SAVED_DATA_CUSTOM_BOSS_EVENTS,
            "minecraft/custom_boss_events.dat",
        )?;
        let weather_data = data(References::SAVED_DATA_WEATHER, "minecraft/weather.dat")?;
        let scheduled_events = data(
            References::SAVED_DATA_SCHEDULED_EVENTS,
            "minecraft/scheduled_events.dat",
        )?;
        let border = |dimension| {
            saved_data(
                ctx,
                References::SAVED_DATA_WORLD_BORDER,
                FileRelation::for_data_file_in_dimension(dimension, "minecraft/world_border.dat"),
            )
        };
        let world_border_overworld = border("overworld")?;
        let world_border_nether = border("the_nether")?;
        let world_border_end = border("the_end")?;
        let game_rules = data(
            References::SAVED_DATA_GAME_RULES,
            "minecraft/game_rules.dat",
        )?;
        let world_gen_settings = data(
            References::SAVED_DATA_WORLD_GEN_SETTINGS,
            "minecraft/world_gen_settings.dat",
        )?;
        let world_clocks = data(
            References::SAVED_DATA_WORLD_CLOCKS,
            "minecraft/world_clocks.dat",
        )?;

        ctx.progress.set_type(Type::Files);
        let Some(mut content) = level_dat.read(ctx.tree)? else {
            return Ok(());
        };
        content = extract_to_file(ctx, &dragon_fight, content, "dragon_fight")?;
        content = extract_player_data_to_file(ctx, &fallback_player_data, content)?;
        content = extract_to_file(
            ctx,
            &wandering_trader,
            content,
            "wandering_trader_migration_data",
        )?;
        content = extract_to_file(ctx, &custom_boss_events, content, "CustomBossEvents")?;
        content = extract_to_file(ctx, &weather_data, content, "weather_data")?;
        content = extract_to_file(ctx, &scheduled_events, content, "scheduled_events")?;
        extract_world_border_to_file(ctx, &world_border_overworld, &content, 1.0)?;
        extract_world_border_to_file(ctx, &world_border_nether, &content, 8.0)?;
        extract_world_border_to_file(ctx, &world_border_end, &content, 1.0)?;
        content = content.remove("world_border");
        content = extract_to_file(ctx, &game_rules, content, "game_rules")?;
        content = extract_world_gen_settings_to_file(ctx, &world_gen_settings, content)?;
        content = extract_to_file(ctx, &world_clocks, content, "world_clocks")?;
        level_dat.write(ctx.tree, &content)?;
        Ok(())
    });
    fix
}

/// `extractPlayerDataToFile`: the single player's data moves to the file of the player's UUID, or to the fallback
/// file when it has none; level.dat keeps only the UUID.
fn extract_player_data_to_file(
    ctx: &mut ContentContext<'_>,
    fallback_file: &crate::filefix::PlayerData,
    content: Dynamic,
) -> Result<Dynamic, FileFixError> {
    let Some(player_tag) = content.get("Player").result() else {
        return Ok(content);
    };
    let data_version = player_tag.get("DataVersion").as_int_or(-1);
    let player_tag_fixed =
        DataFixTypes::PLAYER.update(data_fixer(), player_tag, data_version, ctx.data_version);
    let used_uuid = match player_tag_fixed.get("UUID").result() {
        Some(uuid) => uuid,
        None => {
            fallback_file.write(ctx.tree, &player_tag_fixed)?;
            content.create_int_list([0, 0, 0, 0])
        }
    };
    Ok(content.remove("Player").set("singleplayer_uuid", used_uuid))
}

/// `extractWorldGenSettingsToFile`.
fn extract_world_gen_settings_to_file(
    ctx: &mut ContentContext<'_>,
    target: &crate::filefix::SavedDataNbt,
    content: Dynamic,
) -> Result<Dynamic, FileFixError> {
    let Some(world_gen_settings_tag) = content.get("world_gen_settings").result() else {
        return Ok(content);
    };
    let data_version = content.get("DataVersion").as_int_or(-1);
    let fixed = DataFixTypes::WORLD_GEN_SETTINGS.update(
        data_fixer(),
        world_gen_settings_tag,
        data_version,
        ctx.data_version,
    );
    target.write(ctx.tree, &fixed)?;
    Ok(content.remove("world_gen_settings"))
}
