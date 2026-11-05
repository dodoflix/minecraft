//! `net.minecraft.world.entity.animal.fish.Cod`.

use super::abstract_fish::{self, AbstractFish, abstract_fish_bucketable, abstract_fish_entity};
use super::abstract_schooling_fish::{self, FollowFlockLeaderGoal, SchoolData, SchoolingFish};
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;

pub struct Cod {
    pub mob: MobData,
    school: SchoolData,
}

impl Cod {
    /// `new Cod(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, abstract_fish::define_synched_data);
        let mut cod = Cod { mob, school: SchoolData::default() };
        let navigation = StandardPathNavigation::water_bound(&cod);
        abstract_fish::init_controls(&mut cod.mob, navigation);
        cod.register_goals();
        cod
    }

    /// `registerGoals()`: `AbstractFish` then `AbstractSchoolingFish`.
    fn register_goals(&mut self) {
        abstract_fish::register_goals(self);
        let follow = FollowFlockLeaderGoal::new(self);
        self.mob.goal_selector.add_goal(5, Box::new(follow));
    }
}

impl AbstractFish for Cod {
    fn can_random_swim(&self, level: &ServerLevel) -> bool {
        abstract_schooling_fish::can_random_swim(self, level)
    }
    fn get_flop_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_COD_FLOP
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::COD_BUCKET)
    }
}

impl SchoolingFish for Cod {
    fn school(&self) -> &SchoolData {
        &self.school
    }
    fn school_mut(&mut self) -> &mut SchoolData {
        &mut self.school
    }
}

abstract_fish_bucketable!(Cod);

impl Entity for Cod {
    abstract_fish_entity!("minecraft:cod");

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        abstract_schooling_fish::tick(self, level);
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        self.get_max_school_size()
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        abstract_schooling_fish::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_COD_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_COD_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_COD_HURT)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_fish::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_fish::read_additional_save_data(self, input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static COD: EntityRegistration = EntityRegistration { entity_type: EntityType::COD, create: |ty, id, _registries| Box::new(Cod::new(ty, id)) };
