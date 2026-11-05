//! `net.minecraft.advancements.AdvancementRewards`.

use std::sync::{Arc, OnceLock};

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, INT};
use mc_protocol::components::component_codec as _;

use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::player::ServerPlayer;
use crate::world::inventory::Prediction;
use crate::world::level::storage::loot::codec_ext::OptionalDefaultWith;
use crate::world::level::storage::loot::holder::{HolderSet, holder_set_codec};
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::table::{LootTable, loot_table_direct_codec};
use crate::world::level::storage::loot::{LootParams, LootTable as _};

/// What rewards need from subsystems that own their state outside the level (recipes: `ServerRecipeBook`, functions:
/// `ServerFunctionManager`). Installed once by the owner (`set_reward_hooks`).
pub trait RewardHooks: Send + Sync {
    /// `ServerPlayer.awardRecipesByKey(recipes)`.
    fn award_recipes_by_key(&self, level: &mut ServerLevel, player: &mut ServerPlayer, recipes: &[Identifier]);
    /// `server.getFunctions().execute(function, player.createCommandSourceStack().withSuppressedOutput()
    /// .withPermission(GAMEMASTER))`; a function that does not exist is skipped.
    fn run_function(&self, level: &mut ServerLevel, player: &mut ServerPlayer, function: &Identifier);
}

static REWARD_HOOKS: OnceLock<&'static dyn RewardHooks> = OnceLock::new();

/// Installs the recipe/function seam (first caller wins).
pub fn set_reward_hooks(hooks: &'static dyn RewardHooks) {
    let _ = REWARD_HOOKS.set(hooks);
}

#[derive(Clone)]
pub struct AdvancementRewards {
    pub experience: i32,
    pub loot: HolderSet<LootTable>,
    /// `ResourceKey<Recipe<?>>` ids.
    pub recipes: Vec<Identifier>,
    /// `CacheableFunction`: the function id.
    pub function: Option<Identifier>,
}

impl AdvancementRewards {
    pub fn empty() -> AdvancementRewards {
        AdvancementRewards { experience: 0, loot: HolderSet::Direct(Vec::new()), recipes: Vec::new(), function: None }
    }

    /// `AdvancementRewards.CODEC`.
    pub fn codec() -> Codec<AdvancementRewards> {
        static C: std::sync::LazyLock<Codec<AdvancementRewards>> = std::sync::LazyLock::new(|| {
            let loot_list = holder_set_codec(keys::LOOT_TABLE, loot_table_direct_codec());
            mc_dfu::record!(AdvancementRewards {
                experience: INT.optional_field_of_default("experience", 0),
                loot: loot_list.optional_field_of_default_with("loot", HolderSet::Direct(Vec::new())),
                recipes: IDENTIFIER.list_of().optional_field_of_default("recipes", Vec::new()),
                function: IDENTIFIER.optional_field_of("function"),
            })
            .codec()
        });
        C.clone()
    }

    /// `grant(player)`.
    pub fn grant(&self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        player.give_experience_points(level, self.experience);
        let tables: Vec<Arc<LootTable>> = self.loot.values(level.registries.as_ref());
        let mut changes = false;
        for table in tables {
            let items = {
                let params = LootParams::builder(level)
                    .this_entity(&*player)
                    .origin(player.position())
                    .create(&LootContextParamSets::ADVANCEMENT_REWARD);
                table.get_random_items(&params)
            };
            for stack in items {
                if player.add_item(stack.clone()) {
                    let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
                    let random = &mut player.base_mut().random;
                    let pitch = ((random.next_float() - random.next_float()) * 0.7f32 + 1.0f32) * 2.0f32;
                    level.play_sound_at(None, x, y, z, mc_data::SoundEvent::ENTITY_ITEM_PICKUP, mc_data::SoundSource::Players, 0.2, pitch);
                    changes = true;
                } else {
                    player.drop_reward_item(level, stack);
                }
            }
        }
        if changes {
            player.broadcast_container_changes();
        }
        if let Some(hooks) = REWARD_HOOKS.get() {
            if !self.recipes.is_empty() {
                hooks.award_recipes_by_key(level, player, &self.recipes);
            }
            if let Some(function) = &self.function {
                hooks.run_function(level, player, function);
            }
        }
    }
}
