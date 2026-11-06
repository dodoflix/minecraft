//! `MushroomCow` (mooshroom): red/brown variants, mushroom stew / suspicious stew, shearing into a cow.

use super::cow::{abstract_cow_mob_interact, register_goals};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, GameEvent, Item, ParticleType, SoundEvent};
use mc_dfu::NBT;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::convert::{self, ConversionParams};
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ext};
use mc_game_core::world::entity::player::menu_ctx::PlayerCtx;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{
    InteractionHand, InteractionResult, ItemStack, create_filled_result, hurt_and_break,
};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{LevelReader, block::BlockStateExt as _};
use mc_nbt::CompoundTag;
use mc_protocol::components::{
    DataComponentMap, MOOSHROOM_VARIANT, MushroomCowVariant, SUSPICIOUS_STEW_EFFECTS,
    SuspiciousStewEffects,
};
use mc_protocol::packets::play::{ParticleOptions, RandomizationType, SoundSource};
use uuid::Uuid;

define_accessor!(pub DATA_TYPE: Int = MushroomCow);

const MUTATE_CHANCE: i32 = 1024;
const DEFAULT_VARIANT: MushroomCowVariant = MushroomCowVariant::Red;

/// `MushroomCow.Variant.byId(id)` (`CLAMP`).
fn variant_by_id(id: i32) -> MushroomCowVariant {
    MushroomCowVariant::ALL[id.clamp(0, MushroomCowVariant::ALL.len() as i32 - 1) as usize]
}

/// `MushroomCow.Variant.getBlockState()`.
pub fn variant_block(variant: MushroomCowVariant) -> Block {
    if variant == MushroomCowVariant::Red {
        Block::RED_MUSHROOM
    } else {
        Block::BROWN_MUSHROOM
    }
}

fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.45, 0.7)
        .with_eye_height(0.69)
        .with_attachments(
            EntityAttachments::create_default(0.45, 0.7).with_passenger(Vec3::new(0.0, 0.75, 0.0)),
        )
}

/// `MushroomCow`.
pub struct MushroomCow {
    pub mob: MobData,
    stew_effects: Option<SuspiciousStewEffects>,
    last_lightning_bolt_uuid: Option<Uuid>,
}

impl MushroomCow {
    /// `new MushroomCow(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_TYPE, DEFAULT_VARIANT.id());
        });
        let mut cow = MushroomCow {
            mob,
            stew_effects: None,
            last_lightning_bolt_uuid: None,
        };
        let navigation = StandardPathNavigation::ground(&cow);
        cow.mob.set_navigation(Box::new(navigation));
        register_goals(&mut cow);
        cow
    }

    /// `getVariant()`.
    pub fn get_variant(&self) -> MushroomCowVariant {
        variant_by_id(self.mob.living.entity.entity_data.get(DATA_TYPE))
    }

    /// `setVariant(variant)`.
    pub fn set_variant(&mut self, variant: MushroomCowVariant) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_TYPE, variant.id());
    }

    /// `getOffspringVariant(mate)`.
    fn get_offspring_variant(&mut self, mate: MushroomCowVariant) -> MushroomCowVariant {
        let variant = self.get_variant();
        if variant == mate && self.base_mut().random.next_int_bounded(MUTATE_CHANCE) == 0 {
            if variant == MushroomCowVariant::Brown {
                MushroomCowVariant::Red
            } else {
                MushroomCowVariant::Brown
            }
        } else if self.base_mut().random.next_boolean() {
            variant
        } else {
            mate
        }
    }
}

/// `MushroomCow.checkMushroomSpawnRules`.
pub fn check_mushroom_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("mooshrooms_spawnable_on")
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::MOOSHROOM, check_mushroom_spawn_rules);

impl Entity for MushroomCow {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:mooshroom")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        super::cow::is_cow_food(stack)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_COW_AMBIENT)
    }

    fn get_hurt_sound(
        &self,
        _source: &mc_game_core::world::damagesource::DamageSource,
    ) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_COW_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_COW_DEATH)
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_COW_STEP, 0.15, 1.0);
    }

    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if level.get_block_state(pos.below()).block() == Block::MYCELIUM {
            10.0
        } else {
            level.get_pathfinding_cost_from_light_levels(pos)
        }
    }

    fn thunder_hit(&mut self, _level: &mut ServerLevel) {}

    fn thunder_hit_by(&mut self, level: &mut ServerLevel, bolt: EntityId) {
        let Some(uuid) = level.get_entity(bolt).map(|b| b.uuid()) else {
            return;
        };
        if self.last_lightning_bolt_uuid != Some(uuid) {
            let next = if self.get_variant() == MushroomCowVariant::Red {
                MushroomCowVariant::Brown
            } else {
                MushroomCowVariant::Red
            };
            self.set_variant(next);
            self.last_lightning_bolt_uuid = Some(uuid);
            self.play_sound(level, SoundEvent::ENTITY_MOOSHROOM_CONVERT, 2.0, 1.0);
        }
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.is(Item::BOWL) && !self.is_baby() {
            let mut is_suspicious = false;
            let stew = match self.stew_effects.take() {
                Some(effects) => {
                    is_suspicious = true;
                    let mut stew = ItemStack::new(Item::SUSPICIOUS_STEW);
                    stew.set(SUSPICIOUS_STEW_EFFECTS, effects);
                    stew
                }
                None => ItemStack::new(Item::MUSHROOM_STEW),
            };
            let result =
                create_filled_result(item_stack, &mut PlayerCtx { player, level }, stew, false);
            player.set_item_in_hand(hand, result);
            let milk_sound = if is_suspicious {
                SoundEvent::ENTITY_MOOSHROOM_SUSPICIOUS_MILK
            } else {
                SoundEvent::ENTITY_MOOSHROOM_MILK
            };
            self.play_sound(level, milk_sound, 1.0, 1.0);
            InteractionResult::SUCCESS
        } else if item_stack.is(Item::SHEARS) && self.ready_for_shearing() {
            self.shear(level, SoundSource::Players, &item_stack);
            level.game_event_from_entity(Some(player.id()), GameEvent::SHEAR, self.position());
            mob::interact::with_hand_stack(player, hand, |s, p| {
                hurt_and_break(s, 1, level, p, hand.as_equipment_slot())
            });
            InteractionResult::SUCCESS
        } else if self.get_variant() == MushroomCowVariant::Brown && !self.is_baby() {
            let effects = item_stack.item().block().and_then(|b| {
                mc_game_core::world::level::block::behaviour(b).get_suspicious_effects()
            });
            match effects {
                None => abstract_cow_mob_interact(self, level, player, hand),
                Some(effects) => {
                    let (x, y, z) = (
                        self.base().x(),
                        self.base().y() + f64::from(self.base().get_bb_height()) * 0.5,
                        self.base().z(),
                    );
                    if self.stew_effects.is_some() {
                        level.send_particles_full(
                            ParticleOptions::Simple(ParticleType::SMOKE),
                            false,
                            false,
                            x,
                            y,
                            z,
                            2,
                            0.5,
                            0.0,
                            0.5,
                            0.0,
                            0.2,
                            0.0,
                            RandomizationType::AlternativeWithSpeed,
                        );
                    } else {
                        mob::interact::with_hand_stack(player, hand, |s, p| {
                            mc_game_core::world::item::ItemStackExt::consume(
                                s,
                                1,
                                p.has_infinite_materials(),
                            )
                        });
                        let particle = ParticleOptions::Spell {
                            particle_type: ParticleType::EFFECT,
                            color: -1,
                            power: 1.0,
                        };
                        level.send_particles_full(
                            particle,
                            false,
                            false,
                            x,
                            y,
                            z,
                            4,
                            0.5,
                            0.0,
                            0.5,
                            0.0,
                            0.2,
                            0.0,
                            RandomizationType::AlternativeWithSpeed,
                        );
                        self.stew_effects = Some(effects);
                        self.play_sound(level, SoundEvent::ENTITY_MOOSHROOM_EAT, 2.0, 1.0);
                    }
                    InteractionResult::SUCCESS
                }
            }
        } else {
            abstract_cow_mob_interact(self, level, player, hand)
        }
    }

    fn is_shearable(&self) -> bool {
        true
    }

    fn ready_for_shearing(&self) -> bool {
        !self.is_baby()
    }

    fn shear(&mut self, level: &mut ServerLevel, sound_source: SoundSource, tool: &ItemStack) {
        ext::play_sound_from_entity(
            level,
            None,
            self.id(),
            SoundEvent::ENTITY_MOOSHROOM_SHEAR,
            sound_source,
            1.0,
            1.0,
        );
        let (x, y, z) = (self.base().x(), self.base().y(), self.base().z());
        let y_half = y + f64::from(self.base().get_bb_height()) * 0.5;
        let table = Identifier::parse("minecraft:shearing/mooshroom").expect("valid id");
        let mut drops: Vec<ItemStack> = Vec::new();
        ext::drop_from_shearing_loot_table(level, self, &table, tool, |_, _, drop| {
            for _ in 0..drop.count() {
                drops.push(drop.copy_with_count(1));
            }
        });
        let params = ConversionParams::single(self, level, false, false);
        convert::convert_to(
            self,
            level,
            EntityType::COW,
            &params,
            EntitySpawnReason::Conversion,
            |_, level| {
                level.send_particles(
                    ParticleOptions::Simple(ParticleType::EXPLOSION),
                    x,
                    y_half,
                    z,
                    1,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                );
                for stack in drops {
                    let item = mc_game_core::world::entity::item::ItemEntity::with_stack(
                        level,
                        x,
                        y + f64::from(1.0f32),
                        z,
                        stack,
                    );
                    level.add_fresh_entity(Box::new(item));
                }
            },
        );
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(
            EntityType::MOOSHROOM,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let mate = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<MushroomCow>())
            .map(MushroomCow::get_variant)?;
        let variant = self.get_offspring_variant(mate);
        if let Some(cow) = baby.as_any_mut().downcast_mut::<MushroomCow>() {
            cow.set_variant(variant);
        }
        Some(baby)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_string("Type", self.get_variant().name());
        if let Some(effects) = &self.stew_effects {
            if let Ok(tag) = SuspiciousStewEffects::codec()
                .encode_start(&NBT, effects)
                .into_result()
                .and_then(|d| d.to_tag())
            {
                out.put("stew_effects", tag);
            }
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_variant(
            input
                .get_string("Type")
                .and_then(MushroomCowVariant::from_name)
                .unwrap_or(DEFAULT_VARIANT),
        );
        self.stew_effects = input.get("stew_effects").and_then(|t| {
            SuspiciousStewEffects::codec()
                .parse(&NBT, &mc_dfu::Dyn::from(t))
                .into_result()
                .ok()
        });
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(MOOSHROOM_VARIANT, self.get_variant())
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&MOOSHROOM_VARIANT) {
            self.set_variant(*variant);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MOOSHROOM: EntityRegistration = EntityRegistration {
    entity_type: EntityType::MOOSHROOM,
    create: |ty, id, registries| Box::new(MushroomCow::new(ty, id, registries)),
};
