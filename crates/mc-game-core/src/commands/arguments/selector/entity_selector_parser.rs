//! `EntitySelectorParser`.

use super::entity_selector::{EntityPredicate, EntitySelector, Order};
use super::entity_selector_options::{self, EntitySelectorOptions};
use super::option_state::{InvertableSetOptionState, SetOnceOptionState};
use crate::commands::CommandSourceStack;
use crate::commands::arguments::min_max_bounds::{Doubles, FloatDegrees, Ints};
use crate::commands::message::{dynamic_translatable, simple_translatable};
use crate::commands::shared_suggestion_provider::suggest;
use crate::server::permissions::permissions;
use mc_brigadier::{
    CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader,
    SimpleCommandExceptionType, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_data::EntityType;
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

pub static ERROR_INVALID_NAME_OR_UUID: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.invalid"));
pub static ERROR_UNKNOWN_SELECTOR_TYPE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.selector.unknown"));
pub static ERROR_SELECTORS_NOT_ALLOWED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.selector.not_allowed"));
pub static ERROR_MISSING_SELECTOR_TYPE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.selector.missing"));
pub static ERROR_EXPECTED_END_OF_OPTIONS: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.options.unterminated"));
pub static ERROR_EXPECTED_OPTION_VALUE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.options.valueless"));

/// A suggestion step: `BiFunction<SuggestionsBuilder, Consumer<SuggestionsBuilder>, CompletableFuture<Suggestions>>`.
#[derive(Clone)]
pub enum Suggest {
    Nothing,
    NameOrSelector,
    Name,
    Selector,
    OpenOptions,
    OptionsKeyOrClose,
    OptionsKey,
    OptionsNextOrClose,
    /// An option-specific step (`sort`, `gamemode`, `type` values).
    Custom(Arc<dyn Fn(SuggestionsBuilder) -> Suggestions + Send + Sync>),
}

pub struct EntitySelectorParser<'a> {
    reader: &'a mut StringReader,
    allow_selectors: bool,
    max_results: i32,
    includes_entities: bool,
    world_limited: bool,
    distance: Option<Doubles>,
    level: Option<Ints>,
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    delta_x: Option<f64>,
    delta_y: Option<f64>,
    delta_z: Option<f64>,
    rot_x: Option<FloatDegrees>,
    rot_y: Option<FloatDegrees>,
    predicates: Vec<EntityPredicate>,
    order: Order,
    current_entity: bool,
    player_name: Option<String>,
    start_position: usize,
    entity_uuid: Option<Uuid>,
    suggestions: Suggest,
    name_option: InvertableSetOptionState,
    limited_option: SetOnceOptionState,
    sorted_option: SetOnceOptionState,
    gamemode_option: InvertableSetOptionState,
    team_option: InvertableSetOptionState,
    entity_type: Option<EntityType>,
    type_option: InvertableSetOptionState,
    scores_option: SetOnceOptionState,
    advancements_option: SetOnceOptionState,
    uses_selectors: bool,
}

impl<'a> EntitySelectorParser<'a> {
    pub fn new(reader: &'a mut StringReader, allow_selectors: bool) -> Self {
        Self {
            reader,
            allow_selectors,
            max_results: 0,
            includes_entities: false,
            world_limited: false,
            distance: None,
            level: None,
            x: None,
            y: None,
            z: None,
            delta_x: None,
            delta_y: None,
            delta_z: None,
            rot_x: None,
            rot_y: None,
            predicates: Vec::new(),
            order: Order::Arbitrary,
            current_entity: false,
            player_name: None,
            start_position: 0,
            entity_uuid: None,
            suggestions: Suggest::Nothing,
            name_option: InvertableSetOptionState::default(),
            limited_option: SetOnceOptionState::default(),
            sorted_option: SetOnceOptionState::default(),
            gamemode_option: InvertableSetOptionState::default(),
            team_option: InvertableSetOptionState::default(),
            entity_type: None,
            type_option: InvertableSetOptionState::default(),
            scores_option: SetOnceOptionState::default(),
            advancements_option: SetOnceOptionState::default(),
            uses_selectors: false,
        }
    }

    /// `allowSelectors(source)`.
    pub fn allow_selectors(source: &CommandSourceStack) -> bool {
        source
            .permissions()
            .has_permission(&permissions::COMMANDS_ENTITY_SELECTORS)
    }

    pub fn get_selector(&self) -> EntitySelector {
        let aabb = if self.delta_x.is_none() && self.delta_y.is_none() && self.delta_z.is_none() {
            self.distance.and_then(|d| d.bounds.max).map(|max_range| {
                AABB::new(
                    -max_range,
                    -max_range,
                    -max_range,
                    max_range + 1.0,
                    max_range + 1.0,
                    max_range + 1.0,
                )
            })
        } else {
            Some(Self::create_aabb(
                self.delta_x.unwrap_or(0.0),
                self.delta_y.unwrap_or(0.0),
                self.delta_z.unwrap_or(0.0),
            ))
        };
        EntitySelector::new(
            self.max_results,
            self.includes_entities,
            self.world_limited,
            self.predicates.clone(),
            self.distance,
            [self.x, self.y, self.z],
            aabb,
            self.order,
            self.current_entity,
            self.player_name.clone(),
            self.entity_uuid,
            self.entity_type,
            self.uses_selectors,
        )
    }

    fn create_aabb(x: f64, y: f64, z: f64) -> AABB {
        let (x_neg, y_neg, z_neg) = (x < 0.0, y < 0.0, z < 0.0);
        let x_min = if x_neg { x } else { 0.0 };
        let y_min = if y_neg { y } else { 0.0 };
        let z_min = if z_neg { z } else { 0.0 };
        let x_max = (if x_neg { 0.0 } else { x }) + 1.0;
        let y_max = (if y_neg { 0.0 } else { y }) + 1.0;
        let z_max = (if z_neg { 0.0 } else { z }) + 1.0;
        AABB::new(x_min, y_min, z_min, x_max, y_max, z_max)
    }

    fn finalize_predicates(&mut self) {
        if let Some(rot_x) = self.rot_x {
            self.predicates
                .push(Self::create_rotation_predicate(rot_x, |e| e.base().x_rot()));
        }
        if let Some(rot_y) = self.rot_y {
            self.predicates
                .push(Self::create_rotation_predicate(rot_y, |e| e.base().y_rot()));
        }
        if let Some(level) = self.level {
            self.predicates.push(Arc::new(move |_, e| {
                crate::commands::host::player_experience_level(e).is_some_and(|l| level.matches(l))
            }));
        }
    }

    fn create_rotation_predicate(
        range: FloatDegrees,
        function: impl Fn(&dyn crate::world::entity::Entity) -> f32 + Send + Sync + 'static,
    ) -> EntityPredicate {
        let min = mth::wrap_degrees_f32(range.bounds.min.unwrap_or(0.0));
        let max = mth::wrap_degrees_f32(range.bounds.max.unwrap_or(359.0));
        Arc::new(move |_, e| {
            let rotation = mth::wrap_degrees_f32(function(e));
            if min > max {
                rotation >= min || rotation <= max
            } else {
                rotation >= min && rotation <= max
            }
        })
    }

    fn parse_selector(&mut self) -> Result<(), CommandSyntaxException> {
        self.uses_selectors = true;
        self.suggestions = Suggest::Selector;
        if !self.reader.can_read() {
            return Err(ERROR_MISSING_SELECTOR_TYPE.create_with_context(self.reader));
        }
        let start = self.reader.get_cursor();
        let type_char = self.reader.read();
        let add_alive = match type_char {
            'a' => {
                self.max_results = i32::MAX;
                self.includes_entities = false;
                self.order = Order::Arbitrary;
                self.limit_to_type(EntityType::PLAYER);
                false
            }
            'e' => {
                self.max_results = i32::MAX;
                self.includes_entities = true;
                self.order = Order::Arbitrary;
                true
            }
            'n' => {
                self.max_results = 1;
                self.includes_entities = true;
                self.order = Order::Nearest;
                true
            }
            'p' => {
                self.max_results = 1;
                self.includes_entities = false;
                self.order = Order::Nearest;
                self.limit_to_type(EntityType::PLAYER);
                false
            }
            'r' => {
                self.max_results = 1;
                self.includes_entities = false;
                self.order = Order::Random;
                self.limit_to_type(EntityType::PLAYER);
                false
            }
            's' => {
                self.max_results = 1;
                self.includes_entities = true;
                self.current_entity = true;
                false
            }
            other => {
                self.reader.set_cursor(start);
                return Err(ERROR_UNKNOWN_SELECTOR_TYPE
                    .create_with_context(self.reader, format!("@{other}")));
            }
        };
        if add_alive {
            self.predicates.push(Arc::new(|_, e| e.is_alive()));
        }
        self.suggestions = Suggest::OpenOptions;
        if self.reader.can_read() && self.reader.peek() == '[' {
            self.reader.skip();
            self.suggestions = Suggest::OptionsKeyOrClose;
            self.parse_options()?;
        }
        Ok(())
    }

    fn parse_name_or_uuid(&mut self) -> Result<(), CommandSyntaxException> {
        if self.reader.can_read() {
            self.suggestions = Suggest::Name;
        }
        let start = self.reader.get_cursor();
        let name = self.reader.read_string()?;
        match mc_core::uuid_util::uuid_from_string(&name) {
            Ok(uuid) => {
                self.entity_uuid = Some(uuid);
                self.includes_entities = true;
            }
            Err(_) => {
                if name.is_empty() || mc_brigadier::utf16::len16(&name) > 16 {
                    self.reader.set_cursor(start);
                    return Err(ERROR_INVALID_NAME_OR_UUID.create_with_context(self.reader));
                }
                self.includes_entities = false;
                self.player_name = Some(name);
            }
        }
        self.max_results = 1;
        Ok(())
    }

    fn parse_options(&mut self) -> Result<(), CommandSyntaxException> {
        self.suggestions = Suggest::OptionsKey;
        self.reader.skip_whitespace();
        while self.reader.can_read() && self.reader.peek() != ']' {
            self.reader.skip_whitespace();
            let start = self.reader.get_cursor();
            let key = self.reader.read_string()?;
            let modifier = EntitySelectorOptions::get(self, &key, start)?;
            self.reader.skip_whitespace();
            if !self.reader.can_read() || self.reader.peek() != '=' {
                self.reader.set_cursor(start);
                return Err(ERROR_EXPECTED_OPTION_VALUE.create_with_context(self.reader, key));
            }
            self.reader.skip();
            self.reader.skip_whitespace();
            self.suggestions = Suggest::Nothing;
            modifier(self)?;
            self.reader.skip_whitespace();
            self.suggestions = Suggest::OptionsNextOrClose;
            if self.reader.can_read() {
                if self.reader.peek() != ',' {
                    if self.reader.peek() != ']' {
                        return Err(ERROR_EXPECTED_END_OF_OPTIONS.create_with_context(self.reader));
                    }
                    break;
                }
                self.reader.skip();
                self.suggestions = Suggest::OptionsKey;
            }
        }
        if self.reader.can_read() {
            self.reader.skip();
            self.suggestions = Suggest::Nothing;
            Ok(())
        } else {
            Err(ERROR_EXPECTED_END_OF_OPTIONS.create_with_context(self.reader))
        }
    }

    pub fn should_invert_value(&mut self) -> bool {
        self.reader.skip_whitespace();
        if self.reader.can_read() && self.reader.peek() == '!' {
            self.reader.skip();
            self.reader.skip_whitespace();
            true
        } else {
            false
        }
    }

    pub fn is_tag(&mut self) -> bool {
        self.reader.skip_whitespace();
        if self.reader.can_read() && self.reader.peek() == '#' {
            self.reader.skip();
            self.reader.skip_whitespace();
            true
        } else {
            false
        }
    }

    pub fn get_reader(&mut self) -> &mut StringReader {
        self.reader
    }

    pub fn add_predicate(&mut self, predicate: EntityPredicate) {
        self.predicates.push(predicate);
    }

    pub fn set_world_limited(&mut self) {
        self.world_limited = true;
    }

    pub fn get_distance(&self) -> Option<Doubles> {
        self.distance
    }

    pub fn set_distance(&mut self, distance: Doubles) {
        self.distance = Some(distance);
    }

    pub fn get_level(&self) -> Option<Ints> {
        self.level
    }

    pub fn set_level(&mut self, level: Ints) {
        self.level = Some(level);
    }

    pub fn get_rot_x(&self) -> Option<FloatDegrees> {
        self.rot_x
    }

    pub fn set_rot_x(&mut self, rot_x: FloatDegrees) {
        self.rot_x = Some(rot_x);
    }

    pub fn get_rot_y(&self) -> Option<FloatDegrees> {
        self.rot_y
    }

    pub fn set_rot_y(&mut self, rot_y: FloatDegrees) {
        self.rot_y = Some(rot_y);
    }

    pub fn get_x(&self) -> Option<f64> {
        self.x
    }

    pub fn get_y(&self) -> Option<f64> {
        self.y
    }

    pub fn get_z(&self) -> Option<f64> {
        self.z
    }

    pub fn set_x(&mut self, x: f64) {
        self.x = Some(x);
    }

    pub fn set_y(&mut self, y: f64) {
        self.y = Some(y);
    }

    pub fn set_z(&mut self, z: f64) {
        self.z = Some(z);
    }

    pub fn set_delta_x(&mut self, v: f64) {
        self.delta_x = Some(v);
    }

    pub fn set_delta_y(&mut self, v: f64) {
        self.delta_y = Some(v);
    }

    pub fn set_delta_z(&mut self, v: f64) {
        self.delta_z = Some(v);
    }

    pub fn get_delta_x(&self) -> Option<f64> {
        self.delta_x
    }

    pub fn get_delta_y(&self) -> Option<f64> {
        self.delta_y
    }

    pub fn get_delta_z(&self) -> Option<f64> {
        self.delta_z
    }

    pub fn set_max_results(&mut self, max_results: i32) {
        self.max_results = max_results;
    }

    pub fn set_includes_entities(&mut self, includes_entities: bool) {
        self.includes_entities = includes_entities;
    }

    pub fn set_order(&mut self, order: Order) {
        self.order = order;
    }

    pub fn is_current_entity(&self) -> bool {
        self.current_entity
    }

    pub fn set_suggestions(&mut self, suggestions: Suggest) {
        self.suggestions = suggestions;
    }

    pub fn name_option(&mut self) -> &mut InvertableSetOptionState {
        &mut self.name_option
    }

    pub fn limited_option(&mut self) -> &mut SetOnceOptionState {
        &mut self.limited_option
    }

    pub fn sorted_option(&mut self) -> &mut SetOnceOptionState {
        &mut self.sorted_option
    }

    pub fn gamemode_option(&mut self) -> &mut InvertableSetOptionState {
        &mut self.gamemode_option
    }

    pub fn team_option(&mut self) -> &mut InvertableSetOptionState {
        &mut self.team_option
    }

    pub fn limit_to_type(&mut self, entity_type: EntityType) {
        self.entity_type = Some(entity_type);
    }

    pub fn type_option(&mut self) -> &mut InvertableSetOptionState {
        &mut self.type_option
    }

    pub fn scores_option(&mut self) -> &mut SetOnceOptionState {
        &mut self.scores_option
    }

    pub fn advancements_option(&mut self) -> &mut SetOnceOptionState {
        &mut self.advancements_option
    }

    pub fn name_option_ref(&self) -> &InvertableSetOptionState {
        &self.name_option
    }

    pub fn limited_option_ref(&self) -> &SetOnceOptionState {
        &self.limited_option
    }

    pub fn sorted_option_ref(&self) -> &SetOnceOptionState {
        &self.sorted_option
    }

    pub fn gamemode_option_ref(&self) -> &InvertableSetOptionState {
        &self.gamemode_option
    }

    pub fn team_option_ref(&self) -> &InvertableSetOptionState {
        &self.team_option
    }

    pub fn type_option_ref(&self) -> &InvertableSetOptionState {
        &self.type_option
    }

    pub fn scores_option_ref(&self) -> &SetOnceOptionState {
        &self.scores_option
    }

    pub fn advancements_option_ref(&self) -> &SetOnceOptionState {
        &self.advancements_option
    }

    /// `parse()`.
    pub fn parse(&mut self) -> Result<EntitySelector, CommandSyntaxException> {
        self.start_position = self.reader.get_cursor();
        self.suggestions = Suggest::NameOrSelector;
        if self.reader.can_read() && self.reader.peek() == '@' {
            if !self.allow_selectors {
                return Err(ERROR_SELECTORS_NOT_ALLOWED.create_with_context(self.reader));
            }
            self.reader.skip();
            self.parse_selector()?;
        } else {
            self.parse_name_or_uuid()?;
        }
        self.finalize_predicates();
        Ok(self.get_selector())
    }

    fn fill_selector_suggestions(builder: &mut SuggestionsBuilder) {
        use crate::commands::message::message;
        use mc_text::Component;
        for (text, key) in [
            ("@p", "argument.entity.selector.nearestPlayer"),
            ("@a", "argument.entity.selector.allPlayers"),
            ("@r", "argument.entity.selector.randomPlayer"),
            ("@s", "argument.entity.selector.self"),
            ("@e", "argument.entity.selector.allEntities"),
            ("@n", "argument.entity.selector.nearestEntity"),
        ] {
            builder.suggest_with_tooltip(
                text,
                Some(message(Component::translatable(key, Vec::new()))),
            );
        }
    }

    /// `fillSuggestions(builder, names)`: `names` adds player/entity names.
    pub fn fill_suggestions(
        &self,
        builder: SuggestionsBuilder,
        names: &dyn Fn(&mut SuggestionsBuilder),
    ) -> Suggestions {
        let mut builder = builder.create_offset(self.reader.get_cursor());
        match &self.suggestions {
            Suggest::Nothing => builder.build(),
            Suggest::NameOrSelector => {
                names(&mut builder);
                if self.allow_selectors {
                    Self::fill_selector_suggestions(&mut builder);
                }
                builder.build()
            }
            Suggest::Name => {
                let mut sub = builder.create_offset(self.start_position);
                names(&mut sub);
                builder.add(&sub);
                builder.build()
            }
            Suggest::Selector => {
                let mut sub = builder.create_offset(builder.get_start().saturating_sub(1));
                Self::fill_selector_suggestions(&mut sub);
                builder.add(&sub);
                builder.build()
            }
            Suggest::OpenOptions => {
                builder.suggest("[");
                builder.build()
            }
            Suggest::OptionsKeyOrClose => {
                builder.suggest("]");
                entity_selector_options::suggest_names(self, &mut builder);
                builder.build()
            }
            Suggest::OptionsKey => {
                entity_selector_options::suggest_names(self, &mut builder);
                builder.build()
            }
            Suggest::OptionsNextOrClose => {
                builder.suggest(",");
                builder.suggest("]");
                builder.build()
            }
            Suggest::Custom(f) => f(builder),
        }
    }
}
