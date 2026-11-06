//! `net.minecraft.advancements.AdvancementRewards`.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, INT};
use mc_protocol::packets::play::SoundSource;

use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
use crate::world::item::InteractionHand;
use crate::world::level::storage::loot::LootParams;
use crate::world::level::storage::loot::codec_ext::OptionalDefaultWith;
use crate::world::level::storage::loot::holder::{HolderSet, holder_set_codec};
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::table::{LootTable, loot_table_direct_codec};

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
        AdvancementRewards {
            experience: 0,
            loot: HolderSet::Direct(Vec::new()),
            recipes: Vec::new(),
            function: None,
        }
    }

    /// `AdvancementRewards.CODEC`.
    pub fn codec() -> Codec<AdvancementRewards> {
        static C: std::sync::LazyLock<Codec<AdvancementRewards>> = std::sync::LazyLock::new(|| {
            let loot_list = holder_set_codec(keys::LOOT_TABLE, loot_table_direct_codec());
            mc_dfu::record!(AdvancementRewards {
                experience: INT.optional_field_of_default("experience", 0),
                loot: loot_list
                    .optional_field_of_default_with("loot", HolderSet::Direct(Vec::new())),
                recipes: IDENTIFIER
                    .list_of()
                    .optional_field_of_default("recipes", Vec::new()),
                function: IDENTIFIER.optional_field_of("function"),
            })
            .codec()
        });
        C.clone()
    }

    /// `grant(player)`.
    /// Not implemented: only the experience and loot rewards are given; the `recipes` and `function` rewards are skipped.
    pub fn grant(&self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        player.give_experience_points(level, self.experience);
        let tables: Vec<Arc<LootTable>> = self.loot.values(&level.registries);
        let mut changes = false;
        for table in tables {
            let items = {
                let params = LootParams::builder(level)
                    .this_entity(&*player)
                    .origin(player.position())
                    .create(&LootContextParamSets::ADVANCEMENT_REWARD);
                table.get_random_items(&params)
            };
            for mut stack in items {
                let infinite = player.player.abilities.instabuild;
                if player.player.inventory.add(&mut stack, infinite) {
                    let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
                    let random = &mut player.base_mut().random;
                    let pitch =
                        ((random.next_float() - random.next_float()) * 0.7f32 + 1.0f32) * 2.0f32;
                    level.play_sound_at(
                        None,
                        x,
                        y,
                        z,
                        mc_data::SoundEvent::ENTITY_ITEM_PICKUP,
                        SoundSource::Players,
                        0.2,
                        pitch,
                    );
                    changes = true;
                } else {
                    // `player.drop(itemStack, false, Prediction.SERVER_ONLY)`, then the entity is made immediately collectable
                    if stack.is_empty() {
                        continue;
                    }
                    player.swing(
                        level,
                        InteractionHand::MainHand,
                        SWING_ANIMATION_DEFAULT,
                        true,
                    );
                    if let Some(mut drop) =
                        player.create_item_stack_to_drop(level, stack, false, false)
                    {
                        drop.set_no_pick_up_delay();
                        drop.set_target(Some(player.uuid()));
                        level.add_fresh_entity(Box::new(drop));
                    }
                }
            }
        }
        if changes {
            player.broadcast_menu_changes(level);
        }
    }
}
