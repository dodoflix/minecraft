//! `GameRuleCommand`.
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    LiteralArgumentBuilder,
};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::world::level::gamerules::{
    AnyGameRule, GameRuleType, GameRuleTypeVisitor, GameRuleValue, GameRules,
};
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

static ERROR_GAME_RULE_NOT_SET: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|rule: String, value: String| {
            mc_game_core::commands::message::message(translatable!(
                "commands.gamerule.not_set",
                rule,
                value
            ))
        })
    });

register_command!("GameRuleCommand", |ctx| {
    struct Visitor(Option<LiteralArgumentBuilder<CommandSourceStack>>);
    impl GameRuleTypeVisitor for Visitor {
        fn visit(&mut self, rule: AnyGameRule) {
            let base = self.0.take().expect("set");
            let unqualified = literal(rule.id());
            let qualified = literal(rule.get_identifier().to_string());
            self.0 = Some(
                base.then(build_rule_arguments(rule, unqualified))
                    .then(build_rule_arguments(rule, qualified)),
            );
        }
    }
    let mut visitor = Visitor(Some(
        literal("gamerule").requires(has_permission(LEVEL_GAMEMASTERS)),
    ));
    GameRules::new(*ctx.build_context.enabled_features()).visit_game_rule_types(&mut visitor);
    let base = visitor.0.expect("set");
    ctx.dispatcher.register(base);
});

fn build_rule_arguments(
    rule: AnyGameRule,
    rule_literal: LiteralArgumentBuilder<CommandSourceStack>,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let rule_literal = rule_literal.executes(move |c| Ok(query_rule(c.get_source(), rule)));
    match rule.game_rule_type() {
        GameRuleType::Bool => rule_literal.then(
            argument("value", mc_brigadier::arguments::BoolArgumentType::bool())
                .executes(move |c| set_rule(c, rule)),
        ),
        GameRuleType::Int => {
            let (min, max) = rule.int_range();
            rule_literal.then(
                argument("value", mc_brigadier::arguments::integer_range(min, max))
                    .executes(move |c| set_rule(c, rule)),
            )
        }
    }
}

fn set_rule(
    context: &CommandContext<CommandSourceStack>,
    rule: AnyGameRule,
) -> Result<i32, CommandSyntaxException> {
    let source = context.get_source();
    let value = match rule.game_rule_type() {
        GameRuleType::Bool => GameRuleValue::Bool(*context.get_argument::<bool>("value")),
        GameRuleType::Int => GameRuleValue::Int(*context.get_argument::<i32>("value")),
    };
    let serialized = rule.serialize(value);
    if source.with_server(|server| server.game_rules().get_value(rule)) == value {
        return Err(ERROR_GAME_RULE_NOT_SET.create(rule.id(), serialized));
    }
    source.with_server(|server| server.set_game_rule_value(rule, value));
    source.send_success(
        || translatable!("commands.gamerule.set", rule.id(), serialized.clone()),
        true,
    );
    Ok(rule.get_command_result(value))
}

fn query_rule(source: &CommandSourceStack, rule: AnyGameRule) -> i32 {
    let value = source.with_server(|server| server.game_rules().get_value(rule));
    source.send_success(
        || translatable!("commands.gamerule.query", rule.id(), rule.serialize(value)),
        false,
    );
    rule.get_command_result(value)
}
