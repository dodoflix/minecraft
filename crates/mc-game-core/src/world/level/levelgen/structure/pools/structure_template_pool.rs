//! `StructureTemplatePool` (`worldgen/template_pool`) and `StructureTemplatePool.Projection`.

use super::empty_pool_element::empty_pool_element;
use super::structure_pool_element::{ElementCodecCtx, StructurePoolElement, decode_element};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::feature::tree::shuffle;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::{DecodeCtx, TypedRegistry, load_registry};
use crate::world::level::levelgen::structure::templatesystem::StructureTemplateManager;
use crate::world::level::levelgen::structure::templatesystem::gravity_processor::GravityProcessor;
use crate::world::level::levelgen::structure::templatesystem::structure_processor::StructureProcessor;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_world::heightmap::HeightmapType;
use serde_json::Value;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, LazyLock};

pub const TEMPLATE_POOL_REGISTRY: &str = "worldgen/template_pool";

/// `Pools.EMPTY`.
pub fn empty_pool_id() -> Identifier {
    Identifier::parse("minecraft:empty").expect("valid identifier")
}

/// `StructureTemplatePool.Projection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Projection {
    TerrainMatching,
    Rigid,
}

static TERRAIN_MATCHING_PROCESSORS: LazyLock<Vec<Arc<dyn StructureProcessor>>> =
    LazyLock::new(|| {
        vec![Arc::new(GravityProcessor {
            heightmap: HeightmapType::WorldSurfaceWg,
            offset: -1,
        })]
    });

impl Projection {
    pub const fn get_name(self) -> &'static str {
        match self {
            Projection::TerrainMatching => "terrain_matching",
            Projection::Rigid => "rigid",
        }
    }

    /// `byName(name)` (`CODEC.byName`).
    pub fn by_name(name: &str) -> Option<Projection> {
        [Projection::TerrainMatching, Projection::Rigid]
            .into_iter()
            .find(|p| p.get_name() == name)
    }

    /// `getProcessors()`.
    pub fn get_processors(self) -> &'static [Arc<dyn StructureProcessor>] {
        match self {
            Projection::TerrainMatching => &TERRAIN_MATCHING_PROCESSORS,
            Projection::Rigid => &[],
        }
    }
}

const SIZE_UNSET: i32 = i32::MIN;

pub struct StructureTemplatePool {
    /// `rawTemplates`: the elements with their weights.
    raw_templates: Vec<(Arc<dyn StructurePoolElement>, i32)>,
    /// `templates`: each element repeated `weight` times.
    templates: Vec<Arc<dyn StructurePoolElement>>,
    fallback: Holder<StructureTemplatePool>,
    max_size: AtomicI32,
}

impl StructureTemplatePool {
    /// `new StructureTemplatePool(fallback, templates)`.
    pub fn new(
        fallback: Holder<StructureTemplatePool>,
        raw_templates: Vec<(Arc<dyn StructurePoolElement>, i32)>,
    ) -> StructureTemplatePool {
        let mut templates = Vec::new();
        for (element, weight) in &raw_templates {
            for _ in 0..*weight {
                templates.push(element.clone());
            }
        }
        StructureTemplatePool {
            raw_templates,
            templates,
            fallback,
            max_size: AtomicI32::new(SIZE_UNSET),
        }
    }

    /// `getMaxSize(manager)`.
    pub fn get_max_size(&self, manager: &StructureTemplateManager) -> i32 {
        let cached = self.max_size.load(Ordering::Relaxed);
        if cached != SIZE_UNSET {
            return cached;
        }
        let max = self
            .templates
            .iter()
            .filter(|t| !t.is_empty_element())
            .map(|t| {
                t.get_bounding_box(manager, BlockPos::ZERO, Rotation::None)
                    .get_y_span()
            })
            .max()
            .unwrap_or(0);
        self.max_size.store(max, Ordering::Relaxed);
        max
    }

    /// `getTemplates()`.
    pub fn get_templates(&self) -> &[(Arc<dyn StructurePoolElement>, i32)] {
        &self.raw_templates
    }

    pub fn get_fallback(&self) -> &Holder<StructureTemplatePool> {
        &self.fallback
    }

    /// `getRandomTemplate(random)`.
    pub fn get_random_template(
        &self,
        random: &mut dyn RandomSource,
    ) -> Arc<dyn StructurePoolElement> {
        if self.templates.is_empty() {
            empty_pool_element()
        } else {
            self.templates[random.next_int_bounded(self.templates.len() as i32) as usize].clone()
        }
    }

    /// `getShuffledTemplates(random)`.
    pub fn get_shuffled_templates(
        &self,
        random: &mut dyn RandomSource,
    ) -> Vec<Arc<dyn StructurePoolElement>> {
        let mut copy = self.templates.clone();
        shuffle(&mut copy, random);
        copy
    }

    /// `size()`.
    pub fn size(&self) -> usize {
        self.templates.len()
    }

    /// `StructureTemplatePool.DIRECT_CODEC`.
    pub fn decode_direct(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<StructureTemplatePool> {
        let fields = Fields::new(value)?;
        let fallback = fields.required("fallback", |v| ctx.template_pool(v))?;
        let element_ctx = ElementCodecCtx::Json(ctx);
        let elements = fields.required("elements", |v| {
            parse_list(v, |entry| {
                let entry = Fields::new(entry)?;
                let element = entry.required("element", |v| decode_element(v, &element_ctx))?;
                let weight = entry.required("weight", |v| parse_int_range(v, 1, 150))?;
                Ok((element, weight))
            })
        })?;
        Ok(StructureTemplatePool::new(fallback, elements))
    }
}

impl DecodeCtx<'_> {
    /// `StructureTemplatePool.CODEC` (`RegistryCodecs.holder(TEMPLATE_POOL, DIRECT_CODEC)`).
    pub fn template_pool(&self, value: &Value) -> LoadResult<Holder<StructureTemplatePool>> {
        self.holder_or_direct(TEMPLATE_POOL_REGISTRY, value, |v| {
            StructureTemplatePool::decode_direct(v, self).map(Arc::new)
        })
    }
}

/// Decodes every `worldgen/template_pool` entry of the data source behind `ctx` and binds the holders.
pub fn load_template_pools(
    ctx: &DecodeCtx<'_>,
) -> LoadResult<TypedRegistry<StructureTemplatePool>> {
    let registry = load_registry(ctx, TEMPLATE_POOL_REGISTRY, |v, c| {
        StructureTemplatePool::decode_direct(v, c).map(Arc::new)
    })?;
    ctx.cells
        .check_all_bound::<StructureTemplatePool>(TEMPLATE_POOL_REGISTRY)?;
    Ok(registry)
}
