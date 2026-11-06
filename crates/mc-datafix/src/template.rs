//! DFU `TypeTemplate` (`com.mojang.datafixers.types.templates.*`): the family-parametric type descriptions schemas
//! register. A template is turned into a [`Type`] by [`Template::apply_at`] (Java `apply(family).apply(index)`).
//!
//! Not ported (unused by vanilla and by the DFU machinery reachable from it): `findFieldOrType`, `applyO` /
//! `FamilyOptic` (only needed by `findType(.., recurse = true)`), `toSimpleType`.

use crate::family::RecursiveTypeFamily;
use crate::rewrite::{BitSet, IntFn, RewriteResult};
use crate::ty::{HookFunction, Type, fast_order, tagged_element_result};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub(crate) struct TaggedChoiceTemplate {
    pub name: String,
    pub key_type: Type,
    /// In the iteration order of Java's `Object2ObjectOpenHashMap`.
    pub templates: Vec<(String, Template)>,
    pub size: i32,
}

pub(crate) enum TemplateKind {
    Const(Type),
    Tag(String, Template),
    Named(String, Template),
    Product(Template, Template),
    Sum(Template, Template),
    List(Template),
    CompoundList(Template, Template),
    Check {
        name: String,
        index: i32,
        element: Template,
    },
    Hook {
        element: Template,
        pre: HookFunction,
        post: HookFunction,
    },
    TaggedChoice(TaggedChoiceTemplate),
    RecursivePoint(i32),
}

pub(crate) struct TemplateNode {
    pub kind: TemplateKind,
    hash: u64,
}

/// `TypeTemplate`; structural equality and hash like the Java records (the recursive-family interner relies on it).
#[derive(Clone)]
pub struct Template(pub(crate) Arc<TemplateNode>);

fn comb(a: u64, b: u64) -> u64 {
    a.wrapping_mul(31).wrapping_add(b)
}

fn str_hash(s: &str) -> u64 {
    mc_core::math::java::java_string_hash(s) as u32 as u64
}

impl Template {
    fn make(kind: TemplateKind) -> Template {
        let hash = match &kind {
            TemplateKind::Const(t) => {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                t.hash(&mut h);
                h.finish()
            }
            TemplateKind::Tag(n, e) | TemplateKind::Named(n, e) => comb(str_hash(n), e.hash64()),
            TemplateKind::Product(a, b)
            | TemplateKind::Sum(a, b)
            | TemplateKind::CompoundList(a, b) => comb(a.hash64(), b.hash64()),
            TemplateKind::List(a) => a.hash64() ^ 0x1234_5678,
            TemplateKind::Check {
                name,
                index,
                element,
            } => comb(comb(str_hash(name), *index as u64), element.hash64()),
            TemplateKind::Hook { element, pre, post } => comb(
                comb(element.hash64(), pre.addr() as u64),
                post.addr() as u64,
            ),
            TemplateKind::TaggedChoice(tc) => {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                tc.key_type.hash(&mut h);
                let entries = tc.templates.iter().fold(0u64, |acc, (k, t)| {
                    acc.wrapping_add(str_hash(k) ^ t.hash64())
                });
                comb(comb(str_hash(&tc.name), h.finish()), entries)
            }
            TemplateKind::RecursivePoint(i) => *i as u64 ^ 0x9e37_79b9,
        };
        Template(Arc::new(TemplateNode { kind, hash }))
    }

    pub(crate) fn hash64(&self) -> u64 {
        self.0.hash
    }

    // ---- constructors (`DSL.*` for templates) -------------------------------------------------------------------

    pub fn const_type(ty: &Type) -> Template {
        Template::make(TemplateKind::Const(ty.clone()))
    }
    pub fn tag(name: &str, element: &Template) -> Template {
        Template::make(TemplateKind::Tag(name.to_string(), element.clone()))
    }
    pub fn named(name: &str, element: &Template) -> Template {
        Template::make(TemplateKind::Named(name.to_string(), element.clone()))
    }
    pub fn product(a: &Template, b: &Template) -> Template {
        Template::make(TemplateKind::Product(a.clone(), b.clone()))
    }
    pub fn sum(a: &Template, b: &Template) -> Template {
        Template::make(TemplateKind::Sum(a.clone(), b.clone()))
    }
    pub fn list(element: &Template) -> Template {
        Template::make(TemplateKind::List(element.clone()))
    }
    pub fn compound_list(key: &Template, element: &Template) -> Template {
        Template::make(TemplateKind::CompoundList(key.clone(), element.clone()))
    }
    pub fn check(name: &str, index: i32, element: &Template) -> Template {
        Template::make(TemplateKind::Check {
            name: name.to_string(),
            index,
            element: element.clone(),
        })
    }
    pub fn hook(element: &Template, pre: &HookFunction, post: &HookFunction) -> Template {
        Template::make(TemplateKind::Hook {
            element: element.clone(),
            pre: pre.clone(),
            post: post.clone(),
        })
    }
    pub fn id(index: i32) -> Template {
        Template::make(TemplateKind::RecursivePoint(index))
    }
    /// `DSL.taggedChoice(name, keyType, templates)`: `templates` in the iteration order of the Java map passed in
    /// (`new Object2ObjectOpenHashMap<>(templates)` is applied here).
    pub fn tagged_choice(
        name: &str,
        key_type: &Type,
        templates: Vec<(String, Template)>,
    ) -> Template {
        let templates = fast_order(templates);
        let size = templates.iter().map(|(_, t)| t.size()).max().unwrap_or(0);
        Template::make(TemplateKind::TaggedChoice(TaggedChoiceTemplate {
            name: name.to_string(),
            key_type: key_type.clone(),
            templates,
            size,
        }))
    }

    /// `template.size()`.
    pub fn size(&self) -> i32 {
        match &self.0.kind {
            TemplateKind::Const(_) => 0,
            TemplateKind::Tag(_, e) | TemplateKind::Named(_, e) | TemplateKind::List(e) => e.size(),
            TemplateKind::Hook { element, .. } => element.size(),
            TemplateKind::Product(a, b)
            | TemplateKind::Sum(a, b)
            | TemplateKind::CompoundList(a, b) => a.size().max(b.size()),
            TemplateKind::Check { index, element, .. } => (index + 1).max(element.size()),
            TemplateKind::TaggedChoice(tc) => tc.size,
            TemplateKind::RecursivePoint(i) => i + 1,
        }
    }

    /// `template.apply(family).apply(index)`.
    pub fn apply_at(&self, family: &Arc<RecursiveTypeFamily>, index: i32) -> Type {
        match &self.0.kind {
            TemplateKind::Const(t) => t.clone(),
            TemplateKind::Tag(n, e) => Type::tag(n, &e.apply_at(family, index)),
            TemplateKind::Named(n, e) => Type::named(n, &e.apply_at(family, index)),
            TemplateKind::Product(a, b) => {
                Type::product(&a.apply_at(family, index), &b.apply_at(family, index))
            }
            TemplateKind::Sum(a, b) => {
                Type::sum(&a.apply_at(family, index), &b.apply_at(family, index))
            }
            TemplateKind::List(e) => Type::list(&e.apply_at(family, index)),
            TemplateKind::CompoundList(a, b) => {
                Type::compound_list(&a.apply_at(family, index), &b.apply_at(family, index))
            }
            TemplateKind::Check {
                name,
                index: expected,
                element,
            } => {
                assert!(index >= 0, "IndexOutOfBounds: Check applied at {index}");
                Type::check(name, index, *expected, &element.apply_at(family, index))
            }
            TemplateKind::Hook { element, pre, post } => {
                Type::hook(&element.apply_at(family, index), pre, post)
            }
            TemplateKind::TaggedChoice(tc) => {
                let types: Vec<(String, Type)> = tc
                    .templates
                    .iter()
                    .map(|(k, t)| (k.clone(), t.apply_at(family, index)))
                    .collect();
                // `new Object2ObjectOpenHashMap<>(templates.size())` filled in iteration order
                Type::tagged_choice(&tc.name, &tc.key_type, fast_order(types))
            }
            TemplateKind::RecursivePoint(i) => family.apply(*i),
        }
    }

    /// `template.hmap(family, function)`: the rewrite result of every recursion point replaced by `function(point)`.
    pub fn hmap(&self, family: &Arc<RecursiveTypeFamily>, function: IntFn) -> IntFn {
        let this = self.clone();
        let family = family.clone();
        match &self.0.kind {
            TemplateKind::Const(t) => {
                let t = t.clone();
                Arc::new(move |_| RewriteResult::nop(&t))
            }
            TemplateKind::Tag(_, e) => e.hmap(&family, function),
            TemplateKind::Named(name, e) => {
                let elem = e.hmap(&family, function);
                let name = name.clone();
                Arc::new(move |i| {
                    let r = elem(i);
                    this.apply_at(&family, i).fix_named(&name, r)
                })
            }
            TemplateKind::Check { element, .. } => {
                let elem = element.hmap(&family, function);
                Arc::new(move |i| {
                    let r = elem(i);
                    this.apply_at(&family, i).fix_check(r)
                })
            }
            TemplateKind::Hook { element, .. } => {
                let elem = element.hmap(&family, function);
                Arc::new(move |i| {
                    let r = elem(i);
                    this.apply_at(&family, i).fix_hook(r)
                })
            }
            TemplateKind::Product(a, b) => {
                let (f, g) = (a.hmap(&family, function.clone()), b.hmap(&family, function));
                Arc::new(move |i| this.apply_at(&family, i).merge_product(f(i), g(i)))
            }
            TemplateKind::Sum(a, b) => {
                let (f, g) = (a.hmap(&family, function.clone()), b.hmap(&family, function));
                Arc::new(move |i| this.apply_at(&family, i).merge_sum(f(i), g(i)))
            }
            TemplateKind::CompoundList(a, b) => {
                let (f, g) = (a.hmap(&family, function.clone()), b.hmap(&family, function));
                Arc::new(move |i| this.apply_at(&family, i).merge_compound_list(f(i), g(i)))
            }
            TemplateKind::List(e) => {
                let elem = e.hmap(&family, function);
                Arc::new(move |i| {
                    let view = elem(i);
                    let ty = this.apply_at(&family, i);
                    let element = ty
                        .as_list()
                        .expect("list template applies to a list type")
                        .clone();
                    ty.fix_list(&element, view)
                })
            }
            TemplateKind::RecursivePoint(index) => {
                let index = *index;
                Arc::new(move |_| {
                    let result = function(index);
                    let source = family.apply(index);
                    assert!(
                        source.is_recursive_point(),
                        "Type error: Recursive point template template got a non-recursice type as an input."
                    );
                    assert!(
                        *result.view.ty() == source,
                        "Type error: hmap function input type"
                    );
                    let mut bits: BitSet = result.rec_data;
                    bits.set(index);
                    RewriteResult::create(result.view, bits)
                })
            }
            TemplateKind::TaggedChoice(tc) => {
                let elems: Vec<(String, IntFn)> = tc
                    .templates
                    .iter()
                    .map(|(k, t)| (k.clone(), t.hmap(&family, function.clone())))
                    .collect();
                Arc::new(move |index| {
                    let ty = this.apply_at(&family, index);
                    let mut result = RewriteResult::nop(&ty);
                    for (key, elem) in &elems {
                        let element_result = elem(index);
                        let choice = result
                            .view
                            .new_type()
                            .as_tagged_choice()
                            .expect("tagged choice template applies to a tagged choice");
                        result =
                            tagged_element_result(key, &choice, &element_result).compose(&result);
                    }
                    result
                })
            }
        }
    }
}

impl PartialEq for Template {
    fn eq(&self, o: &Template) -> bool {
        if Arc::ptr_eq(&self.0, &o.0) {
            return true;
        }
        if self.0.hash != o.0.hash {
            return false;
        }
        use TemplateKind::*;
        match (&self.0.kind, &o.0.kind) {
            (Const(a), Const(b)) => a == b,
            (Tag(n1, a), Tag(n2, b)) | (Named(n1, a), Named(n2, b)) => n1 == n2 && a == b,
            (Product(a1, a2), Product(b1, b2))
            | (Sum(a1, a2), Sum(b1, b2))
            | (CompoundList(a1, a2), CompoundList(b1, b2)) => a1 == b1 && a2 == b2,
            (List(a), List(b)) => a == b,
            (
                Check {
                    name: n1,
                    index: i1,
                    element: a,
                },
                Check {
                    name: n2,
                    index: i2,
                    element: b,
                },
            ) => n1 == n2 && i1 == i2 && a == b,
            (
                Hook {
                    element: a,
                    pre: p1,
                    post: q1,
                },
                Hook {
                    element: b,
                    pre: p2,
                    post: q2,
                },
            ) => a == b && p1 == p2 && q1 == q2,
            (TaggedChoice(a), TaggedChoice(b)) => {
                a.name == b.name
                    && a.key_type == b.key_type
                    && a.templates.len() == b.templates.len()
                    && a.templates
                        .iter()
                        .all(|(k, t)| b.templates.iter().any(|(k2, t2)| k == k2 && t == t2))
            }
            (RecursivePoint(a), RecursivePoint(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for Template {}
impl Hash for Template {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.hash.hash(h)
    }
}

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TemplateKind::*;
        match &self.0.kind {
            Const(t) => write!(f, "Const[{t}]"),
            Tag(n, e) => write!(f, "NameTag[{n}: {e}]"),
            Named(n, e) => write!(f, "NamedTypeTag[{n}: {e}]"),
            Product(a, b) => write!(f, "({a}, {b})"),
            Sum(a, b) => write!(f, "({a} | {b})"),
            List(e) => write!(f, "List[{e}]"),
            CompoundList(_, e) => write!(f, "CompoundList[{e}]"),
            Check {
                name,
                index,
                element,
            } => write!(f, "Tag[{name}, {index}: {element}]"),
            Hook { element, .. } => write!(f, "Hook[{element}]"),
            TaggedChoice(tc) => {
                write!(f, "TaggedChoice[{}", tc.name)?;
                for (k, t) in &tc.templates {
                    write!(f, ", {k} -> {t}")?;
                }
                f.write_str("]")
            }
            RecursivePoint(i) => write!(f, "Id[{i}]"),
        }
    }
}

impl std::fmt::Debug for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
