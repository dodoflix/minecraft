//! `com.mojang.datafixers.DSL`: constructors for type templates (`Template`, used by schemas) and for concrete types
//! (`Type`, used by fixes). Java overloads on `TypeTemplate`/`Type` become `name` (template) vs `name_type` (type);
//! the `optionalFields(n1, t1, n2, t2, ..)` varargs overloads take a slice of `(name, template)` pairs.

use crate::finder::OpticFinder;
use crate::schema::{Schema, TemplateMap};
use crate::template::Template;
use crate::ty::{HookFunction, Type};

/// `DSL.TypeReference`: a named type of the data (`References.ITEM_STACK`, ...).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeReference(pub &'static str);

impl TypeReference {
    /// `typeName()`.
    pub fn type_name(&self) -> &'static str {
        self.0
    }
    /// `reference.in(schema)` (Java `in`): the template of this type inside `schema` (a recursion point for
    /// recursive types).
    pub fn in_(&self, schema: &Schema) -> Template {
        schema.id(self.0)
    }
}

impl std::fmt::Display for TypeReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@{}", self.0)
    }
}

pub struct DSL;

impl DSL {
    // ---- primitive types ----------------------------------------------------------------------------------------
    pub fn bool() -> Type {
        Type::bool()
    }
    pub fn int_type() -> Type {
        Type::int()
    }
    pub fn long_type() -> Type {
        Type::long()
    }
    pub fn byte_type() -> Type {
        Type::byte()
    }
    pub fn short_type() -> Type {
        Type::short()
    }
    pub fn float_type() -> Type {
        Type::float()
    }
    pub fn double_type() -> Type {
        Type::double()
    }
    pub fn string() -> Type {
        Type::string()
    }
    /// `NamespacedSchema.namespacedString()`.
    pub fn namespaced_string() -> Type {
        Type::namespaced_string()
    }
    pub fn empty_part_type() -> Type {
        Type::empty_part()
    }
    pub fn remainder_type() -> Type {
        Type::remainder()
    }

    // ---- templates ----------------------------------------------------------------------------------------------
    /// `DSL.emptyPart()`.
    pub fn empty_part() -> Template {
        Template::const_type(&Type::empty_part())
    }
    /// `DSL.remainder()`.
    pub fn remainder() -> Template {
        Template::const_type(&Type::remainder())
    }
    /// `DSL.constType(type)`.
    pub fn const_type(ty: &Type) -> Template {
        Template::const_type(ty)
    }
    /// `DSL.check(name, index, element)`.
    pub fn check(name: &str, index: i32, element: &Template) -> Template {
        Template::check(name, index, element)
    }
    /// `DSL.compoundList(element)` (string keys).
    pub fn compound_list(element: &Template) -> Template {
        DSL::compound_list_kv(&DSL::const_type(&Type::string()), element)
    }
    /// `DSL.compoundList(key, element)` = `and(CompoundList(key, element), remainder())`.
    pub fn compound_list_kv(key: &Template, element: &Template) -> Template {
        DSL::and(&Template::compound_list(key, element), &DSL::remainder())
    }
    pub fn hook(template: &Template, pre: &HookFunction, post: &HookFunction) -> Template {
        Template::hook(template, pre, post)
    }
    pub fn list(element: &Template) -> Template {
        Template::list(element)
    }
    pub fn named(name: &str, element: &Template) -> Template {
        Template::named(name, element)
    }
    /// `DSL.and(first, second)`.
    pub fn and(first: &Template, second: &Template) -> Template {
        Template::product(first, second)
    }
    /// `DSL.and(first, rest...)` / `DSL.and(List)`: right-nested products of at least one template.
    pub fn and_list(types: &[Template]) -> Template {
        let (last, init) = types.split_last().expect("Must have at least one type");
        init.iter()
            .rev()
            .fold(last.clone(), |acc, t| DSL::and(t, &acc))
    }
    /// `DSL.allWithRemainder(first, rest...)`.
    pub fn all_with_remainder(first: &Template, rest: &[Template]) -> Template {
        let mut all = vec![first.clone()];
        all.extend(rest.iter().cloned());
        all.push(DSL::remainder());
        DSL::and_list(&all)
    }
    pub fn id(index: i32) -> Template {
        Template::id(index)
    }
    pub fn or(left: &Template, right: &Template) -> Template {
        Template::sum(left, right)
    }
    pub fn field(name: &str, element: &Template) -> Template {
        Template::tag(name, element)
    }
    /// `DSL.optional(template)`.
    pub fn optional(value: &Template) -> Template {
        DSL::or(value, &DSL::empty_part())
    }
    /// `DSL.fields(n1, t1, n2, t2, ...)`: the fields followed by the remainder.
    pub fn fields(fields: &[(&str, Template)]) -> Template {
        let (first, rest) = fields
            .split_first()
            .expect("fields needs at least one field");
        let first = DSL::field(first.0, &first.1);
        let rest: Vec<Template> = rest.iter().map(|(n, t)| DSL::field(n, t)).collect();
        DSL::all_with_remainder(&first, &rest)
    }
    /// `DSL.fields(n1, t1, ..., rest)`: the fields followed by the explicit `rest` template (no remainder added).
    pub fn fields_with_rest(fields: &[(&str, Template)], rest: &Template) -> Template {
        let mut all: Vec<Template> = fields.iter().map(|(n, t)| DSL::field(n, t)).collect();
        all.push(rest.clone());
        DSL::and_list(&all)
    }
    /// `DSL.optionalFields(n1, t1, n2, t2, ...)`: optional fields followed by the remainder.
    pub fn optional_fields(fields: &[(&str, Template)]) -> Template {
        let mut all: Vec<Template> = fields
            .iter()
            .map(|(n, t)| DSL::optional(&DSL::field(n, t)))
            .collect();
        all.push(DSL::remainder());
        DSL::and_list(&all)
    }
    /// `DSL.optionalFields(n1, t1, ..., rest)`: optional fields followed by the explicit `rest` template.
    pub fn optional_fields_with_rest(fields: &[(&str, Template)], rest: &Template) -> Template {
        let mut all: Vec<Template> = fields
            .iter()
            .map(|(n, t)| DSL::optional(&DSL::field(n, t)))
            .collect();
        all.push(rest.clone());
        DSL::and_list(&all)
    }
    /// `DSL.optionalFieldsLazy(Map<String, Supplier<TypeTemplate>>)` (the map iterates in Java `HashMap` order).
    pub fn optional_fields_lazy(fields: &TemplateMap, schema: &Schema) -> Template {
        let mut all: Vec<Template> = fields
            .iter_java_order()
            .into_iter()
            .map(|(name, supplier)| DSL::optional(&DSL::field(&name, &supplier(schema))))
            .collect();
        all.push(DSL::remainder());
        DSL::and_list(&all)
    }
    /// `DSL.taggedChoice(name, keyType, templates)`; `templates` in the iteration order of the Java map.
    pub fn tagged_choice(
        name: &str,
        key_type: &Type,
        templates: Vec<(String, Template)>,
    ) -> Template {
        Template::tagged_choice(name, key_type, templates)
    }
    /// `DSL.taggedChoiceLazy(name, keyType, Map<K, Supplier<TypeTemplate>>)`.
    pub fn tagged_choice_lazy(
        name: &str,
        key_type: &Type,
        templates: &TemplateMap,
        schema: &Schema,
    ) -> Template {
        // `Collectors.toMap` collects into a new `HashMap` in the source map's iteration order
        let evaluated: Vec<(String, Template)> = templates
            .iter_java_order()
            .into_iter()
            .map(|(k, s)| (k, s(schema)))
            .collect();
        let reordered = mc_core::java_collections::hash_map_ordered(
            evaluated,
            |e| mc_core::math::java::java_string_hash(&e.0),
            mc_core::java_collections::HASH_MAP_DEFAULT_CAPACITY,
        );
        Template::tagged_choice(name, key_type, reordered)
    }

    // ---- concrete types -----------------------------------------------------------------------------------------
    /// `DSL.compoundList(Type value)` (string keys).
    pub fn compound_list_type(value: &Type) -> Type {
        Type::compound_list(&Type::string(), value)
    }
    pub fn compound_list_type_kv(key: &Type, value: &Type) -> Type {
        Type::compound_list(key, value)
    }
    pub fn hook_type(ty: &Type, pre: &HookFunction, post: &HookFunction) -> Type {
        Type::hook(ty, pre, post)
    }
    pub fn list_type(element: &Type) -> Type {
        Type::list(element)
    }
    pub fn named_type(name: &str, element: &Type) -> Type {
        Type::named(name, element)
    }
    pub fn and_type(first: &Type, second: &Type) -> Type {
        Type::product(first, second)
    }
    pub fn and_type3(first: &Type, second: &Type, third: &Type) -> Type {
        DSL::and_type(first, &DSL::and_type(second, third))
    }
    pub fn and_type4(first: &Type, second: &Type, third: &Type, fourth: &Type) -> Type {
        DSL::and_type(first, &DSL::and_type3(second, third, fourth))
    }
    pub fn or_type(first: &Type, second: &Type) -> Type {
        Type::sum(first, second)
    }
    pub fn field_type(name: &str, element: &Type) -> Type {
        Type::tag(name, element)
    }
    /// `DSL.optional(Type)`.
    pub fn optional_type(ty: &Type) -> Type {
        Type::sum(ty, &Type::empty_part())
    }
    /// `DSL.taggedChoiceType(name, keyType, types)`; `types` in the iteration order of the Java map.
    pub fn tagged_choice_type(name: &str, key_type: &Type, types: Vec<(String, Type)>) -> Type {
        Type::tagged_choice(name, key_type, types)
    }

    // ---- finders ------------------------------------------------------------------------------------------------
    /// `DSL.remainderFinder()`.
    pub fn remainder_finder() -> OpticFinder {
        crate::finder::remainder_finder()
    }
    /// `DSL.typeFinder(type)`.
    pub fn type_finder(ty: &Type) -> OpticFinder {
        OpticFinder::field(None, ty)
    }
    /// `DSL.fieldFinder(name, type)`.
    pub fn field_finder(name: &str, ty: &Type) -> OpticFinder {
        OpticFinder::field(Some(name), ty)
    }
    /// `DSL.namedChoice(name, type)`.
    pub fn named_choice(name: &str, ty: &Type) -> OpticFinder {
        OpticFinder::named_choice(name, ty)
    }
}
