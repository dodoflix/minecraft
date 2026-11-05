//! `Sheep`: wool colours, shearing/regrowing (`EatBlockGoal`), colour mixing when breeding.

use super::sheep_color_spawn_rules;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_ai::world::entity::ai::goal::EatBlockGoal;
use mc_game_core::world::entity::ai::goal::{BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal, TemptGoal, WaterAvoidingRandomStrollGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, hurt_and_break};
use mc_game_core::world::level::LevelAccessor as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, DyeColor, SHEEP_COLOR};
use mc_protocol::packets::play::SoundSource;
use std::sync::Arc;

define_accessor!(pub DATA_WOOL_ID: Byte = Sheep);

const DEFAULT_COLOR: DyeColor = DyeColor::White;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.45, 0.65).with_eye_height(0.65625);
    dims.with_attachments(EntityAttachments::create_default(0.45, 0.65).with_passenger(Vec3::new(0.0, 0.5625, 0.0)))
}

/// `Sheep`.
pub struct Sheep {
    pub mob: MobData,
    /// Client animation counter, mirrored from the `EatBlockGoal` each server tick.
    eat_animation_tick: i32,
}

/// `ItemTags.SHEEP_FOOD`.
fn is_sheep_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("sheep_food").is_some_and(|t| t.contains(stack.item()))
}

impl Sheep {
    /// `new Sheep(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_WOOL_ID, 0i8);
        });
        let mut sheep = Sheep { mob, eat_animation_tick: 0 };
        let navigation = StandardPathNavigation::ground(&sheep);
        sheep.mob.set_navigation(Box::new(navigation));
        sheep.register_goals();
        sheep
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, 1.0);
        let follow_parent = FollowParentGoal::new(self, 1.1);
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(PanicGoal::new(1.25)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(3, Box::new(TemptGoal::new(1.1, Arc::new(|stack: &ItemStack| is_sheep_food(stack)), false)));
        goals.add_goal(4, Box::new(follow_parent));
        goals.add_goal(5, Box::new(EatBlockGoal::new()));
        goals.add_goal(6, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        goals.add_goal(7, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
    }

    fn wool(&self) -> i8 {
        self.mob.living.entity.entity_data.get(DATA_WOOL_ID)
    }

    /// `getColor()`.
    pub fn get_color(&self) -> DyeColor {
        DyeColor::from_id(i32::from(self.wool()) & 15).unwrap_or(DEFAULT_COLOR)
    }

    /// `setColor(color)`.
    pub fn set_color(&mut self, color: DyeColor) {
        let current = i32::from(self.wool());
        self.mob.living.entity.entity_data.set(DATA_WOOL_ID, ((current & 240) | (color.id() & 15)) as i8);
    }

    /// `isSheared()`.
    pub fn is_sheared(&self) -> bool {
        (i32::from(self.wool()) & 16) != 0
    }

    /// `setSheared(value)`.
    pub fn set_sheared(&mut self, value: bool) {
        let current = i32::from(self.wool());
        let next = if value { current | 16 } else { current & -17 };
        self.mob.living.entity.entity_data.set(DATA_WOOL_ID, next as i8);
    }

    /// `getEatAnimationTick()` as mirrored for the client animation.
    pub fn eat_animation_tick(&self) -> i32 {
        self.eat_animation_tick
    }

    /// `getHeadEatPositionScale(a)`.
    pub fn get_head_eat_position_scale(&self, a: f32) -> f32 {
        let tick = self.eat_animation_tick;
        if tick <= 0 {
            0.0
        } else if (4..=36).contains(&tick) {
            1.0
        } else if tick < 4 {
            (tick as f32 - a) / 4.0
        } else {
            -((tick - 40) as f32 - a) / 4.0
        }
    }

    /// `getRandomSheepColor(level, pos)`.
    pub fn get_random_sheep_color(level: &mut dyn ServerLevelAccessor, pos: BlockPos) -> DyeColor {
        sheep_color_spawn_rules::get_sheep_color(level, pos)
    }
}

/// `DyeColor.getMixedColor(level, color1, color2)`.
pub fn get_mixed_color(level: &mut ServerLevel, color1: DyeColor, color2: DyeColor) -> DyeColor {
    if let Some(mixed) = find_color_mix_in_recipes(level, color1, color2) {
        return mixed;
    }
    if level.random().next_boolean() { color1 } else { color2 }
}

/// `DyeColor.findColorMixInRecipes(level, color1, color2)`: the first crafting recipe mixing a dye item of each colour.
fn find_color_mix_in_recipes(level: &ServerLevel, color1: DyeColor, color2: DyeColor) -> Option<DyeColor> {
    use mc_game_core::world::item::crafting::CraftingInput;
    use mc_data::RecipeType;
    use mc_protocol::components::DYE;
    let items_of = |color: DyeColor| Item::iter().filter(|item| ItemStack::new(*item).get(&DYE) == Some(&color)).collect::<Vec<_>>();
    let dye1_items = items_of(color1);
    if dye1_items.is_empty() {
        return None;
    }
    let dye2_items = items_of(color2);
    if dye2_items.is_empty() {
        return None;
    }
    let manager = level.recipe_access();
    for dye1 in &dye1_items {
        for dye2 in &dye2_items {
            let input = CraftingInput::of(2, 1, &[ItemStack::new(*dye1), ItemStack::new(*dye2)]);
            if let Some(found) = manager.get_recipe_for(RecipeType::CRAFTING, &input, level, None) {
                let result = found.value.assemble_crafting(&input);
                if let Some(color) = result.get(&DYE) {
                    return Some(*color);
                }
            }
        }
    }
    None
}

impl Entity for Sheep {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:sheep")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        if let Some(goal) = self.mob.goal_selector.find_goal::<EatBlockGoal>() {
            self.eat_animation_tick = goal.get_eat_animation_tick();
        }
        mob::animal::custom_server_ai_step(self);
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_sheep_food(stack)
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut mc_game_core::world::entity::ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.is(Item::SHEARS) {
            if self.ready_for_shearing() {
                self.shear(level, SoundSource::Players, &item_stack);
                self.game_event_from(level, player);
                mob::interact::with_hand_stack(player, hand, |s, p| hurt_and_break(s, 1, level, p, hand.as_equipment_slot()));
                InteractionResult::SUCCESS_SERVER
            } else {
                InteractionResult::CONSUME
            }
        } else {
            mob::interact::mob_interact(self, level, player, hand)
        }
    }

    fn is_shearable(&self) -> bool {
        true
    }

    fn ready_for_shearing(&self) -> bool {
        !self.is_sheared() && !self.is_baby()
    }

    fn shear(&mut self, level: &mut ServerLevel, sound_source: SoundSource, tool: &ItemStack) {
        mob::ext::play_sound_from_entity(level, None, self.id(), SoundEvent::ENTITY_SHEEP_SHEAR, sound_source, 1.0, 1.0);
        let table = Identifier::parse("minecraft:shearing/sheep").expect("valid id");
        mob::ext::drop_from_shearing_loot_table(level, self, &table, tool, |level, e, drop| {
            for _ in 0..drop.count() {
                let Some(item) = mc_game_core::world::entity::item::spawn_at_location(level, e, drop.copy_with_count(1), Vec3::new(0.0, 1.0, 0.0)) else { continue };
                let random = &mut e.base_mut().random;
                let dx = f64::from((random.next_float() - random.next_float()) * 0.1);
                let dy = f64::from(random.next_float() * 0.05);
                let dz = f64::from((random.next_float() - random.next_float()) * 0.1);
                level.with_entity(item, |_, entity| {
                    let motion = entity.base().delta_movement();
                    entity.base_mut().set_delta_movement(motion.add(Vec3::new(dx, dy, dz)));
                });
            }
        });
        self.set_sheared(true);
    }

    fn sheep_sheared(&self) -> Option<bool> {
        Some(self.is_sheared())
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("Sheared", self.is_sheared());
        out.put_byte("Color", self.get_color().id() as i8);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_sheared(input.get_boolean_or("Sheared", false));
        let color = input.get_byte("Color").and_then(|b| DyeColor::from_id(i32::from(b))).unwrap_or(DEFAULT_COLOR);
        self.set_color(color);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SHEEP_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SHEEP_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SHEEP_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_SHEEP_STEP, 0.15, 1.0);
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, partner: EntityId) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(EntityType::SHEEP, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })?;
        let parent1 = self.get_color();
        let parent2 = level.get_entity(partner).and_then(|p| p.as_any().downcast_ref::<Sheep>()).map(|p| p.get_color())?;
        let mixed = get_mixed_color(level, parent1, parent2);
        if let Some(sheep) = baby.as_any_mut().downcast_mut::<Sheep>() {
            sheep.set_color(mixed);
        }
        Some(baby)
    }

    fn ate(&mut self, level: &mut ServerLevel) {
        self.game_event(level, GameEvent::EAT);
        self.set_sheared(false);
        if self.can_age_up() {
            mob::ageable::age_up(self, level, 60, false);
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let pos = self.block_position();
        let color = Sheep::get_random_sheep_color(level, pos);
        self.set_color(color);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(self.base());
        DataComponentMap::builder().add_all(&base).set(SHEEP_COLOR, self.get_color()).build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(color) = stack.get(&SHEEP_COLOR) {
            self.set_color(*color);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

impl Sheep {
    /// `gameEvent(event, player)` for `SHEAR`.
    fn game_event_from(&self, level: &mut ServerLevel, player: &mc_game_core::world::entity::ServerPlayer) {
        level.game_event_from_entity(Some(player.id()), GameEvent::SHEAR, self.position());
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SHEEP: EntityRegistration = EntityRegistration { entity_type: EntityType::SHEEP, create: |ty, id, registries| Box::new(Sheep::new(ty, id, registries)) };
