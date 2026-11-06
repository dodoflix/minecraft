//! `SuggestionProviders`: named suggestion providers the client can be told to use instead of asking the server.

use crate::commands::CommandSourceStack;
use crate::commands::message::message;
use crate::commands::shared_suggestion_provider::{
    suggest_resource, suggest_resource_with_tooltip,
};
use mc_brigadier::{
    CommandContext, CommandSyntaxException, SuggestionProvider, Suggestions, SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_data::{EntityType, SoundEvent};
use mc_text::Component;
use std::sync::{Arc, LazyLock, Mutex};

type Provider = SuggestionProvider<CommandSourceStack>;

static PROVIDERS_BY_NAME: LazyLock<Mutex<Vec<(Identifier, Provider)>>> = LazyLock::new(|| {
    let mut v: Vec<(Identifier, Provider)> = Vec::new();
    let id = |p: &str| Identifier::with_default_namespace(p).expect("valid id");
    v.push((
        id("ask_server"),
        Arc::new(
            |c: &CommandContext<CommandSourceStack>, _b: SuggestionsBuilder| {
                Ok(c.get_source().custom_suggestion(c))
            },
        ),
    ));
    v.push((
        id("available_sounds"),
        Arc::new(
            |_c: &CommandContext<CommandSourceStack>, b: SuggestionsBuilder| {
                Ok(suggest_resource(
                    SoundEvent::iter().map(|s| s.identifier()),
                    b,
                ))
            },
        ),
    ));
    v.push((
        id("summonable_entities"),
        Arc::new(
            |c: &CommandContext<CommandSourceStack>, b: SuggestionsBuilder| {
                let features = c.get_source().enabled_features();
                Ok(suggest_resource_with_tooltip(
                    EntityType::iter().filter(|t| t.data().summonable && is_enabled(t, features)),
                    b,
                    |t| t.identifier(),
                    |t| {
                        Some(message(Component::translatable(
                            t.data().description_id,
                            Vec::new(),
                        )))
                    },
                ))
            },
        ),
    ));
    v.push((
        id("post_effects"),
        Arc::new(|_c: &CommandContext<CommandSourceStack>, b: SuggestionsBuilder| Ok(b.build())),
    ));
    Mutex::new(v)
});

fn is_enabled(t: &EntityType, features: mc_world::level_storage::FeatureFlagSet) -> bool {
    let required: Vec<Identifier> = t
        .data()
        .required_features
        .iter()
        .filter_map(|n| Identifier::try_parse(n))
        .collect();
    mc_world::level_storage::FeatureFlagSet::from_names(required.iter(), |_| {})
        .is_subset_of(features)
}

fn find(name: &str) -> Provider {
    let id = Identifier::with_default_namespace(name).expect("valid id");
    PROVIDERS_BY_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(n, _)| *n == id)
        .expect("built-in provider")
        .1
        .clone()
}

pub fn ask_server() -> Provider {
    find("ask_server")
}

pub fn available_sounds() -> Provider {
    find("available_sounds")
}

pub fn summonable_entities() -> Provider {
    find("summonable_entities")
}

pub fn post_effects() -> Provider {
    find("post_effects")
}

/// `register(name, provider)`: panics when the name is taken.
pub fn register(name: Identifier, provider: Provider) -> Provider {
    let mut providers = PROVIDERS_BY_NAME.lock().unwrap_or_else(|e| e.into_inner());
    assert!(
        !providers.iter().any(|(n, _)| *n == name),
        "A command suggestion provider is already registered with the name '{name}'"
    );
    providers.push((name, provider.clone()));
    provider
}

/// `getProvider(name)` (unknown names ask the server).
pub fn get_provider(name: &Identifier) -> Provider {
    let found = PROVIDERS_BY_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, p)| p.clone());
    found.unwrap_or_else(ask_server)
}

/// `getName(provider)`: the registered name, `minecraft:ask_server` for any other provider.
pub fn get_name(provider: &Provider) -> Identifier {
    let found = PROVIDERS_BY_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(_, p)| Arc::ptr_eq(p, provider))
        .map(|(n, _)| n.clone());
    found.unwrap_or_else(|| Identifier::with_default_namespace("ask_server").expect("valid id"))
}
