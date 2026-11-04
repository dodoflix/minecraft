//! Behavioral tests ported from brigadier's own CommandDispatcher/StringReader/Suggestions test suites
//! (plus extension-point tests for the Rust-specific parts: custom argument types, downcasting).

mod common;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use common::{D, arg, lit};
use mc_brigadier::arguments::{
    ArgumentType, BoolArgumentType, IntegerArgumentType, StringArgumentType, integer, integer_range, value,
};
use mc_brigadier::message::literal as msg;
use mc_brigadier::*;

fn ok(_: &CommandContext<i32>) -> Result<i32, CommandSyntaxException> {
    Ok(SINGLE_SUCCESS)
}

fn err_of(r: Result<i32, CommandSyntaxException>) -> CommandSyntaxException {
    r.expect_err("expected an error")
}

#[test]
fn create_and_execute_command() {
    let d = D::new();
    d.register(lit("foo").executes(|_| Ok(42)));
    assert_eq!(d.execute("foo", 0).unwrap(), 42);
}

#[test]
fn create_and_merge_commands() {
    let d = D::new();
    d.register(lit("base").then(lit("foo").executes(ok)));
    d.register(lit("base").then(lit("bar").executes(ok)));
    assert_eq!(d.get_root().get_child("base").unwrap().get_children().len(), 2);
    assert_eq!(d.execute("base foo", 0).unwrap(), 1);
    assert_eq!(d.execute("base bar", 0).unwrap(), 1);
}

#[test]
fn merge_overrides_command_and_keeps_children() {
    let d = D::new();
    d.register(lit("a").executes(|_| Ok(1)).then(lit("x").executes(ok)));
    d.register(lit("a").executes(|_| Ok(2)));
    assert_eq!(d.execute("a", 0).unwrap(), 2);
    assert_eq!(d.execute("a x", 0).unwrap(), 1);
}

#[test]
fn execute_unknown_command() {
    let d = D::new();
    d.register(lit("bar"));
    d.register(lit("baz"));
    let e = err_of(d.execute("foo", 0));
    assert_eq!(e.get_message(), "Unknown command at position 0: <--[HERE]");
    assert_eq!(e.get_cursor(), Some(0));
}

#[test]
fn execute_impermissible_command() {
    let d = D::new();
    d.register(lit("foo").requires(|_| false));
    assert_eq!(err_of(d.execute("foo", 0)).get_message(), "Unknown command at position 0: <--[HERE]");
}

#[test]
fn execute_empty_command() {
    let d = D::new();
    d.register(lit(""));
    assert_eq!(err_of(d.execute("", 0)).get_message(), "Unknown command at position 0: <--[HERE]");
}

#[test]
fn execute_unknown_subcommand() {
    let d = D::new();
    d.register(lit("foo").executes(ok));
    let e = err_of(d.execute("foo bar", 0));
    assert_eq!(e.get_message(), "Incorrect argument for command at position 4: foo <--[HERE]");
}

#[test]
fn execute_incorrect_literal() {
    let d = D::new();
    d.register(lit("foo").executes(ok).then(lit("bar")));
    let e = err_of(d.execute("foo baz", 0));
    assert_eq!(e.get_message(), "Incorrect argument for command at position 4: foo <--[HERE]");
}

#[test]
fn execute_ambiguous_incorrect_argument() {
    let d = D::new();
    d.register(lit("foo").executes(ok).then(lit("bar")).then(lit("baz")));
    let e = err_of(d.execute("foo unknown", 0));
    assert_eq!(e.get_message(), "Incorrect argument for command at position 4: foo <--[HERE]");
}

#[test]
fn execute_subcommand() {
    let d = D::new();
    d.register(lit("foo").then(lit("a")).then(lit("=").executes(|_| Ok(100))).then(lit("c")).executes(|_| Ok(-1)));
    assert_eq!(d.execute("foo =", 0).unwrap(), 100);
}

#[test]
fn parse_incomplete_literal() {
    let d = D::new();
    d.register(lit("foo").then(lit("bar").executes(ok)));
    let parse = d.parse("foo ", 0);
    assert_eq!(parse.get_reader().get_remaining(), " ");
    assert_eq!(parse.get_context().get_nodes().len(), 1);
}

#[test]
fn parse_incomplete_argument() {
    let d = D::new();
    d.register(lit("foo").then(arg("bar", integer()).executes(ok)));
    let parse = d.parse("foo ", 0);
    assert_eq!(parse.get_reader().get_remaining(), " ");
    assert_eq!(parse.get_context().get_nodes().len(), 1);
}

#[test]
fn execute_ambiguous_parent_subcommand() {
    let d = D::new();
    d.register(lit("test").then(arg("incorrect", integer()).executes(|_| Ok(100))).then(arg("right", integer()).then(arg("sub", integer()).executes(|_| Ok(100)))));
    assert_eq!(d.execute("test 1 2", 0).unwrap(), 100);
}

#[test]
fn execute_ambiguous_parent_subcommand_via_redirect() {
    let d = D::new();
    let real = d.register(lit("test").then(arg("incorrect", integer()).executes(|_| Ok(100))).then(arg("right", integer()).then(arg("sub", integer()).executes(|_| Ok(100)))));
    d.register(lit("redirect").redirect(real));
    assert_eq!(d.execute("redirect 1 2", 0).unwrap(), 100);
}

#[test]
fn execute_redirected_multiple_times() {
    let d = D::new();
    let concrete = d.register(lit("actual").executes(|_| Ok(SINGLE_SUCCESS)));
    let root = d.get_root().clone();
    let redirect = d.register(lit("redirected").redirect(root.clone()));
    let input = "redirected redirected actual";
    let parse = d.parse(input, 0);
    assert_eq!(parse.get_context().get_range().get_str(input), "redirected");
    assert_eq!(parse.get_context().get_nodes().len(), 1);
    assert!(Arc::ptr_eq(parse.get_context().get_root_node(), &root));
    assert!(Arc::ptr_eq(parse.get_context().get_nodes()[0].get_node(), &redirect));

    let child1 = parse.get_context().get_child().unwrap();
    assert_eq!(child1.get_range().get_str(input), "redirected");
    assert_eq!(child1.get_nodes().len(), 1);
    assert!(Arc::ptr_eq(child1.get_nodes()[0].get_node(), &redirect));

    let child2 = child1.get_child().unwrap();
    assert_eq!(child2.get_range().get_str(input), "actual");
    assert!(Arc::ptr_eq(child2.get_nodes()[0].get_node(), &concrete));
    assert_eq!(d.execute_parsed(&parse).unwrap(), 1);
}

#[test]
fn execute_forked_runs_every_source() {
    let d = D::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    d.register(lit("rec").executes(move |c| {
        sink.lock().unwrap().push(*c.get_source());
        Ok(5)
    }));
    d.register(lit("fan").fork(d.get_root().clone(), |c| Ok(vec![*c.get_source() + 1, *c.get_source() + 2])));
    // Forked mode: each execution counts as 1 regardless of its own result.
    assert_eq!(d.execute("fan rec", 10).unwrap(), 2);
    assert_eq!(*seen.lock().unwrap(), [11, 12]);
    // An empty source list stops execution with result 0.
    d.register(lit("none").fork(d.get_root().clone(), |_| Ok(vec![])));
    assert_eq!(d.execute("none rec", 0).unwrap(), 0);
}

#[test]
fn redirect_modifier_changes_source_and_single_failure_propagates() {
    let d = D::new();
    d.register(lit("whoami").executes(|c| Ok(*c.get_source())));
    d.register(lit("plus").then(arg("n", integer()).redirect_with(d.get_root().clone(), |c| Ok(*c.get_source() + IntegerArgumentType::get_integer(c, "n")))));
    assert_eq!(d.execute("plus 5 plus 6 whoami", 1).unwrap(), 12);

    d.register(lit("bad").redirect_with(d.get_root().clone(), |_| Err(SimpleCommandExceptionType::new(msg("nope")).create())));
    assert_eq!(err_of(d.execute("bad whoami", 0)).get_message(), "nope");
}

#[test]
fn fork_with_failing_modifier_is_swallowed() {
    let d = D::new();
    d.register(lit("whoami").executes(|c| Ok(*c.get_source())));
    d.register(lit("f").fork(d.get_root().clone(), |_| Err(SimpleCommandExceptionType::new(msg("nope")).create())));
    assert_eq!(d.execute("f whoami", 0).unwrap(), 0);
}

#[test]
fn execute_orphaned_subcommand() {
    let d = D::new();
    d.register(lit("foo").then(arg("bar", integer())).executes(|_| Ok(SINGLE_SUCCESS)));
    let e = err_of(d.execute("foo 5", 0));
    assert_eq!(e.get_message(), "Unknown command at position 5: foo 5<--[HERE]");
}

#[test]
fn execute_invalid_other() {
    let d = D::new();
    d.register(lit("w").executes(|_| panic!("should not run")));
    d.register(lit("world").executes(|_| Ok(SINGLE_SUCCESS)));
    assert_eq!(d.execute("world", 0).unwrap(), 1);
}

#[test]
fn parse_no_space_separator() {
    let d = D::new();
    d.register(lit("foo").then(arg("bar", integer()).executes(ok)));
    let e = err_of(d.execute("foo$", 0));
    assert_eq!(e.get_message(), "Unknown command at position 0: <--[HERE]");
}

#[test]
fn argument_followed_by_garbage_reports_separator_error() {
    let d = D::new();
    d.register(lit("foo").then(arg("n", integer()).executes(ok)));
    let e = err_of(d.execute("foo 5x", 0));
    assert_eq!(e.get_message(), "Expected whitespace to end one argument, but found trailing data at position 5: foo 5<--[HERE]");
}

#[test]
fn consumer_receives_success_and_failure() {
    let mut d = D::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    d.set_consumer(move |c, success, result| sink.lock().unwrap().push((c.get_input().to_owned(), success, result)));
    d.register(lit("ok").executes(|_| Ok(7)));
    d.register(lit("ko").executes(|_| Err(SimpleCommandExceptionType::new(msg("x")).create())));
    d.register(lit("nocmd").then(lit("x").executes(ok)));
    d.execute("ok", 0).unwrap();
    d.execute("ko", 0).unwrap_err();
    d.execute("nocmd", 0).unwrap_err();
    assert_eq!(
        *log.lock().unwrap(),
        vec![("ok".into(), true, 7), ("ko".into(), false, 0), ("nocmd".into(), false, 0)]
    );
}

#[test]
fn get_path_and_find_node() {
    let d = D::new();
    let bar = d.register(lit("foo").then(lit("bar"))).get_child("bar").unwrap();
    assert_eq!(d.get_path(&bar), ["foo", "bar"]);
    assert!(d.find_node(["foo", "bar"]).is_some_and(|n| Arc::ptr_eq(&n, &bar)));
    assert!(d.find_node(["foo", "nope"]).is_none());
    assert!(d.get_path(&CommandNode::new_literal("x", None, Arc::new(|_| true), None, None, false)).is_empty());
}

#[test]
fn usage_all_and_restricted() {
    let d = D::new();
    d.register(lit("a").requires(|s| *s > 0).executes(ok).then(lit("b").executes(ok)));
    assert_eq!(d.get_all_usage(d.get_root(), &0, false), ["a", "a b"]);
    assert!(d.get_all_usage(d.get_root(), &0, true).is_empty());
    assert_eq!(d.get_all_usage(d.get_root(), &1, true), ["a", "a b"]);
}

#[test]
fn smart_usage_variants() {
    let d = D::new();
    d.register(lit("a").then(lit("b").executes(ok)).then(lit("c").executes(ok)));
    d.register(lit("opt").executes(ok).then(arg("x", integer())));
    let smart = |name: &str| d.get_smart_usage(&d.find_node([name]).unwrap(), &0).into_iter().map(|(n, u)| format!("{}={u}", n.get_name())).collect::<Vec<_>>();
    assert_eq!(smart("a"), ["b=b", "c=c"]);
    assert_eq!(smart("opt"), ["x=[<x>]"]);
    let root = d.get_smart_usage(d.get_root(), &0);
    assert_eq!(root[0].1, "a (b|c)");
    assert_eq!(root[1].1, "opt [<x>]");
}

#[test]
fn node_ordering() {
    let d = D::new();
    d.register(lit("z"));
    d.register(lit("a").then(arg("b", integer())).then(lit("c")));
    let a = d.find_node(["a"]).unwrap();
    let (c, b) = (a.get_child("c").unwrap(), a.get_child("b").unwrap());
    assert_eq!(c.compare_to(&b), std::cmp::Ordering::Less);
    assert_eq!(b.compare_to(&c), std::cmp::Ordering::Greater);
    assert_eq!(a.compare_to(&d.find_node(["z"]).unwrap()), std::cmp::Ordering::Less);
}

#[test]
fn node_equality_is_structural() {
    let a = D::new();
    let b = D::new();
    let (na, nb) = (a.register(lit("x").then(lit("y"))), b.register(lit("x").then(lit("y"))));
    assert!(*na == *nb);
    let c = D::new();
    let nc = c.register(lit("x").then(lit("z")));
    assert!(*na != *nc);
    let (i1, i2) = (a.register(lit("i").then(arg("n", integer_range(1, 2)))), b.register(lit("i").then(arg("n", integer_range(1, 2)))));
    assert!(*i1 == *i2);
}

#[test]
fn create_builder_roundtrip_with_redirect() {
    let d = D::new();
    let target = d.register(lit("target").executes(ok));
    let node = d.register(lit("r").requires(|s| *s == 1).redirect(target.clone()));
    let rebuilt = node.create_builder().build();
    assert!(Arc::ptr_eq(rebuilt.get_redirect().unwrap(), &target));
    assert!(rebuilt.can_use(&1) && !rebuilt.can_use(&0));
    let arg_node = d.register(lit("p").then(arg("c", StringArgumentType::word()).suggests(|_, b| Ok(b.build())))).get_child("c").unwrap();
    match arg_node.create_builder() {
        NodeBuilder::Argument(b) => assert!(b.get_suggestions_provider().is_some() && b.get_name() == "c"),
        NodeBuilder::Literal(_) => panic!("expected argument builder"),
    }
}

#[test]
#[should_panic(expected = "Cannot add children to a redirected node")]
fn cannot_add_children_to_redirect() {
    let d = D::new();
    let _ = lit("a").redirect(d.get_root().clone()).then(lit("b"));
}

#[test]
#[should_panic(expected = "Cannot forward a node with children")]
fn cannot_forward_node_with_children() {
    let d = D::new();
    let _ = lit("a").then(lit("b")).redirect(d.get_root().clone());
}

#[test]
#[should_panic(expected = "Cannot add a RootCommandNode")]
fn cannot_add_root_as_child() {
    let d = D::new();
    d.get_root().add_child(CommandNode::new_root());
}

#[test]
fn argument_lookup_errors() {
    let d = D::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    d.register(lit("a").then(arg("n", integer()).executes(move |c| {
        sink.lock().unwrap().push(c.try_get_argument::<i32>("n").map(|v| *v).map_err(|e| e.0));
        sink.lock().unwrap().push(c.try_get_argument::<i32>("missing").map(|v| *v).map_err(|e| e.0));
        sink.lock().unwrap().push(c.try_get_argument::<String>("n").map(|v| v.len() as i32).map_err(|e| e.0.split(' ').take(3).collect::<Vec<_>>().join(" ")));
        Ok(1)
    })));
    d.execute("a 4", 0).unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0], Ok(4));
    assert_eq!(seen[1], Err("No such argument 'missing' exists on this command".to_owned()));
    assert_eq!(seen[2], Err("Argument 'n' is".to_owned()));
}

#[test]
fn context_exposes_ranges_and_input() {
    let d = D::new();
    let calls = Arc::new(AtomicI32::new(0));
    let c2 = calls.clone();
    d.register(lit("a").then(arg("s", StringArgumentType::string()).executes(move |c| {
        assert_eq!(c.get_input(), "a \"h\u{e9}llo w\"");
        assert_eq!(c.get_argument_range("s").unwrap(), StringRange::between(2, 11));
        assert_eq!(StringArgumentType::get_string(c, "s"), "h\u{e9}llo w");
        assert!(c.has_nodes() && c.get_child().is_none());
        c2.fetch_add(1, Ordering::SeqCst);
        Ok(1)
    })));
    d.execute("a \"h\u{e9}llo w\"", 0).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// ---- extension points: custom argument types ------------------------------------------------

#[derive(Debug, PartialEq)]
struct Point(i32, i32);

struct PointArgument;

impl ArgumentType<i32> for PointArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let x = reader.read_int()?;
        reader.expect(' ')?;
        let y = reader.read_int()?;
        Ok(value(Point(x, y)))
    }

    fn parse_with_source(&self, reader: &mut StringReader, source: &i32) -> Result<ParsedValue, CommandSyntaxException> {
        if *source < 0 {
            return Err(SimpleCommandExceptionType::new(msg("negative source")).create_with_context(reader));
        }
        self.parse(reader)
    }

    fn list_suggestions(&self, _c: &CommandContext<i32>, mut b: SuggestionsBuilder) -> Suggestions {
        b.suggest("~").suggest("~ ~");
        b.build()
    }

    fn get_examples(&self) -> Vec<String> {
        vec!["0 0".into(), "~ ~".into()]
    }

    fn describe(&self) -> String {
        "point()".into()
    }
}

#[test]
fn custom_argument_type_parse_suggest_and_downcast() {
    let d = D::new();
    d.register(lit("goto").then(arg("p", PointArgument).executes(|c| {
        let p = c.get_argument::<Point>("p");
        Ok(p.0 * 10 + p.1)
    })));
    assert_eq!(d.execute("goto 3 4", 0).unwrap(), 34);
    assert_eq!(err_of(d.execute("goto 3 4", -1)).get_message(), "negative source at position 5: goto <--[HERE]");
    // "3 4" spans two words; the argument parses both.
    let node = d.find_node(["goto", "p"]).unwrap();
    assert_eq!(node.to_string(), "<argument p:point()>");
    assert!(node.get_type().unwrap().downcast_ref::<PointArgument>().is_some());
    assert!(node.get_type().unwrap().downcast_ref::<BoolArgumentType>().is_none());
    let pr = d.parse("goto ", 0);
    let s = d.get_completion_suggestions(&pr);
    assert_eq!(s.get_list().iter().map(|x| x.get_text()).collect::<Vec<_>>(), ["~", "~ ~"]);
}

struct Panicky;

impl ArgumentType<i32> for Panicky {
    fn parse(&self, _: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        panic!("boom")
    }
}

#[test]
fn panicking_argument_type_becomes_parse_exception() {
    let d = D::new();
    d.register(lit("p").then(arg("x", Panicky).executes(ok)));
    let e = err_of(d.execute("p y", 0));
    assert_eq!(e.get_message(), "Could not parse command: boom at position 2: p <--[HERE]");
}

// ---- string reader -----------------------------------------------------------------------------

#[test]
fn reader_basics() {
    let mut r = StringReader::new("abc");
    assert!(r.can_read() && r.can_read_n(3) && !r.can_read_n(4));
    assert_eq!((r.peek(), r.peek_at(2)), ('a', 'c'));
    assert_eq!(r.read(), 'a');
    r.skip();
    assert_eq!((r.get_read(), r.get_remaining(), r.get_remaining_length(), r.get_total_length()), ("ab".into(), "c".into(), 1, 3));
    let copy = r.clone();
    r.set_cursor(0);
    assert_eq!(copy.get_cursor(), 2);
}

#[test]
fn reader_utf16_cursor_units() {
    let mut r = StringReader::new("\u{1F600}a\u{e9}b");
    assert_eq!(r.get_total_length(), 5);
    r.set_cursor(2);
    assert_eq!(r.peek(), 'a');
    assert_eq!(r.get_remaining(), "a\u{e9}b");
    assert_eq!(StringRange::between(2, 4).get(&r), "a\u{e9}");
}

#[test]
fn reader_quoted_strings() {
    let mut r = StringReader::new("\"hello world\" 'x'");
    assert_eq!(r.read_quoted_string().unwrap(), "hello world");
    r.skip();
    assert_eq!(r.read_string().unwrap(), "x");
    let mut r = StringReader::new("\"a\\\"b\\\\\"");
    assert_eq!(r.read_string().unwrap(), "a\"b\\");
    let mut r = StringReader::new("hello");
    assert_eq!(r.read_quoted_string().unwrap_err().get_message(), "Expected quote to start a string at position 0: <--[HERE]");
    let mut r = StringReader::new("'abc");
    assert_eq!(r.read_string().unwrap_err().get_message(), "Unclosed quoted string at position 4: 'abc<--[HERE]");
    let mut r = StringReader::new("\"a\\n\"");
    assert_eq!(r.read_string().unwrap_err().get_message(), "Invalid escape sequence 'n' in quoted string at position 3: \"a\\<--[HERE]");
    assert_eq!(StringReader::new("").read_string().unwrap(), "");
}

#[test]
fn reader_numbers_and_bool() {
    let mut r = StringReader::new("-123 4.5 99999999999 true x");
    assert_eq!(r.read_int().unwrap(), -123);
    r.skip();
    assert_eq!(r.read_double().unwrap(), 4.5);
    r.skip();
    assert_eq!(r.read_int().unwrap_err().get_message(), "Invalid integer '99999999999' at position 9: -123 4.5 <--[HERE]");
    assert_eq!(r.get_cursor(), 9);
    assert_eq!(r.read_long().unwrap(), 99999999999);
    r.skip();
    assert!(r.read_boolean().unwrap());
    r.skip();
    assert_eq!(r.read_boolean().unwrap_err().get_message(), "Invalid bool, expected true or false but found 'x' at position 26: ...9999 true <--[HERE]");
}

#[test]
fn reader_expect_and_whitespace() {
    let mut r = StringReader::new("  \tx");
    r.skip_whitespace();
    assert_eq!(r.peek(), 'x');
    assert!(r.expect('x').is_ok());
    assert_eq!(r.expect('y').unwrap_err().get_message(), "Expected 'y' at position 4: \u{20}\u{20}\tx<--[HERE]");
}

// ---- suggestions --------------------------------------------------------------------------------

#[test]
fn suggestions_builder_basics() {
    let mut b = SuggestionsBuilder::new("Hello w", 6);
    assert_eq!((b.get_remaining(), b.get_remaining_lower_case(), b.get_start()), ("w", "w", 6));
    b.suggest("w").suggest("world").suggest("Wombat");
    let s = b.build();
    assert_eq!(s.get_range(), StringRange::between(6, 7));
    assert_eq!(s.get_list().iter().map(|x| x.get_text()).collect::<Vec<_>>(), ["Wombat", "world"]);
    assert_eq!(s.get_list()[1].apply("Hello w"), "Hello world");
    assert!(SuggestionsBuilder::new("x", 1).build().is_empty());
}

#[test]
fn suggestions_merge_expands_ranges() {
    let mut a = SuggestionsBuilder::new("foo b", 4);
    a.suggest("bar");
    let mut b = SuggestionsBuilder::new("foo b", 0);
    b.suggest("foo baz");
    let merged = Suggestions::merge("foo b", vec![a.build(), b.build()]);
    assert_eq!(merged.get_range(), StringRange::between(0, 5));
    assert_eq!(merged.get_list().iter().map(|x| x.get_text()).collect::<Vec<_>>(), ["foo bar", "foo baz"]);
    assert!(Suggestions::merge("x", vec![]).is_empty());
}

#[test]
fn integer_suggestions_sort_numerically() {
    let mut b = SuggestionsBuilder::new("", 0);
    b.suggest_int(100).suggest_int(9).suggest_int(10);
    let s = b.build();
    assert_eq!(s.get_list().iter().map(|x| x.get_value().unwrap()).collect::<Vec<_>>(), [9, 10, 100]);
}

#[test]
fn offset_builder_keeps_input() {
    let b = SuggestionsBuilder::new("abc def", 4);
    let o = b.create_offset(2);
    assert_eq!((o.get_input(), o.get_start(), o.get_remaining()), ("abc def", 2, "c def"));
    assert_eq!(b.restart().get_start(), 4);
}

#[test]
fn completion_at_cursor_inside_command() {
    let d = D::new();
    d.register(lit("foo").then(lit("bar")).then(lit("baz")));
    let pr = d.parse("foo ba", 0);
    let s = d.get_completion_suggestions(&pr);
    assert_eq!(s.get_range(), StringRange::between(4, 6));
    assert_eq!(s.get_list().iter().map(|x| x.get_text()).collect::<Vec<_>>(), ["bar", "baz"]);
    let s = d.get_completion_suggestions_at(&pr, 2);
    assert_eq!(s.get_range(), StringRange::between(0, 2));
    assert_eq!(s.get_list().iter().map(|x| x.get_text()).collect::<Vec<_>>(), ["foo"]);
}

#[test]
fn ambiguity_consumer_reports_pairs() {
    let d = D::new();
    d.register(lit("t").then(arg("a", integer())).then(arg("b", integer())));
    let mut found = Vec::new();
    d.find_ambiguities(&mut |_, c, s, m| found.push((c.get_name().to_owned(), s.get_name().to_owned(), m.len())));
    assert_eq!(found, [("a".to_owned(), "b".to_owned(), 3), ("b".to_owned(), "a".to_owned(), 3)]);
}

#[test]
fn exception_types_are_distinguishable() {
    let a = SimpleCommandExceptionType::new(msg("a"));
    let b = SimpleCommandExceptionType::new(msg("a"));
    assert_eq!(a.create().get_type(), a.create_with_context(&StringReader::new("x")).get_type());
    assert_ne!(a.create().get_type(), b.create().get_type());
    let dynamic = DynamicNCommandExceptionType::new(|args| {
        msg(format!("{}-{}", args[0].downcast_ref::<i32>().unwrap(), args[1].downcast_ref::<&str>().unwrap()))
    });
    assert_eq!(dynamic.create(&[&5i32, &"z"]).get_message(), "5-z");
}
