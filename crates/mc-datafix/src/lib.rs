//! Port of `net.minecraft.util.datafix` on top of a Rust port of the DFU type system (`com.mojang.datafixers`):
//! schemas V99..current, all fixes, `DataFixTypes`, and the global `DataFixer`.
//!
//! # Architecture
//! DFU's statically typed `Type<A>`/`Typed<A>`/optics are ported *dynamically*: a [`typed::Typed`] is
//! `{ ty: Type, ops: Ops, value: Value }`; [`ty::Type`] is a cheaply clonable structural type tree with Java's exact
//! `equals(ignoreRecursionPoints, checkIndex)`; [`value::Value`] mirrors the Java runtime representation (`Pair`,
//! `Either`, `List`, `Dynamic` remainder, ...); optics are paths computed from types ([`optic::TypedOptic`]) with
//! DFU's exact search order and `tType` computation. `TypeRewriteRule`/`RewriteResult`/`View` are ported faithfully
//! including the recursive-family (`Mu`) cata ([`family::RecursiveTypeFamily`]) and the rewrite cache. The PointFree
//! *optimizer* is not ported (performance only). Everything is `Send + Sync`.
//!
//! Deviations (all unobservable in vanilla): `findType(.., recurse = true)` through a recursive family (used by no
//! vanilla fix), `TypeTemplate.findFieldOrType/applyO/toSimpleType`, `Type.updateMu`, `DataFixerBuilder.Result.optimize`,
//! codec error *messages* for missing keys (`No key X in MapLike[..]` has no map dump).
//!
//! # How to port Java DFU code
//! Names keep the Java names in snake_case; a Java `Typed<?>` is [`typed::Typed`], `OpticFinder<T>` is
//! [`finder::OpticFinder`], `Optional` is `Option`, `Pair` is a [`value::Value::Pair`] inside values and a tuple
//! elsewhere, `Either` is `Value::Left/Right`, an `optional(..)` field is `Value::some(v)` / `Value::none()`.
//! Java overloads on `TypeTemplate` vs `Type` become `name` vs `name_type` ([`dsl::DSL`]). `Typed` update methods
//! consume `self` (clone if you still need it); getters borrow. Finder misuse panics (Java throws).
//!
//! | Java | Rust |
//! |---|---|
//! | `DSL.optionalFields("Items", DSL.list(References.ITEM_STACK.in(schema)))` | `DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(schema)))])` |
//! | `DSL.optionalFields("a", t1, "b", t2, rest)` / `DSL.fields(..)` | `DSL::optional_fields_with_rest(&[("a", t1), ("b", t2)], &rest)` / `DSL::fields(&[..])` |
//! | `DSL.and(a, b, c)` / `DSL.or(a, b)` / `DSL.optional(DSL.field("x", t))` | `DSL::and_list(&[a, b, c])` / `DSL::or(&a, &b)` / `DSL::optional(&DSL::field("x", &t))` |
//! | `DSL.and(typeA, typeB)`, `DSL.named(n, type)`, `DSL.list(type)` (on `Type`s) | `DSL::and_type(&a, &b)`, `DSL::named_type(n, &t)`, `DSL::list_type(&t)` |
//! | `schema.register(map, "Item", name -> tmpl)` / `registerSimple(map, "Zombie")` | `schema.register(&mut map, "Item", \|schema, name\| tmpl)` / `schema.register_simple(&mut map, "Zombie")` |
//! | `schema.registerType(true, References.ENTITY, () -> DSL.taggedChoiceLazy("id", namespacedString(), entityTypes))` | `schema.register_type(true, &References::ENTITY, move \|s\| DSL::tagged_choice_lazy("id", &DSL::namespaced_string(), &entity_types, s))` |
//! | `fixTypeEverywhereTyped("X", type, input -> input.update(DSL.remainderFinder(), this::fixTag))` | `fix.fix_type_everywhere_typed("X", &ty, \|input\| input.update_remainder(fix_tag))` |
//! | `OpticFinder<Pair<String,String>> idF = DSL.fieldFinder("id", DSL.named(References.ITEM_NAME.typeName(), NamespacedSchema.namespacedString()))`; `input.getOptional(idF)` | `let id_f = DSL::field_finder("id", &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()));` `input.get_optional(&id_f)` (a `Value::Pair`; `.into_named()` gives `(String, Value)`) |
//! | `this.fixTypeEverywhere("n", type, ops -> input -> input.mapSecond(this::rename))` | `fix.fix_type_everywhere("n", &ty, move \|_ops, input\| input.map_second(\|v\| Value::Str(rename(v.as_str()))))` |
//! | `Either.right(Unit.INSTANCE)`, `Pair.of(a, b)`, `pair.getFirst()` | `Value::none()` (or `Value::right(Value::Unit)`), `Value::pair(a, b)`, `v.first()` |
//! | `typed.updateTyped(entityF, outputType, this::fix)` / `typed.set(finder, value)` | `typed.update_typed_as(&entity_f, &output_type, fix)` / `typed.set(&finder, value)` |
//! | `fixerUpper.addSchema(100, V100::new)` / `addSchema(101, SAME)` / `addSchema(808, 1, SAME_NAMESPACED)` / `addFixer(new X(v100))` | `let v100 = b.add_schema(100, schemas::v100::new);` / `b.add_schema(101, Schema::same);` / `b.add_schema_sub(808, 1, Schema::same_namespaced);` / `b.add_fixer(X::new(&v100, true));` |
//!
//! A schema class is a [`schema::SchemaDef`] (overrides of `registerEntities` / `registerBlockEntities` /
//! `registerTypes`, calling `this.super_register_*` for `super.registerX`), wrapped by a constructor
//! `pub fn new(key: i32, parent: Option<Schema>) -> Schema { Schema::new(key, parent, V100) }`.
//! Entity/block-entity maps are [`schema::TemplateMap`]s (Java `HashMap` iteration order is reproduced).
//! A fix is a constructor `pub fn new(schema: &Schema, changes_type: bool) -> DataFix` calling
//! [`datafix::DataFix::new`] with the body of `makeRule()`; abstract base fixes (`NamedEntityFix`, ...) are
//! constructors taking closures. Hook functions (`Hook.HookFunction`) are [`ty::HookFunction`] values kept in a
//! `LazyLock` per Java constant (equality is identity).
//!
//! Observable-order rule (design contract): fix logic that iterates a Java `HashMap`/`HashSet` and builds ordered output
//! must iterate in Java's order (`mc_core::java_collections`, `mc_dfu::extra::java_hash_map_order`).

pub mod data_fixers;
pub mod datafix;
pub mod datafix_types;
pub mod dsl;
pub mod extra_data_fix_utils;
pub mod family;
pub mod finder;
pub mod fixer;
pub mod fixers;
pub mod fixes;
pub mod optic;
pub mod rewrite;
pub mod schema;
pub mod schemas;
pub mod template;
pub mod ty;
pub mod typed;
pub mod util;
pub mod value;

pub use data_fixers::data_fixer;
pub use datafix::DataFix;
pub use datafix_types::DataFixTypes;
pub use dsl::{DSL, TypeReference};
pub use finder::OpticFinder;
pub use fixer::{DataFixer, DataFixerBuilder};
pub use fixes::references::References;
pub use rewrite::TypeRewriteRule;
pub use schema::{Schema, SchemaDef, TemplateMap};
pub use template::Template;
pub use ty::{HookFunction, TaggedChoiceType, Type};
pub use typed::Typed;
pub use value::Value;
