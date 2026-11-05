//! Item-specific loot functions: potions, stews, attributes, instruments, dyes, books, banners, fireworks, custom model
//! data, exploration maps.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_data::{Item, MobEffect, Potion};
use mc_dfu::extra::{UNSIGNED_BYTE, extra_int_range, non_empty_list};
use mc_dfu::{BOOL, BYTE, Codec, DataResult, INT, MapCodec, STRING, record};
use mc_protocol::components::holder::registry_by_name_codec;
use mc_protocol::components::values::basic::{CUSTOM_MODEL_DATA, DYED_COLOR, DyedItemColor, OminousBottleAmplifier, OMINOUS_BOTTLE_AMPLIFIER};
use mc_protocol::components::values::decor::{
    BANNER_PATTERNS, BannerPatternLayers, FIREWORKS, FIREWORK_EXPLOSION, FireworkExplosion, FireworkShape, Fireworks, INSTRUMENT, InstrumentComponent,
    Filterable, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT, WritableBookContent, WrittenBookContent,
};
use mc_protocol::components::values::effects::{
    ATTRIBUTE_MODIFIERS, EquipmentSlotGroup, ItemAttributeModifiers, ItemAttributeModifiersDisplay, ItemAttributeModifiersEntry, POTION_CONTENTS,
    PotionContents, SUSPICIOUS_STEW_EFFECTS, SuspiciousStewEffects, SuspiciousStewEffectsEntry,
};
use mc_protocol::components::values::predicates::data_driven_set_codec;
use mc_protocol::components::{HolderSet, ItemStack, NoDirect};
use mc_protocol::packets::play::{AttributeModifier, AttributeModifierOperation};

use super::list_operation::{ListOperation, StandAlone};
use super::{ConditionalFunction, LootItemFunctionRef, condition_field};
use crate::registry::keys;
use crate::world::level::storage::loot::context::{ContextKey, LootContext};
use crate::world::level::storage::loot::holder::lift;
use crate::world::level::storage::loot::predicates::ConditionHolder;
use crate::world::level::storage::loot::providers::number::{
    FloatHolder, IntHolder, float_holder_codec, int_exactly, int_holder_codec,
};
use crate::world::level::storage::loot::validation::{ValidationContext, validate_holder_field, validate_holder_list};
use mc_dfu::extra::RGB_COLOR_CODEC;

type Entries = Vec<(&'static str, MapCodec<LootItemFunctionRef>)>;

fn entry<F: ConditionalFunction + Clone>(codec: MapCodec<F>) -> MapCodec<LootItemFunctionRef> {
    lift(codec, |f| Arc::new(f) as LootItemFunctionRef, |t| t.as_any().downcast_ref::<F>())
}

macro_rules! conditional {
    ($name:literal) => {
        fn type_name(&self) -> &'static str {
            $name
        }
        fn condition(&self) -> &Option<ConditionHolder> {
            &self.condition
        }
    };
}

/// `Util.getRandom(list, random)`.
fn random_of<T: Clone>(list: &[T], random: &mut dyn mc_core::random::RandomSource) -> T {
    list[random.next_int_bounded(list.len() as i32) as usize].clone()
}

// ---- potions ----------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SetPotionFunction {
    pub condition: Option<ConditionHolder>,
    pub potion: Potion,
}

impl ConditionalFunction for SetPotionFunction {
    conditional!("set_potion");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let mut contents = stack.get(&POTION_CONTENTS).cloned().unwrap_or_else(empty_potion_contents);
        contents.potion = Some(self.potion);
        stack.set(POTION_CONTENTS, contents);
        stack
    }
}

/// `PotionContents.EMPTY`.
fn empty_potion_contents() -> PotionContents {
    PotionContents { potion: None, custom_color: None, custom_effects: Vec::new(), custom_name: None }
}

#[derive(Clone)]
pub struct SetRandomPotionFunction {
    pub condition: Option<ConditionHolder>,
    pub options: Option<HolderSet<NoDirect>>,
}

impl ConditionalFunction for SetRandomPotionFunction {
    conditional!("set_random_potion");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let registries = context.get_resolver();
        let potions: Vec<Potion> = match &self.options {
            Some(set) => crate::world::level::storage::loot::enchant_api::holder_set_ids(registries, "minecraft:potion", set)
                .iter()
                .filter_map(|id| mc_data::registry::by_name("minecraft:potion").and_then(|r| r.id_of(&id.to_string())).and_then(Potion::from_id))
                .collect(),
            None => Potion::iter().collect(),
        };
        // `HolderSet.getRandomElement` / `Registry.getRandom`: `random.nextInt(size)`
        if !potions.is_empty() {
            let potion = potions[context.get_random().next_int_bounded(potions.len() as i32) as usize];
            let mut contents = stack.get(&POTION_CONTENTS).cloned().unwrap_or_else(empty_potion_contents);
            contents.potion = Some(potion);
            stack.set(POTION_CONTENTS, contents);
        }
        stack
    }
}

// ---- suspicious stew ------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct StewEffectEntry {
    pub effect: MobEffect,
    pub duration: IntHolder,
}

#[derive(Clone)]
pub struct SetStewEffectFunction {
    pub condition: Option<ConditionHolder>,
    pub effects: Vec<StewEffectEntry>,
}

impl ConditionalFunction for SetStewEffectFunction {
    conditional!("set_stew_effect");
    fn validate_fields(&self, context: &ValidationContext) {
        for (i, e) in self.effects.iter().enumerate() {
            let c = context.for_indexed_field("effects", i);
            validate_holder_field(&c, "duration", &e.duration);
        }
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if !stack.is(Item::SUSPICIOUS_STEW) || self.effects.is_empty() {
            return stack;
        }
        let entry = random_of(&self.effects, context.get_random());
        let mut duration = entry.duration.value(context.get_resolver()).get_int(context);
        if !entry.effect.data().instantaneous {
            duration = duration.wrapping_mul(20);
        }
        let mut stew = stack.get(&SUSPICIOUS_STEW_EFFECTS).cloned().unwrap_or(SuspiciousStewEffects { effects: Vec::new() });
        stew.effects.push(SuspiciousStewEffectsEntry { effect: entry.effect, duration });
        stack.set(SUSPICIOUS_STEW_EFFECTS, stew);
        stack
    }
}

// ---- attributes ----------------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct AttributeModifierSpec {
    pub id: Identifier,
    pub attribute: mc_data::Attribute,
    pub operation: AttributeModifierOperation,
    pub amount: FloatHolder,
    pub slots: Vec<EquipmentSlotGroup>,
}

#[derive(Clone)]
pub struct SetAttributesFunction {
    pub condition: Option<ConditionHolder>,
    pub modifiers: Vec<AttributeModifierSpec>,
    pub replace: bool,
}

impl SetAttributesFunction {
    fn update_modifiers(&self, context: &mut LootContext, mut item_modifiers: ItemAttributeModifiers) -> ItemAttributeModifiers {
        for modifier in &self.modifiers {
            let slot = random_of(&modifier.slots, context.get_random());
            let amount = modifier.amount.value(context.get_resolver()).get_float(context);
            // `withModifierAdded`: replaces an entry of the same attribute and modifier id
            item_modifiers.modifiers.retain(|e| !(e.attribute == modifier.attribute && e.modifier.id == modifier.id));
            item_modifiers.modifiers.push(ItemAttributeModifiersEntry {
                attribute: modifier.attribute,
                modifier: AttributeModifier { id: modifier.id.clone(), amount: f64::from(amount), operation: modifier.operation },
                slot,
                display: ItemAttributeModifiersDisplay::Default,
            });
        }
        item_modifiers
    }
}

impl ConditionalFunction for SetAttributesFunction {
    conditional!("set_attributes");
    fn validate_fields(&self, context: &ValidationContext) {
        for (i, m) in self.modifiers.iter().enumerate() {
            let c = context.for_indexed_field("modifiers", i);
            validate_holder_field(&c, "amount", &m.amount);
        }
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let base = if self.replace { ItemAttributeModifiers { modifiers: Vec::new() } } else { stack.get(&ATTRIBUTE_MODIFIERS).cloned().unwrap_or(ItemAttributeModifiers { modifiers: Vec::new() }) };
        let updated = self.update_modifiers(context, base);
        stack.set(ATTRIBUTE_MODIFIERS, updated);
        stack
    }
}

// ---- misc item components -------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SetInstrumentFunction {
    pub condition: Option<ConditionHolder>,
    pub options: HolderSet<NoDirect>,
}

impl ConditionalFunction for SetInstrumentFunction {
    conditional!("set_instrument");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let ids = crate::world::level::storage::loot::enchant_api::holder_set_ids(context.get_resolver(), keys::INSTRUMENT, &self.options);
        if !ids.is_empty() {
            let id = ids[context.get_random().next_int_bounded(ids.len() as i32) as usize].clone();
            stack.set(INSTRUMENT, InstrumentComponent { instrument: mc_protocol::components::Holder::Reference(id) });
        }
        stack
    }
}

#[derive(Clone)]
pub struct SetOminousBottleAmplifierFunction {
    pub condition: Option<ConditionHolder>,
    pub amplifier: IntHolder,
}

impl ConditionalFunction for SetOminousBottleAmplifierFunction {
    conditional!("set_ominous_bottle_amplifier");
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_field(context, "amplifier", &self.amplifier);
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let value = mth::clamp_i32(self.amplifier.value(context.get_resolver()).get_int(context), 0, 4);
        stack.set(OMINOUS_BOTTLE_AMPLIFIER, OminousBottleAmplifier { value });
        stack
    }
}

#[derive(Clone)]
pub struct SetRandomDyesFunction {
    pub condition: Option<ConditionHolder>,
    pub number_of_dyes: IntHolder,
}

impl ConditionalFunction for SetRandomDyesFunction {
    conditional!("set_random_dyes");
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let rolls = self.number_of_dyes.value(context.get_resolver()).get_int(context);
        if rolls <= 0 {
            return stack;
        }
        let mut dyes = Vec::with_capacity(rolls as usize);
        for _ in 0..rolls {
            let all = mc_protocol::components::DyeColor::ALL;
            dyes.push(all[context.get_random().next_int_bounded(all.len() as i32) as usize]);
        }
        apply_dyes(&stack, &dyes)
    }
}

/// `DyedItemColor.applyDyes(itemStack, dyes)`.
fn apply_dyes(stack: &ItemStack, dyes: &[mc_protocol::components::DyeColor]) -> ItemStack {
    let (mut red_total, mut green_total, mut blue_total, mut intensity_total, mut color_count) = (0i32, 0i32, 0i32, 0i32, 0i32);
    if let Some(current) = stack.get(&DYED_COLOR) {
        let (r, g, b) = ((current.rgb >> 16) & 0xFF, (current.rgb >> 8) & 0xFF, current.rgb & 0xFF);
        intensity_total += r.max(g.max(b));
        red_total += r;
        green_total += g;
        blue_total += b;
        color_count += 1;
    }
    for dye in dyes {
        let color = mc_data::misc::DYE_COLORS[dye.id() as usize].texture_diffuse_color;
        let (r, g, b) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
        intensity_total += r.max(g.max(b));
        red_total += r;
        green_total += g;
        blue_total += b;
        color_count += 1;
    }
    let mut red = red_total / color_count;
    let mut green = green_total / color_count;
    let mut blue = blue_total / color_count;
    let average_intensity = intensity_total as f32 / color_count as f32;
    let result_intensity = red.max(green.max(blue)) as f32;
    red = (red as f32 * average_intensity / result_intensity) as i32;
    green = (green as f32 * average_intensity / result_intensity) as i32;
    blue = (blue as f32 * average_intensity / result_intensity) as i32;
    let rgb = (red & 0xFF) << 16 | (green & 0xFF) << 8 | (blue & 0xFF);
    let mut result = stack.copy_with_count(1);
    result.set(DYED_COLOR, DyedItemColor { rgb });
    result
}

#[derive(Clone)]
pub struct SetBookCoverFunction {
    pub condition: Option<ConditionHolder>,
    pub title: Option<Filterable<String>>,
    pub author: Option<String>,
    pub generation: Option<i32>,
}

impl ConditionalFunction for SetBookCoverFunction {
    conditional!("set_book_cover");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let original = stack.get(&WRITTEN_BOOK_CONTENT).cloned().unwrap_or_default();
        stack.set(
            WRITTEN_BOOK_CONTENT,
            WrittenBookContent {
                title: self.title.clone().unwrap_or(original.title),
                author: self.author.clone().unwrap_or(original.author),
                generation: self.generation.unwrap_or(original.generation),
                pages: original.pages,
                resolved: original.resolved,
            },
        );
        stack
    }
}

#[derive(Clone)]
pub struct SetBannerPatternFunction {
    pub condition: Option<ConditionHolder>,
    pub patterns: BannerPatternLayers,
    pub append: bool,
}

impl ConditionalFunction for SetBannerPatternFunction {
    conditional!("set_banner_pattern");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        if self.append {
            let mut layers = stack.get(&BANNER_PATTERNS).cloned().unwrap_or_default();
            layers.layers.extend(self.patterns.layers.iter().cloned());
            stack.set(BANNER_PATTERNS, layers);
        } else {
            stack.set(BANNER_PATTERNS, self.patterns.clone());
        }
        stack
    }
}

#[derive(Clone)]
pub struct SetFireworksFunction {
    pub condition: Option<ConditionHolder>,
    pub explosions: Option<StandAlone<FireworkExplosion>>,
    pub flight_duration: Option<i32>,
}

impl ConditionalFunction for SetFireworksFunction {
    conditional!("set_fireworks");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let old = stack.get(&FIREWORKS).cloned().unwrap_or_default();
        let explosions = self.explosions.as_ref().map_or_else(|| old.explosions.clone(), |op| op.apply(&old.explosions));
        stack.set(FIREWORKS, Fireworks { flight_duration: self.flight_duration.unwrap_or(old.flight_duration), explosions });
        stack
    }
}

#[derive(Clone)]
pub struct SetFireworkExplosionFunction {
    pub condition: Option<ConditionHolder>,
    pub shape: Option<FireworkShape>,
    pub colors: Option<Vec<i32>>,
    pub fade_colors: Option<Vec<i32>>,
    pub trail: Option<bool>,
    pub twinkle: Option<bool>,
}

impl ConditionalFunction for SetFireworkExplosionFunction {
    conditional!("set_firework_explosion");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let original = stack.get(&FIREWORK_EXPLOSION).cloned().unwrap_or_default();
        stack.set(
            FIREWORK_EXPLOSION,
            FireworkExplosion {
                shape: self.shape.unwrap_or(original.shape),
                colors: self.colors.clone().unwrap_or(original.colors),
                fade_colors: self.fade_colors.clone().unwrap_or(original.fade_colors),
                has_trail: self.trail.unwrap_or(original.has_trail),
                has_twinkle: self.twinkle.unwrap_or(original.has_twinkle),
            },
        );
        stack
    }
}

#[derive(Clone)]
pub struct SetCustomModelDataFunction {
    pub condition: Option<ConditionHolder>,
    pub floats: Option<StandAlone<FloatHolder>>,
    pub flags: Option<StandAlone<bool>>,
    pub strings: Option<StandAlone<String>>,
    pub colors: Option<StandAlone<IntHolder>>,
}

impl ConditionalFunction for SetCustomModelDataFunction {
    conditional!("set_custom_model_data");
    fn validate_fields(&self, context: &ValidationContext) {
        if let Some(f) = &self.floats {
            validate_holder_list(context, "floats", &f.value);
        }
        if let Some(c) = &self.colors {
            validate_holder_list(context, "colors", &c.value);
        }
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let component = stack.get(&CUSTOM_MODEL_DATA).cloned().unwrap_or_default();
        let floats = match &self.floats {
            Some(op) => {
                let replacement: Vec<f32> = op.value.iter().map(|p| p.value(context.get_resolver()).get_float(context)).collect();
                op.operation.apply(&component.floats, &replacement, usize::MAX)
            }
            None => component.floats.clone(),
        };
        let flags = self.flags.as_ref().map_or_else(|| component.flags.clone(), |op| op.apply(&component.flags));
        let strings = self.strings.as_ref().map_or_else(|| component.strings.clone(), |op| op.apply(&component.strings));
        let colors = match &self.colors {
            Some(op) => {
                let replacement: Vec<i32> = op.value.iter().map(|p| p.value(context.get_resolver()).get_int(context)).collect();
                op.operation.apply(&component.colors, &replacement, usize::MAX)
            }
            None => component.colors.clone(),
        };
        stack.set(CUSTOM_MODEL_DATA, mc_protocol::components::values::basic::CustomModelData { floats, flags, strings, colors });
        stack
    }
}

#[derive(Clone)]
pub struct SetWritableBookPagesFunction {
    pub condition: Option<ConditionHolder>,
    pub pages: Vec<Filterable<String>>,
    pub page_operation: ListOperation,
}

impl ConditionalFunction for SetWritableBookPagesFunction {
    conditional!("set_writable_book_pages");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let original = stack.get(&WRITABLE_BOOK_CONTENT).cloned().unwrap_or_default();
        let pages = self.page_operation.apply(&original.pages, &self.pages, 100);
        stack.set(WRITABLE_BOOK_CONTENT, WritableBookContent { pages });
        stack
    }
}

#[derive(Clone)]
pub struct SetWrittenBookPagesFunction {
    pub condition: Option<ConditionHolder>,
    pub pages: Vec<Filterable<mc_text::Component>>,
    pub page_operation: ListOperation,
}

impl ConditionalFunction for SetWrittenBookPagesFunction {
    conditional!("set_written_book_pages");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let mut original = stack.get(&WRITTEN_BOOK_CONTENT).cloned().unwrap_or_default();
        original.pages = self.page_operation.apply(&original.pages, &self.pages, usize::MAX);
        stack.set(WRITTEN_BOOK_CONTENT, original);
        stack
    }
}

// ---- exploration map ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct ExplorationMapFunction {
    pub condition: Option<ConditionHolder>,
    pub destination: HolderSet<NoDirect>,
    pub map_decoration: Identifier,
    pub zoom: i8,
    pub search_radius: i32,
    pub skip_known_structures: bool,
}

impl ConditionalFunction for ExplorationMapFunction {
    conditional!("exploration_map");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Origin]
    }
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is_empty() {
            return stack;
        }
        let Some(origin) = context.params().origin else { return stack };
        crate::world::level::storage::loot::map_api::explore(context.get_level(), stack, origin, self)
    }
}

// ---- registration ---------------------------------------------------------------------------------------------------------------------------

fn color_provider_codec() -> Codec<IntHolder> {
    int_holder_codec().with_alternative_conv(&RGB_COLOR_CODEC, int_exactly)
}

pub(super) fn register(entries: &mut Entries) {
    let cond = condition_field;
    entries.push((
        "set_potion",
        entry(record!(SetPotionFunction { condition: cond(), potion: registry_by_name_codec::<Potion>().field_of("id") })),
    ));
    entries.push((
        "set_random_potion",
        entry(record!(SetRandomPotionFunction { condition: cond(), options: data_driven_set_codec("minecraft:potion").optional_field_of("options") })),
    ));
    entries.push((
        "set_stew_effect",
        entry(record!(SetStewEffectFunction {
            condition: cond(),
            effects: stew_effects_codec().optional_field_of_default_vec("effects"),
        })),
    ));
    entries.push((
        "set_attributes",
        entry(record!(SetAttributesFunction {
            condition: cond(),
            modifiers: attribute_modifier_spec_codec().list_of().field_of("modifiers"),
            replace: BOOL.optional_field_of_default("replace", true),
        })),
    ));
    entries.push((
        "set_instrument",
        entry(record!(SetInstrumentFunction { condition: cond(), options: data_driven_set_codec(keys::INSTRUMENT).field_of("options") })),
    ));
    entries.push((
        "set_ominous_bottle_amplifier",
        entry(record!(SetOminousBottleAmplifierFunction { condition: cond(), amplifier: int_holder_codec().field_of("amplifier") })),
    ));
    entries.push((
        "set_random_dyes",
        entry(record!(SetRandomDyesFunction { condition: cond(), number_of_dyes: int_holder_codec().field_of("number_of_dyes") })),
    ));
    entries.push((
        "set_book_cover",
        entry(record!(SetBookCoverFunction {
            condition: cond(),
            title: Filterable::codec(&mc_dfu::string_range(0, 32)).optional_field_of("title"),
            author: STRING.optional_field_of("author"),
            generation: extra_int_range(0, 3).optional_field_of("generation"),
        })),
    ));
    entries.push((
        "set_banner_pattern",
        entry(record!(SetBannerPatternFunction {
            condition: cond(),
            patterns: BannerPatternLayers::codec().field_of("patterns"),
            append: BOOL.field_of("append"),
        })),
    ));
    entries.push((
        "set_fireworks",
        entry(record!(SetFireworksFunction {
            condition: cond(),
            explosions: StandAlone::codec(&FireworkExplosion::codec(), 256).optional_field_of("explosions"),
            flight_duration: UNSIGNED_BYTE.optional_field_of("flight_duration"),
        })),
    ));
    entries.push((
        "set_firework_explosion",
        entry(record!(SetFireworkExplosionFunction {
            condition: cond(),
            shape: FireworkShape::codec().optional_field_of("shape"),
            colors: INT.list_of().optional_field_of("colors"),
            fade_colors: INT.list_of().optional_field_of("fade_colors"),
            trail: BOOL.optional_field_of("trail"),
            twinkle: BOOL.optional_field_of("twinkle"),
        })),
    ));
    entries.push((
        "set_custom_model_data",
        entry(record!(SetCustomModelDataFunction {
            condition: cond(),
            floats: StandAlone::codec(&float_holder_codec(), usize::MAX).optional_field_of("floats"),
            flags: StandAlone::codec(&BOOL, usize::MAX).optional_field_of("flags"),
            strings: StandAlone::codec(&STRING, usize::MAX).optional_field_of("strings"),
            colors: StandAlone::codec(&color_provider_codec(), usize::MAX).optional_field_of("colors"),
        })),
    ));
    entries.push((
        "set_writable_book_pages",
        entry(record!(SetWritableBookPagesFunction {
            condition: cond(),
            pages: WritableBookContent::pages_codec().field_of("pages"),
            page_operation: ListOperation::codec(100),
        })),
    ));
    entries.push((
        "set_written_book_pages",
        entry(record!(SetWrittenBookPagesFunction {
            condition: cond(),
            pages: WrittenBookContent::pages_codec().field_of("pages"),
            page_operation: ListOperation::codec(usize::MAX),
        })),
    ));
    entries.push((
        "exploration_map",
        entry(record!(ExplorationMapFunction {
            condition: cond(),
            destination: data_driven_set_codec(keys::STRUCTURE).field_of("destination"),
            map_decoration: crate::world::level::storage::loot::holder::registry_ref_codec("minecraft:map_decoration_type")
                .optional_field_of_default("decoration", Identifier::new("minecraft", "mansion").expect("valid")),
            zoom: BYTE.optional_field_of_default("zoom", 2),
            search_radius: INT.optional_field_of_default("search_radius", 50),
            skip_known_structures: BOOL.optional_field_of_default("skip_existing_chunks", true),
        })),
    ));
}

fn stew_effects_codec() -> Codec<Vec<StewEffectEntry>> {
    let entry = record!(StewEffectEntry {
        effect: registry_by_name_codec::<MobEffect>().field_of("type"),
        duration: int_holder_codec().field_of("duration"),
    })
    .codec();
    entry.list_of().validate(|entries| {
        let mut seen: Vec<MobEffect> = Vec::new();
        for e in &entries {
            if seen.contains(&e.effect) {
                let name = e.effect.name().to_owned();
                return DataResult::error(move || format!("Encountered duplicate mob effect: '{name}'"));
            }
            seen.push(e.effect);
        }
        DataResult::success(entries)
    })
}

fn attribute_modifier_spec_codec() -> Codec<AttributeModifierSpec> {
    let slots = non_empty_list(&mc_dfu::extra::compact_list_codec(&EquipmentSlotGroup::codec()));
    record!(AttributeModifierSpec {
        id: mc_dfu::extra::IDENTIFIER.field_of("id"),
        attribute: registry_by_name_codec::<mc_data::Attribute>().field_of("attribute"),
        operation: mc_protocol::components::values::effects::attribute_operation_codec().field_of("operation"),
        amount: float_holder_codec().field_of("amount"),
        slots: slots.field_of("slot"),
    })
    .codec()
}

trait OptionalDefaultVec<T> {
    fn optional_field_of_default_vec(&self, name: &str) -> MapCodec<Vec<T>>;
}

impl<T: Clone + Send + Sync + 'static> OptionalDefaultVec<T> for Codec<Vec<T>> {
    fn optional_field_of_default_vec(&self, name: &str) -> MapCodec<Vec<T>> {
        self.optional_field_of(name).xmap(|o| o.unwrap_or_default(), |v: &Vec<T>| if v.is_empty() { None } else { Some(v.clone()) })
    }
}
