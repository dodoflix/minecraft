//! `Schema` (+ `NamespacedSchema`): the types of one data version, built from registered templates.
//!
//! A schema class in Java overrides `registerEntities` / `registerBlockEntities` / `registerTypes` and chains to the
//! parent *instance* through `super`; here that is a [`SchemaDef`] with the same three methods. `this` is the schema
//! whose definition runs, `schema` the schema being built (they differ when a child schema calls into its parent
//! chain). Suppliers receive the schema being built as an argument instead of capturing it.

use crate::dsl::{DSL, TypeReference};
use crate::family::RecursiveTypeFamily;
use crate::template::Template;
use crate::ty::{TaggedChoiceType, Type, ensure_namespaced};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// `Supplier<TypeTemplate>`; gets the schema being built.
pub type TemplateSupplier = Arc<dyn Fn(&Schema) -> Template + Send + Sync>;

/// `Map<String, Supplier<TypeTemplate>>` as built with `Maps.newHashMap()`: iteration follows Java's `HashMap` order
/// ([`TemplateMap::iter_java_order`]), including the table growth caused by entries that were later removed.
#[derive(Clone, Default)]
pub struct TemplateMap {
    entries: Vec<(String, TemplateSupplier)>,
    max_len: usize,
}

impl TemplateMap {
    pub fn new() -> TemplateMap {
        TemplateMap::default()
    }

    /// `map.put(name, supplier)` (replaces in place when present).
    pub fn put_supplier(&mut self, name: &str, supplier: TemplateSupplier) {
        match self.entries.iter_mut().find(|(k, _)| k == name) {
            Some(e) => e.1 = supplier,
            None => {
                self.entries.push((name.to_string(), supplier));
                self.max_len = self.max_len.max(self.entries.len());
            }
        }
    }

    /// `map.put(name, () -> template)`.
    pub fn put(&mut self, name: &str, supplier: impl Fn(&Schema) -> Template + Send + Sync + 'static) {
        self.put_supplier(name, Arc::new(supplier));
    }

    /// `map.remove(name)`.
    pub fn remove(&mut self, name: &str) -> Option<TemplateSupplier> {
        let i = self.entries.iter().position(|(k, _)| k == name)?;
        Some(self.entries.remove(i).1)
    }

    pub fn get(&self, name: &str) -> Option<&TemplateSupplier> {
        self.entries.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries in `java.util.HashMap` iteration order.
    pub fn iter_java_order(&self) -> Vec<(String, TemplateSupplier)> {
        let mut cap = mc_core::java_collections::HASH_MAP_DEFAULT_CAPACITY;
        while self.max_len > (cap as f32 * 0.75f32) as usize {
            cap *= 2;
        }
        mc_core::java_collections::hash_map_ordered(self.entries.clone(), |e| mc_core::math::java::java_string_hash(&e.0), cap)
    }
}

/// The overridable part of a `Schema` subclass. All three methods default to the parent chain (`super`).
pub trait SchemaDef: Send + Sync + 'static {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        this.super_register_entities(schema)
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        this.super_register_block_entities(schema)
    }
    fn register_types(&self, this: &Schema, schema: &Schema, entity_types: &TemplateMap, block_entity_types: &TemplateMap) {
        this.super_register_types(schema, entity_types, block_entity_types)
    }
    /// `true` for `NamespacedSchema` subclasses (`getChoiceType` namespaces the choice name).
    fn namespaced(&self) -> bool {
        false
    }
}

/// `new Schema(versionKey, parent)` without overrides (`DataFixers.SAME`).
struct Same;
impl SchemaDef for Same {}

/// `new NamespacedSchema(versionKey, parent)` without overrides (`DataFixers.SAME_NAMESPACED`).
struct SameNamespaced;
impl SchemaDef for SameNamespaced {
    fn namespaced(&self) -> bool {
        true
    }
}

struct SchemaInner {
    version_key: i32,
    name: String,
    parent: Option<Schema>,
    def: Box<dyn SchemaDef>,
    /// `recursiveTypes` (name -> recursion index), in put order.
    recursive_types: Mutex<Vec<(String, i32)>>,
    type_templates: Mutex<HashMap<String, TemplateSupplier>>,
    types: OnceLock<HashMap<String, Type>>,
}

#[derive(Clone)]
pub struct Schema(Arc<SchemaInner>);

impl Schema {
    /// `new Schema(versionKey, parent)` of a subclass with definition `def`.
    pub fn new(version_key: i32, parent: Option<Schema>, def: impl SchemaDef) -> Schema {
        let sub = version_key % 10;
        let name = format!("V{}{}", version_key / 10, if sub == 0 { String::new() } else { format!(".{sub}") });
        let schema = Schema(Arc::new(SchemaInner {
            version_key,
            name,
            parent,
            def: Box::new(def),
            recursive_types: Mutex::new(Vec::new()),
            type_templates: Mutex::new(HashMap::new()),
            types: OnceLock::new(),
        }));
        let entities = schema.0.def.register_entities(&schema, &schema);
        let block_entities = schema.0.def.register_block_entities(&schema, &schema);
        schema.0.def.register_types(&schema, &schema, &entities, &block_entities);
        let types = schema.build_types();
        let _ = schema.0.types.set(types);
        schema
    }

    /// `Schema::new` as a `BiFunction` (`DataFixers.SAME`).
    pub fn same(version_key: i32, parent: Option<Schema>) -> Schema {
        Schema::new(version_key, parent, Same)
    }

    /// `NamespacedSchema::new` as a `BiFunction` (`DataFixers.SAME_NAMESPACED`).
    pub fn same_namespaced(version_key: i32, parent: Option<Schema>) -> Schema {
        Schema::new(version_key, parent, SameNamespaced)
    }

    fn build_types(&self) -> HashMap<String, Type> {
        let recursive: Vec<(String, i32)> = self.0.recursive_types.lock().unwrap().clone();
        // `Object2IntOpenHashMap` iteration order decides how the family template nests its `or`s
        let ordered = mc_core::java_collections::fast_open_hash_order_string(16, recursive.clone());
        let template = ordered
            .iter()
            .map(|(name, index)| DSL::check(name, *index, &self.get_template(name)))
            .reduce(|a, b| DSL::or(&a, &b))
            .expect("schema without recursive types");
        let family = RecursiveTypeFamily::new(&self.0.name, template);
        let names: Vec<String> = self.0.type_templates.lock().unwrap().keys().cloned().collect();
        let mut types = HashMap::new();
        for name in names {
            let ty = match recursive.iter().find(|(n, _)| *n == name) {
                Some((_, id)) => family.apply(*id),
                None => self.get_template(&name).apply_at(&family, -1),
            };
            types.insert(name, ty);
        }
        types
    }

    pub fn version_key(&self) -> i32 {
        self.0.version_key
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn parent(&self) -> Option<&Schema> {
        self.0.parent.as_ref()
    }
    /// `schema.types()`: the registered type names.
    pub fn types(&self) -> Vec<String> {
        self.0.types.get().map(|t| t.keys().cloned().collect()).unwrap_or_default()
    }

    // ---- super calls ----------------------------------------------------------------------------------------------

    fn parent_or_panic(&self) -> &Schema {
        self.0.parent.as_ref().unwrap_or_else(|| panic!("schema {} has no parent to delegate to", self.0.name))
    }
    /// `super.registerEntities(schema)` (this = the schema whose definition is running).
    pub fn super_register_entities(&self, schema: &Schema) -> TemplateMap {
        let p = self.parent_or_panic();
        p.0.def.register_entities(p, schema)
    }
    pub fn super_register_block_entities(&self, schema: &Schema) -> TemplateMap {
        let p = self.parent_or_panic();
        p.0.def.register_block_entities(p, schema)
    }
    pub fn super_register_types(&self, schema: &Schema, entity_types: &TemplateMap, block_entity_types: &TemplateMap) {
        let p = self.parent_or_panic();
        p.0.def.register_types(p, schema, entity_types, block_entity_types)
    }

    // ---- registration -------------------------------------------------------------------------------------------

    /// `schema.registerSimple(map, name)`.
    pub fn register_simple(&self, map: &mut TemplateMap, name: &str) {
        map.put(name, |_| DSL::remainder());
    }

    /// `schema.register(map, name, Function<String, TypeTemplate>)`.
    pub fn register(&self, map: &mut TemplateMap, name: &str, template: impl Fn(&Schema, &str) -> Template + Send + Sync + 'static) {
        let n = name.to_string();
        map.put(name, move |schema| template(schema, &n));
    }

    /// `schema.register(map, name, Supplier<TypeTemplate>)`.
    pub fn register_with(&self, map: &mut TemplateMap, name: &str, template: impl Fn(&Schema) -> Template + Send + Sync + 'static) {
        map.put(name, template);
    }

    /// `schema.registerType(recursive, type, template)`.
    pub fn register_type(&self, recursive: bool, ty: &TypeReference, template: impl Fn(&Schema) -> Template + Send + Sync + 'static) {
        let name = ty.type_name();
        self.0.type_templates.lock().unwrap().insert(name.to_string(), Arc::new(template));
        let mut rec = self.0.recursive_types.lock().unwrap();
        if recursive && !rec.iter().any(|(n, _)| n == name) {
            let id = rec.len() as i32;
            rec.push((name.to_string(), id));
        }
    }

    // ---- lookup -------------------------------------------------------------------------------------------------

    /// `schema.resolveTemplate(name)`.
    pub fn resolve_template(&self, name: &str) -> Template {
        let supplier = self.0.type_templates.lock().unwrap().get(name).cloned();
        match supplier {
            Some(s) => s(self),
            None => panic!("Unknown type: {name}"),
        }
    }

    /// `schema.id(name)`: a recursion point for recursive types, else the named template.
    pub fn id(&self, name: &str) -> Template {
        let id = self.0.recursive_types.lock().unwrap().iter().find(|(n, _)| n == name).map(|(_, i)| *i);
        match id {
            Some(i) => DSL::id(i),
            None => self.get_template(name),
        }
    }

    fn get_template(&self, name: &str) -> Template {
        DSL::named(name, &self.resolve_template(name))
    }

    fn type_named(&self, name: &str) -> &Type {
        self.0.types.get().and_then(|t| t.get(name)).unwrap_or_else(|| panic!("Unknown type: {name}"))
    }

    /// `schema.getTypeRaw(type)`.
    pub fn get_type_raw(&self, ty: &TypeReference) -> Type {
        self.type_named(ty.type_name()).clone()
    }

    /// `schema.getType(type)`: recursion points are unwrapped to the checked (choice) type.
    pub fn get_type(&self, ty: &TypeReference) -> Type {
        let t = self.type_named(ty.type_name());
        if t.is_recursive_point() {
            t.find_checked_type(-1).expect("Could not find choice type in the recursive type")
        } else {
            t.clone()
        }
    }

    /// `schema.findChoiceType(type)`.
    pub fn find_choice_type(&self, ty: &TypeReference) -> TaggedChoiceType {
        self.get_type(ty).find_choice_type("id", -1).expect("Not a choice type")
    }

    /// `schema.getChoiceType(type, choiceName)`.
    pub fn get_choice_type(&self, ty: &TypeReference, choice_name: &str) -> Type {
        let owned;
        let choice_name = if self.0.def.namespaced() {
            owned = ensure_namespaced(choice_name);
            owned.as_str()
        } else {
            choice_name
        };
        let choice = self.find_choice_type(ty);
        match choice.get(choice_name) {
            Some(t) => t.clone(),
            None => panic!("Data fixer not registered for: {choice_name} in {}", ty.type_name()),
        }
    }
}

impl std::fmt::Debug for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Schema[{}]", self.0.name)
    }
}
