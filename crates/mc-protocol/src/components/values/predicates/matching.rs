//! Evaluation of the predicates (`matches` / `test`).

use super::block::BlockPredicate;
use super::component_predicates::*;
use super::matchers::{DataComponentMatchers, DataComponentPredicate};
use super::state::{PropertyMatcher, StatePropertiesPredicate, ValueMatcher};
use super::{AdventureModePredicate, CollectionPredicate};
use crate::components::holder::{Holder, HolderSet, NoDirect};
use crate::components::item_stack::{ItemStack, ItemStackTemplate};
use crate::components::map::DataComponentGetter;
use crate::components::values::basic::{
    CUSTOM_DATA, DAMAGE, ENCHANTMENTS, MAX_DAMAGE, STORED_ENCHANTMENTS,
};
use crate::components::values::containers::{BUNDLE_CONTENTS, CONTAINER};
use crate::components::values::decor::{
    FIREWORK_EXPLOSION, FIREWORKS, FireworkExplosion, JUKEBOX_PLAYABLE, TRIM,
    WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
};
use crate::components::values::effects::{
    ATTRIBUTE_MODIFIERS, ItemAttributeModifiersEntry, MobEffectInstance, POTION_CONTENTS,
};
use mc_core::ident::Identifier;
use mc_data::block::BlockState;
use mc_data::props::{PropertyDef, PropertyKind};
use mc_nbt::{CompoundTag, Tag};

/// Members of a tag of a data-driven registry (`dir` = its data pack directory).
fn tag_members(dir: &str, tag: &Identifier) -> Vec<String> {
    mc_data::tags::names(dir, &tag.to_string()).unwrap_or_default()
}

/// `HolderSet.contains(holder)` for a reference-only set of a data-driven registry. Inline (direct) holders are
/// never members.
pub fn holder_set_contains<T>(set: &HolderSet<NoDirect>, dir: &str, holder: &Holder<T>) -> bool {
    let Holder::Reference(name) = holder else {
        return false;
    };
    match set {
        HolderSet::Direct(entries) => entries.iter().any(|h| h.name() == Some(name)),
        HolderSet::Named(tag) => tag_members(dir, tag).iter().any(|m| *m == name.to_string()),
    }
}

/// The entries of a reference-only holder set (`for (Holder<T> h : set)`).
fn holder_set_entries(set: &HolderSet<NoDirect>, dir: &str) -> Vec<Holder<NoDirect>> {
    match set {
        HolderSet::Direct(v) => v.clone(),
        HolderSet::Named(tag) => tag_members(dir, tag)
            .iter()
            .filter_map(|n| Identifier::parse(n).ok())
            .map(Holder::Reference)
            .collect(),
    }
}

// ---- block state properties ---------------------------------------------------------------------------------------

/// `Property.getValue(String)`: the index of the value named `text`.
fn typed_index(def: &PropertyDef, text: &str) -> Option<usize> {
    match def.kind {
        PropertyKind::Integer => {
            let v: i32 = text.parse().ok()?;
            let idx = i64::from(v) - i64::from(def.min);
            usize::try_from(idx).ok().filter(|i| *i < def.values.len())
        }
        _ => def.value_index(text),
    }
}

/// Position in `compareTo` order (booleans list `true` first, `false < true`).
// shortcut: enum properties compare by their `getPossibleValues()` position, which equals the ordinal order for every
// vanilla property built from enum constants; upgrade if a property with a custom value order is added.
fn rank(def: &PropertyDef, index: usize) -> usize {
    if def.kind == PropertyKind::Boolean {
        def.values.len() - 1 - index
    } else {
        index
    }
}

impl ValueMatcher {
    /// `matches(property, value)` against the value at index `actual` of `def`.
    pub fn matches(&self, def: &PropertyDef, actual: usize) -> bool {
        let actual = rank(def, actual);
        match self {
            ValueMatcher::Exact(v) => typed_index(def, v).is_some_and(|e| rank(def, e) == actual),
            ValueMatcher::Ranged { min, max } => {
                if let Some(min) = min {
                    if !typed_index(def, min).is_some_and(|m| actual >= rank(def, m)) {
                        return false;
                    }
                }
                if let Some(max) = max {
                    if !typed_index(def, max).is_some_and(|m| actual <= rank(def, m)) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

impl PropertyMatcher {
    pub fn matches(&self, state: BlockState) -> bool {
        state
            .values()
            .find(|(def, _)| def.name == self.name)
            .is_some_and(|(def, idx)| self.value_matcher.matches(def, idx))
    }
}

impl StatePropertiesPredicate {
    /// `matches(BlockState)`.
    pub fn matches(&self, state: BlockState) -> bool {
        self.properties.iter().all(|m| m.matches(state))
    }
}

// ---- blocks -------------------------------------------------------------------------------------------------------

impl BlockPredicate {
    /// `matchesState(state)`.
    pub fn matches_state(&self, state: BlockState) -> bool {
        self.blocks
            .as_ref()
            .is_none_or(|b| b.contains(state.block()))
            && self.properties.as_ref().is_none_or(|p| p.matches(state))
    }

    /// `matches(BlockInWorld)`: the state, and the saved block entity data when `nbt` is set.
    pub fn matches_in_world(&self, state: BlockState, block_entity: Option<&CompoundTag>) -> bool {
        self.matches_state(state)
            && self
                .nbt
                .as_ref()
                .is_none_or(|n| block_entity.is_some_and(|t| n.matches_compound(t)))
    }

    /// `matchesBlockEntity(level, blockEntity)` with the entity's `saveWithFullMetadata` tag and `collectComponents()`.
    pub fn matches_block_entity(
        &self,
        entity: Option<(&CompoundTag, &crate::components::map::DataComponentMap)>,
    ) -> bool {
        if let Some(nbt) = &self.nbt {
            if !entity.is_some_and(|(tag, _)| nbt.matches_compound(tag)) {
                return false;
            }
        }
        self.components.is_empty()
            || entity.is_some_and(|(_, components)| self.components.test(components))
    }
}

impl AdventureModePredicate {
    /// `test(BlockInWorld)` (without vanilla's last-checked-block cache).
    pub fn test(&self, state: BlockState, block_entity: Option<&CompoundTag>) -> bool {
        self.predicates
            .iter()
            .any(|p| p.matches_in_world(state, block_entity))
    }
}

// ---- nbt ------------------------------------------------------------------------------------------------------------

impl super::state::NbtPredicate {
    /// `matches(Tag)` for a compound.
    pub fn matches_compound(&self, tag: &CompoundTag) -> bool {
        self.matches_tag(Some(&Tag::Compound(tag.clone())))
    }

    /// `matches(DataComponentGetter)`: against `CUSTOM_DATA` (empty when absent).
    pub fn matches_components<G: DataComponentGetter>(&self, components: &G) -> bool {
        match components.get(&CUSTOM_DATA) {
            Some(data) => self.matches_compound(data.tag()),
            None => self.matches_compound(&CompoundTag::new()),
        }
    }
}

// ---- items ----------------------------------------------------------------------------------------------------------

impl DataComponentMatchers {
    /// `test(DataComponentGetter)`.
    pub fn test<G: DataComponentGetter>(&self, values: &G) -> bool {
        self.exact.test(values) && self.partial.0.iter().all(|p| p.matches(values))
    }
}

impl ItemPredicate {
    /// `test(ItemInstance)`.
    pub fn test(&self, stack: &ItemStack) -> bool {
        self.items.as_ref().is_none_or(|i| i.contains(stack.item()))
            && self.count.matches(stack.count())
            && self.components.test(stack)
    }

    /// `test(ItemInstance)` of an `ItemStackTemplate`.
    pub fn test_template(&self, template: &ItemStackTemplate) -> bool {
        self.test(&ItemStack::with_patch(
            template.item,
            template.count,
            &template.components,
        ))
    }
}

impl EnchantmentPredicate {
    /// `containedIn(ItemEnchantments)`.
    pub fn contained_in(
        &self,
        applied: &crate::components::values::basic::ItemEnchantments,
    ) -> bool {
        if let Some(set) = &self.enchantments {
            holder_set_entries(set, "enchantment").iter().any(|e| {
                let level = applied.get_level(e);
                level != 0 && (self.level.is_any() || self.level.matches(level))
            })
        } else if !self.level.is_any() {
            applied
                .entries()
                .iter()
                .any(|(_, level)| self.level.matches(*level))
        } else {
            !applied.is_empty()
        }
    }
}

impl MobEffectInstancePredicate {
    pub fn matches(&self, instance: &MobEffectInstance) -> bool {
        self.amplifier.matches(instance.amplifier)
            && self.duration.matches(instance.duration)
            && self.ambient.is_none_or(|a| a == instance.ambient)
            && self.visible.is_none_or(|v| v == instance.visible)
    }
}

impl MobEffectsPredicate {
    /// `test(MobEffectInstance)`.
    pub fn test(&self, effect: &MobEffectInstance) -> bool {
        self.effect_map
            .iter()
            .find(|(e, _)| *e == effect.effect)
            .is_some_and(|(_, p)| p.matches(effect))
    }
}

impl FireworkPredicate {
    pub fn test(&self, explosion: &FireworkExplosion) -> bool {
        self.shape.is_none_or(|s| s == explosion.shape)
            && self.twinkle.is_none_or(|t| t == explosion.has_twinkle)
            && self.trail.is_none_or(|t| t == explosion.has_trail)
    }
}

impl AttributeEntryPredicate {
    pub fn test(&self, entry: &ItemAttributeModifiersEntry) -> bool {
        self.attribute
            .as_ref()
            .is_none_or(|a| a.contains(entry.attribute))
            && self.id.as_ref().is_none_or(|id| *id == entry.modifier.id)
            && self.amount.matches(entry.modifier.amount)
            && self.operation.is_none_or(|o| o == entry.modifier.operation)
            && self.slot.is_none_or(|s| s == entry.slot)
    }
}

/// `Optional<CollectionPredicate>.isEmpty() || test(values)`.
fn opt_test<P: Clone + PartialEq + Send + Sync + 'static, T>(
    pred: &Option<CollectionPredicate<P>>,
    values: &[T],
    matches: impl Fn(&P, &T) -> bool,
) -> bool {
    pred.as_ref().is_none_or(|p| p.test(values, matches))
}

impl PotionsPredicate {
    /// `matches(PotionContents)`: the potion is in `potions` and `getAllEffects()` satisfies `effects`.
    pub fn matches(&self, contents: &crate::components::values::effects::PotionContents) -> bool {
        let potion_ok = match &self.potions {
            None => true,
            Some(set) => contents.potion.is_some_and(|potion| set.contains(potion)),
        };
        potion_ok && {
            let mut all: Vec<MobEffectInstance> = Vec::new();
            if let Some(potion) = contents.potion {
                all.extend(potion.data().effects.iter().map(|d| MobEffectInstance {
                    effect: d.effect,
                    duration: d.duration,
                    amplifier: d.amplifier,
                    ambient: d.ambient,
                    visible: d.visible,
                    show_icon: d.show_icon,
                    hidden_effect: None,
                }));
            }
            all.extend(contents.custom_effects.iter().cloned());
            opt_test(&self.effects, &all, |e, v| e.test(v))
        }
    }
}

impl DataComponentPredicate {
    /// `matches(DataComponentGetter)`.
    pub fn matches<G: DataComponentGetter>(&self, components: &G) -> bool {
        use DataComponentPredicate as P;
        match self {
            P::AnyValue(t) => components.has(*t),
            P::Damage(p) => match components.get(&DAMAGE) {
                None => false,
                Some(&damage) => {
                    let max_damage = *components.get_or_default(&MAX_DAMAGE, &0);
                    p.durability.matches(max_damage.wrapping_sub(damage))
                        && p.damage.matches(damage)
                }
            },
            P::Enchantments(p) => components
                .get(&ENCHANTMENTS)
                .is_some_and(|v| p.enchantments.iter().all(|e| e.contained_in(v))),
            P::StoredEnchantments(p) => components
                .get(&STORED_ENCHANTMENTS)
                .is_some_and(|v| p.enchantments.iter().all(|e| e.contained_in(v))),
            P::PotionContents(p) => components
                .get(&POTION_CONTENTS)
                .is_some_and(|contents| p.matches(contents)),
            P::CustomData(p) => p.value.matches_components(components),
            P::Container(p) => components.get(&CONTAINER).is_some_and(|c| {
                let items: Vec<&ItemStackTemplate> = c.non_empty_items().collect();
                opt_test(&p.items, &items, |e, v| e.test_template(v))
            }),
            P::BundleContents(p) => components
                .get(&BUNDLE_CONTENTS)
                .is_some_and(|c| opt_test(&p.items, c.items(), |e, v| e.test_template(v))),
            P::FireworkExplosion(p) => components
                .get(&FIREWORK_EXPLOSION)
                .is_some_and(|v| p.predicate.test(v)),
            P::Fireworks(p) => components.get(&FIREWORKS).is_some_and(|v| {
                opt_test(&p.explosions, &v.explosions, |e, x| e.test(x))
                    && p.flight_duration.matches(v.flight_duration)
            }),
            P::WritableBookContent(p) => components
                .get(&WRITABLE_BOOK_CONTENT)
                .is_some_and(|v| opt_test(&p.pages, &v.pages, |e, page| page.raw == e.contents)),
            P::WrittenBookContent(p) => components.get(&WRITTEN_BOOK_CONTENT).is_some_and(|v| {
                p.author.as_ref().is_none_or(|a| *a == v.author)
                    && p.title.as_ref().is_none_or(|t| *t == v.title.raw)
                    && p.generation.matches(v.generation)
                    && p.resolved.is_none_or(|r| r == v.resolved)
                    && opt_test(&p.pages, &v.pages, |e, page| page.raw == e.contents)
            }),
            P::AttributeModifiers(p) => components
                .get(&ATTRIBUTE_MODIFIERS)
                .is_some_and(|v| opt_test(&p.modifiers, &v.modifiers, |e, entry| e.test(entry))),
            P::Trim(p) => components.get(&TRIM).is_some_and(|v| {
                p.material
                    .as_ref()
                    .is_none_or(|m| holder_set_contains(m, "trim_material", &v.material))
                    && p.pattern
                        .as_ref()
                        .is_none_or(|m| holder_set_contains(m, "trim_pattern", &v.pattern))
            }),
            P::JukeboxPlayable(p) => components.get(&JUKEBOX_PLAYABLE).is_some_and(|v| {
                p.song
                    .as_ref()
                    .is_none_or(|s| holder_set_contains(s, "jukebox_song", &v.song))
            }),
            P::VillagerVariant(p) => components
                .get(&crate::components::values::entity::VILLAGER_VARIANT)
                .is_some_and(|h| {
                    h.name()
                        .and_then(|n| mc_data::VillagerType::by_name(&n.to_string()))
                        .is_some_and(|t| p.villager_types.contains(t))
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::values::basic::{DAMAGE, MAX_DAMAGE};
    use mc_data::{Block, Item};

    fn block_predicate(json: &str) -> BlockPredicate {
        mc_dfu::parse_json(&BlockPredicate::codec(), json)
            .into_result()
            .unwrap()
    }

    fn item_predicate(json: &str) -> ItemPredicate {
        mc_dfu::parse_json(&ItemPredicate::codec(), json)
            .into_result()
            .unwrap()
    }

    #[test]
    fn block_state_matching() {
        let p = block_predicate(
            r#"{"blocks":["minecraft:wheat"],"state":{"age":{"min":"3","max":"5"}}}"#,
        );
        let wheat = Block::by_name("minecraft:wheat").unwrap();
        let state = |age: &str| wheat.default_state().with_property_str("age", age).unwrap();
        assert!(!p.matches_state(state("2")));
        assert!(p.matches_state(state("3")));
        assert!(p.matches_state(state("5")));
        assert!(!p.matches_state(state("6")));
        assert!(!p.matches_state(Block::by_name("minecraft:stone").unwrap().default_state()));
        // exact value, unknown property, boolean ordering
        assert!(block_predicate(r#"{"state":{"age":"7"}}"#).matches_state(state("7")));
        assert!(!block_predicate(r#"{"state":{"nope":"7"}}"#).matches_state(state("7")));
        let lit = |v: &str| {
            Block::by_name("minecraft:furnace")
                .unwrap()
                .default_state()
                .with_property_str("lit", v)
                .unwrap()
        };
        let at_least_true = block_predicate(r#"{"state":{"lit":{"min":"true"}}}"#);
        assert!(
            at_least_true.matches_state(lit("true")) && !at_least_true.matches_state(lit("false"))
        );
        let tag = block_predicate(r##"{"blocks":"#minecraft:logs"}"##);
        assert!(tag.matches_state(Block::by_name("minecraft:oak_log").unwrap().default_state()));
    }

    #[test]
    fn block_entity_nbt_matching() {
        let p = block_predicate(r#"{"nbt":"{a:1b}"}"#);
        let state = Block::by_name("minecraft:stone").unwrap().default_state();
        let mut tag = CompoundTag::new();
        assert!(!p.matches_in_world(state, None) && !p.matches_in_world(state, Some(&tag)));
        tag.put_byte("a", 1);
        tag.put_int("b", 2);
        assert!(p.matches_in_world(state, Some(&tag)));
    }

    #[test]
    fn item_and_component_matching() {
        let p = item_predicate(
            r#"{"items":"minecraft:diamond_pickaxe","count":{"min":2},"predicates":{"minecraft:damage":{"damage":{"min":5},"durability":{"max":100}}}}"#,
        );
        let mut stack =
            ItemStack::with_count(Item::by_name("minecraft:diamond_pickaxe").unwrap(), 2);
        assert!(!p.test(&stack));
        stack.set(DAMAGE, 7);
        assert!(!p.test(&stack)); // durability 1561 - 7 is above 100
        stack.set(MAX_DAMAGE, 105);
        assert!(p.test(&stack));
        stack.set_count(1);
        assert!(!p.test(&stack));
        assert!(!p.test(&ItemStack::with_count(
            Item::by_name("minecraft:stone").unwrap(),
            2
        )));
    }

    #[test]
    fn exact_and_any_component_matching() {
        let p = item_predicate(
            r#"{"components":{"minecraft:damage":3},"predicates":{"minecraft:unbreakable":{}}}"#,
        );
        let mut stack = ItemStack::new(Item::by_name("minecraft:diamond_pickaxe").unwrap());
        stack.set(DAMAGE, 3);
        assert!(!p.test(&stack));
        stack.set(crate::components::values::basic::UNBREAKABLE, ());
        assert!(p.test(&stack));
        stack.set(DAMAGE, 4);
        assert!(!p.test(&stack));
    }

    #[test]
    fn collection_predicates() {
        use crate::components::values::containers::{BUNDLE_CONTENTS, BundleContents};
        let p = item_predicate(
            r#"{"predicates":{"minecraft:bundle_contents":{"items":{"contains":[{"items":"minecraft:stone"},{"items":"minecraft:dirt"}],"size":{"min":2}}}}}"#,
        );
        let mut stack = ItemStack::new(Item::by_name("minecraft:bundle").unwrap());
        let slot = |item: &str| ItemStackTemplate::of(Item::by_name(item).unwrap());
        stack.set(
            BUNDLE_CONTENTS,
            BundleContents::new(vec![slot("minecraft:stone")]),
        );
        assert!(!p.test(&stack));
        stack.set(
            BUNDLE_CONTENTS,
            BundleContents::new(vec![slot("minecraft:dirt"), slot("minecraft:stone")]),
        );
        assert!(p.test(&stack));
        stack.set(
            BUNDLE_CONTENTS,
            BundleContents::new(vec![slot("minecraft:dirt"), slot("minecraft:dirt")]),
        );
        assert!(!p.test(&stack));
    }

    #[test]
    fn adventure_mode_any_predicate() {
        let p = mc_dfu::parse_json(
            &AdventureModePredicate::codec(),
            r#"[{"blocks":"minecraft:stone"},{"blocks":"minecraft:dirt"}]"#,
        )
        .into_result()
        .unwrap();
        assert!(p.test(
            Block::by_name("minecraft:dirt").unwrap().default_state(),
            None
        ));
        assert!(!p.test(
            Block::by_name("minecraft:sand").unwrap().default_state(),
            None
        ));
    }
}
