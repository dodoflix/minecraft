//! Compares against `fixtures/golden.txt`, produced by running the real brigadier 1.3.11 through
//! `scripts/vanilla-java.sh crates/mc-brigadier/tests/fixtures/BrigadierGolden.java`.


use std::any::Any;

use crate::common::*;
use mc_brigadier::arguments::*;
use mc_brigadier::java_fmt::{double_to_string, float_to_string};
use mc_brigadier::message::literal as msg;
use mc_brigadier::utf16::substring;
use mc_brigadier::*;

const GOLDEN: &str = include_str!("../fixtures/golden.txt");

fn path(d: &D, n: &std::sync::Arc<CommandNode<i32>>) -> String {
    d.get_path(n).join(" ")
}

fn norm(s: &str) -> String {
    s.to_owned()
}

/// Sections computed from scratch (everything not driven by an input stored in the fixture).
fn static_sections(d: &D) -> Vec<String> {
    let mut out = Vec::new();
    let root = d.get_root();
    for restricted in [false, true] {
        for src in [0, 2, 3] {
            out.push(format!("USAGE_ALL {restricted} {src}"));
            for u in d.get_all_usage(root, &src, restricted) {
                out.push(format!("  {u}"));
            }
        }
    }
    for src in [0, 3] {
        out.push(format!("SMART_ROOT {src}"));
        for (n, u) in d.get_smart_usage(root, &src) {
            out.push(format!("  {} => {u}", n.get_name()));
        }
        for p in ["foo", "admin", "opt", "multi", "give", "pick2"] {
            out.push(format!("SMART {p} {src}"));
            for (n, u) in d.get_smart_usage(&d.find_node([p]).unwrap(), &src) {
                out.push(format!("  {} => {u}", n.get_name()));
            }
        }
    }
    let paths: [&[&str]; 12] = [
        &["foo"], &["foo", "n"], &["foo", "n", "w"], &["give", "target", "item", "count"], &["tp", "x"], &["tp", "x", "y", "z"],
        &["flag", "v"], &["big", "n"], &["say", "msg"], &["alias"], &["nope"], &["foo", "nope"],
    ];
    for p in paths {
        let body = match d.find_node(p.iter().copied()) {
            None => "null".to_owned(),
            Some(n) => norm(&format!("{n} | {} | [{}] | {}", n.get_usage_text(), n.get_examples().join(", "), path(d, &n))),
        };
        out.push(format!("NODE {} => {body}", p.join(" ")));
    }
    out.push(format!("PATH_REDIRECT {}", path(d, root.get_child("alias").unwrap().get_redirect().unwrap())));
    let foreign = CommandNode::<i32>::new_literal("zzz", None, std::sync::Arc::new(|_| true), None, None, false);
    out.push(format!("PATH_FOREIGN {}", d.get_path(&foreign).len()));

    // ambiguities
    let amb = D::new();
    amb.register(
        lit("amb").then(arg("a", integer())).then(arg("b", integer_range(0, 5))).then(arg("w", StringArgumentType::word())).then(lit("123")).then(lit("x")),
    );
    amb.register(lit("ok").then(lit("a")).then(lit("b")).then(arg("n", integer())));
    let mut amb_out = Vec::new();
    amb.find_ambiguities(&mut |parent, child, sibling, matches| {
        let mut m: Vec<&String> = matches.iter().collect();
        m.sort();
        // Java TreeSet<String> order == UTF-16 order == byte order for these ASCII inputs.
        amb_out.push(format!(
            "AMBIG {} | {} | {} | [{}]",
            amb.get_path(parent).join(" "),
            child.get_name(),
            sibling.get_name(),
            m.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    });
    out.extend(amb_out);
    out
}

fn arg_section() -> Vec<String> {
    fn s(t: &dyn ArgumentType<i32>) -> String {
        t.describe()
    }
    let ex = |t: &dyn ArgumentType<i32>| t.get_examples().join(", ");
    let mut out = Vec::new();
    out.push(format!(
        "ARG {}",
        [
            s(&integer()), s(&integer_min(3)), s(&integer_range(1, 5)), s(&long_arg()), s(&long_arg_min(3)), s(&long_arg_range(1, 5)),
            s(&float_arg()), s(&float_arg_min(1.5)), s(&float_arg_range(1.0, 2.5)), s(&double_arg()), s(&double_arg_min(-1.0)),
            s(&double_arg_range(1e10, 2.5e-5)), s(&StringArgumentType::word()), s(&StringArgumentType::string()),
            s(&StringArgumentType::greedy_string()), "BoolArgumentType".to_owned(),
        ]
        .join(" | ")
    ));
    let eq = |a: &dyn ArgumentType<i32>, b: &dyn ArgumentType<i32>| a.equals(b);
    out.push(format!(
        "ARG_EQ {} {} {} {}",
        eq(&integer_range(1, 2), &integer_range(1, 2)),
        eq(&integer_range(1, 2), &integer_range(1, 3)),
        eq(&double_arg_min(1.0), &double_arg_min(1.0)),
        eq(&StringArgumentType::word(), &StringArgumentType::word()),
    ));
    for _ in 0..2 {
        out.push(format!(
            "EXAMPLES [{}][{}][{}][{}][{}][{}]",
            ex(&StringArgumentType::word()), ex(&StringArgumentType::string()), ex(&StringArgumentType::greedy_string()),
            ex(&integer()), ex(&double_arg()), ex(&BoolArgumentType::bool())
        ));
    }
    out
}

fn value_string(v: &ParsedValue) -> String {
    let a: &dyn Any = &**v;
    if let Some(x) = a.downcast_ref::<i32>() { x.to_string() }
    else if let Some(x) = a.downcast_ref::<i64>() { x.to_string() }
    else if let Some(x) = a.downcast_ref::<f64>() { double_to_string(*x) }
    else if let Some(x) = a.downcast_ref::<f32>() { float_to_string(*x) }
    else if let Some(x) = a.downcast_ref::<bool>() { x.to_string() }
    else if let Some(x) = a.downcast_ref::<String>() { x.clone() }
    else { "?".into() }
}

fn parse_line(d: &D, input: &str) -> String {
    let pr = d.parse(input, 0);
    let mut b = format!(
        "cursor={} range={} exc={}",
        pr.get_reader().get_cursor(),
        pr.get_context().get_range(),
        pr.get_exceptions().len()
    );
    for (n, e) in pr.get_exceptions() {
        b.push_str(&format!(" {{{n}: {}}}", e.get_message()));
    }
    let mut c = Some(pr.get_context());
    while let Some(ctx) = c {
        b.push_str(" ctx[");
        for n in ctx.get_nodes() {
            b.push_str(&format!("{n},"));
        }
        let mut keys: Vec<_> = ctx.get_arguments().iter().collect();
        keys.sort_by(|a, b| a.0.cmp(b.0));
        b.push_str(&format!(
            "cmd={},args=[{}]",
            ctx.get_command().is_some(),
            keys.iter().map(|k| k.0.as_str()).collect::<Vec<_>>().join(", ")
        ));
        for (k, a) in keys {
            b.push_str(&format!(" {k}={}@{}", value_string(a.get_result()), a.get_range()));
        }
        b.push(']');
        c = ctx.get_child();
    }
    b
}

fn read_line(method: &str, input: &str) -> String {
    let mut r = StringReader::new(input);
    let res = (|| -> Result<String, CommandSyntaxException> {
        Ok(match method {
            "int" => r.read_int()?.to_string(),
            "long" => r.read_long()?.to_string(),
            "double" => double_to_string(r.read_double()?),
            "float" => float_to_string(r.read_float()?),
            "unquoted" => format!("<{}>", r.read_unquoted_string()),
            "quoted" => format!("<{}>", r.read_quoted_string()?),
            "string" => format!("<{}>", r.read_string()?),
            "bool" => r.read_boolean()?.to_string(),
            m => panic!("unknown method {m}"),
        })
    })();
    let res = match res {
        Ok(v) => format!("OK {v}"),
        Err(e) => format!("ERR {}", e.get_message()),
    };
    format!("{res}\tcursor={} read=<{}> rem=<{}>", r.get_cursor(), r.get_read(), r.get_remaining())
}

fn bound_types() -> Vec<Box<dyn ArgumentType<i32>>> {
    vec![
        Box::new(integer_range(0, 100)), Box::new(long_arg_range(1, 50)), Box::new(float_arg_range(0.5, 50.5)),
        Box::new(double_arg_range(1.0, 1e10)), Box::new(double_arg_range(-1.5e-5, 3.0)), Box::new(float_arg_range(1e-5, 1e8)),
        Box::new(double_arg_min(1e7)), Box::new(float_arg_min(-1e7)),
    ]
}

#[test]
fn golden() {
    let mut d = sample();
    let statics = static_sections(&d);
    let static_prefixes = ["USAGE_ALL", "SMART", "NODE", "PATH_", "AMBIG", "ARG ", "ARG_EQ", "EXAMPLES"];
    let mut expected_static = Vec::new();
    let mut statics_expected_from_arg = arg_section();
    let mut mismatches = Vec::new();
    let mut checked = 0usize;

    for line in GOLDEN.lines() {
        let kind = line.split([' ', '\t']).next().unwrap();
        let rest = line.get(kind.len() + 1..).unwrap_or("");
        let is_static = line.starts_with("  ") || static_prefixes.iter().any(|p| line.starts_with(p));
        if is_static {
            expected_static.push(line.to_owned());
            continue;
        }
        let got = match kind {
            "EXEC" => {
                let (src, rest) = rest.split_once('\t').unwrap();
                let (input, _) = rest.rsplit_once('\t').unwrap();
                format!("EXEC {src}\t{input}\t{}", exec(&mut d, src.parse().unwrap(), input))
            }
            "SUGG" => {
                let mut it = rest.splitn(3, '\t');
                let (src, input, _) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
                let pr = d.parse(input, src.parse().unwrap());
                format!("SUGG {src}\t{input}\t{}", sugg(&d.get_completion_suggestions(&pr), input))
            }
            "SUGG_AT" => {
                let mut it = rest.splitn(4, '\t');
                let (src, input, cursor) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
                let cursor: usize = cursor.parse().unwrap();
                let pr = d.parse(input, src.parse().unwrap());
                let s = d.get_completion_suggestions_at(&pr, cursor);
                format!("SUGG_AT {src}\t{input}\t{cursor}\t{}", sugg(&s, &substring(input, 0, cursor)))
            }
            "PARSE" => {
                let (input, _) = rest.split_once('\t').unwrap();
                format!("PARSE {input}\t{}", parse_line(&d, input))
            }
            "READ" => {
                let mut it = rest.splitn(3, '\t');
                let (m, input) = (it.next().unwrap(), it.next().unwrap());
                format!("READ {m}\t{input}\t{}", read_line(m, input))
            }
            "SKIPWS" => {
                let (input, _) = rest.rsplit_once('\t').unwrap();
                let mut r = StringReader::new(input);
                r.skip_whitespace();
                format!("SKIPWS\t{input}\t{}", r.get_cursor())
            }
            "EXPECT" => {
                let input = rest.split('\t').next().unwrap();
                let mut r = StringReader::new(input);
                let res = match r.expect('x') {
                    Ok(()) => "OK".to_owned(),
                    Err(e) => format!("ERR {}", e.get_message()),
                };
                format!("EXPECT\t{input}\t{res}\t{}", r.get_cursor())
            }
            "ESCAPE" => {
                let input = rest.split('\t').next().unwrap();
                format!("ESCAPE\t{input}\t{}", escape_if_required(input))
            }
            "BOUND" => {
                // BOUND <type>\t<input>\t<result>\t<cursor>; types appear in a fixed order per input
                let mut it = rest.splitn(4, '\t');
                let (ty, input) = (it.next().unwrap(), it.next().unwrap());
                let t = bound_types().into_iter().find(|t| t.describe() == ty).unwrap_or_else(|| panic!("no type {ty}"));
                let mut r = StringReader::new(format!("{input} tail"));
                let res = match t.parse(&mut r) {
                    Ok(v) => format!("OK {}", value_string(&v)),
                    Err(e) => format!("ERR {}", e.get_message()),
                };
                format!("BOUND {ty}\t{input}\t{res}\t{}", r.get_cursor())
            }
            "DBL" => {
                let (hex, _) = rest.split_once(' ').unwrap();
                format!("DBL {hex} {}", double_to_string(f64::from_bits(u64::from_str_radix(hex, 16).unwrap())))
            }
            "FLT" => {
                let (hex, _) = rest.split_once(' ').unwrap();
                format!("FLT {hex} {}", float_to_string(f32::from_bits(u32::from_str_radix(hex, 16).unwrap())))
            }
            "CTX" => {
                let (cur, _) = rest.split_once('\t').unwrap();
                let input = format!("{}\u{1F600}\u{e9}", &"abcdefghijklmnopqrstuvwxyz0123456789"[..30]);
                let mut r = StringReader::new(input.as_str());
                r.set_cursor(cur.parse::<usize>().unwrap().min(33));
                let e = SimpleCommandExceptionType::new(msg("m")).create_with_context(&r);
                format!("CTX {cur}\t{}", e.get_message())
            }
            "NOCTX" => {
                let e = SimpleCommandExceptionType::new(msg("plain")).create();
                format!("NOCTX {} {:?} {:?} {}", e.get_message(), e.get_context(), e.get_input(), e.get_cursor().map_or(-1, |c| c as i64))
                    .replace("None", "null")
            }
            "CTX_SHORT" => {
                let mut r = StringReader::new("short");
                r.set_cursor(5);
                format!("CTX_SHORT {}", SimpleCommandExceptionType::new(msg("big")).create_with_context(&r).get_message())
            }
            "DYN2" => {
                let e = Dynamic2CommandExceptionType::<i32, &'static str>::new(|a, b| msg(format!("{a}/{b}"))).create(1, "x");
                format!("DYN2 {}", e.get_message())
            }
            other => panic!("unhandled fixture line kind {other}: {line}"),
        };
        checked += 1;
        if got != line {
            mismatches.push(format!("fixture: {line}\n   rust: {got}"));
        }
    }

    // static sections: compare as one ordered block (ARG/ARG_EQ/EXAMPLES come from arg_section)
    let mut computed_static = statics;
    computed_static.append(&mut statics_expected_from_arg);
    if computed_static != expected_static {
        for (x, y) in computed_static.iter().zip(expected_static.iter()).filter(|(x, y)| x != y).take(15) {
            mismatches.push(format!("static fixture: {y}\n   static rust: {x}"));
        }
        mismatches.push(format!("static len rust={} fixture={}", computed_static.len(), expected_static.len()));
    }
    assert!(mismatches.is_empty(), "{} mismatches of {checked} lines:\n{}", mismatches.len(), mismatches.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
    assert!(checked > 5000, "fixture unexpectedly small: {checked}");
}
