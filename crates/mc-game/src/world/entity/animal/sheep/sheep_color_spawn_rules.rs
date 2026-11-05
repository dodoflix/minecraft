//! `SheepColorSpawnRules`: the biome-dependent weighted colour of naturally spawned sheep.

use crate::registry::keys;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_protocol::components::DyeColor;

/// A `SheepColorProvider`: a constant colour, or a weighted list of providers (`weighted(builder()...)`).
enum Provider {
    Single(DyeColor),
    Weighted(&'static [(Provider, i32)]),
}

impl Provider {
    /// `SheepColorProvider.get(random)`: `WeightedList.getRandomOrThrow(random)` (one `nextInt(totalWeight)`), then the
    /// chosen provider's own `get(random)`.
    fn get(&self, random: &mut dyn RandomSource) -> DyeColor {
        match self {
            Provider::Single(color) => *color,
            Provider::Weighted(elements) => {
                let total_weight: i32 = elements.iter().map(|(_, w)| *w).sum();
                let mut selection = random.next_int_bounded(total_weight);
                for (provider, weight) in *elements {
                    if selection < *weight {
                        return provider.get(random);
                    }
                    selection -= *weight;
                }
                unreachable!("selection is below the total weight")
            }
        }
    }
}

/// `commonColors(defaultColor)`.
macro_rules! common_colors {
    ($default:expr) => {
        Provider::Weighted(&[(Provider::Single($default), 499), (Provider::Single(DyeColor::Pink), 1)])
    };
}

static TEMPERATE: Provider = Provider::Weighted(&[
    (Provider::Single(DyeColor::Black), 5),
    (Provider::Single(DyeColor::Gray), 5),
    (Provider::Single(DyeColor::LightGray), 5),
    (Provider::Single(DyeColor::Brown), 3),
    (common_colors!(DyeColor::White), 82),
]);
static WARM: Provider = Provider::Weighted(&[
    (Provider::Single(DyeColor::Gray), 5),
    (Provider::Single(DyeColor::LightGray), 5),
    (Provider::Single(DyeColor::White), 5),
    (Provider::Single(DyeColor::Black), 3),
    (common_colors!(DyeColor::Brown), 82),
]);
static COLD: Provider = Provider::Weighted(&[
    (Provider::Single(DyeColor::LightGray), 5),
    (Provider::Single(DyeColor::Gray), 5),
    (Provider::Single(DyeColor::White), 5),
    (Provider::Single(DyeColor::Brown), 3),
    (common_colors!(DyeColor::Black), 82),
]);

/// `SheepColorSpawnRules.getSheepColor(biome, random)` for the biome at `pos`.
pub fn get_sheep_color(level: &mut dyn ServerLevelAccessor, pos: BlockPos) -> DyeColor {
    let registries = level.registry_access().clone();
    let in_tag = |tag: &str| {
        let Some(biome) = level.get_biome_id(pos) else { return false };
        let tag = Identifier::parse(tag).expect("valid tag id");
        registries.registry(keys::BIOME).is_some_and(|r| r.is_in_tag(biome.0, &tag))
    };
    let configuration = if in_tag("minecraft:spawns_warm_variant_farm_animals") {
        &WARM
    } else if in_tag("minecraft:spawns_cold_variant_farm_animals") {
        &COLD
    } else {
        &TEMPERATE
    };
    configuration.get(level.random())
}
