//! `Panda`: main/hidden genes, sneezing, rolling, sitting and eating, lying on the back, bamboo-dependent breeding.

use super::panda_goals::{
    PandaAttackGoal, PandaAvoidGoal, PandaBreedGoal, PandaHurtByTargetGoal, PandaLieOnBackGoal,
    PandaLookAtPlayerGoal, PandaPanicGoal, PandaRollGoal, PandaSitGoal, PandaSneezeGoal,
};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::move_control::{self, MoveControl, MoveControlBase};
use mc_game_core::world::entity::ai::goal::{
    FloatGoal, FollowParentGoal, RandomLookAroundGoal, TemptGoal, WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item::{self, ItemEntity};
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable, animal, ext, navigation};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use std::sync::Arc;

define_accessor!(pub UNHAPPY_COUNTER: Int = Panda);
define_accessor!(pub SNEEZE_COUNTER: Int = Panda);
define_accessor!(pub EAT_COUNTER: Int = Panda);
define_accessor!(pub MAIN_GENE_ID: Byte = Panda);
define_accessor!(pub HIDDEN_GENE_ID: Byte = Panda);
define_accessor!(pub DATA_ID_FLAGS: Byte = Panda);

const FLAG_SNEEZE: i32 = 2;
const FLAG_ROLL: i32 = 4;
const FLAG_SIT: i32 = 8;
const FLAG_ON_BACK: i32 = 16;
pub const TOTAL_ROLL_STEPS: i32 = 32;
pub const TOTAL_UNHAPPY_TIME: i32 = 32;

/// `Panda.Gene`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gene {
    Normal,
    Lazy,
    Worried,
    Playful,
    Brown,
    Weak,
    Aggressive,
}

impl Gene {
    pub const ALL: [Gene; 7] = [
        Gene::Normal,
        Gene::Lazy,
        Gene::Worried,
        Gene::Playful,
        Gene::Brown,
        Gene::Weak,
        Gene::Aggressive,
    ];

    pub fn id(self) -> i32 {
        self as i32
    }

    /// `getSerializedName()`.
    pub fn name(self) -> &'static str {
        match self {
            Gene::Normal => "normal",
            Gene::Lazy => "lazy",
            Gene::Worried => "worried",
            Gene::Playful => "playful",
            Gene::Brown => "brown",
            Gene::Weak => "weak",
            Gene::Aggressive => "aggressive",
        }
    }

    pub fn from_name(name: &str) -> Option<Gene> {
        Self::ALL.into_iter().find(|g| g.name() == name)
    }

    /// `isRecessive()`.
    pub fn is_recessive(self) -> bool {
        matches!(self, Gene::Brown | Gene::Weak)
    }

    /// `getVariantFromGenes(mainGene, hiddenGene)`.
    pub fn get_variant_from_genes(main_gene: Gene, hidden_gene: Gene) -> Gene {
        if main_gene.is_recessive() {
            if main_gene == hidden_gene {
                main_gene
            } else {
                Gene::Normal
            }
        } else {
            main_gene
        }
    }

    /// `byId(id)` (`ByIdMap.continuous(.., ZERO)`).
    pub fn by_id(id: i32) -> Gene {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Gene::Normal)
    }

    /// `getRandom(random)`.
    pub fn get_random(random: &mut dyn RandomSource) -> Gene {
        let next_int = random.next_int_bounded(16);
        if next_int == 0 {
            Gene::Lazy
        } else if next_int == 1 {
            Gene::Worried
        } else if next_int == 2 {
            Gene::Playful
        } else if next_int == 4 {
            Gene::Aggressive
        } else if next_int < 9 {
            Gene::Weak
        } else if next_int < 11 {
            Gene::Brown
        } else {
            Gene::Normal
        }
    }
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let base = EntityDimensions::of_type(EntityType::PANDA).scale(0.5);
    base.with_eye_height(0.28125).with_attachments(
        EntityAttachments::create_default(base.width, base.height)
            .with_passenger(Vec3::new(0.0, 0.5625, 0.0)),
    )
}

/// `ItemTags.PANDA_FOOD`.
fn is_panda_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("panda_food").is_some_and(|t| t.contains(stack.item()))
}

/// `Panda.canPickUpAndEat(entity)`.
pub fn can_pick_up_and_eat(entity: &ItemEntity) -> bool {
    mc_data::tags::get::<Item>("panda_eats_from_ground")
        .is_some_and(|t| t.contains(entity.get_item().item()))
        && entity.is_alive()
        && !entity.has_pick_up_delay()
}

/// `Panda.PandaMoveControl`.
#[derive(Default)]
pub struct PandaMoveControl {
    base: MoveControlBase,
}

impl MoveControl for PandaMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if mob
            .as_any()
            .downcast_ref::<Panda>()
            .is_some_and(|p| p.can_perform_action(level))
        {
            move_control::base_tick(&mut self.base, mob, level);
        }
    }
}

/// `Panda`.
pub struct Panda {
    pub mob: MobData,
    pub got_bamboo: bool,
    pub did_bite: bool,
    pub roll_counter: i32,
    roll_delta: Vec3,
    /// `lookAtPlayerGoal.setTarget(player)` requested by `PandaBreedGoal`, applied by `PandaLookAtPlayerGoal` (see there).
    pub look_at_request: Option<Option<EntityId>>,
}

impl Panda {
    /// `new Panda(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(UNHAPPY_COUNTER, 0);
            b.define(SNEEZE_COUNTER, 0);
            b.define(MAIN_GENE_ID, 0i8);
            b.define(HIDDEN_GENE_ID, 0i8);
            b.define(DATA_ID_FLAGS, 0i8);
            b.define(EAT_COUNTER, 0);
        });
        let mut panda = Panda {
            mob,
            got_bamboo: false,
            did_bite: false,
            roll_counter: 0,
            roll_delta: Vec3::ZERO,
            look_at_request: None,
        };
        let navigation = StandardPathNavigation::ground(&panda);
        panda.mob.set_navigation(Box::new(navigation));
        panda.register_goals();
        panda.mob.move_control = Some(Box::<PandaMoveControl>::default());
        if !panda.is_baby() {
            panda.mob.can_pick_up_loot = true;
        }
        panda
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = PandaBreedGoal::new(self, 1.0);
        let look_at = PandaLookAtPlayerGoal::new(self, 6.0);
        let follow_parent = FollowParentGoal::new(self, 1.25);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(2, Box::new(PandaPanicGoal::new(2.0)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(3, Box::new(PandaAttackGoal::new(f64::from(1.2f32), true)));
        goals.add_goal(
            4,
            Box::new(TemptGoal::new(
                1.0,
                Arc::new(|stack: &ItemStack| is_panda_food(stack)),
                false,
            )),
        );
        goals.add_goal(
            6,
            Box::new(PandaAvoidGoal::new(
                Arc::new(|e: &dyn Entity| e.is_player()),
                8.0,
                2.0,
                2.0,
            )),
        );
        goals.add_goal(
            6,
            Box::new(PandaAvoidGoal::new(
                Arc::new(|e: &dyn Entity| e.mob().is_some_and(|m| m.kind == MobKind::Monster)),
                4.0,
                2.0,
                2.0,
            )),
        );
        goals.add_goal(7, Box::new(PandaSitGoal::new()));
        goals.add_goal(8, Box::new(PandaLieOnBackGoal::new()));
        goals.add_goal(8, Box::new(PandaSneezeGoal));
        goals.add_goal(9, Box::new(look_at));
        goals.add_goal(10, Box::new(RandomLookAroundGoal::new()));
        goals.add_goal(12, Box::new(PandaRollGoal));
        goals.add_goal(13, Box::new(follow_parent));
        goals.add_goal(14, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        self.mob
            .target_selector
            .add_goal(1, Box::new(PandaHurtByTargetGoal::new()));
    }

    // ---- synched data ----
    pub fn get_unhappy_counter(&self) -> i32 {
        self.base().entity_data.get(UNHAPPY_COUNTER)
    }

    pub fn set_unhappy_counter(&mut self, value: i32) {
        self.base_mut().entity_data.set(UNHAPPY_COUNTER, value);
    }

    fn get_flag(&self, flag: i32) -> bool {
        (i32::from(self.base().entity_data.get(DATA_ID_FLAGS)) & flag) != 0
    }

    fn set_flag(&mut self, flag: i32, value: bool) {
        let current = i32::from(self.base().entity_data.get(DATA_ID_FLAGS));
        let new = if value {
            current | flag
        } else {
            current & !flag
        };
        self.base_mut().entity_data.set(DATA_ID_FLAGS, new as i8);
    }

    pub fn is_sneezing(&self) -> bool {
        self.get_flag(FLAG_SNEEZE)
    }

    pub fn is_sitting(&self) -> bool {
        self.get_flag(FLAG_SIT)
    }

    pub fn sit(&mut self, value: bool) {
        self.set_flag(FLAG_SIT, value);
    }

    pub fn is_on_back(&self) -> bool {
        self.get_flag(FLAG_ON_BACK)
    }

    pub fn set_on_back(&mut self, value: bool) {
        self.set_flag(FLAG_ON_BACK, value);
    }

    pub fn is_eating(&self) -> bool {
        self.base().entity_data.get(EAT_COUNTER) > 0
    }

    pub fn eat(&mut self, value: bool) {
        self.base_mut()
            .entity_data
            .set(EAT_COUNTER, i32::from(value));
    }

    fn get_eat_counter(&self) -> i32 {
        self.base().entity_data.get(EAT_COUNTER)
    }

    fn set_eat_counter(&mut self, value: i32) {
        self.base_mut().entity_data.set(EAT_COUNTER, value);
    }

    pub fn sneeze(&mut self, value: bool) {
        self.set_flag(FLAG_SNEEZE, value);
        if !value {
            self.set_sneeze_counter(0);
        }
    }

    pub fn get_sneeze_counter(&self) -> i32 {
        self.base().entity_data.get(SNEEZE_COUNTER)
    }

    pub fn set_sneeze_counter(&mut self, value: i32) {
        self.base_mut().entity_data.set(SNEEZE_COUNTER, value);
    }

    pub fn is_rolling(&self) -> bool {
        self.get_flag(FLAG_ROLL)
    }

    pub fn roll(&mut self, value: bool) {
        self.set_flag(FLAG_ROLL, value);
    }

    // ---- genes ----
    pub fn get_main_gene(&self) -> Gene {
        Gene::by_id(i32::from(self.base().entity_data.get(MAIN_GENE_ID)))
    }

    pub fn set_main_gene(&mut self, gene: Gene) {
        self.base_mut()
            .entity_data
            .set(MAIN_GENE_ID, gene.id() as i8);
    }

    pub fn get_hidden_gene(&self) -> Gene {
        Gene::by_id(i32::from(self.base().entity_data.get(HIDDEN_GENE_ID)))
    }

    pub fn set_hidden_gene(&mut self, gene: Gene) {
        self.base_mut()
            .entity_data
            .set(HIDDEN_GENE_ID, gene.id() as i8);
    }

    /// `getVariant()`.
    pub fn get_variant(&self) -> Gene {
        Gene::get_variant_from_genes(self.get_main_gene(), self.get_hidden_gene())
    }

    pub fn is_lazy(&self) -> bool {
        self.get_variant() == Gene::Lazy
    }

    pub fn is_worried(&self) -> bool {
        self.get_variant() == Gene::Worried
    }

    pub fn is_playful(&self) -> bool {
        self.get_variant() == Gene::Playful
    }

    pub fn is_brown(&self) -> bool {
        self.get_variant() == Gene::Brown
    }

    pub fn is_weak(&self) -> bool {
        self.get_variant() == Gene::Weak
    }

    /// `getOneOfGenesRandomly()`.
    fn get_one_of_genes_randomly(&mut self) -> Gene {
        if self.base_mut().random.next_boolean() {
            self.get_main_gene()
        } else {
            self.get_hidden_gene()
        }
    }

    /// `setAttributes()`.
    pub fn set_attributes(&mut self) {
        if self.is_weak()
            && let Some(health) = self
                .mob
                .living
                .attributes
                .get_instance(Attribute::MAX_HEALTH)
        {
            health.set_base_value(10.0);
        }
        if self.is_lazy()
            && let Some(speed) = self
                .mob
                .living
                .attributes
                .get_instance(Attribute::MOVEMENT_SPEED)
        {
            speed.set_base_value(f64::from(0.07f32));
        }
    }

    // ---- state ----
    /// `isScared()`.
    pub fn is_scared(&self, level: &ServerLevel) -> bool {
        self.is_worried() && level.is_thundering()
    }

    /// `canPerformAction()`.
    pub fn can_perform_action(&self, level: &ServerLevel) -> bool {
        !self.is_on_back()
            && !self.is_scared(level)
            && !self.is_eating()
            && !self.is_rolling()
            && !self.is_sitting()
    }

    /// `tryToSit()`.
    pub fn try_to_sit(&mut self, level: &mut ServerLevel) {
        if !self.is_in_water() {
            mob::set_zza(self, 0.0);
            navigation::stop(self, level);
            self.sit(true);
        }
    }

    /// `handleEating()`.
    fn handle_eating(&mut self, level: &mut ServerLevel) {
        let mainhand = EquipmentSlot::MainHand;
        if !self.is_eating()
            && self.is_sitting()
            && !self.is_scared(level)
            && !self.get_item_by_slot(mainhand).is_empty()
            && self.base_mut().random.next_int_bounded(80) == 1
        {
            self.eat(true);
        } else if self.get_item_by_slot(mainhand).is_empty() || !self.is_sitting() {
            self.eat(false);
        }
        if self.is_eating() {
            self.add_eating_particles(level);
            if self.get_eat_counter() > 80 && self.base_mut().random.next_int_bounded(20) == 1 {
                if self.get_eat_counter() > 100
                    && mc_data::tags::get::<Item>("panda_eats_from_ground")
                        .is_some_and(|t| t.contains(self.get_item_by_slot(mainhand).item()))
                {
                    self.set_item_slot(level, mainhand, ItemStack::empty());
                    self.game_event(level, GameEvent::EAT);
                    self.sit(false);
                }
                self.eat(false);
                return;
            }
            let counter = self.get_eat_counter() + 1;
            self.set_eat_counter(counter);
        }
    }

    /// `addEatingParticles()`: the sound plus the particle vectors, whose random draws are kept (the particles themselves are
    /// client side).
    fn add_eating_particles(&mut self, level: &mut ServerLevel) {
        if self.get_eat_counter() % 5 == 0 {
            let volume = 0.5 + 0.5 * self.base_mut().random.next_int_bounded(2) as f32;
            let pitch = (self.base_mut().random.next_float() - self.base_mut().random.next_float())
                * 0.2
                + 1.0;
            self.play_sound(level, SoundEvent::ENTITY_PANDA_EAT, volume, pitch);
            if !self.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
                for _ in 0..6 {
                    // velocity (3 floats) and position (3 floats)
                    for _ in 0..6 {
                        self.base_mut().random.next_float();
                    }
                }
            }
        }
    }

    /// `handleRoll()`.
    fn handle_roll(&mut self) {
        self.roll_counter += 1;
        if self.roll_counter > TOTAL_ROLL_STEPS {
            self.roll(false);
        } else {
            let movement = self.base().delta_movement();
            if self.roll_counter == 1 {
                let angle = self.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
                let multiplier = if self.is_baby() { 0.1f32 } else { 0.2f32 };
                self.roll_delta = Vec3::new(
                    movement.x + f64::from(-mth::sin(f64::from(angle)) * multiplier),
                    0.0,
                    movement.z + f64::from(mth::cos(f64::from(angle)) * multiplier),
                );
                let delta = self.roll_delta.add_xyz(0.0, 0.27, 0.0);
                self.base_mut().set_delta_movement(delta);
            } else if self.roll_counter != 7 && self.roll_counter != 15 && self.roll_counter != 23 {
                let delta = Vec3::new(self.roll_delta.x, movement.y, self.roll_delta.z);
                self.base_mut().set_delta_movement(delta);
            } else {
                let y = if self.base().on_ground() {
                    0.27
                } else {
                    movement.y
                };
                self.base_mut().set_delta_movement(Vec3::new(0.0, y, 0.0));
            }
        }
    }

    /// `afterSneeze()`.
    fn after_sneeze(&mut self, level: &mut ServerLevel) {
        // `level.addParticle(SNEEZE ..)` is client side
        self.play_sound(level, SoundEvent::ENTITY_PANDA_SNEEZE, 1.0, 1.0);
        let bb = self.base().bounding_box().inflate_scalar(10.0);
        // the sneezing panda is lent: it is in the list it asks for, in query order
        level.lend(self.as_dyn_mut(), |level| {
            let pandas = level.get_entities_of_class(
                &bb,
                |e| e.as_any().is::<Panda>(),
                |e| {
                    e.as_any().downcast_ref::<Panda>().is_some_and(|p| {
                        !p.is_baby()
                            && p.base().on_ground()
                            && !p.is_in_water()
                            && p.can_perform_action(level)
                    })
                },
            );
            for id in pandas {
                level.with_entity(id, |level, p| p.jump_from_ground(level));
            }
        });
        if level.mob_drops() {
            let table = Identifier::parse("minecraft:gameplay/panda_sneeze").expect("valid id");
            ext::drop_from_gift_loot_table(level, self, &table, |level, e, stack| {
                item::spawn_at_location(level, e, stack, Vec3::ZERO);
            });
        }
    }
}

impl Entity for Panda {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:panda")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn is_aggressive(&self) -> bool {
        self.get_variant() == Gene::Aggressive
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::MainHand && self.can_pick_up_loot()
    }

    fn can_be_leashed(&self) -> bool {
        false
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        if !self.is_aggressive() {
            self.did_bite = true;
        }
        mc_game_core::world::entity::living_entity::combat::mob_do_hurt_target(self, level, target)
    }

    fn play_attack_sound(&mut self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_PANDA_BITE, 1.0, 1.0);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.is_worried() {
            if level.is_thundering() && !self.is_in_water() {
                self.sit(true);
                self.eat(false);
            } else if !self.is_eating() {
                self.sit(false);
            }
        }
        let target = mob::tick::get_target_checked(self, level);
        if target.is_none() {
            self.got_bamboo = false;
            self.did_bite = false;
        }
        if self.get_unhappy_counter() > 0 {
            if let Some(target) = target.and_then(|t| level.get_entity(t)) {
                ext::mob_look_at(self, target, 90.0, 90.0);
            }
            if self.get_unhappy_counter() == 29 || self.get_unhappy_counter() == 14 {
                self.play_sound(level, SoundEvent::ENTITY_PANDA_CANT_BREED, 1.0, 1.0);
            }
            let counter = self.get_unhappy_counter() - 1;
            self.set_unhappy_counter(counter);
        }
        if self.is_sneezing() {
            let counter = self.get_sneeze_counter() + 1;
            self.set_sneeze_counter(counter);
            if self.get_sneeze_counter() > 20 {
                self.sneeze(false);
                self.after_sneeze(level);
            } else if self.get_sneeze_counter() == 1 {
                self.play_sound(level, SoundEvent::ENTITY_PANDA_PRE_SNEEZE, 1.0, 1.0);
            }
        }
        if self.is_rolling() {
            self.handle_roll();
        } else {
            self.roll_counter = 0;
        }
        if self.is_sitting() {
            self.set_x_rot(0.0);
        }
        // `updateSitAmount` / `updateOnBackAnimation` / `updateRollAmount` only feed the client animation
        self.handle_eating(level);
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item_id: EntityId) {
        if !self.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            return;
        }
        let Some(stack) = level
            .get_entity(item_id)
            .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
            .filter(|i| can_pick_up_and_eat(i))
            .map(ItemEntity::get_item)
        else {
            return;
        };
        mc_game_core::world::entity::player::pickup::on_item_pickup_by_id(level, &*self, item_id);
        self.set_item_slot(level, EquipmentSlot::MainHand, stack.clone());
        mob::equipment::set_guaranteed_drop(self, EquipmentSlot::MainHand);
        ext::take(level, &*self, item_id, stack.count());
        level.with_entity(item_id, |_, e| e.discard());
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        self.sit(false);
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, amount)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let main = Gene::get_random(level.random());
        self.set_main_gene(main);
        let hidden = Gene::get_random(level.random());
        self.set_hidden_gene(hidden);
        self.set_attributes();
        let group_data =
            group_data.unwrap_or_else(|| Box::new(AgeableMobGroupData::with_chance(0.2)));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(
            EntityType::PANDA,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let partner_is_panda = level
            .get_entity(partner)
            .is_some_and(|p| p.as_any().is::<Panda>());
        if partner_is_panda {
            // `setGeneFromParents(this, partner)`: `this.random` of the baby, `getOneOfGenesRandomly()` on each parent's random
            let baby_panda = baby.as_any_mut().downcast_mut::<Panda>().expect("panda");
            let genes_of = |parent: &mut Panda| parent.get_one_of_genes_randomly();
            let partner_gene = |level: &mut ServerLevel| {
                level
                    .with_entity(partner, |_, p| {
                        p.as_any_mut()
                            .downcast_mut::<Panda>()
                            .map(|p| p.get_one_of_genes_randomly())
                    })
                    .flatten()
            };
            if baby_panda.base_mut().random.next_boolean() {
                let main = genes_of(self);
                let hidden = partner_gene(level);
                baby_panda.set_main_gene(main);
                baby_panda.set_hidden_gene(hidden.unwrap_or(Gene::Normal));
            } else {
                let main = partner_gene(level);
                let hidden = genes_of(self);
                baby_panda.set_main_gene(main.unwrap_or(Gene::Normal));
                baby_panda.set_hidden_gene(hidden);
            }
            if baby_panda.base_mut().random.next_int_bounded(32) == 0 {
                let gene = Gene::get_random(&mut baby_panda.base_mut().random);
                baby_panda.set_main_gene(gene);
            }
            if baby_panda.base_mut().random.next_int_bounded(32) == 0 {
                let gene = Gene::get_random(&mut baby_panda.base_mut().random);
                baby_panda.set_hidden_gene(gene);
            }
            baby_panda.set_attributes();
        } else if let Some(baby_panda) = baby.as_any_mut().downcast_mut::<Panda>() {
            baby_panda.set_attributes();
        }
        Some(baby)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let interaction_item = player.get_item_in_hand(hand);
        if self.is_scared(level) {
            InteractionResult::PASS
        } else if self.is_on_back() {
            self.set_on_back(false);
            InteractionResult::SUCCESS
        } else if self.is_food(&interaction_item) {
            if mob::tick::get_target_checked(self, level).is_some() {
                self.got_bamboo = true;
            }
            if self.can_age_up() {
                self.use_player_item(level, player, hand);
                let seconds = ageable::get_speed_up_seconds_when_feeding(-ageable::get_age(self));
                ageable::age_up(self, level, seconds, true);
            } else {
                if self.is_baby() {
                    return InteractionResult::PASS;
                }
                if ageable::get_age(self) == 0 && self.can_fall_in_love() {
                    self.use_player_item(level, player, hand);
                    animal::set_in_love(self, level, Some(&*player));
                } else {
                    if self.is_sitting() || self.is_in_water() {
                        return InteractionResult::PASS;
                    }
                    self.try_to_sit(level);
                    self.eat(true);
                    let current = self.get_item_by_slot(EquipmentSlot::MainHand);
                    if !current.is_empty() && !player.player.abilities.instabuild {
                        item::spawn_at_location(level, self, current, Vec3::ZERO);
                    }
                    self.set_item_slot(
                        level,
                        EquipmentSlot::MainHand,
                        ItemStack::with_count(interaction_item.item(), 1),
                    );
                    self.use_player_item(level, player, hand);
                }
            }
            InteractionResult::SUCCESS_SERVER
        } else if self.is_baby()
            && mc_game_core::world::entity::animal::equine::abstract_horse::is_holding(
                player,
                Item::GOLDEN_DANDELION,
            )
        {
            mob::interact::mob_interact(self, level, player, hand)
        } else {
            InteractionResult::PASS
        }
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if self.is_aggressive() {
            Some(SoundEvent::ENTITY_PANDA_AGGRESSIVE_AMBIENT)
        } else if self.is_worried() {
            Some(SoundEvent::ENTITY_PANDA_WORRIED_AMBIENT)
        } else {
            Some(SoundEvent::ENTITY_PANDA_AMBIENT)
        }
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: mc_core::pos::BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_PANDA_STEP, 0.15, 1.0);
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_panda_food(stack)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PANDA_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PANDA_HURT)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_string("MainGene", self.get_main_gene().name());
        out.put_string("HiddenGene", self.get_hidden_gene().name());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_main_gene(
            input
                .get_string("MainGene")
                .and_then(Gene::from_name)
                .unwrap_or(Gene::Normal),
        );
        self.set_hidden_gene(
            input
                .get_string("HiddenGene")
                .and_then(Gene::from_name)
                .unwrap_or(Gene::Normal),
        );
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PANDA: EntityRegistration = EntityRegistration {
    entity_type: EntityType::PANDA,
    create: |ty, id, registries| Box::new(Panda::new(ty, id, registries)),
};
