//! The normalised description of a [`Type`]: one line per node, written in the grammar of the recorded vanilla types.
//!
//! The dump of the real `Type` trees (`scripts/datafix-oracle.sh schema-dump`, `SchemaWalk.java`) uses the same grammar, so
//! equal descriptions mean equal type trees up to the behaviour of custom hook functions. Two spaces of indent per depth, every line ends with LF. Strings are quoted
//! with `\` and `"` escaped and control characters as `\n`, `\r`, `\t` or `\u00XX`. Nodes (children one level deeper):
//!
//! | line | children |
//! |---|---|
//! | `prim NAME` | none (`Bool`, `Byte`, `Short`, `Int`, `Long`, `Float`, `Double`, `String`, `NamespacedString`) |
//! | `empty`, `passthrough` | none (`EmptyPart`, `EmptyPartPassthrough`) |
//! | `product`, `sum` | first, second |
//! | `list` | element |
//! | `clist` | key, value |
//! | `tag "NAME"`, `named "NAME"` | element |
//! | `hook PRE POST` | delegate; `PRE`/`POST` are `identity` ([`HookFunction::identity`]) or `custom` (any other function) |
//! | `check "NAME" INDEX EXPECTED` | delegate |
//! | `choice "NAME"` | the key type, then per entry `case "KEY"` with the entry type below it, sorted by key in `String.compareTo` order |
//! | `mu "FAMILY" INDEX` | none (a recursion point is never unfolded) |

use crate::ty::{HookFunction, Type, TypeKind};
use std::fmt::Write;

impl Type {
    /// The normalised description (see the module documentation).
    pub fn description(&self) -> String {
        let mut out = String::new();
        describe(self, 0, &mut out);
        out
    }
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn line(depth: usize, out: &mut String, f: impl FnOnce(&mut String)) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    f(out);
    out.push('\n');
}

fn describe(ty: &Type, depth: usize, out: &mut String) {
    match ty.kind() {
        TypeKind::Primitive(p) => line(depth, out, |o| {
            o.push_str("prim ");
            o.push_str(p.name());
        }),
        TypeKind::EmptyPart => line(depth, out, |o| o.push_str("empty")),
        TypeKind::Passthrough => line(depth, out, |o| o.push_str("passthrough")),
        TypeKind::Product(a, b) => {
            line(depth, out, |o| o.push_str("product"));
            describe(a, depth + 1, out);
            describe(b, depth + 1, out);
        }
        TypeKind::Sum(a, b) => {
            line(depth, out, |o| o.push_str("sum"));
            describe(a, depth + 1, out);
            describe(b, depth + 1, out);
        }
        TypeKind::List(e) => {
            line(depth, out, |o| o.push_str("list"));
            describe(e, depth + 1, out);
        }
        TypeKind::CompoundList(k, v) => {
            line(depth, out, |o| o.push_str("clist"));
            describe(k, depth + 1, out);
            describe(v, depth + 1, out);
        }
        TypeKind::Tag(name, e) => {
            line(depth, out, |o| {
                o.push_str("tag ");
                quote(name, o);
            });
            describe(e, depth + 1, out);
        }
        TypeKind::Named(name, e) => {
            line(depth, out, |o| {
                o.push_str("named ");
                quote(name, o);
            });
            describe(e, depth + 1, out);
        }
        TypeKind::Check {
            name,
            index,
            expected_index,
            delegate,
        } => {
            line(depth, out, |o| {
                o.push_str("check ");
                quote(name, o);
                let _ = write!(o, " {index} {expected_index}");
            });
            describe(delegate, depth + 1, out);
        }
        TypeKind::Hook {
            delegate,
            pre,
            post,
        } => {
            let kind = |f: &HookFunction| {
                if *f == HookFunction::identity() {
                    "identity"
                } else {
                    "custom"
                }
            };
            line(depth, out, |o| {
                let _ = write!(o, "hook {} {}", kind(pre), kind(post));
            });
            describe(delegate, depth + 1, out);
        }
        TypeKind::TaggedChoice(c) => {
            line(depth, out, |o| {
                o.push_str("choice ");
                quote(&c.name, o);
            });
            describe(&c.key_type, depth + 1, out);
            let mut entries: Vec<&(String, Type)> = c.types.iter().collect();
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            for (key, t) in entries {
                line(depth + 1, out, |o| {
                    o.push_str("case ");
                    quote(key, o);
                });
                describe(t, depth + 2, out);
            }
        }
        TypeKind::RecursivePoint { family, index, .. } => line(depth, out, |o| {
            o.push_str("mu ");
            quote(family.name(), o);
            let _ = write!(o, " {index}");
        }),
    }
}
