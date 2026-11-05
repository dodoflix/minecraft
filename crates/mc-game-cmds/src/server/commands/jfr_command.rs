//! `JfrCommand`.
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::{CommandSourceStack, LEVEL_OWNERS, has_permission, literal};
use mc_game_core::util::profiling::jfr;
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType, SimpleCommandExceptionType};
use mc_text::{ChatFormatting, ClickEvent, Component, HoverEvent};
use std::path::Path;
use std::sync::LazyLock;

static START_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.jfr.start.failed"));
static DUMP_FAILED: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|reason: String| message(translatable!("commands.jfr.dump.failed", reason))));

// Vanilla registers `jfr` when `JvmProfiler.INSTANCE.isAvailable()`; the server's own recorder always is.
register_command!("JfrCommand", |ctx| {
    if jfr::instance().is_available() {
        ctx.dispatcher.register(
            literal("jfr")
                .requires(has_permission(LEVEL_OWNERS))
                .then(literal("start").executes(|c| start_jfr(c.get_source())))
                .then(literal("stop").executes(|c| stop_jfr(c.get_source()))),
        );
    }
});

fn start_jfr(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if !jfr::instance().start(jfr::ENVIRONMENT_DESCRIPTION) {
        return Err(START_FAILED.create());
    }
    source.send_success(|| translatable!("commands.jfr.started"), false);
    Ok(1)
}

fn stop_jfr(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    let saved = jfr::instance().stop().map_err(|e| DUMP_FAILED.create(e))?;
    // `Paths.get(".").relativize(path.normalize())`; a dedicated server `isPublished()`, so the clipboard gets the relative path
    let saved_recording = saved.strip_prefix(Path::new(".")).unwrap_or(&saved).to_string_lossy().into_owned();
    let file_text = Component::literal(saved_recording.clone()).with_format(ChatFormatting::Underline).with_style_fn(|style| {
        style.with_click_event(Some(ClickEvent::CopyToClipboard(saved_recording.clone()))).with_hover_event(Some(HoverEvent::show_text(translatable!("chat.copy.click"))))
    });
    source.send_success(|| translatable!("commands.jfr.stopped", file_text.clone()), false);
    Ok(1)
}
