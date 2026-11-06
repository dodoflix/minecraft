//! `RuinedPortalStructure`.

use super::ruined_portal_piece::{
    Properties, RuinedPortalPiece, VerticalPlacement, get_height_map_type,
};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, quart_pos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings,
};
use mc_world::heightmap::HeightmapType;
use mc_worldgen::generator::{BlockColumn, NoiseColumn};
use std::sync::Arc;

const STRUCTURE_LOCATION_PORTALS: [&str; 10] = [
    "ruined_portal/portal_1",
    "ruined_portal/portal_2",
    "ruined_portal/portal_3",
    "ruined_portal/portal_4",
    "ruined_portal/portal_5",
    "ruined_portal/portal_6",
    "ruined_portal/portal_7",
    "ruined_portal/portal_8",
    "ruined_portal/portal_9",
    "ruined_portal/portal_10",
];
const STRUCTURE_LOCATION_GIANT_PORTALS: [&str; 3] = [
    "ruined_portal/giant_portal_1",
    "ruined_portal/giant_portal_2",
    "ruined_portal/giant_portal_3",
];
const PROBABILITY_OF_GIANT_PORTAL: f32 = 0.05;
const MIN_Y_INDEX: i32 = 15;

/// `RuinedPortalStructure.Setup`.
#[derive(Clone, Copy, Debug)]
pub struct Setup {
    pub placement: VerticalPlacement,
    pub air_pocket_probability: f32,
    pub mossiness: f32,
    pub overgrown: bool,
    pub vines: bool,
    pub can_be_cold: bool,
    pub replace_with_blackstone: bool,
    pub weight: f32,
}

impl Setup {
    /// `Setup.CODEC`.
    fn decode(value: &serde_json::Value) -> LoadResult<Setup> {
        let f = Fields::new(value)?;
        Ok(Setup {
            placement: f.required("placement", |v| {
                let name = parse_str(v)?;
                VerticalPlacement::by_name(name)
                    .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
            })?,
            air_pocket_probability: f
                .required("air_pocket_probability", |v| parse_float_range(v, 0.0, 1.0))?,
            mossiness: f.required("mossiness", |v| parse_float_range(v, 0.0, 1.0))?,
            overgrown: f.required("overgrown", parse_bool)?,
            vines: f.required("vines", parse_bool)?,
            can_be_cold: f.required("can_be_cold", parse_bool)?,
            replace_with_blackstone: f.required("replace_with_blackstone", parse_bool)?,
            weight: f.required("weight", parse_positive_float)?,
        })
    }
}

pub struct RuinedPortalStructure {
    pub settings: StructureSettings,
    setups: Vec<Setup>,
}

impl RuinedPortalStructure {
    pub fn new(settings: StructureSettings, setups: Vec<Setup>) -> RuinedPortalStructure {
        RuinedPortalStructure { settings, setups }
    }

    /// `RuinedPortalStructure.CODEC`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        let settings = StructureSettings::decode(fields, ctx)?;
        let setups = fields.required("setups", |v| {
            let setups = parse_list(v, Setup::decode)?;
            if setups.is_empty() {
                Err(LoadError::new("List must have contents"))
            } else {
                Ok(setups)
            }
        })?;
        Ok(Arc::new(RuinedPortalStructure::new(settings, setups)))
    }
}

/// `sample(random, limit)`.
fn sample(random: &mut dyn RandomSource, limit: f32) -> bool {
    if limit == 0.0 {
        false
    } else if limit == 1.0 {
        true
    } else {
        random.next_float() < limit
    }
}

/// `getRandomWithinInterval(random, minPreferred, max)`.
fn get_random_within_interval(random: &mut dyn RandomSource, min_preferred: i32, max: i32) -> i32 {
    if min_preferred < max {
        mth::random_between_inclusive(random, min_preferred, max)
    } else {
        max
    }
}

/// `findSuitableY(random, generator, verticalPlacement, airPocket, surfaceYAtCenter, ySpan, boundingBox, ..)`.
fn find_suitable_y(
    random: &mut dyn RandomSource,
    min_block_y: i32,
    columns: &[NoiseColumn],
    vertical_placement: VerticalPlacement,
    air_pocket: bool,
    surface_y_at_center: i32,
    y_span: i32,
) -> i32 {
    let min_y = min_block_y + MIN_Y_INDEX;
    let new_y = match vertical_placement {
        VerticalPlacement::InNether => {
            if air_pocket {
                mth::random_between_inclusive(random, 32, 100)
            } else if random.next_float() < 0.5 {
                mth::random_between_inclusive(random, 27, 29)
            } else {
                mth::random_between_inclusive(random, 29, 100)
            }
        }
        VerticalPlacement::InMountain => {
            let max_y = surface_y_at_center - y_span;
            get_random_within_interval(random, 70, max_y)
        }
        VerticalPlacement::Underground => {
            let max_y = surface_y_at_center - y_span;
            get_random_within_interval(random, min_y, max_y)
        }
        VerticalPlacement::PartlyBuried => {
            surface_y_at_center - y_span + mth::random_between_inclusive(random, 2, 8)
        }
        _ => surface_y_at_center,
    };
    let heightmap = if vertical_placement == VerticalPlacement::OnOceanFloor {
        HeightmapType::OceanFloorWg
    } else {
        HeightmapType::WorldSurfaceWg
    };
    let mut projected_y = new_y;
    while projected_y > min_y {
        let mut corners_on_solid_ground = 0;
        for column in columns {
            let block_state = column.get_block(projected_y);
            if heightmap.is_opaque(block_state) {
                corners_on_solid_ground += 1;
                if corners_on_solid_ground == 3 {
                    return projected_y;
                }
            }
        }
        projected_y -= 1;
    }
    projected_y
}

impl Structure for RuinedPortalStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn type_name(&self) -> &'static str {
        "ruined_portal"
    }

    fn find_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>> {
        let chosen_setup = if self.setups.len() > 1 {
            let mut total = 0.0f32;
            for s in &self.setups {
                total += s.weight;
            }
            let mut pick = context.random.next_float();
            let mut chosen = None;
            for s in &self.setups {
                pick -= s.weight / total;
                if pick < 0.0 {
                    chosen = Some(*s);
                    break;
                }
            }
            chosen
        } else {
            Some(self.setups[0])
        };
        // vanilla throws `IllegalStateException` when float rounding leaves no setup picked
        let setup = chosen_setup.expect("ruined portal setup pick");
        let air_pocket = sample(&mut context.random, setup.air_pocket_probability);
        let path = if context.random.next_float() < PROBABILITY_OF_GIANT_PORTAL {
            STRUCTURE_LOCATION_GIANT_PORTALS[context
                .random
                .next_int_bounded(STRUCTURE_LOCATION_GIANT_PORTALS.len() as i32)
                as usize]
        } else {
            STRUCTURE_LOCATION_PORTALS[context
                .random
                .next_int_bounded(STRUCTURE_LOCATION_PORTALS.len() as i32)
                as usize]
        };
        let template_location =
            Identifier::with_default_namespace(path).expect("valid ruined portal template id");
        let template = context
            .env
            .structure_template_manager
            .get_or_create(&template_location);
        let rotation = Rotation::get_random(&mut context.random);
        let mirror = if context.random.next_float() < 0.5 {
            Mirror::None
        } else {
            Mirror::FrontBack
        };
        let size = template.get_size();
        let pivot = BlockPos::new(size.x / 2, 0, size.z / 2);
        let base_position = context.chunk_pos.get_world_position();
        let bounding_box = template.get_bounding_box_with(base_position, rotation, pivot, mirror);
        let center = bounding_box.get_center();
        let surface_y = context.env.get_base_height(
            center.x(),
            center.z(),
            get_height_map_type(setup.placement),
            context.height_accessor,
        ) - 1;
        // the base columns of the bottom corners draw no randomness, so they can be read before `findSuitableY`
        let columns: Vec<NoiseColumn> = [
            (bounding_box.min_x(), bounding_box.min_z()),
            (bounding_box.max_x(), bounding_box.min_z()),
            (bounding_box.min_x(), bounding_box.max_z()),
            (bounding_box.max_x(), bounding_box.max_z()),
        ]
        .iter()
        .map(|&(x, z)| context.get_base_column(x, z))
        .collect();
        let projected_y = find_suitable_y(
            &mut context.random,
            context.height_accessor.min_y,
            &columns,
            setup.placement,
            air_pocket,
            surface_y,
            bounding_box.get_y_span(),
        );
        let origin = BlockPos::new(base_position.x(), projected_y, base_position.z());
        Some(GenerationStub::new(origin, move |builder, context| {
            let sea_level = context.env.get_sea_level();
            let biome = context.biome_resolver.get_noise_biome(
                quart_pos::from_block(origin.x()),
                quart_pos::from_block(origin.y()),
                quart_pos::from_block(origin.z()),
            );
            let properties = Properties {
                cold: setup.can_be_cold
                    && context
                        .env
                        .biomes
                        .get(biome)
                        .cold_enough_to_snow(origin, sea_level),
                mossiness: setup.mossiness,
                air_pocket,
                overgrown: setup.overgrown,
                vines: setup.vines,
                replace_with_blackstone: setup.replace_with_blackstone,
            };
            builder.add_piece(Box::new(RuinedPortalPiece::new(
                &context.env.structure_template_manager,
                origin,
                setup.placement,
                properties,
                &template_location,
                rotation,
                mirror,
                pivot,
            )));
        }))
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static RUINED_PORTAL: TypeEntry<dyn Structure> = TypeEntry {
    name: "ruined_portal",
    decode: RuinedPortalStructure::decode,
};
