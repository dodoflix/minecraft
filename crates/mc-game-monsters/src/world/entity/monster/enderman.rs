//! `net.minecraft.world.entity.monster.Enderman` (+ its four goals).

use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::{Attribute, Block, EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::ResetUniversalAngerTargetGoal;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type,
};
use mc_game_core::world::entity::ai::goal::{
    Flag, FloatGoal, Goal, GoalFlags, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal,
    WaterAvoidingRandomStrollGoal, reduced_tick_delay,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::teleport::clamped_pack_difference_in_position;
use mc_game_core::world::entity::living_entity::util::lm;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, NeutralData, neutral};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::block::helpers::update_from_neighbour_shapes;
use mc_game_core::world::level::block::state::{decode_block_state, encode_block_state};
use mc_game_core::world::level::collision::{
    BlockGetterExt as _, ClipBlock, ClipContext, ClipFluid,
};
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::storage::loot::LootParams;
use mc_game_core::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_nbt::CompoundTag;
use std::sync::{Arc, LazyLock};

define_accessor!(pub DATA_CARRY_STATE: OptionalBlockState = Enderman);
define_accessor!(pub DATA_CREEPY: Boolean = Enderman);
define_accessor!(pub DATA_STARED_AT: Boolean = Enderman);

static SPEED_MODIFIER_ATTACKING_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::with_default_namespace("attacking").expect("valid id"));
const MIN_DEAGGRESSION_TIME: i32 = 600;
/// `TimeUtil.rangeOfSeconds(20, 39)` as (min, max) ticks.
const PERSISTENT_ANGER_TIME: (i32, i32) = (20 * 20, 39 * 20);

fn attacking_modifier() -> AttributeModifier {
    AttributeModifier {
        id: SPEED_MODIFIER_ATTACKING_ID.clone(),
        amount: f64::from(0.15f32),
        operation: AttributeModifierOperation::AddValue,
    }
}

/// `Enderman`.
pub struct Enderman {
    pub mob: MobData,
    target_change_time: i32,
}

impl Enderman {
    /// `new Enderman(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_CARRY_STATE, None);
            b.define(DATA_CREEPY, false);
            b.define(DATA_STARED_AT, false);
        });
        let mut enderman = Enderman {
            mob,
            target_change_time: 0,
        };
        enderman.mob.neutral = Some(NeutralData::default());
        let navigation = StandardPathNavigation::ground(&enderman);
        enderman.mob.set_navigation(Box::new(navigation));
        enderman.register_goals();
        enderman.mob.set_pathfinding_malus(PathType::Water, -1.0);
        enderman
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(EndermanFreezeWhenLookedAt::new()));
        goals.add_goal(2, Box::new(MeleeAttackGoal::new(1.0, false)));
        goals.add_goal(
            7,
            Box::new(WaterAvoidingRandomStrollGoal::with_probability(1.0, 0.0)),
        );
        goals.add_goal(8, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        goals.add_goal(10, Box::new(EndermanLeaveBlockGoal));
        goals.add_goal(11, Box::new(EndermanTakeBlockGoal));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(EndermanLookForPlayerGoal::new()));
        targets.add_goal(2, Box::new(HurtByTargetGoal::new(Vec::new())));
        targets.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::with_must_reach(
                TargetType::Class(class_of_type(EntityType::ENDERMITE)),
                true,
                false,
            )),
        );
        targets.add_goal(4, Box::new(ResetUniversalAngerTargetGoal::new(false)));
    }

    /// `setCarriedBlock(carryingBlock)`.
    pub fn set_carried_block(&mut self, carrying_block: Option<BlockState>) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_CARRY_STATE, carrying_block);
    }

    /// The `target == null` half of `setTarget`. The attribute lookup creates the `movement_speed` instance when absent, which
    /// the saved attribute list then contains.
    fn clear_target_state(&mut self) {
        self.target_change_time = 0;
        let data = &mut self.mob.living.entity.entity_data;
        data.set(DATA_CREEPY, false);
        data.set(DATA_STARED_AT, false);
        if let Some(speed) = lm(self).attributes.get_instance(Attribute::MOVEMENT_SPEED) {
            speed.remove_modifier(&SPEED_MODIFIER_ATTACKING_ID);
        }
    }

    /// `getCarriedBlock()`.
    pub fn get_carried_block(&self) -> Option<BlockState> {
        self.mob.living.entity.entity_data.get(DATA_CARRY_STATE)
    }

    /// `isCreepy()`.
    pub fn is_creepy(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_CREEPY)
    }

    /// `hasBeenStaredAt()`.
    pub fn has_been_stared_at(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_STARED_AT)
    }

    /// `setBeingStaredAt()`.
    pub fn set_being_stared_at(&mut self) {
        self.mob.living.entity.entity_data.set(DATA_STARED_AT, true);
    }

    /// `teleport()`.
    pub fn teleport_randomly(&mut self, level: &mut ServerLevel) -> bool {
        if self.is_alive() {
            let (x, y, z) = (self.base().x(), self.base().y(), self.base().z());
            let random = &mut self.base_mut().random;
            let xx = x + (random.next_double() - 0.5) * 64.0;
            let yy = y + f64::from(random.next_int_bounded(64) - 32);
            let zz = z + (random.next_double() - 0.5) * 64.0;
            self.teleport_to_position(level, xx, yy, zz)
        } else {
            false
        }
    }

    /// `teleportTowards(entity)`.
    fn teleport_towards(&mut self, level: &mut ServerLevel, target: EntityId) -> bool {
        let Some((tx, teye, tz)) = level
            .get_entity(target)
            .map(|t| (t.base().x(), t.base().get_eye_y(), t.base().z()))
        else {
            return false;
        };
        let (x, y, z, y_half) = (
            self.base().x(),
            self.base().y(),
            self.base().z(),
            self.base().get_y_progress(0.5),
        );
        let dir = Vec3::new(x - tx, y_half - teye, z - tz).normalize();
        let random = &mut self.base_mut().random;
        let xx = x + (random.next_double() - 0.5) * 8.0 - dir.x * 16.0;
        let yy = y + f64::from(random.next_int_bounded(16) - 8) - dir.y * 16.0;
        let zz = z + (random.next_double() - 0.5) * 8.0 - dir.z * 16.0;
        self.teleport_to_position(level, xx, yy, zz)
    }

    /// `teleport(x, y, z)`.
    pub fn teleport_to_position(
        &mut self,
        level: &mut ServerLevel,
        x: f64,
        y: f64,
        z: f64,
    ) -> bool {
        if self.base().is_passenger() {
            return false;
        }
        let old_pos = self.position();
        let result = self.random_teleport(level, x, y, z, true, "enderman_does_not_teleport_to");
        if result {
            level.game_event_from_entity_ref(&*self, GameEvent::TELEPORT, old_pos);
            if !self.base().is_silent() {
                let old_block_pos = BlockPos::containing(old_pos.x, old_pos.y, old_pos.z);
                let packed_diff = clamped_pack_difference_in_position(
                    old_block_pos,
                    self.base().block_position(),
                    127,
                    127,
                    127,
                );
                level.level_event(None, 2018, old_block_pos, packed_diff);
                let (xo, yo, zo) = (self.base().xo, self.base().yo, self.base().zo);
                let source = self.get_sound_source();
                level.play_sound_at(
                    None,
                    xo,
                    yo,
                    zo,
                    SoundEvent::ENTITY_ENDERMAN_TELEPORT,
                    source,
                    1.0,
                    1.0,
                );
                self.play_sound(level, SoundEvent::ENTITY_ENDERMAN_TELEPORT, 1.0, 1.0);
            }
        }
        result
    }

    /// `repeatedlyTryToTeleport()`.
    fn repeatedly_try_to_teleport(&mut self, level: &mut ServerLevel) {
        if !self.base().is_passenger() {
            for _ in 0..64 {
                if self.teleport_randomly(level) {
                    return;
                }
            }
        }
    }
}

/// `LivingEntity.PLAYER_NOT_WEARING_DISGUISE_ITEM.test(player)`.
fn player_not_wearing_disguise_item(player: &dyn Entity) -> bool {
    if !player.is_player() {
        return true;
    }
    let helmet =
        player.get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::Head);
    !mc_data::tags::get::<Item>("gaze_disguise_equipment")
        .is_some_and(|t| t.contains(helmet.item()))
}

/// `Enderman.isBeingStaredBy(player)`.
fn is_being_stared_by(level: &ServerLevel, enderman: &dyn Entity, player: &dyn Entity) -> bool {
    player_not_wearing_disguise_item(player)
        && mob::ext::is_looking_at_me(
            level,
            enderman,
            player,
            0.025,
            true,
            false,
            &[enderman.base().get_eye_y()],
        )
}

fn enderman_mut(mob: &mut dyn Entity) -> &mut Enderman {
    mob.as_any_mut()
        .downcast_mut::<Enderman>()
        .expect("Enderman")
}

impl Entity for Enderman {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_walk_target_value(&self, _pos: BlockPos, _level: &dyn LevelReader) -> f32 {
        0.0
    }

    fn set_target(&mut self, level: &ServerLevel, target: Option<EntityId>) {
        mob::tick::set_target(self, level, target);
        let tick_count = self.base().tick_count;
        if target.is_none() {
            self.clear_target_state();
        } else {
            self.target_change_time = tick_count;
            self.mob.living.entity.entity_data.set(DATA_CREEPY, true);
            if let Some(speed) = lm(self).attributes.get_instance(Attribute::MOVEMENT_SPEED)
                && !speed.has_modifier(&SPEED_MODIFIER_ATTACKING_ID)
            {
                speed.add_transient_modifier(attacking_modifier());
            }
        }
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        let (min, max) = PERSISTENT_ANGER_TIME;
        let time = self.base_mut().random.next_int_bounded(max - min + 1) + min;
        neutral::set_time_to_remain_angry(self, level, i64::from(time));
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        if let Some(state) = self.get_carried_block() {
            out.put("carriedBlockState", encode_block_state(state));
        }
        neutral::add_persistent_anger_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        let carried = input
            .get("carriedBlockState")
            .and_then(decode_block_state)
            .filter(|s| !s.is_air());
        self.set_carried_block(carried);
        neutral::read_persistent_anger_save_data(self, level, input);
        if level.is_none() {
            // no level to resolve `angry_at` while generating: vanilla's `setTarget(null)` runs
            self.clear_target_state();
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        self.mob.living.jumping = false;
        neutral::update_persistent_anger(self, level, true);
        mob::tick::ai_step(self, level);
    }

    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if level.is_bright_outside()
            && self.base().tick_count >= self.target_change_time + MIN_DEAGGRESSION_TIME
        {
            let br = self.get_light_level_dependent_magic_value(level);
            if br > 0.5
                && level.can_see_sky(self.base().block_position())
                && self.base_mut().random.next_float() * 30.0 < (br - 0.4) * 2.0
            {
                self.set_target(level, None);
                self.teleport_randomly(level);
            }
        }
    }

    fn can_randomly_teleport_to(&self, level: &ServerLevel, x: f64, y: f64, z: f64) -> bool {
        use mc_game_core::world::level::material::FluidStateExt as _;
        let pos_below = BlockPos::containing(x, y, z).below();
        let state_below = level.get_block_state(pos_below);
        !state_below.get_fluid_state().is_tag("water")
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_creepy() {
            SoundEvent::ENTITY_ENDERMAN_SCREAM
        } else {
            SoundEvent::ENTITY_ENDERMAN_AMBIENT
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDERMAN_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDERMAN_DEATH)
    }

    fn drop_custom_death_loot(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        killed_by_player: bool,
    ) {
        mob::equipment::drop_custom_death_loot(self, level, source, killed_by_player);
        if let Some(carrying_block) = self.get_carried_block() {
            let mut fake_tool = ItemStack::new(Item::DIAMOND_AXE);
            if let Some(provider) = Identifier::try_parse("enderman_loot_drop") {
                let difficulty = mc_game_core::world::level::server_level_accessor::ServerLevelAccessor::get_current_difficulty_at(&*level, self.base().block_position());
                let registries = level.registries.clone();
                mc_game_core::world::item::enchantment_helper::enchant_item_from_provider(
                    &mut fake_tool,
                    &registries,
                    &provider,
                    &difficulty,
                    &mut self.base_mut().random,
                );
            }
            let drops = {
                let this: &dyn Entity = &*self;
                let builder = LootParams::builder(level)
                    .origin(this.position())
                    .tool(&fake_tool)
                    .optional_this_entity(Some(this));
                mc_game_core::world::level::storage::loot::block_drops::state_get_drops(
                    carrying_block,
                    builder,
                )
            };
            for stack in drops {
                mc_game_core::world::entity::item::spawn_at_location(
                    level,
                    self,
                    stack,
                    Vec3::ZERO,
                );
            }
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        let thrown_potion = source
            .direct_info()
            .and_then(|d| d.thrown_potion_hurts_water_sensitive);
        if source.is_tag(level, "is_projectile") && !self.base().is_passenger() {
            self.repeatedly_try_to_teleport(level);
            false
        } else if let Some(hurts) = thrown_potion {
            let hurt_with_clean_water = if hurts {
                mc_game_core::world::entity::living_entity::damage::hurt_server(
                    self, level, source, damage,
                )
            } else {
                false
            };
            self.repeatedly_try_to_teleport(level);
            hurt_with_clean_water
        } else {
            let result = mc_game_core::world::entity::living_entity::damage::hurt_server(
                self, level, source, damage,
            );
            if !source.causing_is_living() && self.base_mut().random.next_int_bounded(10) != 0 {
                self.teleport_randomly(level);
            }
            result
        }
    }

    fn projectile_receives_side_effects_on_hit(&self, was_hurt: bool) -> bool {
        was_hurt
    }

    fn requires_custom_persistence(&self) -> bool {
        (self.mob_is_passenger_or_leashed()) || self.get_carried_block().is_some()
    }
}

impl Enderman {
    /// `super.requiresCustomPersistence()` (`Mob`: passenger or leashed).
    fn mob_is_passenger_or_leashed(&self) -> bool {
        self.base().is_passenger() || mob::leashable::is_leashed(self)
    }
}

/// `Enderman.EndermanFreezeWhenLookedAt`.
struct EndermanFreezeWhenLookedAt {
    target: Option<EntityId>,
}

impl EndermanFreezeWhenLookedAt {
    fn new() -> Self {
        Self { target: None }
    }
}

impl Goal for EndermanFreezeWhenLookedAt {
    fn name(&self) -> &'static str {
        "Enderman$EndermanFreezeWhenLookedAt"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target = mob.get_target();
        let Some(target) = self.target.and_then(|id| level.get_entity(id)) else {
            return false;
        };
        if !target.is_player() {
            return false;
        }
        let dist = target.base().distance_to_sqr_vec(mob.position());
        if dist > 256.0 {
            false
        } else {
            is_being_stared_by(level, mob, target)
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob::navigation::stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(target) = self.target.and_then(|id| level.get_entity(id)) {
            let (x, y, z) = (
                target.base().x(),
                target.base().get_eye_y(),
                target.base().z(),
            );
            mob::look_at_xyz(mob, x, y, z);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}

/// `Enderman.EndermanLeaveBlockGoal`.
struct EndermanLeaveBlockGoal;

impl EndermanLeaveBlockGoal {
    fn can_place_block(
        level: &ServerLevel,
        mob: &dyn Entity,
        pos: BlockPos,
        carried: BlockState,
        target_state: BlockState,
        below_state: BlockState,
        below: BlockPos,
    ) -> bool {
        target_state.is_air()
            && !below_state.is_air()
            && below_state.block() != Block::BEDROCK
            && below_state.is_collision_shape_full_block_at(level, below)
            && carried.can_survive(level, pos)
            && level
                .get_entities(
                    Some(mob.id()),
                    &AABB::unit_cube_from_lower_corner(Vec3::new(
                        f64::from(pos.x()),
                        f64::from(pos.y()),
                        f64::from(pos.z()),
                    )),
                    |e| !e.is_spectator(),
                )
                .is_empty()
    }
}

impl Goal for EndermanLeaveBlockGoal {
    fn name(&self) -> &'static str {
        "Enderman$EndermanLeaveBlockGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if enderman_mut(mob).get_carried_block().is_none() || !level.mob_griefing() {
            false
        } else {
            mob.base_mut()
                .random
                .next_int_bounded(reduced_tick_delay(2000))
                == 0
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let (x, y, z) = (mob.base().x(), mob.base().y(), mob.base().z());
        let random = &mut mob.base_mut().random;
        let xt = mth::floor_f64(x - 1.0 + random.next_double() * 2.0);
        let yt = mth::floor_f64(y + random.next_double() * 2.0);
        let zt = mth::floor_f64(z - 1.0 + random.next_double() * 2.0);
        let pos = BlockPos::new(xt, yt, zt);
        let target_state = level.get_block_state(pos);
        let below = pos.below();
        let below_state = level.get_block_state(below);
        let Some(carried) = enderman_mut(mob).get_carried_block() else {
            return;
        };
        let carried = update_from_neighbour_shapes(carried, level, pos);
        if Self::can_place_block(level, mob, pos, carried, target_state, below_state, below) {
            level.set_block_and_update(pos, carried);
            level.game_event_from_entity_ref_in_state(
                &*mob,
                Some(carried),
                GameEvent::BLOCK_PLACE,
                Vec3::at_center_of(&pos),
            );
            enderman_mut(mob).set_carried_block(None);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Enderman.EndermanTakeBlockGoal`.
struct EndermanTakeBlockGoal;

impl Goal for EndermanTakeBlockGoal {
    fn name(&self) -> &'static str {
        "Enderman$EndermanTakeBlockGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if enderman_mut(mob).get_carried_block().is_some() || !level.mob_griefing() {
            false
        } else {
            mob.base_mut()
                .random
                .next_int_bounded(reduced_tick_delay(20))
                == 0
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let (x, y, z) = (mob.base().x(), mob.base().y(), mob.base().z());
        let random = &mut mob.base_mut().random;
        let xt = mth::floor_f64(x - 2.0 + random.next_double() * 4.0);
        let yt = mth::floor_f64(y + random.next_double() * 3.0);
        let zt = mth::floor_f64(z - 2.0 + random.next_double() * 4.0);
        let pos = BlockPos::new(xt, yt, zt);
        let block_state = level.get_block_state(pos);
        let from = Vec3::new(
            f64::from(mob.block_position().x()) + 0.5,
            f64::from(yt) + 0.5,
            f64::from(mob.block_position().z()) + 0.5,
        );
        let to = Vec3::new(
            f64::from(xt) + 0.5,
            f64::from(yt) + 0.5,
            f64::from(zt) + 0.5,
        );
        let result = level.clip(&ClipContext::new(
            from,
            to,
            ClipBlock::Outline,
            ClipFluid::None,
            mc_game_core::world::entity::movement::collision_context(mob),
        ));
        let reachable = result.block_pos == pos;
        if block_state.block().is_in_tag("enderman_holdable") && reachable {
            level.remove_block(pos, false);
            level.game_event_from_entity_ref_in_state(
                &*mob,
                Some(block_state),
                GameEvent::BLOCK_DESTROY,
                Vec3::at_center_of(&pos),
            );
            enderman_mut(mob).set_carried_block(Some(block_state.block().default_state()));
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Enderman.EndermanLookForPlayerGoal` (a `NearestAttackableTargetGoal<Player>`).
struct EndermanLookForPlayerGoal {
    base: NearestAttackableTargetGoal,
    pending_target: Option<EntityId>,
    aggro_time: i32,
    teleport_time: i32,
    continue_aggro_target_conditions: TargetingConditions,
}

impl EndermanLookForPlayerGoal {
    fn new() -> Self {
        let base = NearestAttackableTargetGoal::build(TargetType::Player, 10, false, false, None)
            .with_source_selector(Arc::new(
                |mob: &dyn Entity, target: &dyn Entity, level: &ServerLevel| {
                    neutral::is_angry_at(mob, level, target)
                },
            ));
        Self {
            base,
            pending_target: None,
            aggro_time: 0,
            teleport_time: 0,
            continue_aggro_target_conditions: TargetingConditions::for_combat()
                .ignore_line_of_sight(),
        }
    }

    /// `isAngerInducing.test(target, level)`.
    fn is_anger_inducing(mob: &dyn Entity, target: &dyn Entity, level: &ServerLevel) -> bool {
        (is_being_stared_by(level, mob, target) || neutral::is_angry_at(mob, level, target))
            && !mob.has_indirect_passenger(level, target)
    }
}

impl Goal for EndermanLookForPlayerGoal {
    fn name(&self) -> &'static str {
        "Enderman$EndermanLookForPlayerGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let follow = self.base.target_goal.get_follow_distance(mob);
        let conditions = TargetingConditions::for_combat()
            .range(follow)
            .source_selector(Some(Arc::new(
                |mob: &dyn Entity, target: &dyn Entity, level: &ServerLevel| {
                    Self::is_anger_inducing(mob, target, level)
                },
            )));
        let b = mob.base();
        self.pending_target =
            mob::ext::get_nearest_player(level, &conditions, mob, b.x(), b.y(), b.z());
        self.pending_target.is_some()
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.aggro_time = self.adjusted_tick_delay(5);
        self.teleport_time = 0;
        enderman_mut(mob).set_being_stared_at();
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.pending_target = None;
        Goal::stop(&mut self.base, mob, level);
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if let Some(pending) = self.pending_target {
            let Some(target) = level.get_entity(pending) else {
                return false;
            };
            if !Self::is_anger_inducing(mob, target, level) {
                false
            } else {
                mob::ext::mob_look_at(mob, target, 10.0, 10.0);
                true
            }
        } else {
            if let Some(target) = self.base.target.and_then(|id| level.get_entity(id)) {
                if mob.has_indirect_passenger(level, target) {
                    return false;
                }
                if self
                    .continue_aggro_target_conditions
                    .test(level, Some(&*mob), target)
                {
                    return true;
                }
            }
            self.base.can_continue_to_use(mob, level)
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if mob.get_target().is_none() {
            self.base.set_target(None);
        }
        if let Some(pending) = self.pending_target {
            self.aggro_time -= 1;
            if self.aggro_time <= 0 {
                self.base.target = Some(pending);
                self.pending_target = None;
                Goal::start(&mut self.base, mob, level);
            }
        } else if let Some(target_id) = self.base.target
            && !mob.base().is_passenger()
        {
            let Some(target) = level.get_entity(target_id) else {
                return;
            };
            let stared = is_being_stared_by(level, mob, target);
            let dist = target.base().distance_to_sqr_vec(mob.position());
            if stared {
                if dist < 16.0 {
                    enderman_mut(mob).teleport_randomly(level);
                }
                self.teleport_time = 0;
            } else if dist > 256.0 {
                let ready = self.teleport_time >= self.adjusted_tick_delay(30);
                self.teleport_time += 1;
                if ready && enderman_mut(mob).teleport_towards(level, target_id) {
                    self.teleport_time = 0;
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Target])
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ENDERMAN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ENDERMAN,
    create: |ty, id, registries| Box::new(Enderman::new(ty, id, registries)),
};
