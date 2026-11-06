//! `net.minecraft.world.entity.animal.fish.Salmon`.

use super::abstract_fish::{self, AbstractFish, abstract_fish_bucketable, abstract_fish_entity};
use super::abstract_schooling_fish::{self, FollowFlockLeaderGoal, SchoolData, SchoolingFish};
use linkme::distributed_slice;
use mc_core::random::RandomSource as _;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::entity::DATA_POSE;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{SALMON_SIZE, SalmonVariant};

define_accessor!(pub DATA_TYPE: Int = Salmon);

/// `Salmon.Variant.DEFAULT`.
pub const DEFAULT_VARIANT: SalmonVariant = SalmonVariant::Medium;

/// `Salmon.Variant.boundingBoxScale`.
fn bounding_box_scale(variant: SalmonVariant) -> f32 {
    match variant {
        SalmonVariant::Small => 0.5,
        SalmonVariant::Medium => 1.0,
        SalmonVariant::Large => 1.5,
    }
}

pub struct Salmon {
    pub mob: MobData,
    school: SchoolData,
}

impl Salmon {
    /// `new Salmon(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            abstract_fish::define_synched_data(b);
            b.define(DATA_TYPE, DEFAULT_VARIANT.id());
        });
        let mut salmon = Salmon {
            mob,
            school: SchoolData::default(),
        };
        let navigation = StandardPathNavigation::water_bound(&salmon);
        abstract_fish::init_controls(&mut salmon.mob, navigation);
        salmon.register_goals();
        salmon.refresh_dimensions();
        salmon
    }

    fn register_goals(&mut self) {
        abstract_fish::register_goals(self);
        let follow = FollowFlockLeaderGoal::new(self);
        self.mob.goal_selector.add_goal(5, Box::new(follow));
    }

    /// `setVariant(variant)`.
    fn set_variant(&mut self, variant: SalmonVariant) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_TYPE, variant.id());
    }

    /// `getVariant()`: `ByIdMap.continuous(.., CLAMP)`.
    pub fn get_variant(&self) -> SalmonVariant {
        let id = self
            .mob
            .living
            .entity
            .entity_data
            .get(DATA_TYPE)
            .clamp(0, SalmonVariant::ALL.len() as i32 - 1);
        SalmonVariant::from_id(id).unwrap_or(DEFAULT_VARIANT)
    }

    /// `getSalmonScale()`.
    pub fn get_salmon_scale(&self) -> f32 {
        bounding_box_scale(self.get_variant())
    }
}

impl AbstractFish for Salmon {
    fn can_random_swim(&self, level: &ServerLevel) -> bool {
        abstract_schooling_fish::can_random_swim(self, level)
    }
    fn get_flop_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_SALMON_FLOP
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::SALMON_BUCKET)
    }
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        mc_game_core::world::entity::bucketable::save_default_data_to_bucket_tag(self, bucket);
        bucket.set(SALMON_SIZE, self.get_variant());
    }
}

impl SchoolingFish for Salmon {
    fn school(&self) -> &SchoolData {
        &self.school
    }
    fn school_mut(&mut self) -> &mut SchoolData {
        &mut self.school
    }
    fn get_max_school_size(&self) -> i32 {
        5
    }
}

abstract_fish_bucketable!(Salmon);

impl Entity for Salmon {
    abstract_fish_entity!("minecraft:salmon");

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        abstract_schooling_fish::tick(self, level);
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        self.get_max_school_size()
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        EntityDimensions::of_type(self.get_type()).scale(self.get_salmon_scale())
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_POSE.id() || accessor_id == DATA_TYPE.id() {
            self.refresh_dimensions();
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        // `WeightedList.builder().add(SMALL, 30).add(MEDIUM, 50).add(LARGE, 15).build().getRandom(this.random)`
        let mut selection = self.base_mut().random.next_int_bounded(95);
        for (variant, weight) in [
            (SalmonVariant::Small, 30),
            (SalmonVariant::Medium, 50),
            (SalmonVariant::Large, 15),
        ] {
            selection -= weight;
            if selection < 0 {
                self.set_variant(variant);
                break;
            }
        }
        abstract_schooling_fish::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&SALMON_SIZE) {
            self.set_variant(*variant);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SALMON_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SALMON_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SALMON_HURT)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_fish::add_additional_save_data(self, out);
        out.put_string("type", self.get_variant().name());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_fish::read_additional_save_data(self, input);
        let variant = input
            .get_string("type")
            .and_then(SalmonVariant::from_name)
            .unwrap_or(DEFAULT_VARIANT);
        self.set_variant(variant);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SALMON: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SALMON,
    create: |ty, id, _registries| Box::new(Salmon::new(ty, id)),
};
