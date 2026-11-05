//! `net.minecraft.world.entity.animal.fish.TropicalFish`.

use mc_core::random::RandomSource as _;
use mc_game_core::world::level::level_access::{BlockGetter as _, LevelAccessor as _};
use mc_game_core::world::level::material::FluidStateExt as _;
use super::abstract_fish::{self, AbstractFish, abstract_fish_bucketable, abstract_fish_entity};
use super::abstract_schooling_fish::{self, FollowFlockLeaderGoal, SchoolData, SchoolSpawnGroupData, SchoolingFish};
use mc_game_core::define_accessor;
use mc_game_core::impl_spawn_group_data;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{DyeColor, TROPICAL_FISH_BASE_COLOR, TROPICAL_FISH_PATTERN, TROPICAL_FISH_PATTERN_COLOR, TropicalFishPattern};

define_accessor!(pub DATA_ID_TYPE_VARIANT: Int = TropicalFish);

/// `TropicalFish.Variant`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Variant {
    pub pattern: TropicalFishPattern,
    pub base_color: DyeColor,
    pub pattern_color: DyeColor,
}

const fn v(pattern: TropicalFishPattern, base_color: DyeColor, pattern_color: DyeColor) -> Variant {
    Variant { pattern, base_color, pattern_color }
}

impl Variant {
    /// `new Variant(packedId)`.
    pub fn from_packed(packed: i32) -> Variant {
        Variant { pattern: get_pattern(packed), base_color: get_base_color(packed), pattern_color: get_pattern_color(packed) }
    }

    /// `getPackedId()`.
    pub fn get_packed_id(&self) -> i32 {
        pack_variant(self.pattern, self.base_color, self.pattern_color)
    }
}

/// `DEFAULT_VARIANT`.
pub const DEFAULT_VARIANT: Variant = v(TropicalFishPattern::Kob, DyeColor::White, DyeColor::White);

/// `COMMON_VARIANTS`.
pub const COMMON_VARIANTS: [Variant; 22] = [
    v(TropicalFishPattern::Stripey, DyeColor::Orange, DyeColor::Gray),
    v(TropicalFishPattern::Flopper, DyeColor::Gray, DyeColor::Gray),
    v(TropicalFishPattern::Flopper, DyeColor::Gray, DyeColor::Blue),
    v(TropicalFishPattern::Clayfish, DyeColor::White, DyeColor::Gray),
    v(TropicalFishPattern::Sunstreak, DyeColor::Blue, DyeColor::Gray),
    v(TropicalFishPattern::Kob, DyeColor::Orange, DyeColor::White),
    v(TropicalFishPattern::Spotty, DyeColor::Pink, DyeColor::LightBlue),
    v(TropicalFishPattern::Blockfish, DyeColor::Purple, DyeColor::Yellow),
    v(TropicalFishPattern::Clayfish, DyeColor::White, DyeColor::Red),
    v(TropicalFishPattern::Spotty, DyeColor::White, DyeColor::Yellow),
    v(TropicalFishPattern::Glitter, DyeColor::White, DyeColor::Gray),
    v(TropicalFishPattern::Clayfish, DyeColor::White, DyeColor::Orange),
    v(TropicalFishPattern::Dasher, DyeColor::Cyan, DyeColor::Pink),
    v(TropicalFishPattern::Brinely, DyeColor::Lime, DyeColor::LightBlue),
    v(TropicalFishPattern::Betty, DyeColor::Red, DyeColor::White),
    v(TropicalFishPattern::Snooper, DyeColor::Gray, DyeColor::Red),
    v(TropicalFishPattern::Blockfish, DyeColor::Red, DyeColor::White),
    v(TropicalFishPattern::Flopper, DyeColor::White, DyeColor::Yellow),
    v(TropicalFishPattern::Kob, DyeColor::Red, DyeColor::White),
    v(TropicalFishPattern::Sunstreak, DyeColor::Gray, DyeColor::White),
    v(TropicalFishPattern::Dasher, DyeColor::Cyan, DyeColor::Yellow),
    v(TropicalFishPattern::Flopper, DyeColor::Yellow, DyeColor::Yellow),
];

/// `getPredefinedName(index)`.
pub fn get_predefined_name(index: usize) -> String {
    format!("entity.minecraft.tropical_fish.predefined.{index}")
}

/// `packVariant(pattern, baseColor, patternColor)`.
fn pack_variant(pattern: TropicalFishPattern, base_color: DyeColor, pattern_color: DyeColor) -> i32 {
    pattern.packed_id() & 65535 | (base_color.id() & 0xFF) << 16 | (pattern_color.id() & 0xFF) << 24
}

/// `DyeColor.byId` (`ByIdMap.continuous(.., ZERO)`).
fn dye_by_id(id: i32) -> DyeColor {
    DyeColor::from_id(id).unwrap_or(DyeColor::White)
}

/// `getBaseColor(packedVariant)`.
pub fn get_base_color(packed_variant: i32) -> DyeColor {
    dye_by_id(packed_variant >> 16 & 0xFF)
}

/// `getPatternColor(packedVariant)`.
pub fn get_pattern_color(packed_variant: i32) -> DyeColor {
    dye_by_id(packed_variant >> 24 & 0xFF)
}

/// `getPattern(packedVariant)`: `Pattern.byId` is `ByIdMap.sparse(.., KOB)`.
pub fn get_pattern(packed_variant: i32) -> TropicalFishPattern {
    let id = packed_variant & 65535;
    TropicalFishPattern::ALL.iter().copied().find(|p| p.packed_id() == id).unwrap_or(TropicalFishPattern::Kob)
}

/// `TropicalFish.TropicalFishGroupData`.
pub struct TropicalFishGroupData {
    pub school: SchoolSpawnGroupData,
    pub variant: Variant,
}
impl_spawn_group_data!(TropicalFishGroupData);

pub struct TropicalFish {
    pub mob: MobData,
    school: SchoolData,
    is_school: bool,
}

impl TropicalFish {
    /// `new TropicalFish(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            abstract_fish::define_synched_data(b);
            b.define(DATA_ID_TYPE_VARIANT, DEFAULT_VARIANT.get_packed_id());
        });
        let mut fish = TropicalFish { mob, school: SchoolData::default(), is_school: true };
        let navigation = StandardPathNavigation::water_bound(&fish);
        abstract_fish::init_controls(&mut fish.mob, navigation);
        abstract_fish::register_goals(&mut fish);
        let follow = FollowFlockLeaderGoal::new(&mut fish);
        fish.mob.goal_selector.add_goal(5, Box::new(follow));
        fish
    }

    fn set_packed_variant(&mut self, packed: i32) {
        self.mob.living.entity.entity_data.set(DATA_ID_TYPE_VARIANT, packed);
    }

    fn get_packed_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_ID_TYPE_VARIANT)
    }

    pub fn get_base_color(&self) -> DyeColor {
        get_base_color(self.get_packed_variant())
    }

    pub fn get_pattern_color(&self) -> DyeColor {
        get_pattern_color(self.get_packed_variant())
    }

    pub fn get_pattern(&self) -> TropicalFishPattern {
        get_pattern(self.get_packed_variant())
    }

    fn set_pattern(&mut self, pattern: TropicalFishPattern) {
        let base = self.get_packed_variant();
        self.set_packed_variant(pack_variant(pattern, get_base_color(base), get_pattern_color(base)));
    }

    fn set_base_color(&mut self, base_color: DyeColor) {
        let base = self.get_packed_variant();
        self.set_packed_variant(pack_variant(get_pattern(base), base_color, get_pattern_color(base)));
    }

    fn set_pattern_color(&mut self, pattern_color: DyeColor) {
        let base = self.get_packed_variant();
        self.set_packed_variant(pack_variant(get_pattern(base), get_base_color(base), pattern_color));
    }
}

impl AbstractFish for TropicalFish {
    fn can_random_swim(&self, level: &ServerLevel) -> bool {
        abstract_schooling_fish::can_random_swim(self, level)
    }
    fn get_flop_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_TROPICAL_FISH_FLOP
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::TROPICAL_FISH_BUCKET)
    }
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        mc_game_core::world::entity::bucketable::save_default_data_to_bucket_tag(self, bucket);
        bucket.set(TROPICAL_FISH_PATTERN, self.get_pattern());
        bucket.set(TROPICAL_FISH_BASE_COLOR, self.get_base_color());
        bucket.set(TROPICAL_FISH_PATTERN_COLOR, self.get_pattern_color());
    }
}

impl SchoolingFish for TropicalFish {
    fn school(&self) -> &SchoolData {
        &self.school
    }
    fn school_mut(&mut self) -> &mut SchoolData {
        &mut self.school
    }
}

abstract_fish_bucketable!(TropicalFish);

impl Entity for TropicalFish {
    abstract_fish_entity!("minecraft:tropical_fish");

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        abstract_schooling_fish::tick(self, level);
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        self.get_max_school_size()
    }

    fn is_max_group_size_reached(&self, _group_size: i32) -> bool {
        !self.is_school
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_fish::add_additional_save_data(self, out);
        out.put_int("Variant", self.get_packed_variant());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_fish::read_additional_save_data(self, input);
        let packed = input.get_int_or("Variant", DEFAULT_VARIANT.get_packed_id());
        self.set_packed_variant(Variant::from_packed(packed).get_packed_id());
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(pattern) = stack.get(&TROPICAL_FISH_PATTERN) {
            self.set_pattern(*pattern);
        }
        if let Some(color) = stack.get(&TROPICAL_FISH_BASE_COLOR) {
            self.set_base_color(*color);
        }
        if let Some(color) = stack.get(&TROPICAL_FISH_PATTERN_COLOR) {
            self.set_pattern_color(*color);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_TROPICAL_FISH_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_TROPICAL_FISH_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_TROPICAL_FISH_HURT)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let mut group_data = abstract_schooling_fish::finalize_spawn(self, level, difficulty, reason, group_data);
        let random = level.random();
        let existing = group_data.as_ref().and_then(|g| g.as_any().downcast_ref::<TropicalFishGroupData>()).map(|g| g.variant);
        let variant = if let Some(variant) = existing {
            variant
        } else if random.next_float() < 0.9 {
            let variant = COMMON_VARIANTS[random.next_int_bounded(COMMON_VARIANTS.len() as i32) as usize];
            // `new TropicalFishGroupData(this, variant)`: this fish becomes the leader of the group
            group_data = Some(Box::new(TropicalFishGroupData { school: SchoolSpawnGroupData::new(&*self, self.id()), variant }));
            variant
        } else {
            self.is_school = false;
            let pattern = TropicalFishPattern::ALL[random.next_int_bounded(TropicalFishPattern::ALL.len() as i32) as usize];
            let base_color = DyeColor::ALL[random.next_int_bounded(DyeColor::ALL.len() as i32) as usize];
            let pattern_color = DyeColor::ALL[random.next_int_bounded(DyeColor::ALL.len() as i32) as usize];
            v(pattern, base_color, pattern_color)
        };
        self.set_packed_variant(variant.get_packed_id());
        group_data
    }
}

/// `TropicalFish.checkTropicalFishSpawnRules`.
fn check_tropical_fish_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    level.get_fluid_state(pos.below()).is_tag("water")
        && level.get_block_state(pos.above()).block() == Block::WATER
        && (biome_allows_any_height(level, pos) || spawn_predicates::check_surface_water_animal_spawn_rules(ty, level, reason, pos, random))
}

/// `level.getBiome(pos).is(BiomeTags.ALLOWS_TROPICAL_FISH_SPAWNS_AT_ANY_HEIGHT)`.
fn biome_allows_any_height(level: &dyn ServerLevelAccessor, pos: BlockPos) -> bool {
    let registries = level.registry_access();
    let Some(registry) = registries.registry(keys::BIOME) else { return false };
    level.get_biome_id(pos).is_some_and(|id| registry.is_in_tag(id.0, &mc_core::ident::Identifier::new("minecraft", "allows_tropical_fish_spawns_at_any_height").expect("valid id")))
}

register_spawn_placement!(EntityType::TROPICAL_FISH, check_tropical_fish_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static TROPICAL_FISH: EntityRegistration = EntityRegistration { entity_type: EntityType::TROPICAL_FISH, create: |ty, id, _registries| Box::new(TropicalFish::new(ty, id)) };
