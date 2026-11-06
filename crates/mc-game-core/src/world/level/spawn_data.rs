//! `net.minecraft.world.level.SpawnData`: the entity a spawner spawns next (`SpawnData.CODEC`), with the optional
//! custom light rules and the equipment loot table.

use crate::server::packs::pack_format::InclusiveRange;
use crate::util::weighted_list::WeightedList;
use crate::world::entity::equipment_table::{self, EquipmentTable};
use crate::world::level::{LevelReader, LightLayer};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_dfu::extra::{IDENTIFIER, fixed_size};
use mc_dfu::{Codec, DataResult, INT, record};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::common::compound_tag_codec;
use std::sync::LazyLock;

/// `SpawnData(entityToSpawn, customSpawnRules, equipment)`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnData {
    pub entity_to_spawn: CompoundTag,
    pub custom_spawn_rules: Option<CustomSpawnRules>,
    pub equipment: Option<EquipmentTable>,
}

impl SpawnData {
    /// The record constructor: `id` is normalised through `Identifier.CODEC` (dropped when it is not a valid identifier).
    pub fn new(
        mut entity_to_spawn: CompoundTag,
        custom_spawn_rules: Option<CustomSpawnRules>,
        equipment: Option<EquipmentTable>,
    ) -> Self {
        match entity_to_spawn
            .get("id")
            .and_then(|t| mc_dfu::parse_nbt(&IDENTIFIER, t).result())
        {
            Some(id) => entity_to_spawn.put_string("id", id.to_string()),
            None => {
                entity_to_spawn.remove("id");
            }
        }
        Self {
            entity_to_spawn,
            custom_spawn_rules,
            equipment,
        }
    }

    /// `new SpawnData()`.
    pub fn empty() -> Self {
        Self::new(CompoundTag::new(), None, None)
    }

    /// `new SpawnData(tag, empty, empty)` for a bare entity id.
    pub fn of_entity_id(id: &Identifier) -> Self {
        let mut tag = CompoundTag::new();
        tag.put_string("id", id.to_string());
        Self::new(tag, None, None)
    }

    /// `SpawnData.CODEC`.
    pub fn codec() -> Codec<SpawnData> {
        static C: LazyLock<Codec<SpawnData>> = LazyLock::new(|| {
            struct Fields {
                entity: CompoundTag,
                custom_spawn_rules: Option<CustomSpawnRules>,
                equipment: Option<EquipmentTable>,
            }
            record!(Fields {
                entity: compound_tag_codec().field_of("entity"),
                custom_spawn_rules: CustomSpawnRules::codec()
                    .optional_field_of("custom_spawn_rules"),
                equipment: equipment_table::CODEC.optional_field_of("equipment"),
            })
            .codec()
            .xmap(
                |f: Fields| SpawnData::new(f.entity, f.custom_spawn_rules, f.equipment),
                |s: &SpawnData| Fields {
                    entity: s.entity_to_spawn.clone(),
                    custom_spawn_rules: s.custom_spawn_rules,
                    equipment: s.equipment.clone(),
                },
            )
        });
        C.clone()
    }

    /// `SpawnData.LIST_CODEC`.
    pub fn list_codec() -> Codec<WeightedList<SpawnData>> {
        static C: LazyLock<Codec<WeightedList<SpawnData>>> =
            LazyLock::new(|| WeightedList::codec(&SpawnData::codec()));
        C.clone()
    }
}

/// `SpawnData.CustomSpawnRules(blockLightLimit, skyLightLimit)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomSpawnRules {
    pub block_light_limit: InclusiveRange<i32>,
    pub sky_light_limit: InclusiveRange<i32>,
}

const LIGHT_RANGE: InclusiveRange<i32> = InclusiveRange {
    min_inclusive: 0,
    max_inclusive: 15,
};

fn create_range(min: i32, max: i32) -> DataResult<InclusiveRange<i32>> {
    if min <= max {
        DataResult::success(InclusiveRange {
            min_inclusive: min,
            max_inclusive: max,
        })
    } else {
        DataResult::error_str("min_inclusive must be less than or equal to max_inclusive")
    }
}

/// `InclusiveRange.INT`: a single number, a `[min, max]` list or a `{min_inclusive, max_inclusive}` object.
fn inclusive_int_codec() -> Codec<InclusiveRange<i32>> {
    struct Bounds {
        min_inclusive: i32,
        max_inclusive: i32,
    }
    let array = INT.list_of().comap_flat_map(
        |list| fixed_size(list, 2).flat_map(|l| create_range(l[0], l[1])),
        |r: &InclusiveRange<i32>| vec![r.min_inclusive, r.max_inclusive],
    );
    let object = record!(Bounds {
        min_inclusive: INT.field_of("min_inclusive"),
        max_inclusive: INT.field_of("max_inclusive")
    })
    .codec()
    .comap_flat_map(
        |b: Bounds| create_range(b.min_inclusive, b.max_inclusive),
        |r: &InclusiveRange<i32>| Bounds {
            min_inclusive: r.min_inclusive,
            max_inclusive: r.max_inclusive,
        },
    );
    INT.either(&array.with_alternative(&object)).comap_flat_map(
        |either| either.map(|min| create_range(min, min), DataResult::success),
        |r: &InclusiveRange<i32>| {
            if r.min_inclusive == r.max_inclusive {
                mc_dfu::Either::Left(r.min_inclusive)
            } else {
                mc_dfu::Either::Right(*r)
            }
        },
    )
}

impl CustomSpawnRules {
    fn light_limit(name: &str) -> mc_dfu::MapCodec<InclusiveRange<i32>> {
        inclusive_int_codec()
            .lenient_optional_field_of_default(name, LIGHT_RANGE)
            .validate(|range| {
                if range.min_inclusive >= LIGHT_RANGE.min_inclusive
                    && range.max_inclusive <= LIGHT_RANGE.max_inclusive
                {
                    DataResult::success(range)
                } else {
                    DataResult::error_str("Light values must be withing range [0, 15]")
                }
            })
    }

    /// `CustomSpawnRules.CODEC`.
    pub fn codec() -> Codec<CustomSpawnRules> {
        record!(CustomSpawnRules {
            block_light_limit: Self::light_limit("block_light_limit"),
            sky_light_limit: Self::light_limit("sky_light_limit"),
        })
        .codec()
    }

    /// `isValidPosition(blockSpawnPos, level)`.
    pub fn is_valid_position(&self, block_spawn_pos: BlockPos, level: &dyn LevelReader) -> bool {
        self.block_light_limit
            .is_value_in_range(level.get_brightness(LightLayer::Block, block_spawn_pos))
            && self.sky_light_limit.is_value_in_range(
                (level.get_brightness(LightLayer::Sky, block_spawn_pos) - level.get_sky_darken())
                    .max(0),
            )
    }
}

/// `SpawnData.entityToSpawn.getString("id").isPresent()` + `size() == 1` (the `hasNoConfiguration` test of the spawners).
pub fn has_no_configuration(entity_to_spawn: &CompoundTag) -> bool {
    entity_to_spawn.len() == 1 && matches!(entity_to_spawn.get("id"), Some(Tag::String(_)))
}
