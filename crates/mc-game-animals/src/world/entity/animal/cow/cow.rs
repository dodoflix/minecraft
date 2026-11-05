//! `AbstractCow` (shared goals, milking) and `Cow` (variants, sound variants).

use super::cow_sound_variant::CowSoundVariant;
use super::cow_variant::{self, CowVariant};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::{BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal, TemptGoal, WaterAvoidingRandomStrollGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::player::menu_ctx::PlayerCtx;
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::variant::{self, SpawnContext};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, create_filled_result};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{COW_SOUND_VARIANT, COW_VARIANT, DataComponentMap, Holder};
use std::sync::Arc;

define_accessor!(pub DATA_VARIANT_ID: CowVariant = Cow);
define_accessor!(pub DATA_SOUND_VARIANT_ID: CowSoundVariant = Cow);

/// `ItemTags.COW_FOOD`.
pub fn is_cow_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("cow_food").is_some_and(|t| t.contains(stack.item()))
}

/// `AbstractCow.registerGoals()`.
pub fn register_goals(cow: &mut (impl Entity + ?Sized + Sized)) {
    let float = FloatGoal::new(cow);
    let breed = BreedGoal::new(cow, 1.0);
    let follow_parent = FollowParentGoal::new(cow, 1.25);
    let look_at = LookAtPlayerGoal::new_player(cow, 6.0);
    let goals = &mut cow.mob_mut().expect("mob").goal_selector;
    goals.add_goal(0, Box::new(float));
    goals.add_goal(1, Box::new(PanicGoal::new(2.0)));
    goals.add_goal(2, Box::new(breed));
    goals.add_goal(3, Box::new(TemptGoal::new(1.25, Arc::new(|i: &ItemStack| is_cow_food(i)), false)));
    goals.add_goal(4, Box::new(follow_parent));
    goals.add_goal(5, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
    goals.add_goal(6, Box::new(look_at));
    goals.add_goal(7, Box::new(RandomLookAroundGoal::new()));
}

/// `AbstractCow.mobInteract(player, hand)`: milking with a bucket.
pub fn abstract_cow_mob_interact(cow: &mut dyn Entity, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
    let item_stack = player.get_item_in_hand(hand);
    if item_stack.is(Item::BUCKET) && !cow.is_baby() {
        let p = player.position();
        level.play_sound_at(Some(player.id()), p.x, p.y, p.z, SoundEvent::ENTITY_COW_MILK, mc_protocol::packets::play::SoundSource::Players, 1.0, 1.0);
        let result = create_filled_result(item_stack, &mut PlayerCtx { player, level }, ItemStack::new(Item::MILK_BUCKET), false);
        player.set_item_in_hand(hand, result);
        InteractionResult::SUCCESS
    } else {
        mob::interact::mob_interact(cow, level, player, hand)
    }
}

/// `Cow.BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.45, 0.7).with_eye_height(0.69).with_attachments(EntityAttachments::create_default(0.45, 0.7).with_passenger(Vec3::new(0.0, 0.75, 0.0)))
}

/// `Cow`.
pub struct Cow {
    pub mob: MobData,
    registries: RegistryAccess,
    variant_key: Identifier,
    sound_variant_key: Identifier,
    sound_variant: CowSoundVariant,
}

impl Cow {
    /// `new Cow(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(registries, keys::COW_VARIANT, &Identifier::parse(cow_variant::TEMPERATE).expect("valid id"));
        let sound_registry = registries.registry_or_throw(keys::COW_SOUND_VARIANT);
        let default_sound = sound_registry.id_of(&Identifier::parse("minecraft:classic").expect("valid id")).unwrap_or(0);
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_VARIANT_ID, default_variant as i32);
            b.define(DATA_SOUND_VARIANT_ID, default_sound as i32);
        });
        let variant_key = registries.registry_or_throw(keys::COW_VARIANT).name_of(default_variant).expect("variant entry");
        let sound_variant_key = sound_registry.name_of(default_sound).expect("sound variant entry");
        let sound_variant = *registries.get::<CowSoundVariant>(keys::COW_SOUND_VARIANT, &sound_variant_key).expect("typed cow sound variant");
        let mut cow = Cow { mob, registries: registries.clone(), variant_key, sound_variant_key, sound_variant };
        let navigation = StandardPathNavigation::ground(&cow);
        cow.mob.set_navigation(Box::new(navigation));
        register_goals(&mut cow);
        cow
    }

    /// `setVariant(variant)` by registry id.
    pub fn set_variant(&mut self, id: u32) {
        self.mob.living.entity.entity_data.set(DATA_VARIANT_ID, id as i32);
        if let Some(name) = self.registries.registry(keys::COW_VARIANT).and_then(|r| r.name_of(id)) {
            self.variant_key = name;
        }
    }

    /// `getVariant()` (registry id).
    pub fn get_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_VARIANT_ID)
    }

    fn set_sound_variant(&mut self, id: u32) {
        self.mob.living.entity.entity_data.set(DATA_SOUND_VARIANT_ID, id as i32);
        if let Some(name) = self.registries.registry(keys::COW_SOUND_VARIANT).and_then(|r| r.name_of(id)) {
            if let Some(v) = self.registries.get::<CowSoundVariant>(keys::COW_SOUND_VARIANT, &name) {
                self.sound_variant = *v;
            }
            self.sound_variant_key = name;
        }
    }
}

impl Entity for Cow {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:cow")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_cow_food(stack)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(self.sound_variant.ambient_sound)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(self.sound_variant.hurt_sound)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(self.sound_variant.death_sound)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        let sound = self.sound_variant.step_sound;
        self.play_sound(level, sound, 0.15, 1.0);
    }

    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        abstract_cow_mob_interact(self, level, player, hand)
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, partner: EntityId) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(EntityType::COW, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })?;
        let own = self.get_variant();
        let partner_variant = level.get_entity(partner).and_then(|p| p.as_any().downcast_ref::<Cow>()).map(Cow::get_variant);
        if let (Some(partner_variant), Some(cow)) = (partner_variant, baby.as_any_mut().downcast_mut::<Cow>()) {
            let pick = if self.base_mut().random.next_boolean() { own } else { partner_variant };
            cow.set_variant(pick as u32);
        }
        Some(baby)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let pos = self.block_position();
        let mut selected = None;
        level.with_random_taken(&mut |lvl, random| {
            let context = SpawnContext::create(lvl, pos);
            selected = variant::select_variant_to_spawn::<CowVariant>(&context, keys::COW_VARIANT, random);
            true
        });
        if let Some(id) = selected.and_then(|key| self.registries.registry(keys::COW_VARIANT).and_then(|r| r.id_of(&key))) {
            self.set_variant(id);
        }
        let registries = self.registries.clone();
        if let Some(key) = variant::pick_random_entry(&registries, keys::COW_SOUND_VARIANT, level.random()) {
            if let Some(id) = registries.registry(keys::COW_SOUND_VARIANT).and_then(|r| r.id_of(&key)) {
                self.set_sound_variant(id);
            }
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        variant::write_variant(out, &self.variant_key);
        out.put_string("sound_variant", self.sound_variant_key.to_string());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        if let Some(id) = variant::read_variant_key(input).and_then(|k| self.registries.registry(keys::COW_VARIANT).and_then(|r| r.id_of(&k))) {
            self.set_variant(id);
        }
        if let Some(id) = input.get_string("sound_variant").and_then(Identifier::try_parse).and_then(|k| self.registries.registry(keys::COW_SOUND_VARIANT).and_then(|r| r.id_of(&k))) {
            self.set_sound_variant(id);
        }
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(self.base());
        DataComponentMap::builder()
            .add_all(&base)
            .set(COW_VARIANT, Holder::Reference(self.variant_key.clone()))
            .set(COW_SOUND_VARIANT, Holder::Reference(self.sound_variant_key.clone()))
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(Holder::Reference(key)) = stack.get(&COW_VARIANT) {
            if let Some(id) = self.registries.registry(keys::COW_VARIANT).and_then(|r| r.id_of(key)) {
                self.set_variant(id);
            }
        }
        if let Some(Holder::Reference(key)) = stack.get(&COW_SOUND_VARIANT) {
            if let Some(id) = self.registries.registry(keys::COW_SOUND_VARIANT).and_then(|r| r.id_of(key)) {
                self.set_sound_variant(id);
            }
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static COW: EntityRegistration = EntityRegistration { entity_type: EntityType::COW, create: |ty, id, registries| Box::new(Cow::new(ty, id, registries)) };
