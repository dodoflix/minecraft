//! `mc_dfu::dynamic_like` (Dynamic / OptionalDynamic API over NbtOps and JsonOps) against the real DFU/vanilla classes
//! (`fixtures/dynamic/DynamicGolden.java`, regenerate with `fixtures/dynamic/gen.sh`). Results include compound iteration order.

use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_dfu::{DataResult, Dyn, Dynamic, JNum, JSON, NBT, Ops};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::LazyLock;

static GOLDEN: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/dynamic/golden.txt.gz"));

fn dump(d: &Dyn) -> String {
    let arr = |it: &mut dyn Iterator<Item = String>| it.collect::<Vec<_>>().join(",");
    match d {
        Dyn::Empty => "END".into(),
        Dyn::Byte(v) => format!("{v}b"),
        Dyn::Short(v) => format!("{v}s"),
        Dyn::Int(v) => format!("{v}"),
        Dyn::Long(v) => format!("{v}L"),
        Dyn::Float(v) => format!("{}f", java_float_to_string(*v)),
        Dyn::Double(v) => format!("{}d", java_double_to_string(*v)),
        Dyn::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Dyn::List(l) => format!("[{}]", arr(&mut l.iter().map(dump))),
        Dyn::Map(m) => format!(
            "{{{}}}",
            arr(&mut m
                .iter()
                .map(|(k, v)| format!("{}:{}", k.as_str().unwrap(), dump(v))))
        ),
        Dyn::Bytes(v) => format!("[B;{}]", arr(&mut v.iter().map(|x| x.to_string()))),
        Dyn::Ints(v) => format!("[I;{}]", arr(&mut v.iter().map(|x| x.to_string()))),
        Dyn::Longs(v) => format!("[L;{}]", arr(&mut v.iter().map(|x| x.to_string()))),
        other => panic!("not an NBT value: {other}"),
    }
}

fn dump_val(ops: &Ops, d: &Dyn) -> String {
    if ops.is_nbt() {
        dump(d)
    } else {
        d.to_json_string()
    }
}

fn dyn_(d: &Dynamic) -> String {
    dump_val(&d.ops(), d.value())
}

fn num(n: &JNum) -> String {
    match n {
        JNum::Byte(v) => format!("b:{v}"),
        JNum::Short(v) => format!("s:{v}"),
        JNum::Int(v) => format!("i:{v}"),
        JNum::Long(v) => format!("l:{v}"),
        JNum::Float(v) => format!("f:{}", java_float_to_string(*v)),
        JNum::Double(v) => format!("d:{}", java_double_to_string(*v)),
        JNum::Lazy(t) => format!("lazy:{t}"),
    }
}

fn res<T>(r: DataResult<T>, f: impl FnOnce(&T) -> String) -> String {
    match r.result_ref() {
        Some(v) => format!("OK:{}", f(v)),
        None => format!("ERR:{}", r.error_message().unwrap()),
    }
}

fn list(c: &[Dynamic]) -> String {
    format!("[{}]", c.iter().map(dyn_).collect::<Vec<_>>().join(";"))
}

fn pairs(c: &[(Dynamic, Dynamic)]) -> String {
    format!(
        "{{{}}}",
        c.iter()
            .map(|(k, v)| format!("{}={}", dyn_(k), dyn_(v)))
            .collect::<Vec<_>>()
            .join(";")
    )
}

fn arr<T: ToString>(v: &[T]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn parse(ops: &Ops, s: &str) -> Dyn {
    if ops.is_nbt() {
        Dyn::from(&mc_nbt::snbt::parse(s).unwrap())
    } else {
        Dyn::from_json_str(s).unwrap()
    }
}

fn mk(ops: &Ops, s: &str) -> Dynamic {
    Dynamic::new(*ops, parse(ops, s))
}

fn plus_one(x: Dynamic) -> Dynamic {
    let v = x.as_int_or(0).wrapping_add(1);
    x.create_int(v)
}

fn exec(op: &str, d: &Dynamic, a: &[&str]) -> String {
    let ops = d.ops();
    let opt_arg = |s: &str| if s == "none" { None } else { Some(mk(&ops, s)) };
    match op {
        "get" => res(d.get(a[0]).into_data_result(), dyn_),
        "get2" => res(d.get(a[0]).get(a[1]).into_data_result(), dyn_),
        "as_number" => res(d.as_number(), num),
        "as_string" => res(d.as_string(), |s| s.clone()),
        "as_boolean" => res(d.as_boolean(), |b| b.to_string()),
        "as_stream_opt" => res(d.as_stream_opt(), |s| list(s)),
        "as_map_opt" => res(d.as_map_opt(), |s| pairs(s)),
        "as_byte_buffer" => res(d.as_byte_buffer_opt(), |b| arr(b)),
        "as_int_stream" => res(d.as_int_stream_opt(), |b| arr(b)),
        "as_long_stream" => res(d.as_long_stream_opt(), |b| arr(b)),
        "as_stream" => format!("OK:{}", list(&d.as_stream())),
        "as_byte_buffer_plain" => format!("OK:{}", arr(&d.as_byte_buffer())),
        "as_int_stream_plain" => format!("OK:{}", arr(&d.as_int_stream())),
        "as_long_stream_plain" => format!("OK:{}", arr(&d.as_long_stream())),
        "as_list_dump" => format!("OK:[{}]", d.as_list(|x| dyn_(&x)).join(", ")),
        "as_map_dump" => format!(
            "OK:{{{}}}",
            d.as_map(|x| dyn_(&x), |x| dyn_(&x))
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "as_number_or" => format!(
            "OK:{}",
            num(&d.as_number_or(JNum::Int(a[0].parse().unwrap())))
        ),
        "as_int_or" => format!("OK:{}", d.as_int_or(a[0].parse().unwrap())),
        "as_long_or" => format!("OK:{}", d.as_long_or(a[0].parse().unwrap())),
        "as_float_or" => format!(
            "OK:{}",
            java_float_to_string(d.as_float_or(a[0].parse().unwrap()))
        ),
        "as_double_or" => format!(
            "OK:{}",
            java_double_to_string(d.as_double_or(a[0].parse().unwrap()))
        ),
        "as_byte_or" => format!("OK:{}", d.as_byte_or(a[0].parse().unwrap())),
        "as_short_or" => format!("OK:{}", d.as_short_or(a[0].parse().unwrap())),
        "as_boolean_or" => format!("OK:{}", d.as_boolean_or(a[0].parse().unwrap())),
        "as_string_or" => format!("OK:{}", d.as_string_or(a[0])),
        "get_element" => res(d.get_element(a[0]), |v| dump_val(&ops, v)),
        "get_generic" => res(d.get_generic(&ops.create_string(a[0])), |v| {
            dump_val(&ops, v)
        }),
        "get_element_or" => format!(
            "OK:{}",
            dump_val(&ops, &d.get_element_or(a[0], ops.create_int(-1)))
        ),
        "opt_as_string" => res(d.get(a[0]).as_string(), |s| s.clone()),
        "opt_as_string_or" => format!("OK:{}", d.get(a[0]).as_string_or(a[1])),
        "opt_as_int_or" => format!("OK:{}", d.get(a[0]).as_int_or(a[1].parse().unwrap())),
        "opt_as_number" => res(d.get(a[0]).as_number(), num),
        "opt_as_stream" => res(d.get(a[0]).as_stream_opt(), |s| list(s)),
        "opt_as_stream_plain" => format!("OK:{}", list(&d.get(a[0]).as_stream())),
        "opt_or_empty_map" => format!("OK:{}", dyn_(&d.get(a[0]).or_else_empty_map())),
        "opt_or_empty_list" => format!("OK:{}", dyn_(&d.get(a[0]).or_else_empty_list())),
        "opt_get_element" => res(d.get(a[0]).get_element(a[1]), |v| dump_val(&ops, v)),
        "set" => format!("OK:{}", dyn_(&d.set(a[0], mk(&ops, a[1])))),
        "remove" => format!("OK:{}", dyn_(&d.remove(a[0]))),
        "rename" => format!("OK:{}", dyn_(&d.rename_field(a[0], a[1]))),
        "rename_fix" => format!("OK:{}", dyn_(&d.rename_and_fix_field(a[0], a[1], plus_one))),
        "update" => format!("OK:{}", dyn_(&d.update(a[0], plus_one))),
        "update_generic" => format!(
            "OK:{}",
            dyn_(&d.update_generic(&ops.create_string(a[0]), |_| ops.create_string("g")))
        ),
        "replace" => format!("OK:{}", dyn_(&d.replace_field(a[0], a[1], opt_arg(a[2])))),
        "set_if_present" => format!("OK:{}", dyn_(&d.set_field_if_present(a[0], opt_arg(a[1])))),
        "copy_field" => format!("OK:{}", dyn_(&Dynamic::copy_field(d, a[0], d, a[1]))),
        "copy_fix_field" => format!(
            "OK:{}",
            dyn_(&Dynamic::copy_and_fix_field(d, a[0], d, a[1], |x| x.create_string("c")))
        ),
        "merge" => res(d.merge(&mk(&ops, a[0])).into_data_result(), dyn_),
        "merge_entry" => res(
            d.merge_entry(&d.create_string(a[0]), &mk(&ops, a[1]))
                .into_data_result(),
            dyn_,
        ),
        "get_map_values" => res(d.get_map_values(), |m| pairs(m)),
        "update_map_values" => {
            let mode: i32 = a[0].parse().unwrap();
            format!(
                "OK:{}",
                dyn_(&d.update_map_values(|(k, v)| {
                    let ks = k.as_string_or("");
                    match mode {
                        0 => (k, v),
                        1 => (k.create_string(format!("{ks}_x")), v),
                        2 => (k.create_string(ks.chars().rev().collect::<String>()), v),
                        3 => (k.create_string("z"), v),
                        4 => {
                            let nv = v.create_int(v.as_int_or(0).wrapping_add(1));
                            (k, nv)
                        }
                        _ => (k.create_string(ks.to_uppercase()), v),
                    }
                }))
            )
        }
        "create_map_rev" => match d.get_map_values() {
            DataResult::Success(mut es) => {
                es.reverse();
                format!("OK:{}", dyn_(&d.create_map(es)))
            }
            e => format!("ERR:{}", e.error_message().unwrap()),
        },
        "create_list" => format!(
            "OK:{}",
            dyn_(&d.create_list([d.create_int(1), d.create_string("x"), d.empty_map()]))
        ),
        "create_misc" => {
            let parts = [
                d.create_byte(1),
                d.create_short(2),
                d.create_long(3),
                d.create_float(-0.0),
                d.create_double(-0.0),
                d.create_boolean(true),
                d.create_numeric(&JNum::Byte(5)),
                d.create_numeric(&JNum::Float(1.5)),
                d.create_numeric(&JNum::Long(7)),
                d.empty_map(),
                d.empty_list(),
                d.create_byte_list([1, 2]),
                d.create_int_list([3, 4]),
                d.create_long_list([5, 6]),
            ];
            format!(
                "OK:{}",
                parts.iter().map(dyn_).collect::<Vec<_>>().join("|")
            )
        }
        "convert" => format!(
            "OK:{}",
            dyn_(&d.convert(&if a[0] == "nbt" { NBT } else { JSON }))
        ),
        "to_string" => format!("OK:{}", d.to_string().replace('\n', "\\n")),
        "hash" => format!("OK:{}", d.hash_code().wrapping_sub(1)),
        "eq" => format!("OK:{}", *d == mk(&ops, a[0])),
        "chain" => {
            let r = d
                .set(a[0], d.create_int(1))
                .remove(a[1])
                .set(a[2], d.create_int(2))
                .remove(a[3])
                .rename_field(a[4], a[5]);
            format!("OK:{}", dyn_(&r))
        }
        _ => panic!("unknown op {op}"),
    }
}

#[test]
fn dynamic_api_matches_java() {
    std::panic::set_hook(Box::new(|_| {}));
    let mut bad: Vec<String> = Vec::new();
    let mut n = 0;
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (op, kind, input, nargs) = (f[0], f[1], f[2], f[3].parse::<usize>().unwrap());
        let args = &f[4..4 + nargs];
        let want = f[4 + nargs];
        let ops = if kind == "nbt" { NBT } else { JSON };
        let d = mk(&ops, input);
        let got = match catch_unwind(AssertUnwindSafe(|| exec(op, &d, args))) {
            Ok(r) => r.replace('\n', "\\n").replace('\t', "\\t"),
            Err(_) => "THROW".to_string(),
        };
        if got != want {
            bad.push(format!(
                "{op} {kind} {input} {args:?}\n   java: {want}\n   rust: {got}"
            ));
        }
        n += 1;
    }
    let _ = std::panic::take_hook();
    assert!(n > 10000, "{n} cases");
    assert!(
        bad.is_empty(),
        "{} of {n} cases differ; first:\n{}",
        bad.len(),
        bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}
