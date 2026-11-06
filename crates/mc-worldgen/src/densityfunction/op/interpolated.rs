//! `InterpolatedFunction`: samples its input at cell corners and trilinearly interpolates in between.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, floor_div, floor_mod, mth};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InterpolatedFunction {
    pub input: DensityFunction,
    pub cell_size_xz: i32,
    pub cell_size_y: i32,
}

impl DfOp for InterpolatedFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        Arc::new(Sampler::new(
            self.input.compile_sampler(ctx),
            self.cell_size_xz,
            self.cell_size_y,
        ))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        if input.ptr_eq(&self.input) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Interpolated(InterpolatedFunction {
                input,
                cell_size_xz: self.cell_size_xz,
                cell_size_y: self.cell_size_y,
            }))
        }
    }
    fn range(&self) -> Interval {
        self.input.range()
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes()
    }
}

/// `InterpolatedFunction.Sampler`.
pub struct Sampler {
    input: SharedSampler,
    cell_size_xz: i32,
    cell_size_y: i32,
    cell_size_xz_inv: f32,
    cell_size_y_inv: f32,
}

impl Sampler {
    pub fn new(input: SharedSampler, cell_size_xz: i32, cell_size_y: i32) -> Sampler {
        Sampler {
            input,
            cell_size_xz,
            cell_size_y,
            cell_size_xz_inv: 1.0 / cell_size_xz as f32,
            cell_size_y_inv: 1.0 / cell_size_y as f32,
        }
    }

    fn sample_with_non_block_step(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        let block_volume = DensityVolume::new(
            volume.size_x * volume.step_block_x,
            volume.size_y * volume.step_block_y,
            volume.size_z * volume.step_block_z,
            volume.min_block_x,
            volume.min_block_y,
            volume.min_block_z,
            1,
            1,
            1,
        );
        let mut block_buffer = context.acquire_buffer(&block_volume);
        self.sample_with_block_step(context, &mut block_buffer, &block_volume);
        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                for y in 0..volume.size_y {
                    let value = block_buffer.get(block_volume.index_unchecked(
                        x * volume.step_block_x,
                        y * volume.step_block_y,
                        z * volume.step_block_z,
                    ));
                    out.set(volume.index_unchecked(x, y, z), value);
                }
            }
        }
    }

    fn sample_with_block_step(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        let min_cell_x = floor_div(volume.min_block_x, self.cell_size_xz);
        let min_cell_y = floor_div(volume.min_block_y, self.cell_size_y);
        let min_cell_z = floor_div(volume.min_block_z, self.cell_size_xz);
        let max_cell_x = floor_div(volume.max_block_x(), self.cell_size_xz);
        let max_cell_y = floor_div(volume.max_block_y(), self.cell_size_y);
        let max_cell_z = floor_div(volume.max_block_z(), self.cell_size_xz);
        let cell_count_x = max_cell_x - min_cell_x + 1;
        let cell_count_y = max_cell_y - min_cell_y + 1;
        let cell_count_z = max_cell_z - min_cell_z + 1;
        let cell_volume = DensityVolume::new(
            if floor_mod(volume.max_block_x(), self.cell_size_xz) == 0 {
                cell_count_x
            } else {
                cell_count_x + 1
            },
            if floor_mod(volume.max_block_y(), self.cell_size_y) == 0 {
                cell_count_y
            } else {
                cell_count_y + 1
            },
            if floor_mod(volume.max_block_z(), self.cell_size_xz) == 0 {
                cell_count_z
            } else {
                cell_count_z + 1
            },
            min_cell_x * self.cell_size_xz,
            min_cell_y * self.cell_size_y,
            min_cell_z * self.cell_size_xz,
            self.cell_size_xz,
            self.cell_size_y,
            self.cell_size_xz,
        );
        let mut cell_buffer = context.acquire_buffer(&cell_volume);
        self.input
            .sample_volume(context, &mut cell_buffer, &cell_volume);

        for cell_z in 0..cell_count_z {
            let next_cell_z = (cell_z + 1).min(cell_volume.size_z - 1);
            for cell_x in 0..cell_count_x {
                let next_cell_x = (cell_x + 1).min(cell_volume.size_x - 1);
                let at = |x, y, z| cell_buffer.get(cell_volume.index_unchecked(x, y, z));
                let mut v000 = at(cell_x, 0, cell_z);
                let mut v100 = at(next_cell_x, 0, cell_z);
                let mut v001 = at(cell_x, 0, next_cell_z);
                let mut v101 = at(next_cell_x, 0, next_cell_z);
                for cell_y in 0..cell_count_y {
                    let next_cell_y = (cell_y + 1).min(cell_volume.size_y - 1);
                    let v010 = at(cell_x, next_cell_y, cell_z);
                    let v110 = at(next_cell_x, next_cell_y, cell_z);
                    let v011 = at(cell_x, next_cell_y, next_cell_z);
                    let v111 = at(next_cell_x, next_cell_y, next_cell_z);
                    self.fill_cell(
                        out,
                        volume,
                        &cell_volume,
                        (cell_x, cell_y, cell_z),
                        [v000, v100, v010, v110, v001, v101, v011, v111],
                    );
                    v000 = v010;
                    v100 = v110;
                    v001 = v011;
                    v101 = v111;
                }
            }
        }
    }

    /// `fillCell`; `corners` are `[v000, v100, v010, v110, v001, v101, v011, v111]` (`vXYZ`).
    fn fill_cell(
        &self,
        out: &mut DensityBuffer,
        output_volume: &DensityVolume,
        cell_volume: &DensityVolume,
        (cell_x, cell_y, cell_z): (i32, i32, i32),
        [v000, v100, v010, v110, v001, v101, v011, v111]: [f32; 8],
    ) {
        let cell_output_x = cell_volume.block_x(cell_x) - output_volume.min_block_x;
        let cell_output_y = cell_volume.block_y(cell_y) - output_volume.min_block_y;
        let cell_output_z = cell_volume.block_z(cell_z) - output_volume.min_block_z;
        let x0 = 0.max(-cell_output_x);
        let y0 = 0.max(-cell_output_y);
        let z0 = 0.max(-cell_output_z);
        let x1 = self.cell_size_xz.min(output_volume.size_x - cell_output_x) - 1;
        let y1 = self.cell_size_y.min(output_volume.size_y - cell_output_y) - 1;
        let z1 = self.cell_size_xz.min(output_volume.size_z - cell_output_z) - 1;

        for z in z0..=z1 {
            let output_z = cell_output_z + z;
            let alpha_z = z as f32 * self.cell_size_xz_inv;
            let v00_ = mth::lerp(alpha_z, v000, v001);
            let v01_ = mth::lerp(alpha_z, v010, v011);
            let v10_ = mth::lerp(alpha_z, v100, v101);
            let v11_ = mth::lerp(alpha_z, v110, v111);

            for x in x0..=x1 {
                let output_x = cell_output_x + x;
                let alpha_x = x as f32 * self.cell_size_xz_inv;
                let v_0_ = mth::lerp(alpha_x, v00_, v10_);
                let v_1_ = mth::lerp(alpha_x, v01_, v11_);
                let value_step = (v_1_ - v_0_) * self.cell_size_y_inv;
                let mut value = v_0_ + value_step * y0 as f32;
                let mut output_index =
                    output_volume.index_unchecked(output_x, cell_output_y + y0, output_z);

                #[allow(
                    clippy::explicit_counter_loop,
                    reason = "output_index starts at a computed offset, not at the loop start"
                )]
                for _ in y0..=y1 {
                    out.set(output_index, value);
                    output_index += 1;
                    value += value_step;
                }
            }
        }
    }
}

impl DensitySampler for Sampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        if (volume.step_block_x == self.cell_size_xz || volume.size_x == 1)
            && (volume.step_block_y == self.cell_size_y || volume.size_y == 1)
            && (volume.step_block_z == self.cell_size_xz || volume.size_z == 1)
            && floor_mod(volume.min_block_x, self.cell_size_xz) == 0
            && floor_mod(volume.min_block_y, self.cell_size_y) == 0
            && floor_mod(volume.min_block_z, self.cell_size_xz) == 0
        {
            self.input.sample_volume(context, out, volume);
        } else if volume.step_block_x == 1 && volume.step_block_y == 1 && volume.step_block_z == 1 {
            self.sample_with_block_step(context, out, volume);
        } else {
            self.sample_with_non_block_step(context, out, volume);
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let x_in_cell = floor_mod(block_x, self.cell_size_xz);
        let y_in_cell = floor_mod(block_y, self.cell_size_y);
        let z_in_cell = floor_mod(block_z, self.cell_size_xz);
        if x_in_cell == 0 && y_in_cell == 0 && z_in_cell == 0 {
            return self.input.sample_value(context, block_x, block_y, block_z);
        }
        let volume = DensityVolume::new(
            2,
            2,
            2,
            block_x - x_in_cell,
            block_y - y_in_cell,
            block_z - z_in_cell,
            self.cell_size_xz,
            self.cell_size_y,
            self.cell_size_xz,
        );
        let mut input_buffer = context.acquire_buffer(&volume);
        self.input
            .sample_volume(context, &mut input_buffer, &volume);
        let at = |x, y, z| input_buffer.get(volume.index_unchecked(x, y, z));
        mth::lerp3(
            x_in_cell as f32 / self.cell_size_xz as f32,
            y_in_cell as f32 / self.cell_size_y as f32,
            z_in_cell as f32 / self.cell_size_xz as f32,
            at(0, 0, 0),
            at(1, 0, 0),
            at(0, 1, 0),
            at(1, 1, 0),
            at(0, 0, 1),
            at(1, 0, 1),
            at(0, 1, 1),
            at(1, 1, 1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::op::slice::test_support::{FnSampler, fn_sampler};

    /// Same expression (and evaluation order) as `f` in `tests/oracle/OpsOracle.java`.
    fn oracle_f(x: i32, y: i32, z: i32) -> f32 {
        x as f32 * 0.37 + (y * y) as f32 * 0.011 - z as f32 * 0.21 + (x * z) as f32 * 0.003
    }

    type OracleInput = Arc<FnSampler<fn(i32, i32, i32) -> f32>>;

    fn oracle_sampler() -> (OracleInput, Sampler) {
        let input = fn_sampler(oracle_f as fn(i32, i32, i32) -> f32);
        let sampler = Sampler::new(input.clone(), 4, 8);
        (input, sampler)
    }

    fn fixture_bits(prefix: &str) -> impl Iterator<Item = Vec<&str>> {
        include_str!("../../../tests/fixtures/ops.txt")
            .lines()
            .filter(move |l| l.starts_with(prefix))
            .map(|l| l.split(' ').skip(1).collect())
    }

    fn sample(sampler: &Sampler, volume: &DensityVolume) -> DensityBuffer {
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&SamplerContext::empty_uncached(), &mut out, volume);
        out
    }

    #[test]
    fn point_samples_match_vanilla_bits() {
        let (_, sampler) = oracle_sampler();
        let context = SamplerContext::empty_uncached();
        let mut count = 0;
        for fields in fixture_bits("point ") {
            let [x, y, z] = [0, 1, 2].map(|i| fields[i].parse::<i32>().unwrap());
            let expected = u32::from_str_radix(fields[3], 16).unwrap();
            assert_eq!(
                sampler.sample_value(&context, x, y, z).to_bits(),
                expected,
                "({x}, {y}, {z})"
            );
            count += 1;
        }
        assert_eq!(count, 6);
    }

    #[test]
    fn point_sample_inside_a_cell_is_the_lerp_of_the_eight_corners() {
        let (input, sampler) = oracle_sampler();
        let context = SamplerContext::empty_uncached();
        // Cell origin (4, 0, 4), offset (1, 3, 3) in a 4 x 8 x 4 cell.
        let corner = |dx, dy, dz| oracle_f(4 + 4 * dx, 8 * dy, 4 + 4 * dz);
        let expected = mth::lerp3(
            0.25,
            0.375,
            0.75,
            corner(0, 0, 0),
            corner(1, 0, 0),
            corner(0, 1, 0),
            corner(1, 1, 0),
            corner(0, 0, 1),
            corner(1, 0, 1),
            corner(0, 1, 1),
            corner(1, 1, 1),
        );
        assert_eq!(
            sampler.sample_value(&context, 5, 3, 7).to_bits(),
            expected.to_bits()
        );
        assert_eq!(
            *input.volumes.lock().unwrap(),
            [DensityVolume::new(2, 2, 2, 4, 0, 4, 4, 8, 4)]
        );
        // A cell corner is sampled directly, without a volume.
        assert_eq!(sampler.sample_value(&context, 8, 8, 4), oracle_f(8, 8, 4));
        assert_eq!(input.volumes.lock().unwrap().len(), 1);
    }

    fn check_volume_against_fixture(name: &str, volume: &DensityVolume) -> Vec<DensityVolume> {
        let (input, sampler) = oracle_sampler();
        let out = sample(&sampler, volume);
        let mut count = 0;
        for fields in fixture_bits("volume ").filter(|f| f[0] == name) {
            let index: usize = fields[1].parse().unwrap();
            let expected = u32::from_str_radix(fields[2], 16).unwrap();
            assert_eq!(out.get(index).to_bits(), expected, "{name}[{index}]");
            count += 1;
        }
        assert_eq!(count, volume.size());
        input.volumes.lock().unwrap().clone()
    }

    #[test]
    fn block_step_volume_matches_vanilla_and_samples_the_cell_corners() {
        let volume = DensityVolume::with_unit_step(6, 7, 5, -3, -2, 1);
        let sampled = check_volume_against_fixture("block", &volume);
        assert_eq!(sampled, [DensityVolume::new(3, 3, 3, -4, -8, 0, 4, 8, 4)]);
    }

    #[test]
    fn non_block_step_volume_matches_vanilla_and_goes_through_a_block_volume() {
        let volume = DensityVolume::new(3, 3, 3, -5, -3, 2, 2, 5, 3);
        let sampled = check_volume_against_fixture("nonblock", &volume);
        // Block volume is 6 x 15 x 9 from (-5, -3, 2): cells x -2..=0, y -1..=1, z 0..=2, plus one corner
        // layer along y and z whose maxima (11, 10) are not cell-aligned.
        assert_eq!(sampled, [DensityVolume::new(3, 4, 4, -8, -8, 0, 4, 8, 4)]);
    }

    #[test]
    fn cell_aligned_volume_is_passed_straight_to_the_input() {
        let volume = DensityVolume::new(3, 2, 3, 8, 16, -4, 4, 8, 4);
        let sampled = check_volume_against_fixture("aligned", &volume);
        assert_eq!(sampled, [volume]);
    }

    #[test]
    fn single_column_volume_matches_vanilla() {
        let volume = DensityVolume::with_unit_step(1, 9, 1, 4, 0, 4);
        check_volume_against_fixture("flat", &volume);
    }

    #[test]
    fn linear_input_is_reproduced_exactly_by_the_volume_paths() {
        // Power-of-two cells make every alpha exact, so interpolating a linear function is lossless.
        let lin = |x: i32, y: i32, z: i32| (x + 2 * y + 4 * z) as f32;
        let sampler = Sampler::new(fn_sampler(lin), 4, 8);
        for volume in [
            DensityVolume::with_unit_step(9, 11, 6, -6, -5, 3),
            DensityVolume::new(4, 3, 3, -7, 2, 5, 3, 4, 2),
        ] {
            let out = sample(&sampler, &volume);
            for z in 0..volume.size_z {
                for x in 0..volume.size_x {
                    for y in 0..volume.size_y {
                        let expected = lin(volume.block_x(x), volume.block_y(y), volume.block_z(z));
                        assert_eq!(out.get(volume.index_unchecked(x, y, z)), expected);
                    }
                }
            }
        }
    }

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let function = DensityFunction::constant(1.0).interpolated(4, 8);
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let rewritten =
            function.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(2.0));
        assert_eq!(rewritten, DensityFunction::constant(2.0).interpolated(4, 8));
    }
}
