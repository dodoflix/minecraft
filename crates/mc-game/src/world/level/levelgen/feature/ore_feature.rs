//! `OreFeature`: blobs of ore along a random segment, written straight into the chunk sections (`BulkSectionAccess`).

use super::Feature;
use super::abstract_ore_feature::AbstractOreFeature;
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct OreFeature(pub AbstractOreFeature);

impl Feature for OreFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let size = self.0.size;
        let dir = random.next_float() * mth::PI;
        let spread_xy = size as f32 / 8.0;
        let max_radius = mth::ceil_f32((size as f32 / 16.0 * 2.0 + 1.0) / 2.0);
        let (sin, cos) = (f64::from(dir).sin(), f64::from(dir).cos());
        let spread = f64::from(spread_xy);
        let x0 = f64::from(origin.x()) + sin * spread;
        let x1 = f64::from(origin.x()) - sin * spread;
        let z0 = f64::from(origin.z()) + cos * spread;
        let z1 = f64::from(origin.z()) - cos * spread;
        let y0 = f64::from(origin.y() + random.next_int_bounded(3) - 2);
        let y1 = f64::from(origin.y() + random.next_int_bounded(3) - 2);
        let x_start = origin.x() - mth::ceil_f32(spread_xy) - max_radius;
        let y_start = origin.y() - 2 - max_radius;
        let z_start = origin.z() - mth::ceil_f32(spread_xy) - max_radius;
        let size_xz = 2 * (mth::ceil_f32(spread_xy) + max_radius);
        let size_y = 2 * (2 + max_radius);

        for xprobe in x_start..=x_start + size_xz {
            for zprobe in z_start..=z_start + size_xz {
                if y_start <= level.get_height_at(HeightmapType::OceanFloorWg, xprobe, zprobe) {
                    return self.do_place(level, random, [x0, x1, z0, z1, y0, y1], [x_start, y_start, z_start], size_xz, size_y);
                }
            }
        }
        false
    }
}

impl OreFeature {
    /// `doPlace`; `ends` = `[x0, x1, z0, z1, y0, y1]`, `start` = `[xStart, yStart, zStart]`.
    fn do_place(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, ends: [f64; 6], start: [i32; 3], size_xz: i32, size_y: i32) -> bool {
        let [x0, x1, z0, z1, y0, y1] = ends;
        let [x_start, y_start, z_start] = start;
        let size = self.0.size;
        let n = size as usize;
        let mut placed = 0;
        let mut tested: Vec<bool> = Vec::new();
        let mut data = vec![0.0f64; n * 4];

        for i in 0..n {
            let step = i as f32 / size as f32;
            let step_d = f64::from(step);
            let xx = mth::lerp_f64(step_d, x0, x1);
            let yy = mth::lerp_f64(step_d, y0, y1);
            let zz = mth::lerp_f64(step_d, z0, z1);
            let ss = random.next_double() * f64::from(size) / 16.0;
            let r = (f64::from(mth::sin(f64::from(mth::PI * step)) + 1.0) * ss + 1.0) / 2.0;
            data[i * 4] = xx;
            data[i * 4 + 1] = yy;
            data[i * 4 + 2] = zz;
            data[i * 4 + 3] = r;
        }

        for i1 in 0..n.saturating_sub(1) {
            if data[i1 * 4 + 3] <= 0.0 {
                continue;
            }
            for i2 in i1 + 1..n {
                if data[i2 * 4 + 3] <= 0.0 {
                    continue;
                }
                let dx = data[i1 * 4] - data[i2 * 4];
                let dy = data[i1 * 4 + 1] - data[i2 * 4 + 1];
                let dz = data[i1 * 4 + 2] - data[i2 * 4 + 2];
                let dr = data[i1 * 4 + 3] - data[i2 * 4 + 3];
                if dr * dr > dx * dx + dy * dy + dz * dz {
                    if dr > 0.0 {
                        data[i2 * 4 + 3] = -1.0;
                    } else {
                        data[i1 * 4 + 3] = -1.0;
                    }
                }
            }
        }

        for i in 0..n {
            let r = data[i * 4 + 3];
            if r < 0.0 {
                continue;
            }
            let (xx, yy, zz) = (data[i * 4], data[i * 4 + 1], data[i * 4 + 2]);
            let x_min = mth::floor_f64(xx - r).max(x_start);
            let y_min = mth::floor_f64(yy - r).max(y_start);
            let z_min = mth::floor_f64(zz - r).max(z_start);
            let x_max = mth::floor_f64(xx + r).max(x_min);
            let y_max = mth::floor_f64(yy + r).max(y_min);
            let z_max = mth::floor_f64(zz + r).max(z_min);

            for x in x_min..=x_max {
                let xd = (f64::from(x) + 0.5 - xx) / r;
                if !(xd * xd < 1.0) {
                    continue;
                }
                for y in y_min..=y_max {
                    let yd = (f64::from(y) + 0.5 - yy) / r;
                    if !(xd * xd + yd * yd < 1.0) {
                        continue;
                    }
                    for z in z_min..=z_max {
                        let zd = (f64::from(z) + 0.5 - zz) / r;
                        if xd * xd + yd * yd + zd * zd < 1.0 && !level.is_outside_build_height(y) {
                            let index = (x - x_start + (y - y_start) * size_xz + (z - z_start) * size_xz * size_y) as usize;
                            if tested.get(index).copied().unwrap_or(false) {
                                continue;
                            }
                            if index >= tested.len() {
                                tested.resize(index + 1, false);
                            }
                            tested[index] = true;
                            let ore_pos = BlockPos::new(x, y, z);
                            if level.ensure_can_write(ore_pos) {
                                let block_state = level.get_block_state_in_section(ore_pos);
                                for target_state in &self.0.target_states {
                                    let can_place =
                                        self.0.can_place_ore(block_state, &|p| level.get_block_state_in_section(p), random, target_state, ore_pos);
                                    if can_place {
                                        level.set_block_state_in_section(ore_pos, target_state.state);
                                        placed += 1;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        placed > 0
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static ORE: TypeEntry<dyn Feature> = TypeEntry { name: "ore", decode: |fields, ctx| Ok(Arc::new(OreFeature(AbstractOreFeature::decode(fields, ctx)?))) };
