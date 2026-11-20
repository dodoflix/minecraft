//! `net.minecraft.util.datafix.schemas.V3818_3`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap, TemplateSupplier};
use crate::template::Template;
use std::sync::Arc;

pub struct V3818_3;

impl SchemaDef for V3818_3 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::DATA_COMPONENTS, |s| {
            components().to_template(s)
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3818_3)
}

/// A `SequencedMap<String, Supplier<TypeTemplate>>` of data component templates (a `LinkedHashMap`): iteration is in
/// insertion order, and a `put` of a present key keeps its place.
#[derive(Default)]
pub(crate) struct Components(Vec<(&'static str, TemplateSupplier)>);

impl Components {
    pub(crate) fn put(
        &mut self,
        name: &'static str,
        template: impl Fn(&Schema) -> Template + Send + Sync + 'static,
    ) {
        let supplier: TemplateSupplier = Arc::new(template);
        match self.0.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = supplier,
            None => self.0.push((name, supplier)),
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.0.retain(|(n, _)| *n != name);
    }

    /// `DSL.optionalFieldsLazy(components)`.
    pub(crate) fn to_template(&self, schema: &Schema) -> Template {
        let mut all: Vec<Template> = self
            .0
            .iter()
            .map(|(name, supplier)| DSL::optional(&DSL::field(name, &supplier(schema))))
            .collect();
        all.push(DSL::remainder());
        DSL::and_list(&all)
    }
}

fn adventure_blocks(s: &Schema) -> Template {
    DSL::optional_fields(&[(
        "predicates",
        DSL::list(&DSL::optional_fields(&[(
            "blocks",
            DSL::or(
                &References::BLOCK_NAME.in_(s),
                &DSL::list(&References::BLOCK_NAME.in_(s)),
            ),
        )])),
    )])
}

/// `V3818_3.components`.
pub(crate) fn components() -> Components {
    let mut components = Components::default();
    components.put("minecraft:bees", |s| {
        DSL::list(&DSL::optional_fields(&[(
            "entity_data",
            References::ENTITY_TREE.in_(s),
        )]))
    });
    components.put("minecraft:block_entity_data", |s| {
        References::BLOCK_ENTITY.in_(s)
    });
    components.put("minecraft:bundle_contents", |s| {
        DSL::list(&References::ITEM_STACK.in_(s))
    });
    components.put("minecraft:can_break", adventure_blocks);
    components.put("minecraft:can_place_on", adventure_blocks);
    components.put("minecraft:charged_projectiles", |s| {
        DSL::list(&References::ITEM_STACK.in_(s))
    });
    components.put("minecraft:container", |s| {
        DSL::list(&DSL::optional_fields(&[(
            "item",
            References::ITEM_STACK.in_(s),
        )]))
    });
    components.put("minecraft:entity_data", |s| References::ENTITY_TREE.in_(s));
    components.put("minecraft:pot_decorations", |s| {
        DSL::list(&References::ITEM_NAME.in_(s))
    });
    components.put("minecraft:food", |s| {
        DSL::optional_fields(&[("using_converts_to", References::ITEM_STACK.in_(s))])
    });
    components.put("minecraft:custom_name", |s| {
        References::TEXT_COMPONENT.in_(s)
    });
    components.put("minecraft:item_name", |s| References::TEXT_COMPONENT.in_(s));
    components.put("minecraft:lore", |s| {
        DSL::list(&References::TEXT_COMPONENT.in_(s))
    });
    components.put("minecraft:written_book_content", |s| {
        DSL::optional_fields(&[(
            "pages",
            DSL::list(&DSL::or(
                &DSL::optional_fields(&[
                    ("raw", References::TEXT_COMPONENT.in_(s)),
                    ("filtered", References::TEXT_COMPONENT.in_(s)),
                ]),
                &References::TEXT_COMPONENT.in_(s),
            )),
        )])
    });
    components
}
