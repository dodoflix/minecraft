//! `net.minecraft.util.ProblemReporter` (collector), `ValidationContext`, `ValidationContextSource` and the
//! `Validatable` helpers.

use std::cell::RefCell;
use std::rc::Rc;

use mc_core::ident::Identifier;

use super::context::{ContextKey, ContextKeySet};
use super::holder::{Holder, HolderSet};
use crate::registry::RegistryAccess;

/// `ProblemReporter.Problem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub description: String,
    pub fatal: bool,
}

impl Problem {
    pub fn new(description: impl Into<String>) -> Problem {
        Problem { description: description.into(), fatal: false }
    }
}

/// `ProblemReporter.Collector`: problems with the path they were found at (insertion ordered, deduplicated).
#[derive(Clone, Default)]
pub struct ProblemCollector {
    entries: Rc<RefCell<Vec<(String, Problem)>>>,
}

impl ProblemCollector {
    pub fn new() -> ProblemCollector {
        ProblemCollector::default()
    }

    fn report(&self, path: &str, problem: Problem) {
        let mut entries = self.entries.borrow_mut();
        if !entries.iter().any(|(p, q)| p == path && *q == problem) {
            entries.push((path.to_owned(), problem));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    pub fn has_fatal_problems(&self) -> bool {
        self.entries.borrow().iter().any(|(_, p)| p.fatal)
    }

    /// `forEach((path, problem) -> ...)`.
    pub fn for_each(&self, mut output: impl FnMut(&str, &Problem)) {
        for (path, problem) in self.entries.borrow().iter() {
            output(path, problem);
        }
    }

    /// `getReport()`: problems grouped by path (`HashMultimap`, so group order is unspecified; first-seen here).
    pub fn get_report(&self) -> String {
        let entries = self.entries.borrow();
        let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
        for (path, problem) in entries.iter() {
            match groups.iter_mut().find(|(p, _)| p == path) {
                Some((_, v)) => v.push(&problem.description),
                None => groups.push((path, vec![&problem.description])),
            }
        }
        groups.iter().map(|(path, v)| format!(" at {path}: {}", v.join("; "))).collect::<Vec<_>>().join("\n")
    }
}

/// `ValidationContext`.
#[derive(Clone)]
pub struct ValidationContext<'a> {
    problems: ProblemCollector,
    path: String,
    context_key_set: ContextKeySet,
    resolver: Option<&'a RegistryAccess>,
    visited: Vec<String>,
}

impl<'a> ValidationContext<'a> {
    /// `new ValidationContext(reporter, contextKeySet, resolver)`.
    pub fn new(problems: &ProblemCollector, context_key_set: ContextKeySet, resolver: &'a RegistryAccess) -> ValidationContext<'a> {
        ValidationContext { problems: problems.clone(), path: String::new(), context_key_set, resolver: Some(resolver), visited: Vec::new() }
    }

    /// `new ValidationContext(reporter, contextKeySet)` (references not allowed).
    pub fn without_resolver(problems: &ProblemCollector, context_key_set: ContextKeySet) -> ValidationContext<'static> {
        ValidationContext { problems: problems.clone(), path: String::new(), context_key_set, resolver: None, visited: Vec::new() }
    }

    pub fn context_key_set(&self) -> &ContextKeySet {
        &self.context_key_set
    }

    /// `forChild(subContext)` with the path element's text.
    pub fn for_child(&self, element: &str) -> ValidationContext<'a> {
        let mut c = self.clone();
        c.path.push_str(element);
        c
    }

    pub fn for_field(&self, name: &str) -> ValidationContext<'a> {
        self.for_child(&format!(".{name}"))
    }

    pub fn for_indexed_field(&self, name: &str, index: usize) -> ValidationContext<'a> {
        self.for_child(&format!(".{name}[{index}]"))
    }

    pub fn for_indexed(&self, index: usize) -> ValidationContext<'a> {
        self.for_child(&format!("[{index}]"))
    }

    pub fn for_map_field(&self, name: &str, key: &str) -> ValidationContext<'a> {
        self.for_child(&format!(".{name}[{key}]"))
    }

    /// `enterElement(ElementReferencePathElement(id), id)`.
    pub fn enter_element(&self, element_text: &str, registry: &str, id: &Identifier) -> ValidationContext<'a> {
        let mut c = self.for_child(element_text);
        c.visited.push(visited_key("element", registry, id));
        c
    }

    pub fn has_visited_element(&self, registry: &str, id: &Identifier) -> bool {
        self.visited.contains(&visited_key("element", registry, id))
    }

    pub fn enter_tag(&self, element_text: &str, registry: &str, tag: &Identifier) -> ValidationContext<'a> {
        let mut c = self.for_child(element_text);
        c.visited.push(visited_key("tag", registry, tag));
        c
    }

    pub fn has_visited_tag(&self, registry: &str, tag: &Identifier) -> bool {
        self.visited.contains(&visited_key("tag", registry, tag))
    }

    pub fn report_problem(&self, problem: Problem) {
        self.problems.report(&self.path, problem);
    }

    pub fn report(&self, description: impl Into<String>) {
        self.report_problem(Problem::new(description));
    }

    /// `validateContextUsage(user)`.
    pub fn validate_context_usage(&self, referenced: &[ContextKey]) {
        let mut not_provided: Vec<ContextKey> = referenced.iter().copied().filter(|k| !self.context_key_set.allows(*k)).collect();
        not_provided.sort();
        not_provided.dedup();
        if !not_provided.is_empty() {
            let list: Vec<String> = not_provided.iter().map(|k| k.to_string()).collect();
            self.report(format!("Parameters [{}] are not provided in this context", list.join(", ")));
        }
    }

    /// `resolver()`: references are not allowed without one.
    pub fn resolver(&self) -> &'a RegistryAccess {
        self.resolver.expect("References not allowed")
    }
}

fn visited_key(kind: &str, registry: &str, id: &Identifier) -> String {
    format!("{kind}:{registry}:{id}")
}

/// `ValidationContextSource`.
pub struct ValidationContextSource<'a> {
    problems: ProblemCollector,
    loot_data: &'a RegistryAccess,
}

impl<'a> ValidationContextSource<'a> {
    pub fn new(problems: &ProblemCollector, loot_data: &'a RegistryAccess) -> ValidationContextSource<'a> {
        ValidationContextSource { problems: problems.clone(), loot_data }
    }

    /// `context(params)`.
    pub fn context(&self, params: ContextKeySet) -> ValidationContext<'a> {
        ValidationContext::new(&self.problems, params, self.loot_data)
    }

    /// `entityContext()` (`ADVANCEMENT_ENTITY`).
    pub fn entity_context(&self) -> ValidationContext<'a> {
        self.context(super::parameters::LootContextParamSets::ADVANCEMENT_ENTITY)
    }
}

/// `Validatable`.
pub trait Validatable {
    fn validate(&self, context: &ValidationContext);
}

/// `Validatable.validate(context, name, optional)`.
pub fn validate_opt<V: Validatable>(context: &ValidationContext, name: &str, value: &Option<V>) {
    if let Some(v) = value {
        v.validate(&context.for_field(name));
    }
}

/// `Validatable.validate(context, name, list)`.
pub fn validate_list<V: Validatable>(context: &ValidationContext, name: &str, list: &[V]) {
    for (i, v) in list.iter().enumerate() {
        v.validate(&context.for_indexed_field(name, i));
    }
}

/// Implemented by the unsized loot data types so holders can validate their value (`holder.value().validate(ctx)`).
pub trait ValidatableData: Send + Sync + 'static {
    const REGISTRY: &'static str;
    fn validate_value(&self, context: &ValidationContext);
}

/// `Validatable.validateHolder(context, holder)`.
pub fn validate_holder<T: ?Sized + ValidatableData>(context: &ValidationContext, holder: &Holder<T>) {
    let element_context = match holder {
        Holder::Reference { registry, id } => {
            if context.has_visited_element(registry, id) {
                let mut p = Problem::new(format!("{id} of type {registry} is recursively called"));
                p.fatal = true;
                context.report_problem(p);
                return;
            }
            context.enter_element(&format!("->{{{id}@{registry}}}"), registry, id)
        }
        Holder::Direct(_) => context.clone(),
    };
    if let Some(v) = holder.try_value(context.resolver()) {
        v.validate_value(&element_context);
    }
}

pub fn validate_holder_field<T: ?Sized + ValidatableData>(context: &ValidationContext, name: &str, holder: &Holder<T>) {
    validate_holder(&context.for_field(name), holder);
}

pub fn validate_holder_opt<T: ?Sized + ValidatableData>(context: &ValidationContext, name: &str, holder: &Option<Holder<T>>) {
    if let Some(h) = holder {
        validate_holder_field(context, name, h);
    }
}

/// `validateHolder(context, name, list)`: element values only (references are not entered).
pub fn validate_holder_list<T: ?Sized + ValidatableData>(context: &ValidationContext, name: &str, list: &[Holder<T>]) {
    for (i, h) in list.iter().enumerate() {
        if let Some(v) = h.try_value(context.resolver()) {
            v.validate_value(&context.for_indexed_field(name, i));
        }
    }
}

/// `Validatable.validateHolderSet(context, name, set, minSize)`.
pub fn validate_holder_set_min<T: ?Sized + ValidatableData>(context: &ValidationContext, name: &str, set: &HolderSet<T>, min_size: usize) {
    let named = context.for_field(name);
    validate_holder_set_in(&named, set);
    if set.size(named.resolver()) < min_size {
        named.report(format!("List must contain at least {min_size} element(s)"));
    }
}

pub fn validate_holder_set<T: ?Sized + ValidatableData>(context: &ValidationContext, name: &str, set: &HolderSet<T>) {
    validate_holder_set_in(&context.for_field(name), set);
}

fn validate_holder_set_in<T: ?Sized + ValidatableData>(context: &ValidationContext, set: &HolderSet<T>) {
    let resolver = context.resolver();
    if !set.is_bound(resolver) {
        return;
    }
    let collection_context = match set {
        HolderSet::Named { registry, tag } => {
            if context.has_visited_tag(registry, tag) {
                let mut p = Problem::new(format!("#{tag} of type {registry} is recursively called"));
                p.fatal = true;
                context.report_problem(p);
                return;
            }
            context.enter_tag(&format!("->{{#{tag}@{registry}}}"), registry, tag)
        }
        HolderSet::Direct(_) => context.clone(),
    };
    for (i, holder) in set.holders(resolver).iter().enumerate() {
        validate_holder(&collection_context.for_indexed(i), holder);
    }
}
