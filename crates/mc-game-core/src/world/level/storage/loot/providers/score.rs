//! `loot.providers.score`: `ScoreboardNameProvider` (`fixed`, `context`).

use std::any::Any;
use std::sync::{Arc, LazyLock};

use mc_dfu::{Codec, Either, STRING, record};

use crate::world::level::storage::loot::context::{ContextKey, EntityTarget, LootContext};
use crate::world::level::storage::loot::holder::{TypeTable, lift};
use crate::world::level::storage::loot::validation::ValidationContext;

/// `ScoreboardNameProvider`.
pub trait ScoreboardNameProvider: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    /// `getScoreHolder(context)`: the score holder's scoreboard name.
    fn get_score_holder(&self, context: &LootContext) -> Option<String>;
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

/// `FixedScoreboardNameProvider`.
#[derive(Clone)]
pub struct FixedScoreboardNameProvider {
    pub name: String,
}

impl ScoreboardNameProvider for FixedScoreboardNameProvider {
    fn type_name(&self) -> &'static str {
        "fixed"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_score_holder(&self, _context: &LootContext) -> Option<String> {
        Some(self.name.clone())
    }
}

/// `ContextScoreboardNameProvider`.
#[derive(Clone)]
pub struct ContextScoreboardNameProvider {
    pub target: EntityTarget,
}

impl ScoreboardNameProvider for ContextScoreboardNameProvider {
    fn type_name(&self) -> &'static str {
        "context"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_score_holder(&self, context: &LootContext) -> Option<String> {
        self.target.get(context).map(|e| e.get_scoreboard_name())
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![self.target.context_param()]
    }
}

static TYPES: LazyLock<Arc<TypeTable<dyn ScoreboardNameProvider>>> = LazyLock::new(|| {
    TypeTable::new(
        "minecraft:loot_score_provider_type",
        vec![
            (
                "fixed",
                lift(
                    record!(FixedScoreboardNameProvider {
                        name: STRING.field_of("name")
                    }),
                    |v| Arc::new(v) as Arc<dyn ScoreboardNameProvider>,
                    |t| t.as_any().downcast_ref::<FixedScoreboardNameProvider>(),
                ),
            ),
            (
                "context",
                lift(
                    record!(ContextScoreboardNameProvider {
                        target: EntityTarget::codec().field_of("target")
                    }),
                    |v| Arc::new(v) as Arc<dyn ScoreboardNameProvider>,
                    |t| t.as_any().downcast_ref::<ContextScoreboardNameProvider>(),
                ),
            ),
        ],
    )
});

/// `ScoreboardNameProviders.CODEC`.
pub fn scoreboard_name_provider_codec() -> Codec<Arc<dyn ScoreboardNameProvider>> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<Arc<dyn ScoreboardNameProvider>>> = LazyLock::new(|| {
            let typed = TYPES.codec(|e| e.type_name());
            let inline = EntityTarget::codec().xmap(
                |target| ContextScoreboardNameProvider { target },
                |c: &ContextScoreboardNameProvider| c.target,
            );
            inline.either(&typed).xmap(
                |e| match e {
                    Either::Left(c) => Arc::new(c) as Arc<dyn ScoreboardNameProvider>,
                    Either::Right(p) => p,
                },
                |p: &Arc<dyn ScoreboardNameProvider>| match p
                    .as_any()
                    .downcast_ref::<ContextScoreboardNameProvider>()
                {
                    Some(c) => Either::Left(c.clone()),
                    None => Either::Right(p.clone()),
                },
            )
        });
        C.clone()
    })
}
