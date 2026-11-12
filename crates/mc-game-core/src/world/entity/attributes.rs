//! `net.minecraft.world.entity.ai.attributes.*`: `AttributeInstance`, `AttributeMap` (+ the default
//! `AttributeSupplier` of an entity type, from `mc_data`'s `DefaultAttributes` table) and the NBT form of
//! `AttributeInstance.Packed`.

use super::object_open_hash_map::{JavaHash, ObjectOpenHashMap, identifier_hash};
use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_data::Attribute;
use mc_data::EntityType;
use mc_nbt::{CompoundTag, ListTag, Tag};
pub use mc_protocol::packets::play::{AttributeModifier, AttributeModifierOperation};
use std::cell::Cell;

/// `Attribute.sanitizeValue` (`RangedAttribute` clamps, NaN -> minimum).
pub fn sanitize_value(attribute: Attribute, value: f64) -> f64 {
    let data = attribute.data();
    match (data.min_value, data.max_value) {
        (Some(min), Some(max)) => {
            if value.is_nan() {
                min
            } else {
                mc_core::math::mth::clamp_f64(value, min, max)
            }
        }
        _ => value,
    }
}

/// `AttributeInstance`.
#[derive(Clone, Debug)]
pub struct AttributeInstance {
    attribute: Attribute,
    /// `modifiersByOperation` (per-operation open hash maps: `calculateValue` iterates them in table order).
    modifiers_by_operation: [ObjectOpenHashMap<Identifier, AttributeModifier>; 3],
    /// `modifierById` (`Object2ObjectArrayMap`: insertion order, removal shifts).
    modifier_by_id: IndexMap<Identifier, AttributeModifier>,
    permanent_modifiers: IndexMap<Identifier, AttributeModifier>,
    base_value: f64,
    dirty: Cell<bool>,
    cached_value: Cell<f64>,
    /// Modified since the map last handed out `attributesToUpdate` / `attributesToSync`.
    pending_update: bool,
    pending_sync: bool,
}

fn operation_index(op: AttributeModifierOperation) -> usize {
    match op {
        AttributeModifierOperation::AddValue => 0,
        AttributeModifierOperation::AddMultipliedBase => 1,
        AttributeModifierOperation::AddMultipliedTotal => 2,
    }
}

impl AttributeInstance {
    pub fn new(attribute: Attribute) -> Self {
        Self {
            attribute,
            modifiers_by_operation: Default::default(),
            modifier_by_id: IndexMap::new(),
            permanent_modifiers: IndexMap::new(),
            base_value: attribute.data().default_value,
            dirty: Cell::new(true),
            cached_value: Cell::new(0.0),
            pending_update: false,
            pending_sync: false,
        }
    }

    pub fn get_attribute(&self) -> Attribute {
        self.attribute
    }

    pub fn get_base_value(&self) -> f64 {
        self.base_value
    }

    pub fn set_base_value(&mut self, base_value: f64) {
        if base_value != self.base_value {
            self.base_value = base_value;
            self.set_dirty();
        }
    }

    pub fn get_modifiers(&self) -> impl Iterator<Item = &AttributeModifier> {
        self.modifier_by_id.values()
    }

    /// `getModifiers(operation)` (`@VisibleForTesting`): the per-operation map in its iteration order.
    pub fn get_modifiers_for_operation(
        &self,
        operation: AttributeModifierOperation,
    ) -> impl Iterator<Item = &AttributeModifier> {
        self.modifiers_by_operation[operation_index(operation)].values()
    }

    pub fn get_permanent_modifiers(&self) -> impl Iterator<Item = &AttributeModifier> {
        self.permanent_modifiers.values()
    }

    pub fn get_modifier(&self, id: &Identifier) -> Option<&AttributeModifier> {
        self.modifier_by_id.get(id)
    }

    pub fn has_modifier(&self, id: &Identifier) -> bool {
        self.modifier_by_id.contains_key(id)
    }

    fn add_modifier(&mut self, modifier: AttributeModifier) {
        assert!(
            !self.modifier_by_id.contains_key(&modifier.id),
            "Modifier is already applied on this attribute!"
        );
        self.modifier_by_id
            .insert(modifier.id.clone(), modifier.clone());
        self.modifiers_by_operation[operation_index(modifier.operation)]
            .insert(modifier.id.clone(), modifier);
        self.set_dirty();
    }

    /// `addOrUpdateTransientModifier`. shortcut: vanilla compares by object identity (`modifier != oldModifier`),
    /// here an equal value counts as the same object; all vanilla callers pass shared constants.
    pub fn add_or_update_transient_modifier(&mut self, modifier: AttributeModifier) {
        let old = self
            .modifier_by_id
            .insert(modifier.id.clone(), modifier.clone());
        if old.as_ref() != Some(&modifier) {
            self.modifiers_by_operation[operation_index(modifier.operation)]
                .insert(modifier.id.clone(), modifier);
            self.set_dirty();
        }
    }

    pub fn add_transient_modifier(&mut self, modifier: AttributeModifier) {
        self.add_modifier(modifier);
    }

    pub fn add_or_replace_permanent_modifier(&mut self, modifier: AttributeModifier) {
        self.remove_modifier(&modifier.id);
        self.add_permanent_modifier(modifier);
    }

    pub fn add_permanent_modifier(&mut self, modifier: AttributeModifier) {
        self.add_modifier(modifier.clone());
        self.permanent_modifiers
            .insert(modifier.id.clone(), modifier);
    }

    /// `addPermanentModifiers`.
    pub fn add_permanent_modifiers<'a>(
        &mut self,
        modifiers: impl IntoIterator<Item = &'a AttributeModifier>,
    ) {
        for m in modifiers {
            self.add_permanent_modifier(m.clone());
        }
    }

    fn set_dirty(&mut self) {
        self.dirty.set(true);
        self.pending_update = true;
        self.pending_sync = true;
    }

    pub fn remove_modifier(&mut self, id: &Identifier) -> bool {
        let Some(modifier) = self.modifier_by_id.shift_remove(id) else {
            return false;
        };
        self.modifiers_by_operation[operation_index(modifier.operation)].remove(id);
        self.permanent_modifiers.shift_remove(id);
        self.set_dirty();
        true
    }

    pub fn remove_modifiers(&mut self) {
        let ids: Vec<Identifier> = self.modifier_by_id.keys().cloned().collect();
        for id in ids {
            self.remove_modifier(&id);
        }
    }

    pub fn get_value(&self) -> f64 {
        if self.dirty.get() {
            self.cached_value.set(self.calculate_value());
            self.dirty.set(false);
        }
        self.cached_value.get()
    }

    fn calculate_value(&self) -> f64 {
        let mut base = self.base_value;
        for m in self.modifiers_by_operation[0].values() {
            base += m.amount;
        }
        let mut result = base;
        for m in self.modifiers_by_operation[1].values() {
            result += base * m.amount;
        }
        for m in self.modifiers_by_operation[2].values() {
            result *= 1.0 + m.amount;
        }
        sanitize_value(self.attribute, result)
    }

    pub fn replace_from(&mut self, other: &AttributeInstance) {
        self.base_value = other.base_value;
        self.modifier_by_id = other.modifier_by_id.clone();
        self.permanent_modifiers = other.permanent_modifiers.clone();
        // `getModifiers(operation).putAll(otherMap)`: re-inserted in the other map's iteration order
        self.modifiers_by_operation = Default::default();
        for (i, map) in other.modifiers_by_operation.iter().enumerate() {
            for m in map.values() {
                self.modifiers_by_operation[i].insert(m.id.clone(), m.clone());
            }
        }
        self.set_dirty();
    }

    /// `apply(Packed)`.
    pub fn apply(&mut self, base_value: f64, modifiers: Vec<AttributeModifier>) {
        self.base_value = base_value;
        for m in modifiers {
            self.modifier_by_id.insert(m.id.clone(), m.clone());
            self.modifiers_by_operation[operation_index(m.operation)]
                .insert(m.id.clone(), m.clone());
            self.permanent_modifiers.insert(m.id.clone(), m);
        }
        self.set_dirty();
    }

    /// `pack()` as NBT (`AttributeInstance.Packed.CODEC`).
    pub fn pack(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_string("id", self.attribute.identifier().to_string());
        tag.put_double("base", self.base_value);
        if !self.permanent_modifiers.is_empty() {
            let mut list = ListTag::new();
            for m in self.permanent_modifiers.values() {
                let mut mt = CompoundTag::new();
                mt.put_string("id", m.id.to_string());
                mt.put_double("amount", m.amount);
                mt.put_string("operation", operation_name(m.operation));
                list.push(Tag::Compound(mt));
            }
            tag.put("modifiers", list);
        }
        tag
    }
}

fn operation_name(op: AttributeModifierOperation) -> &'static str {
    match op {
        AttributeModifierOperation::AddValue => "add_value",
        AttributeModifierOperation::AddMultipliedBase => "add_multiplied_base",
        AttributeModifierOperation::AddMultipliedTotal => "add_multiplied_total",
    }
}

fn parse_modifier(tag: &Tag) -> Option<AttributeModifier> {
    let c = tag.as_compound()?;
    let id = Identifier::parse(c.get_string("id")?).ok()?;
    let amount = c.get("amount")?.as_number()?.double_value();
    let operation = match c.get_string("operation")? {
        "add_value" => AttributeModifierOperation::AddValue,
        "add_multiplied_base" => AttributeModifierOperation::AddMultipliedBase,
        "add_multiplied_total" => AttributeModifierOperation::AddMultipliedTotal,
        _ => return None,
    };
    Some(AttributeModifier {
        id,
        amount,
        operation,
    })
}

/// `Holder.Reference.hashCode()` as the oracle pins it (registry key hash: `registry().hashCode() * 31 + identifier().hashCode()`;
/// vanilla's identity hash gives an arbitrary order), so `AttributeMap.attributes` iterates like the recorded runs.
impl JavaHash for Attribute {
    fn java_hash(&self) -> i32 {
        let registry = Identifier::parse("minecraft:attribute").expect("registry id");
        identifier_hash(&registry)
            .wrapping_mul(31)
            .wrapping_add(identifier_hash(&self.identifier()))
    }
}

/// `AttributeMap`: instances are created lazily from the entity type's default supplier. `attributes` keeps insertion order
/// (`attributesToUpdate`/`attributesToSync` are identity-hash sets in vanilla); [`AttributeMap::in_map_order`] is the iteration
/// order of vanilla's `Object2ObjectOpenHashMap`.
#[derive(Clone, Debug)]
pub struct AttributeMap {
    supplier: EntityType,
    attributes: IndexMap<Attribute, AttributeInstance>,
}

impl AttributeMap {
    /// `new AttributeMap(DefaultAttributes.getSupplier(type))`.
    pub fn new(supplier: EntityType) -> Self {
        Self {
            supplier,
            attributes: IndexMap::new(),
        }
    }

    fn supplier_base(&self, attribute: Attribute) -> Option<f64> {
        self.supplier
            .data()
            .default_attributes
            .iter()
            .find(|(a, _)| *a == attribute)
            .map(|(_, v)| *v)
    }

    pub fn supplier_has_attribute(&self, attribute: Attribute) -> bool {
        self.supplier_base(attribute).is_some()
    }

    /// `getInstance(attribute)`: `None` if the entity type has no such attribute.
    pub fn get_instance(&mut self, attribute: Attribute) -> Option<&mut AttributeInstance> {
        if !self.attributes.contains_key(&attribute) {
            let base = self.supplier_base(attribute)?;
            let mut instance = AttributeInstance::new(attribute);
            // `supplier.createInstance`: `result.replaceFrom(template)` ends in `setDirty()`, so a fresh instance
            // is queued for `attributesToUpdate` / `attributesToSync`
            instance.base_value = base;
            instance.set_dirty();
            self.attributes.insert(attribute, instance);
        }
        self.attributes.get_mut(&attribute)
    }

    /// `attributes.values()` in the open hash map's iteration order.
    fn in_map_order(&self) -> impl Iterator<Item = &AttributeInstance> {
        let mut table = ObjectOpenHashMap::new();
        for (index, attribute) in self.attributes.keys().enumerate() {
            table.insert(*attribute, index);
        }
        let order: Vec<usize> = table.values().copied().collect();
        order.into_iter().map(|index| &self.attributes[index])
    }

    pub fn has_attribute(&self, attribute: Attribute) -> bool {
        self.attributes.contains_key(&attribute) || self.supplier_has_attribute(attribute)
    }

    pub fn has_modifier(&self, attribute: Attribute, id: &Identifier) -> bool {
        self.attributes
            .get(&attribute)
            .is_some_and(|i| i.has_modifier(id))
    }

    /// `getValue(attribute)`; panics like vanilla when the type doesn't have the attribute.
    pub fn get_value(&self, attribute: Attribute) -> f64 {
        // does not create the instance (only `getInstance` does): the supplier's default is answered directly
        match self.attributes.get(&attribute) {
            Some(i) => i.get_value(),
            None => match self.supplier_base(attribute) {
                Some(base) => sanitize_value(attribute, base),
                None => panic!("Can't find attribute {}", attribute.identifier()),
            },
        }
    }

    pub fn get_base_value(&self, attribute: Attribute) -> f64 {
        match self.attributes.get(&attribute) {
            Some(i) => i.get_base_value(),
            None => self
                .supplier_base(attribute)
                .unwrap_or_else(|| panic!("Can't find attribute {}", attribute.identifier())),
        }
    }

    /// `getAttributesToUpdate()`: instances modified since the last call (clears the marks).
    pub fn take_attributes_to_update(&mut self) -> Vec<Attribute> {
        let mut out = Vec::new();
        for (a, i) in self.attributes.iter_mut() {
            if std::mem::take(&mut i.pending_update) {
                out.push(*a);
            }
        }
        out
    }

    /// `getAttributesToSync()`: modified syncable instances (clears the mark of the returned ones).
    pub fn take_attributes_to_sync(&mut self) -> Vec<Attribute> {
        let mut out = Vec::new();
        for (a, i) in self.attributes.iter_mut() {
            if i.attribute.data().syncable && std::mem::take(&mut i.pending_sync) {
                out.push(*a);
            }
        }
        out
    }

    /// `getSyncableAttributes()`.
    pub fn get_syncable_attributes(&self) -> Vec<&AttributeInstance> {
        self.in_map_order()
            .filter(|i| i.attribute.data().syncable)
            .collect()
    }

    pub fn instances(&self) -> impl Iterator<Item = &AttributeInstance> {
        self.in_map_order()
    }

    pub fn add_transient_attribute_modifiers(
        &mut self,
        modifiers: &[(Attribute, AttributeModifier)],
    ) {
        for (attribute, modifier) in modifiers {
            if let Some(instance) = self.get_instance(*attribute) {
                instance.remove_modifier(&modifier.id);
                instance.add_transient_modifier(modifier.clone());
            }
        }
    }

    pub fn remove_attribute_modifiers(&mut self, modifiers: &[(Attribute, AttributeModifier)]) {
        for (attribute, modifier) in modifiers {
            if let Some(instance) = self.attributes.get_mut(attribute) {
                instance.remove_modifier(&modifier.id);
            }
        }
    }

    pub fn assign_all_values(&mut self, other: &AttributeMap) {
        for other_instance in other.in_map_order() {
            if let Some(own) = self.get_instance(other_instance.attribute) {
                own.replace_from(other_instance);
            }
        }
    }

    pub fn assign_base_values(&mut self, other: &AttributeMap) {
        for other_instance in other.in_map_order() {
            if let Some(own) = self.get_instance(other_instance.attribute) {
                own.set_base_value(other_instance.base_value);
            }
        }
    }

    pub fn assign_permanent_modifiers(&mut self, other: &AttributeMap) {
        for other_instance in other.in_map_order() {
            if let Some(own) = self.get_instance(other_instance.attribute) {
                for m in other_instance.get_permanent_modifiers() {
                    own.add_permanent_modifier(m.clone());
                }
            }
        }
    }

    pub fn reset_base_value(&mut self, attribute: Attribute) -> bool {
        let Some(base) = self.supplier_base(attribute) else {
            return false;
        };
        if let Some(i) = self.attributes.get_mut(&attribute) {
            i.set_base_value(base);
        }
        true
    }

    /// `pack()` as the NBT list (`AttributeInstance.Packed.LIST_CODEC`).
    pub fn pack(&self) -> ListTag {
        let mut list = ListTag::new();
        for i in self.in_map_order() {
            list.push(Tag::Compound(i.pack()));
        }
        list
    }

    /// `apply(List<Packed>)` from NBT: elements that fail to decode are skipped (partial list results).
    pub fn apply(&mut self, packed: &ListTag) {
        for tag in packed.iter() {
            let Some(c) = tag.as_compound() else { continue };
            let Some(attribute) = c
                .get_string("id")
                .and_then(|n| Identifier::parse(n).ok())
                .and_then(|id| Attribute::by_name(&id.to_string()))
            else {
                continue;
            };
            let base = c
                .get("base")
                .and_then(|t| t.as_number())
                .map_or(0.0, |n| n.double_value());
            let modifiers: Vec<AttributeModifier> = c
                .get_list("modifiers")
                .map(|l| l.iter().filter_map(parse_modifier).collect())
                .unwrap_or_default();
            if let Some(instance) = self.get_instance(attribute) {
                instance.apply(base, modifiers);
            }
        }
    }
}
