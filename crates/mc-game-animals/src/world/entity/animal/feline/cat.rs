//! `Cat`: tameable feline with collar, variants, bed/chest affinity and the morning gift.

use super::cat_sound_variant::{CatSoundSet, CatSoundVariant};
use super::cat_variant::CatVariant;
use crate::world::entity::animal::turtle;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{BedPart, EntitySpawnReason, Pose};
use mc_data::props;
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_ai::world::entity::ai::goal::OcelotAttackGoal;
use mc_game_ai::world::entity::ai::goal::target::NonTameRandomTargetGoal;
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, LeapAtTargetGoal};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::Flag;
use mc_game_core::world::entity::ai::goal::move_to_block;
use mc_game_core::world::entity::ai::goal::target::TargetType;
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowOwnerGoal, Goal, GoalFlags, LookAtPlayerGoal, MoveToBlock,
    MoveToBlockData, SitWhenOrderedToGoal, TamableAnimalPanicGoal, TemptGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ext, interact, leashable, tamable};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::variant::{self, SpawnContext};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::dye_color;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::level_access::LevelAccessor as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter as _, LevelReader as _};
use mc_nbt::CompoundTag;
use mc_protocol::components::{DYE, DyeColor};
use std::any::Any;
use std::sync::Arc;

define_accessor!(pub DATA_VARIANT_ID: CatVariant = Cat);
define_accessor!(pub IS_LYING: Boolean = Cat);
define_accessor!(pub RELAX_STATE_ONE: Boolean = Cat);
define_accessor!(pub DATA_COLLAR_COLOR: Int = Cat);
define_accessor!(pub DATA_SOUND_VARIANT_ID: CatSoundVariant = Cat);

const DEFAULT_COLLAR_COLOR: DyeColor = DyeColor::Red;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.3, 0.35).with_eye_height(0.34375);
    dims.with_attachments(
        EntityAttachments::create_default(0.3, 0.35).with_passenger(Vec3::new(0.0, 0.3125, 0.0)),
    )
}

fn is_cat_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("cat_food").is_some_and(|t| t.contains(stack.item()))
}

fn cat_of(mob: &dyn Entity) -> &Cat {
    mob.as_any().downcast_ref::<Cat>().expect("Cat")
}

/// `Cat.CatAvoidEntityGoal<Player>`.
pub struct CatAvoidEntityGoal {
    avoid: AvoidEntityGoal,
}

impl Default for CatAvoidEntityGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl CatAvoidEntityGoal {
    pub fn new() -> Self {
        Self {
            avoid: AvoidEntityGoal::new(Arc::new(|e| e.is_player()), 16.0, 0.8, 1.33),
        }
    }
}

impl Goal for CatAvoidEntityGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !tamable::is_tame(mob) && self.avoid.can_use_impl(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !tamable::is_tame(mob) && self.avoid.can_continue_to_use_impl(mob)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.avoid.start_impl(mob, level);
    }
    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.avoid.stop_impl();
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.avoid.tick_impl(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.avoid.flags()
    }
}

/// `Cat.CatTemptGoal`.
pub struct CatTemptGoal {
    tempt: TemptGoal,
    selected_player: Option<EntityId>,
}

impl Default for CatTemptGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl CatTemptGoal {
    pub fn new() -> Self {
        Self {
            tempt: TemptGoal::new(0.6, Arc::new(|stack: &ItemStack| is_cat_food(stack)), true),
            selected_player: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.tempt.is_running()
    }
}

impl Goal for CatTemptGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.tempt.can_use(mob, level) && !tamable::is_tame(mob)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let can_scare =
            if self.selected_player.is_some() && self.selected_player == self.tempt.player() {
                false
            } else {
                self.tempt.can_scare()
            };
        self.tempt.scare_check(mob, level, can_scare) && self.can_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tempt.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tempt.stop(mob, level);
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tempt.tick(mob, level);
        if self.selected_player.is_none()
            && mob
                .base_mut()
                .random
                .next_int_bounded(self.adjusted_tick_delay(600))
                == 0
        {
            self.selected_player = self.tempt.player();
        } else if mob
            .base_mut()
            .random
            .next_int_bounded(self.adjusted_tick_delay(500))
            == 0
        {
            self.selected_player = None;
        }
    }
    fn flags(&self) -> GoalFlags {
        self.tempt.flags()
    }
}

/// `Cat.CatRelaxOnOwnerGoal`.
pub struct CatRelaxOnOwnerGoal {
    owner_player: Option<EntityId>,
    goal_pos: Option<BlockPos>,
    on_bed_ticks: i32,
}

impl Default for CatRelaxOnOwnerGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl CatRelaxOnOwnerGoal {
    pub fn new() -> Self {
        Self {
            owner_player: None,
            goal_pos: None,
            on_bed_ticks: 0,
        }
    }

    fn space_is_occupied(&self, cat: &dyn Entity, level: &ServerLevel) -> bool {
        let Some(goal_pos) = self.goal_pos else {
            return false;
        };
        let bb = AABB::from_block_pos(&goal_pos).inflate_scalar(2.0);
        level
            .get_entities_of_class(&bb, |e| e.as_any().is::<Cat>(), |_| true)
            .into_iter()
            .filter(|id| *id != cat.id())
            .filter_map(|id| level.get_entity(id))
            .any(|other| {
                let c = cat_of(other);
                c.is_lying() || c.is_relax_state_one()
            })
    }
}

impl Goal for CatRelaxOnOwnerGoal {
    fn can_use(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !tamable::is_tame(cat) || tamable::is_ordered_to_sit(cat) {
            return false;
        }
        let Some(owner) = tamable::get_owner(cat, level).and_then(|o| level.get_entity(o)) else {
            return false;
        };
        if !owner.is_player() {
            return false;
        }
        self.owner_player = Some(owner.id());
        if !owner.living().is_some_and(|l| l.is_sleeping()) {
            return false;
        }
        if cat.base().distance_to_sqr_vec(owner.position()) > 100.0 {
            return false;
        }
        let owner_pos = owner.block_position();
        let state = level.get_block_state(owner_pos);
        if state.is_tag("cats_can_lie_on") {
            self.goal_pos = Some(match state.try_get(props::HORIZONTAL_FACING) {
                Some(bed_dir) => owner_pos.relative(bed_dir.get_opposite()),
                None => owner_pos,
            });
            return !self.space_is_occupied(cat, level);
        }
        false
    }

    fn can_continue_to_use(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        tamable::is_tame(cat)
            && !tamable::is_ordered_to_sit(cat)
            && self
                .owner_player
                .and_then(|p| level.get_entity(p))
                .is_some_and(|p| p.living().is_some_and(|l| l.is_sleeping()))
            && self.goal_pos.is_some()
            && !self.space_is_occupied(cat, level)
    }

    fn start(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(goal_pos) = self.goal_pos {
            tamable::set_in_sitting_pose(cat, false);
            mob::navigation::move_to(
                cat,
                level,
                f64::from(goal_pos.x()),
                f64::from(goal_pos.y()),
                f64::from(goal_pos.z()),
                f64::from(1.1f32),
            );
        }
    }

    fn stop(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(c) = cat.as_any_mut().downcast_mut::<Cat>() {
            c.set_lying(false);
        }
        let sleep_timer = self
            .owner_player
            .and_then(|p| level.get_entity(p))
            .and_then(|p| p.as_any().downcast_ref::<ServerPlayer>())
            .map_or(0, |p| p.player.sleep_counter);
        if sleep_timer >= 100 {
            let chance = level
                .environment_value(
                    mc_game_core::world::attribute::attributes::CAT_WAKING_UP_GIFT_CHANCE,
                    &cat.position(),
                )
                .as_float();
            if level.random().next_float() < chance {
                self.give_morning_gift(cat, level);
            }
        }
        self.on_bed_ticks = 0;
        if let Some(c) = cat.as_any_mut().downcast_mut::<Cat>() {
            c.set_relax_state_one(false);
        }
        mob::navigation::stop(cat, level);
    }

    fn tick(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        let (Some(owner_id), Some(goal_pos)) = (self.owner_player, self.goal_pos) else {
            return;
        };
        tamable::set_in_sitting_pose(cat, false);
        mob::navigation::move_to(
            cat,
            level,
            f64::from(goal_pos.x()),
            f64::from(goal_pos.y()),
            f64::from(goal_pos.z()),
            f64::from(1.1f32),
        );
        let Some(owner) = level.get_entity(owner_id) else {
            return;
        };
        if cat.base().distance_to_sqr_vec(owner.position()) < 2.5 {
            self.on_bed_ticks += 1;
            if self.on_bed_ticks > self.adjusted_tick_delay(16) {
                let c = cat.as_any_mut().downcast_mut::<Cat>().expect("Cat");
                c.set_lying(true);
                c.set_relax_state_one(false);
            } else {
                ext::mob_look_at(cat, owner, 45.0, 45.0);
                cat.as_any_mut()
                    .downcast_mut::<Cat>()
                    .expect("Cat")
                    .set_relax_state_one(true);
            }
        } else {
            cat.as_any_mut()
                .downcast_mut::<Cat>()
                .expect("Cat")
                .set_lying(false);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

impl CatRelaxOnOwnerGoal {
    /// `giveMorningGift()`.
    fn give_morning_gift(&self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        let base = match leashable::get_leash_holder(cat).and_then(|h| level.get_entity(h)) {
            Some(holder) => holder.block_position(),
            None => cat.block_position(),
        };
        let random = &mut cat.base_mut().random;
        let (dx, dy, dz) = (
            random.next_int_bounded(11) - 5,
            random.next_int_bounded(5) - 2,
            random.next_int_bounded(11) - 5,
        );
        cat.random_teleport(
            level,
            f64::from(base.x() + dx),
            f64::from(base.y() + dy),
            f64::from(base.z() + dz),
            false,
            "cat_does_not_teleport_to",
        );
        let cat_pos = cat.block_position();
        let y_body_rot = cat.living().map_or(0.0, |l| l.y_body_rot);
        let radians = y_body_rot * (std::f64::consts::PI / 180.0) as f32;
        ext::drop_from_gift_loot_table(
            level,
            cat,
            &Identifier::parse("minecraft:gameplay/cat_morning_gift").expect("valid id"),
            |level, _cat, stack| {
                let entity = ItemEntity::with_stack(
                    level,
                    f64::from(cat_pos.x()) - f64::from(mth::sin(f64::from(radians))),
                    f64::from(cat_pos.y()),
                    f64::from(cat_pos.z()) + f64::from(mth::cos(f64::from(radians))),
                    stack,
                );
                level.add_fresh_entity(Box::new(entity));
            },
        );
    }
}

/// `CatLieOnBlockGoal`.
pub struct CatLieOnBlockGoal {
    mtb: MoveToBlockData,
}

impl CatLieOnBlockGoal {
    pub fn new(speed_modifier: f64, search_range: i32) -> Self {
        let mut mtb = MoveToBlockData::with_vertical(speed_modifier, search_range, 6);
        mtb.vertical_search_start = -2;
        Self { mtb }
    }
}

impl MoveToBlock for CatLieOnBlockGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.mtb
    }
    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.mtb
    }
    fn next_start_tick(&self, _mob: &mut dyn Entity) -> i32 {
        40
    }
    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        level.is_empty_block(pos.above()) && level.get_block_state(pos).is_tag("cats_can_lie_on")
    }
}

impl Goal for CatLieOnBlockGoal {
    fn can_use(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        tamable::is_tame(cat)
            && !tamable::is_ordered_to_sit(cat)
            && !cat_of(cat).is_lying()
            && move_to_block::can_use(self, cat, level)
    }
    fn can_continue_to_use(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_continue_to_use(self, cat, level)
    }
    fn start(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::start(self, cat, level);
        tamable::set_in_sitting_pose(cat, false);
    }
    fn stop(&mut self, cat: &mut dyn Entity, _level: &mut ServerLevel) {
        cat.as_any_mut()
            .downcast_mut::<Cat>()
            .expect("Cat")
            .set_lying(false);
    }
    fn requires_update_every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, cat, level);
        tamable::set_in_sitting_pose(cat, false);
        let reached = self.is_reached_target();
        let c = cat.as_any_mut().downcast_mut::<Cat>().expect("Cat");
        if !reached {
            c.set_lying(false);
        } else if !c.is_lying() {
            c.set_lying(true);
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}

/// `CatSitOnBlockGoal`.
pub struct CatSitOnBlockGoal {
    mtb: MoveToBlockData,
}

impl CatSitOnBlockGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            mtb: MoveToBlockData::new(speed_modifier, 8),
        }
    }
}

impl MoveToBlock for CatSitOnBlockGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.mtb
    }
    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.mtb
    }
    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        if !level.is_empty_block(pos.above()) {
            return false;
        }
        let state: BlockState = level.get_block_state(pos);
        if !state.is_tag("cats_can_sit_on") {
            return false;
        }
        // BLOCK_TYPE_VALIDATION
        if state.block() == Block::CHEST {
            return ext::chest_open_count(level, pos) < 1;
        }
        if state.block() == Block::FURNACE {
            return state.get(props::LIT);
        }
        if state.is_tag("beds") {
            return state
                .try_get(props::BED_PART)
                .is_none_or(|part| part != BedPart::Head);
        }
        true
    }
}

impl Goal for CatSitOnBlockGoal {
    fn can_use(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        tamable::is_tame(cat)
            && !tamable::is_ordered_to_sit(cat)
            && move_to_block::can_use(self, cat, level)
    }
    fn can_continue_to_use(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_continue_to_use(self, cat, level)
    }
    fn start(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::start(self, cat, level);
        tamable::set_in_sitting_pose(cat, false);
    }
    fn stop(&mut self, cat: &mut dyn Entity, _level: &mut ServerLevel) {
        tamable::set_in_sitting_pose(cat, false);
    }
    fn requires_update_every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, cat: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, cat, level);
        let reached = self.is_reached_target();
        tamable::set_in_sitting_pose(cat, reached);
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}

/// `Cat`.
pub struct Cat {
    pub mob: MobData,
    lie_down_amount: f32,
    lie_down_amount_o: f32,
    lie_down_amount_tail: f32,
    lie_down_amount_o_tail: f32,
    is_lying_on_top_of_sleeping_player: bool,
    relax_state_one_amount: f32,
    relax_state_one_amount_o: f32,
    variant_key: Identifier,
    sound_variant_key: Identifier,
    sound_variant: CatSoundVariant,
    pending_variant: Option<Identifier>,
    pending_sound_variant: Option<Identifier>,
}

impl Cat {
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(
            registries,
            keys::CAT_VARIANT,
            &Identifier::parse("minecraft:black").expect("valid id"),
        );
        let sound_registry = registries.registry_or_throw(keys::CAT_SOUND_VARIANT);
        let default_sound = sound_registry
            .id_of(&Identifier::parse("minecraft:classic").expect("valid id"))
            .unwrap_or(0);
        let mob = MobData::new(ty, id, MobKind::TamableAnimal, |b| {
            b.define(DATA_VARIANT_ID, default_variant as i32);
            b.define(DATA_SOUND_VARIANT_ID, default_sound as i32);
            b.define(IS_LYING, false);
            b.define(RELAX_STATE_ONE, false);
            b.define(DATA_COLLAR_COLOR, DEFAULT_COLLAR_COLOR.id());
        });
        let variant_key = registries
            .registry_or_throw(keys::CAT_VARIANT)
            .name_of(default_variant)
            .expect("variant entry");
        let sound_variant_key = sound_registry
            .name_of(default_sound)
            .expect("sound variant entry");
        let sound_variant = *registries
            .get::<CatSoundVariant>(keys::CAT_SOUND_VARIANT, &sound_variant_key)
            .expect("typed cat sound variant");
        let mut cat = Cat {
            mob,
            lie_down_amount: 0.0,
            lie_down_amount_o: 0.0,
            lie_down_amount_tail: 0.0,
            lie_down_amount_o_tail: 0.0,
            is_lying_on_top_of_sleeping_player: false,
            relax_state_one_amount: 0.0,
            relax_state_one_amount_o: 0.0,
            variant_key,
            sound_variant_key,
            sound_variant,
            pending_variant: None,
            pending_sound_variant: None,
        };
        let navigation = StandardPathNavigation::ground(&cat);
        cat.mob.set_navigation(Box::new(navigation));
        cat.register_goals();
        cat.reassess_tame_goals(None);
        cat
    }

    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, 0.8);
        let look_at = LookAtPlayerGoal::new_player(self, 10.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(1, Box::new(TamableAnimalPanicGoal::new(1.5)));
        goals.add_goal(2, Box::new(SitWhenOrderedToGoal::new()));
        goals.add_goal(3, Box::new(CatRelaxOnOwnerGoal::new()));
        goals.add_goal(4, Box::new(CatTemptGoal::new()));
        goals.add_goal(5, Box::new(CatLieOnBlockGoal::new(1.1, 8)));
        goals.add_goal(6, Box::new(FollowOwnerGoal::new(1.0, 10.0, 5.0)));
        goals.add_goal(7, Box::new(CatSitOnBlockGoal::new(0.8)));
        goals.add_goal(8, Box::new(LeapAtTargetGoal::new(0.3)));
        goals.add_goal(9, Box::new(OcelotAttackGoal::new()));
        goals.add_goal(10, Box::new(breed));
        goals.add_goal(
            11,
            Box::new(WaterAvoidingRandomStrollGoal::with_probability(
                0.8,
                1.0000001E-5,
            )),
        );
        goals.add_goal(12, Box::new(look_at));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(
            1,
            Box::new(NonTameRandomTargetGoal::new(
                TargetType::of_type(EntityType::RABBIT),
                false,
                None,
            )),
        );
        targets.add_goal(
            1,
            Box::new(NonTameRandomTargetGoal::new(
                TargetType::of_type(EntityType::TURTLE),
                false,
                Some(Arc::new(turtle::baby_on_land_selector)),
            )),
        );
    }

    /// `reassessTameGoals()`: the avoid-players goal exists exactly while the cat is not tame.
    fn reassess_tame_goals(&mut self, level: Option<&mut ServerLevel>) {
        let mut selector = std::mem::take(&mut self.mob.goal_selector);
        let is_avoid = |g: &dyn Goal| (g as &dyn Any).is::<CatAvoidEntityGoal>();
        match level {
            Some(level) => selector.remove_all_goals(self, level, is_avoid),
            None => selector.remove_all_goals_not_running(is_avoid),
        }
        if !tamable::is_tame(self) {
            selector.add_goal(4, Box::new(CatAvoidEntityGoal::new()));
        }
        self.mob.goal_selector = selector;
    }

    fn get_sound_set(&self) -> &CatSoundSet {
        if self.is_baby() {
            &self.sound_variant.baby_sounds
        } else {
            &self.sound_variant.adult_sounds
        }
    }

    pub fn set_variant(&mut self, registries: &RegistryAccess, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VARIANT_ID, id as i32);
        if let Some(name) = registries
            .registry(keys::CAT_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            self.variant_key = name;
        }
    }

    pub fn get_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_VARIANT_ID)
    }

    pub fn set_sound_variant(&mut self, registries: &RegistryAccess, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_SOUND_VARIANT_ID, id as i32);
        if let Some(name) = registries
            .registry(keys::CAT_SOUND_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            if let Some(v) = registries.get::<CatSoundVariant>(keys::CAT_SOUND_VARIANT, &name) {
                self.sound_variant = *v;
            }
            self.sound_variant_key = name;
        }
    }

    pub fn set_lying(&mut self, value: bool) {
        self.mob.living.entity.entity_data.set(IS_LYING, value);
    }

    pub fn is_lying(&self) -> bool {
        self.mob.living.entity.entity_data.get(IS_LYING)
    }

    pub fn set_relax_state_one(&mut self, value: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(RELAX_STATE_ONE, value);
    }

    pub fn is_relax_state_one(&self) -> bool {
        self.mob.living.entity.entity_data.get(RELAX_STATE_ONE)
    }

    pub fn get_collar_color(&self) -> DyeColor {
        dye_color::by_id(self.mob.living.entity.entity_data.get(DATA_COLLAR_COLOR))
    }

    pub fn set_collar_color(&mut self, color: DyeColor) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_COLLAR_COLOR, color.id());
    }

    pub fn is_lying_on_top_of_sleeping_player(&self) -> bool {
        self.is_lying_on_top_of_sleeping_player
    }

    /// `hiss()`.
    pub fn hiss(&mut self, level: &mut ServerLevel) {
        let sound = self.get_sound_set().hiss_sound;
        mc_game_core::world::entity::living_entity::damage::make_sound(self, level, sound);
    }

    /// `handleLieDown()`.
    fn handle_lie_down(&mut self, level: &mut ServerLevel) {
        if (self.is_lying() || self.is_relax_state_one()) && self.base().tick_count % 5 == 0 {
            let volume = 0.6
                + 0.4 * (self.base_mut().random.next_float() - self.base_mut().random.next_float());
            let sound = self.get_sound_set().purr_sound;
            self.play_sound(level, sound, volume, 1.0);
        }
        self.update_lie_down_amount();
        self.update_relax_state_one_amount();
        self.is_lying_on_top_of_sleeping_player = false;
        if self.is_lying() {
            let cat_pos = self.block_position();
            let bb = AABB::from_block_pos(&cat_pos).inflate(2.0, 2.0, 2.0);
            let players = level.get_entities_of_class(&bb, |e| e.is_player(), |_| true);
            for id in players {
                if level
                    .get_entity(id)
                    .is_some_and(|p| p.living().is_some_and(|l| l.is_sleeping()))
                {
                    self.is_lying_on_top_of_sleeping_player = true;
                    break;
                }
            }
        }
    }

    fn update_lie_down_amount(&mut self) {
        self.lie_down_amount_o = self.lie_down_amount;
        self.lie_down_amount_o_tail = self.lie_down_amount_tail;
        if self.is_lying() {
            self.lie_down_amount = (self.lie_down_amount + 0.15).min(1.0);
            self.lie_down_amount_tail = (self.lie_down_amount_tail + 0.08).min(1.0);
        } else {
            self.lie_down_amount = (self.lie_down_amount - 0.22).max(0.0);
            self.lie_down_amount_tail = (self.lie_down_amount_tail - 0.13).max(0.0);
        }
    }

    fn update_relax_state_one_amount(&mut self) {
        self.relax_state_one_amount_o = self.relax_state_one_amount;
        if self.is_relax_state_one() {
            self.relax_state_one_amount = (self.relax_state_one_amount + 0.1).min(1.0);
        } else {
            self.relax_state_one_amount = (self.relax_state_one_amount - 0.13).max(0.0);
        }
    }

    /// `tryToTame(player)`.
    fn try_to_tame(&mut self, level: &mut ServerLevel, player: &ServerPlayer) {
        if self.base_mut().random.next_int_bounded(3) == 0 {
            tamable::tame(self, level, player);
            tamable::set_ordered_to_sit(self, true);
            broadcast_entity_event(level, self.id(), 7);
        } else {
            broadcast_entity_event(level, self.id(), 6);
        }
    }

    fn is_owned_by_player(&self, player: &ServerPlayer) -> bool {
        tamable::get_owner_reference(self) == Some(player.uuid())
    }
}

impl Entity for Cat {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:cat")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        variant::write_variant(out, &self.variant_key);
        out.put_string("sound_variant", self.sound_variant_key.to_string());
        out.put_byte("CollarColor", self.get_collar_color().id() as i8);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.pending_variant = variant::read_variant_key(input);
        self.pending_sound_variant = input
            .get_string("sound_variant")
            .and_then(Identifier::try_parse);
        let collar = if input.contains("CollarColor") {
            dye_color::by_id(i32::from(input.get_byte_or("CollarColor", 0)))
        } else {
            DEFAULT_COLLAR_COLOR
        };
        self.set_collar_color(collar);
    }

    fn resolve_registry_refs(&mut self, registries: &RegistryAccess) {
        if let Some(key) = self.pending_variant.take()
            && let Some(id) = registries
                .registry(keys::CAT_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_variant(registries, id);
        }
        if let Some(key) = self.pending_sound_variant.take()
            && let Some(id) = registries
                .registry(keys::CAT_SOUND_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_sound_variant(registries, id);
        }
    }

    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        super::custom_server_ai_step(self);
    }

    fn get_ambient_sound_in(&mut self, _level: &mut ServerLevel) -> Option<SoundEvent> {
        if tamable::is_tame(self) {
            if mob::animal::is_in_love(self) {
                Some(self.get_sound_set().purr_sound)
            } else if self.base_mut().random.next_int_bounded(4) == 0 {
                Some(self.get_sound_set().purreow_sound)
            } else {
                Some(self.get_sound_set().ambient_sound)
            }
        } else {
            Some(self.get_sound_set().stray_ambient_sound)
        }
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        120
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(self.get_sound_set().hurt_sound)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(self.get_sound_set().death_sound)
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        let sound = self.get_sound_set().eat_sound;
        self.play_sound(level, sound, 1.0, 1.0);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        let tempting = self
            .mob
            .goal_selector
            .find_goal::<CatTemptGoal>()
            .is_some_and(CatTemptGoal::is_running);
        if tempting && !tamable::is_tame(self) && self.base().tick_count % 100 == 0 {
            let sound = self.get_sound_set().beg_for_food_sound;
            self.play_sound(level, sound, 1.0, 1.0);
        }
        self.handle_lie_down(level);
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(
            EntityType::CAT,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let partner_info = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Cat>())
            .map(|p| (p.get_variant(), p.get_collar_color()));
        if let (Some((partner_variant, partner_collar)), Some(cat)) =
            (partner_info, baby.as_any_mut().downcast_mut::<Cat>())
        {
            let registries = level.registries.clone();
            let own_variant = self.get_variant();
            let pick = if self.base_mut().random.next_boolean() {
                own_variant
            } else {
                partner_variant
            };
            cat.set_variant(&registries, pick as u32);
            if tamable::is_tame(self) {
                tamable::set_owner_reference(cat, tamable::get_owner_reference(self));
                cat.set_tame(None, true, true);
                let mixed =
                    dye_color::get_mixed_color(level, self.get_collar_color(), partner_collar);
                cat.set_collar_color(mixed);
            }
        }
        Some(baby)
    }

    fn can_mate(&self, partner: &dyn Entity) -> bool {
        if !tamable::is_tame(self) {
            return false;
        }
        match partner.as_any().downcast_ref::<Cat>() {
            None => false,
            Some(cat) => tamable::is_tame(cat) && mob::animal::can_mate(self, partner),
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let group_data = mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data);
        let registries = level.registry_access().clone();
        let pos = self.block_position();
        let mut selected: Option<Identifier> = None;
        level.with_random_taken(&mut |level, random| {
            let context = SpawnContext::create(level, pos);
            selected =
                variant::select_variant_to_spawn::<CatVariant>(&context, keys::CAT_VARIANT, random);
            true
        });
        if let Some(key) = selected
            && let Some(id) = registries
                .registry(keys::CAT_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_variant(&registries, id);
        }
        if let Some(key) =
            variant::pick_random_entry(&registries, keys::CAT_SOUND_VARIANT, level.random())
            && let Some(id) = registries
                .registry(keys::CAT_SOUND_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_sound_variant(&registries, id);
        }
        group_data
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        let infinite = player.player.abilities.instabuild;
        if tamable::is_tame(self) {
            if self.is_owned_by_player(player) {
                let is_collar_dye = mc_data::tags::get::<Item>("cat_collar_dyes")
                    .is_some_and(|t| t.contains(item_stack.item()));
                if is_collar_dye {
                    if let Some(color) = item_stack.get(&DYE).copied()
                        && color != self.get_collar_color()
                    {
                        self.set_collar_color(color);
                        interact::with_hand_stack(player, hand, |stack, _| {
                            stack.consume(1, infinite)
                        });
                        self.set_persistence_required();
                        return InteractionResult::SUCCESS;
                    }
                } else if self.is_food(&item_stack)
                    && self.get_health() < self.mob.living.get_max_health()
                {
                    tamable::feed(self, level, player, hand, &item_stack, 1.0, 1.0);
                    return InteractionResult::SUCCESS;
                }
                let parent_interaction = interact::mob_interact(self, level, player, hand);
                if !parent_interaction.consumes_action() {
                    let ordered = tamable::is_ordered_to_sit(self);
                    tamable::set_ordered_to_sit(self, !ordered);
                    return InteractionResult::SUCCESS;
                }
                return parent_interaction;
            }
        } else if self.is_food(&item_stack) {
            interact::use_player_item(level, player, hand);
            self.try_to_tame(level, player);
            self.set_persistence_required();
            self.play_eating_sound(level);
            return InteractionResult::SUCCESS;
        }
        let interact = interact::mob_interact(self, level, player, hand);
        if interact.consumes_action() {
            self.set_persistence_required();
        }
        interact
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_cat_food(stack)
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !tamable::is_tame(self) && self.base().tick_count > 2400
    }

    fn set_tame(
        &mut self,
        level: Option<&mut ServerLevel>,
        is_tame: bool,
        include_side_effects: bool,
    ) {
        tamable::set_tame(self, is_tame, include_side_effects);
        self.reassess_tame_goals(level);
    }

    fn is_stepping_carefully(&self) -> bool {
        self.is_crouching() || self.is_shift_key_down()
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static CAT: EntityRegistration = EntityRegistration {
    entity_type: EntityType::CAT,
    create: |ty, id, registries| Box::new(Cat::new(ty, id, registries)),
};
