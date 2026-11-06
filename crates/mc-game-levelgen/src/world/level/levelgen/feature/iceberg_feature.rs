//! `IcebergFeature`: round or elliptic icebergs floating at sea level.

use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_block_state};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{Feature, set_block};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::f64::consts::PI;
use std::sync::Arc;

pub struct IcebergFeature {
    pub state: BlockState,
}

/// `Math.pow(x, 2.0)`.
fn sq(x: f64) -> f64 {
    x * x
}

fn is_iceberg_state(state: BlockState) -> bool {
    state.is_block(Block::PACKED_ICE)
        || state.is_block(Block::SNOW_BLOCK)
        || state.is_block(Block::BLUE_ICE)
}

impl Feature for IcebergFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let origin = BlockPos::new(origin.x(), generator.get_sea_level(), origin.z());
        let snow_on_top = random.next_double() > 0.7;
        let main_block_state = self.state;
        let shape_angle = random.next_double() * 2.0 * PI;
        let shape_ellipse_a = 11 - random.next_int_bounded(5);
        let shape_ellipse_c = 3 + random.next_int_bounded(3);
        let is_ellipse = random.next_double() > 0.7;
        let mut over_water_height = if is_ellipse {
            random.next_int_bounded(6) + 6
        } else {
            random.next_int_bounded(15) + 3
        };
        if !is_ellipse && random.next_double() > 0.9 {
            over_water_height += random.next_int_bounded(19) + 7;
        }
        let under_water_height = (over_water_height + random.next_int_bounded(11)).min(18);
        let width =
            (over_water_height + random.next_int_bounded(7) - random.next_int_bounded(5)).min(11);
        let a = if is_ellipse { shape_ellipse_a } else { 11 };

        for xo in -a..a {
            for zo in -a..a {
                for y_off in 0..over_water_height {
                    let radius = if is_ellipse {
                        self.height_dependent_radius_ellipse(y_off, over_water_height, width)
                    } else {
                        self.height_dependent_radius_round(random, y_off, over_water_height, width)
                    };
                    if is_ellipse || xo < radius {
                        self.generate_iceberg_block(
                            level,
                            random,
                            origin,
                            over_water_height,
                            xo,
                            y_off,
                            zo,
                            radius,
                            a,
                            is_ellipse,
                            shape_ellipse_c,
                            shape_angle,
                            snow_on_top,
                            main_block_state,
                        );
                    }
                }
            }
        }

        self.smooth(
            level,
            origin,
            width,
            over_water_height,
            is_ellipse,
            shape_ellipse_a,
        );

        for xo in -a..a {
            for zo in -a..a {
                let mut y_off = -1;
                while y_off > -under_water_height {
                    let new_a = if is_ellipse {
                        mth::ceil_f32(
                            a as f32
                                * (1.0
                                    - sq(f64::from(y_off)) as f32
                                        / (under_water_height as f32 * 8.0)),
                        )
                    } else {
                        a
                    };
                    let radius = self.height_dependent_radius_steep(
                        random,
                        -y_off,
                        under_water_height,
                        width,
                    );
                    if xo < radius {
                        self.generate_iceberg_block(
                            level,
                            random,
                            origin,
                            under_water_height,
                            xo,
                            y_off,
                            zo,
                            radius,
                            new_a,
                            is_ellipse,
                            shape_ellipse_c,
                            shape_angle,
                            snow_on_top,
                            main_block_state,
                        );
                    }
                    y_off -= 1;
                }
            }
        }

        let do_cut_out = if is_ellipse {
            random.next_double() > 0.1
        } else {
            random.next_double() > 0.7
        };
        if do_cut_out {
            self.generate_cut_out(
                random,
                level,
                width,
                over_water_height,
                origin,
                is_ellipse,
                shape_ellipse_a,
                shape_angle,
                shape_ellipse_c,
            );
        }
        true
    }
}

impl IcebergFeature {
    fn generate_cut_out(
        &self,
        random: &mut dyn RandomSource,
        level: &mut dyn WorldGenLevel,
        width: i32,
        height: i32,
        global_origin: BlockPos,
        is_ellipse: bool,
        shape_ellipse_a: i32,
        shape_angle: f64,
        shape_ellipse_c: i32,
    ) {
        let random_sign_x = if random.next_boolean() { -1 } else { 1 };
        let random_sign_z = if random.next_boolean() { -1 } else { 1 };
        let mut x_off = random.next_int_bounded((width / 2 - 2).max(1));
        if random.next_boolean() {
            x_off = width / 2 + 1 - random.next_int_bounded((width - width / 2 - 1).max(1));
        }
        let mut z_off = random.next_int_bounded((width / 2 - 2).max(1));
        if random.next_boolean() {
            z_off = width / 2 + 1 - random.next_int_bounded((width - width / 2 - 1).max(1));
        }
        if is_ellipse {
            x_off = random.next_int_bounded((shape_ellipse_a - 5).max(1));
            z_off = x_off;
        }
        let local_origin = BlockPos::new(random_sign_x * x_off, 0, random_sign_z * z_off);
        let angle = if is_ellipse {
            shape_angle + PI / 2.0
        } else {
            random.next_double() * 2.0 * PI
        };

        for y_off in 0..height - 3 {
            let radius = self.height_dependent_radius_round(random, y_off, height, width);
            self.carve(
                radius,
                y_off,
                global_origin,
                level,
                false,
                angle,
                local_origin,
                shape_ellipse_a,
                shape_ellipse_c,
            );
        }
        let mut y_off = -1;
        while y_off > -height + random.next_int_bounded(5) {
            let radius = self.height_dependent_radius_steep(random, -y_off, height, width);
            self.carve(
                radius,
                y_off,
                global_origin,
                level,
                true,
                angle,
                local_origin,
                shape_ellipse_a,
                shape_ellipse_c,
            );
            y_off -= 1;
        }
    }

    fn carve(
        &self,
        radius: i32,
        y_off: i32,
        global_origin: BlockPos,
        level: &mut dyn WorldGenLevel,
        under_water: bool,
        angle: f64,
        local_origin: BlockPos,
        shape_ellipse_a: i32,
        shape_ellipse_c: i32,
    ) {
        let a = radius + 1 + shape_ellipse_a / 3;
        let c = (radius - 3).min(3) + shape_ellipse_c / 2 - 1;

        for xo in -a..a {
            for zo in -a..a {
                let signed_dist = self.signed_distance_ellipse(xo, zo, local_origin, a, c, angle);
                if signed_dist < 0.0 {
                    let pos = global_origin.offset(xo, y_off, zo);
                    let state = level.get_block_state(pos);
                    if is_iceberg_state(state) || state.is_block(Block::SNOW_BLOCK) {
                        if under_water {
                            set_block(level, pos, Block::WATER.default_state());
                        } else {
                            set_block(level, pos, Block::AIR.default_state());
                            self.remove_floating_snow_layer(level, pos);
                        }
                    }
                }
            }
        }
    }

    fn remove_floating_snow_layer(&self, level: &mut dyn WorldGenLevel, pos: BlockPos) {
        if level.get_block_state(pos.above()).is_block(Block::SNOW) {
            set_block(level, pos.above(), Block::AIR.default_state());
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors IcebergFeature.generateIcebergBlock"
    )]
    fn generate_iceberg_block(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        height: i32,
        xo: i32,
        y_off: i32,
        zo: i32,
        radius: i32,
        a: i32,
        is_ellipse: bool,
        shape_ellipse_c: i32,
        shape_angle: f64,
        snow_on_top: bool,
        main_block_state: BlockState,
    ) {
        let signed_dist = if is_ellipse {
            self.signed_distance_ellipse(
                xo,
                zo,
                BlockPos::ZERO,
                a,
                self.get_ellipse_c(y_off, height, shape_ellipse_c),
                shape_angle,
            )
        } else {
            self.signed_distance_circle(xo, zo, BlockPos::ZERO, radius, random)
        };
        if signed_dist < 0.0 {
            let pos = origin.offset(xo, y_off, zo);
            let compare_val = if is_ellipse {
                -0.5
            } else {
                f64::from(-6 - random.next_int_bounded(3))
            };
            if signed_dist > compare_val && random.next_double() > 0.9 {
                return;
            }
            self.set_iceberg_block(
                pos,
                level,
                random,
                height - y_off,
                height,
                is_ellipse,
                snow_on_top,
                main_block_state,
            );
        }
    }

    fn set_iceberg_block(
        &self,
        pos: BlockPos,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        h_diff: i32,
        height: i32,
        is_ellipse: bool,
        snow_on_top: bool,
        main_block_state: BlockState,
    ) {
        let state = level.get_block_state(pos);
        if state.is_air()
            || state.is_block(Block::SNOW_BLOCK)
            || state.is_block(Block::ICE)
            || state.is_block(Block::WATER)
        {
            let randomness = !is_ellipse || random.next_double() > 0.05;
            let divisor = if is_ellipse { 3 } else { 2 };
            if snow_on_top
                && !state.is_block(Block::WATER)
                && f64::from(h_diff)
                    <= f64::from(random.next_int_bounded(1.max(height / divisor)))
                        + f64::from(height) * 0.6
                && randomness
            {
                set_block(level, pos, Block::SNOW_BLOCK.default_state());
            } else {
                set_block(level, pos, main_block_state);
            }
        }
    }

    fn get_ellipse_c(&self, y_off: i32, height: i32, shape_ellipse_c: i32) -> i32 {
        let mut c = shape_ellipse_c;
        if y_off > 0 && height - y_off <= 3 {
            c = shape_ellipse_c - (4 - (height - y_off));
        }
        c
    }

    fn signed_distance_circle(
        &self,
        xo: i32,
        zo: i32,
        origin: BlockPos,
        radius: i32,
        random: &mut dyn RandomSource,
    ) -> f64 {
        let off = 10.0 * mth::clamp_f32(random.next_float(), 0.2, 0.8) / radius as f32;
        f64::from(off) + sq(f64::from(xo - origin.x())) + sq(f64::from(zo - origin.z()))
            - sq(f64::from(radius))
    }

    fn signed_distance_ellipse(
        &self,
        xo: i32,
        zo: i32,
        origin: BlockPos,
        a: i32,
        c: i32,
        angle: f64,
    ) -> f64 {
        let dx = f64::from(xo - origin.x());
        let dz = f64::from(zo - origin.z());
        sq((dx * angle.cos() - dz * angle.sin()) / f64::from(a))
            + sq((dx * angle.sin() + dz * angle.cos()) / f64::from(c))
            - 1.0
    }

    fn height_dependent_radius_round(
        &self,
        random: &mut dyn RandomSource,
        y_off: i32,
        height: i32,
        width: i32,
    ) -> i32 {
        let k = 3.5 - random.next_float();
        let mut scale = (1.0 - sq(f64::from(y_off)) as f32 / (height as f32 * k)) * width as f32;
        if height > 15 + random.next_int_bounded(5) {
            let temp_y_off = if y_off < 3 + random.next_int_bounded(6) {
                y_off / 2
            } else {
                y_off
            };
            scale = (1.0 - temp_y_off as f32 / (height as f32 * k * 0.4)) * width as f32;
        }
        mth::ceil_f32(scale / 2.0)
    }

    fn height_dependent_radius_ellipse(&self, y_off: i32, height: i32, width: i32) -> i32 {
        let scale = (1.0 - sq(f64::from(y_off)) as f32 / (height as f32 * 1.0)) * width as f32;
        mth::ceil_f32(scale / 2.0)
    }

    fn height_dependent_radius_steep(
        &self,
        random: &mut dyn RandomSource,
        y_off: i32,
        height: i32,
        width: i32,
    ) -> i32 {
        let k = 1.0 + random.next_float() / 2.0;
        let scale = (1.0 - y_off as f32 / (height as f32 * k)) * width as f32;
        mth::ceil_f32(scale / 2.0)
    }

    fn below_is_air(&self, level: &dyn WorldGenLevel, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_air()
    }

    fn smooth(
        &self,
        level: &mut dyn WorldGenLevel,
        origin: BlockPos,
        width: i32,
        height: i32,
        is_ellipse: bool,
        shape_ellipse_a: i32,
    ) {
        let a = if is_ellipse {
            shape_ellipse_a
        } else {
            width / 2
        };

        for x in -a..=a {
            for z in -a..=a {
                for y_off in 0..=height {
                    let pos = origin.offset(x, y_off, z);
                    let state = level.get_block_state(pos);
                    if is_iceberg_state(state) || state.is_block(Block::SNOW) {
                        if self.below_is_air(level, pos) {
                            set_block(level, pos, Block::AIR.default_state());
                            set_block(level, pos.above(), Block::AIR.default_state());
                        } else if is_iceberg_state(state) {
                            let sides = [
                                level.get_block_state(pos.west()),
                                level.get_block_state(pos.east()),
                                level.get_block_state(pos.north()),
                                level.get_block_state(pos.south()),
                            ];
                            let counter = sides
                                .into_iter()
                                .filter(|&side| !is_iceberg_state(side))
                                .count();
                            if counter >= 3 {
                                set_block(level, pos, Block::AIR.default_state());
                            }
                        }
                    }
                }
            }
        }
    }
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static ICEBERG: TypeEntry<dyn Feature> = TypeEntry {
    name: "iceberg",
    decode: |fields, _| {
        Ok(Arc::new(IcebergFeature {
            state: fields.required("state", parse_block_state)?,
        }))
    },
};
