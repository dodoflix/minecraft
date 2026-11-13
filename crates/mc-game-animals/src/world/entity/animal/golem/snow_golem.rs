//! `SnowGolem`: snowball thrower that leaves a snow trail, melts in warm biomes, sheared for its pumpkin.

use super::abstract_golem::AMBIENT_SOUND_INTERVAL;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{Block, EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::goal::{
    LookAtPlayerGoal, RandomLookAroundGoal, RangedAttackGoal, WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::projectile::spawn::spawn_projectile;
use mc_game_core::world::entity::projectile::throwableitemprojectile::snowball::Snowball;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, hurt_and_break};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::{BlockGetter as _, LevelAccessor as _};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
use std::sync::Arc;

define_accessor!(pub DATA_PUMPKIN_ID: Byte = SnowGolem);

const PUMPKIN_FLAG: i8 = 16;

pub struct SnowGolem {
    pub mob: MobData,
}

impl SnowGolem {
    /// `new SnowGolem(type, level)` (`registerGoals` and `createNavigation` run as in `Mob.<init>`).
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            b.define(DATA_PUMPKIN_ID, PUMPKIN_FLAG);
        });
        let mut golem = SnowGolem { mob };
        let navigation = StandardPathNavigation::ground(&golem);
        golem.mob.set_navigation(Box::new(navigation));
        golem.register_goals();
        golem
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(RangedAttackGoal::new(1.25, 20, 10.0)));
        goals.add_goal(
            2,
            Box::new(WaterAvoidingRandomStrollGoal::with_probability(
                1.0,
                1.0000001E-5f32,
            )),
        );
        goals.add_goal(3, Box::new(look_at));
        goals.add_goal(4, Box::new(RandomLookAroundGoal::new()));
        self.mob.target_selector.add_goal(
            1,
            Box::new(NearestAttackableTargetGoal::build(
                TargetType::Class(Arc::new(|e| e.mob().is_some())),
                10,
                true,
                false,
                Some(Arc::new(|target, _level| target.is_enemy())),
            )),
        );
    }

    /// `hasPumpkin()`.
    pub fn has_pumpkin(&self) -> bool {
        (self.mob.living.entity.entity_data.get(DATA_PUMPKIN_ID) & PUMPKIN_FLAG) != 0
    }

    /// `setPumpkin(pumpkin)`.
    pub fn set_pumpkin(&mut self, pumpkin: bool) {
        let current = self.mob.living.entity.entity_data.get(DATA_PUMPKIN_ID);
        self.mob.living.entity.entity_data.set(
            DATA_PUMPKIN_ID,
            if pumpkin {
                current | PUMPKIN_FLAG
            } else {
                current & -17
            },
        );
    }
}

impl Entity for SnowGolem {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:snow_golem")
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("Pumpkin", self.has_pumpkin());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_pumpkin(input.get_boolean_or("Pumpkin", true));
    }

    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        let pos = self.position();
        if level.attribute_bool_vec(
            mc_game_core::world::attribute::attributes::SNOW_GOLEM_MELTS,
            &pos,
        ) {
            let source = level.damage_sources().on_fire();
            self.hurt_server(level, &source, 1.0);
        }
        if !level.mob_griefing() {
            return;
        }
        let snow = Block::SNOW.default_state();
        for i in 0..4 {
            let xx =
                mth::floor_f64(self.base().x() + f64::from(((i % 2) * 2 - 1) as f32 * 0.25f32));
            let yy = mth::floor_f64(self.base().y());
            let zz =
                mth::floor_f64(self.base().z() + f64::from(((i / 2 % 2) * 2 - 1) as f32 * 0.25f32));
            let snow_pos = BlockPos::new(xx, yy, zz);
            if level.get_block_state(snow_pos).is_air() && snow.can_survive(level, snow_pos) {
                level.set_block_and_update(snow_pos, snow);
                level.game_event_from_entity_ref_in_state(
                    &*self,
                    Some(snow),
                    GameEvent::BLOCK_PLACE,
                    mc_core::phys::Vec3::at_center_of(&snow_pos),
                );
            }
        }
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, _power: f32) {
        let Some((tx, ty_eye, tz)) = level
            .get_entity(target)
            .map(|t| (t.base().x(), t.base().get_eye_y(), t.base().z()))
        else {
            return;
        };
        let xd = tx - self.base().x();
        let yd = ty_eye - f64::from(1.1f32);
        let zd = tz - self.base().z();
        let yo = (xd * xd + zd * zd).sqrt() * f64::from(0.2f32);
        let item_stack = ItemStack::new(Item::SNOWBALL);
        let snowball = Snowball::with_owner(level, &*self, &item_stack);
        let uncertainty = self.ranged_attack_uncertainty(level);
        spawn_projectile(level, Box::new(snowball), &item_stack, |p| {
            let y = p.base().y();
            p.shoot(xd, yd + yo - y, zd, 1.6, uncertainty);
        });
        let pitch = 0.4 / (self.base_mut().random.next_float() * 0.4 + 0.8);
        self.play_sound(level, SoundEvent::ENTITY_SNOW_GOLEM_SHOOT, 1.0, pitch);
    }

    fn ranged_attack_uncertainty(&self, _level: &ServerLevel) -> f32 {
        12.0
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.is(Item::SHEARS) && self.ready_for_shearing() {
            self.shear(level, SoundSource::Players, &item_stack);
            level.game_event_from_entity_ref(&*player, GameEvent::SHEAR, self.position());
            mob::interact::with_hand_stack(player, hand, |s, p| {
                hurt_and_break(s, 1, level, p, hand.as_equipment_slot())
            });
            InteractionResult::SUCCESS
        } else {
            InteractionResult::PASS
        }
    }

    fn is_shearable(&self) -> bool {
        true
    }

    fn ready_for_shearing(&self) -> bool {
        self.has_pumpkin()
    }

    fn shear(&mut self, level: &mut ServerLevel, sound_source: SoundSource, tool: &ItemStack) {
        // the entity is out of the level while it is being sheared: its position goes with the packet
        level.play_sound_from_entity_at(
            None,
            self.id(),
            self.position(),
            SoundEvent::ENTITY_SNOW_GOLEM_SHEAR,
            sound_source,
            1.0,
            1.0,
        );
        self.set_pumpkin(false);
        let table = Identifier::parse("minecraft:shearing/snow_golem").expect("valid id");
        mob::ext::drop_from_shearing_loot_table(level, self, &table, tool, |level, e, drop| {
            let eye_height = e.base().eye_height;
            mc_game_core::world::entity::item::spawn_at_location(
                level,
                e,
                drop,
                Vec3::new(0.0, f64::from(eye_height), 0.0),
            );
        });
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SNOW_GOLEM_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SNOW_GOLEM_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SNOW_GOLEM_DEATH)
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        false
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(
            0.0,
            f64::from(0.75f32 * self.base().eye_height),
            f64::from(self.base().get_bb_width() * 0.4),
        )
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SNOW_GOLEM: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SNOW_GOLEM,
    create: |ty, id, registries| Box::new(SnowGolem::new(ty, id, registries)),
};
