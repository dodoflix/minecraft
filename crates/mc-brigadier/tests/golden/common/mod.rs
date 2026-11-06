//! Sample command tree shared by the tests; mirrors `fixtures/BrigadierGolden.java#sample`.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]

use std::sync::{Arc, Mutex};

use mc_brigadier::arguments::{
    BoolArgumentType, IntegerArgumentType, LongArgumentType, StringArgumentType, double_arg,
    double_arg_range, float_arg_range, integer, integer_min, integer_range, long_arg_range,
};
use mc_brigadier::message::literal as msg;
use mc_brigadier::*;

pub type D = CommandDispatcher<i32>;

pub fn lit(name: &str) -> LiteralArgumentBuilder<i32> {
    literal::<i32>(name)
}

pub fn arg(name: &str, t: impl ArgumentType<i32>) -> RequiredArgumentBuilder<i32> {
    argument::<i32>(name, t)
}

pub fn sample() -> D {
    let d = D::new();
    let foo_node = d.register(
        lit("foo")
            .then(lit("bar").executes(|_| Ok(1)))
            .then(lit("Baz").executes(|_| Ok(2)))
            .then(
                arg("n", integer_range(0, 100))
                    .executes(|c| Ok(IntegerArgumentType::get_integer(c, "n")))
                    .then(arg("w", StringArgumentType::word()).executes(|c| {
                        Ok(IntegerArgumentType::get_integer(c, "n")
                            + StringArgumentType::get_string(c, "w")
                                .encode_utf16()
                                .count() as i32)
                    })),
            ),
    );
    d.register(lit("say").then(
        arg("msg", StringArgumentType::greedy_string()).executes(|c| {
            Ok(StringArgumentType::get_string(c, "msg")
                .encode_utf16()
                .count() as i32)
        }),
    ));
    d.register(
        lit("give").then(
            arg("target", StringArgumentType::word()).then(
                arg("item", StringArgumentType::string())
                    .executes(|c| {
                        Ok(StringArgumentType::get_string(c, "item")
                            .encode_utf16()
                            .count() as i32)
                    })
                    .then(
                        arg("count", integer_min(1))
                            .executes(|c| Ok(IntegerArgumentType::get_integer(c, "count"))),
                    ),
            ),
        ),
    );
    d.register(
        lit("admin")
            .requires(|s| *s >= 2)
            .then(lit("stop").executes(|_| Ok(7)))
            .then(lit("op").requires(|s| *s >= 3).executes(|_| Ok(8))),
    );
    d.register(lit("tp").then(arg("x", double_arg_range(-10.0, 10.0)).then(
        arg("y", double_arg()).then(arg("z", float_arg_range(0.0, 5.0)).executes(|_| Ok(9))),
    )));
    d.register(
        lit("flag").then(
            arg("v", BoolArgumentType::bool())
                .executes(|c| Ok(i32::from(BoolArgumentType::get_bool(c, "v")))),
        ),
    );
    d.register(
        lit("big").then(
            arg("n", long_arg_range(0, 1000))
                .executes(|c| Ok(LongArgumentType::get_long(c, "n") as i32)),
        ),
    );
    d.register(lit("alias").redirect(foo_node));
    d.register(lit("run").redirect(d.get_root().clone()));
    d.register(lit("each").fork(d.get_root().clone(), |c| {
        Ok(vec![*c.get_source(), *c.get_source() + 10])
    }));
    d.register(lit("as").then(
        arg("n", integer()).redirect_with(d.get_root().clone(), |c| {
            Ok(*c.get_source() + IntegerArgumentType::get_integer(c, "n"))
        }),
    ));
    d.register(lit("whoami").executes(|c| Ok(*c.get_source())));
    d.register(
        lit("opt")
            .executes(|_| Ok(1))
            .then(lit("x").executes(|_| Ok(2)))
            .then(lit("y")),
    );
    d.register(
        lit("multi")
            .then(lit("a").executes(|_| Ok(1)))
            .then(lit("b").executes(|_| Ok(1)))
            .then(lit("c")),
    );
    d.register(
        lit("fail").executes(|_| Err(SimpleCommandExceptionType::new(msg("boom")).create())),
    );
    d.register(
        lit("pick").then(
            arg("c", StringArgumentType::word())
                .suggests(|_, mut b| {
                    b.suggest("beta")
                        .suggest("Alpha")
                        .suggest("alpha")
                        .suggest("Beta")
                        .suggest_int(10);
                    b.suggest_int_with_tooltip(9, Some(msg("nine")))
                        .suggest_int(100);
                    b.suggest("\u{1F600}")
                        .suggest("\u{FF5E}")
                        .suggest("\u{DF}")
                        .suggest("ss")
                        .suggest("\u{3A3}")
                        .suggest("\u{3C3}");
                    b.suggest_with_tooltip("x", Some(msg("tip")));
                    Ok(b.build())
                })
                .executes(|_| Ok(1)),
        ),
    );
    d.register(
        lit("pick2")
            .then(arg("c", StringArgumentType::word()).suggests(|_, mut b| {
                b.suggest("Alpha")
                    .suggest("alpha")
                    .suggest("gamma")
                    .suggest("Gamma")
                    .suggest_int(3);
                Ok(b.build())
            }))
            .then(lit("alpha"))
            .then(lit("Alphabet"))
            .then(lit("beta")),
    );
    d
}

/// Runs `execute` and renders like the Java generator: `OK n [ok,result,source]...` / `ERR msg | raw | cursor`.
pub fn exec(d: &mut D, src: i32, input: &str) -> String {
    let events = Arc::new(Mutex::new(String::new()));
    let sink = events.clone();
    d.set_consumer(move |c, ok, r| {
        sink.lock()
            .unwrap()
            .push_str(&format!(" [{ok},{r},{}]", c.get_source()));
    });
    let res = match d.execute(input, src) {
        Ok(n) => format!("OK {n}"),
        Err(e) => format!(
            "ERR {} | {} | {}",
            e.get_message(),
            e.get_raw_message().get_string(),
            e.get_cursor().map_or(-1, |c| c as i64)
        ),
    };
    let events = events.lock().unwrap().clone();
    format!("{res}{events}")
}

pub fn sugg(s: &Suggestions, input: &str) -> String {
    let items: Vec<String> = s
        .get_list()
        .iter()
        .map(|x| {
            format!(
                "{}/{}/{}/{}",
                x.get_text(),
                x.get_tooltip().map_or("-".to_owned(), |t| t.get_string()),
                if x.get_value().is_some() { "I" } else { "S" },
                x.apply(input)
            )
        })
        .collect();
    format!(
        "range={}-{} [{}]",
        s.get_range().get_start(),
        s.get_range().get_end(),
        items.join(";")
    )
}
