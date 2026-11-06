//! JSON codecs of the density function types: `DensityFunction.CODEC` (holder reference | direct),
//! `DensityFunctions.DIRECT_CODEC` (constant | `type`-dispatched object), each type's `MapCodec` and
//! `CubicSpline.codec`. Primitive parsers mirror DFU's `Codec.INT`/`FLOAT`/`DOUBLE`/`xxxRange`.

use serde_json::{Map, Value};

use mc_core::ident::Identifier;

use super::Axis;
use super::cubic_spline::{CubicSpline, Multipoint};
use super::function::{DensityFunction, DfNode, HolderHolder, NoiseHolder};
use super::generator::{
    ConstantFunction, DistanceMetric, DistanceToPointFunction, EndIslandFunction, GradientFunction,
    NoiseFunction, ShiftKind, ShiftNoiseFunction, SimpleDensityFunction, TilingMode,
};
use super::op::{
    BinaryFunction, BinaryType, BlendDensityFunction, CacheFunction, ClampFunction,
    FindTopSurfaceFunction, InterpolatedFunction, IntervalSelectFunction, LerpFunction,
    PowFunction, RangeChoiceFunction, RoundFunction, RoundType, SliceFunction, SplineFunction,
    UnaryFunction, UnaryType,
};
use crate::data::{LoadError, WorldgenRegistries};
use crate::noise_settings::{DIMENSION_MAX_Y, DIMENSION_MIN_Y};
use crate::synth::BlendedNoise;

pub type LoadResult<T> = Result<T, LoadError>;

/// `DensityFunctions.MAX_REASONABLE_NOISE_VALUE`.
const MAX_REASONABLE_NOISE_VALUE: f32 = 1000000.0;

// ---- primitive codecs -----------------------------------------------------------------------------

pub fn parse_f64(value: &Value) -> LoadResult<f64> {
    match value {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| LoadError::new("number out of range")),
        _ => Err(LoadError::new("expected number")),
    }
}

/// `Codec.FLOAT`.
pub fn parse_f32(value: &Value) -> LoadResult<f32> {
    parse_f64(value).map(|v| v as f32)
}

/// `Codec.doubleRange(min, max)`.
pub fn parse_double_range(value: &Value, min: f64, max: f64) -> LoadResult<f64> {
    let v = parse_f64(value)?;
    if v >= min && v <= max {
        Ok(v)
    } else {
        Err(LoadError::new(format!(
            "Value {v} outside of range [{min}:{max}]"
        )))
    }
}

/// `DensityFunctions.NOISE_VALUE_CODEC`.
pub fn parse_noise_value(value: &Value) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if (-MAX_REASONABLE_NOISE_VALUE..=MAX_REASONABLE_NOISE_VALUE).contains(&v) {
        Ok(v)
    } else {
        Err(LoadError::new(format!(
            "Value {v} outside of range [-1000000.0:1000000.0]"
        )))
    }
}

/// `Codec.INT` (a number's `intValue()`: integers wrap, fractions truncate, out-of-range doubles keep the
/// low 32 bits of their integer part like `BigDecimal.intValue()`).
pub fn parse_int(value: &Value) -> LoadResult<i32> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .map(|v| v as i32)
            .or_else(|| n.as_f64().map(f64_int_value))
            .ok_or_else(|| LoadError::new("number out of range")),
        _ => Err(LoadError::new("expected number")),
    }
}

fn f64_int_value(v: f64) -> i32 {
    // |v| >= 2^100 is a multiple of 2^48, so its low 32 bits are zero (also keeps `as i128` exact).
    if v.abs() >= 2f64.powi(100) {
        0
    } else {
        v.trunc() as i128 as i32
    }
}

/// `Codec.intRange(min, max)`.
pub fn parse_int_range(value: &Value, min: i32, max: i32) -> LoadResult<i32> {
    let v = parse_int(value)?;
    if (min..=max).contains(&v) {
        Ok(v)
    } else {
        Err(LoadError::new(format!(
            "Value {v} outside of range [{min}:{max}]"
        )))
    }
}

/// `ExtraCodecs.POSITIVE_INT`.
pub fn parse_positive_int(value: &Value) -> LoadResult<i32> {
    let v = parse_int(value)?;
    if v >= 1 {
        Ok(v)
    } else {
        Err(LoadError::new(format!("Value must be positive: {v}")))
    }
}

thread_local! {
    static NUMERIC_BOOLEANS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with numbers accepted as booleans (`NbtOps.getBooleanValue` reads any numeric tag as one): for JSON that
/// was converted from NBT, where a boolean is a byte. `JsonOps` itself accepts booleans only.
pub fn with_numeric_booleans<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            NUMERIC_BOOLEANS.set(self.0);
        }
    }
    let _restore = Restore(NUMERIC_BOOLEANS.replace(true));
    f()
}

pub fn parse_bool(value: &Value) -> LoadResult<bool> {
    value
        .as_bool()
        .or_else(|| {
            NUMERIC_BOOLEANS
                .get()
                .then(|| value.as_f64().map(|n| n != 0.0))
                .flatten()
        })
        .ok_or_else(|| LoadError::new("expected boolean"))
}

pub fn parse_str(value: &Value) -> LoadResult<&str> {
    value
        .as_str()
        .ok_or_else(|| LoadError::new("expected string"))
}

pub fn parse_identifier(value: &Value) -> LoadResult<Identifier> {
    Identifier::read(parse_str(value)?).map_err(LoadError::new)
}

/// `Codec.listOf()`: parses every element with `parse`, tagging errors with `[index]`.
pub fn parse_list<'a, T>(
    value: &'a Value,
    mut parse: impl FnMut(&'a Value) -> LoadResult<T>,
) -> LoadResult<Vec<T>> {
    let array = value
        .as_array()
        .ok_or_else(|| LoadError::new("expected list"))?;
    array
        .iter()
        .enumerate()
        .map(|(i, element)| parse(element).map_err(|e| e.at_index(i)))
        .collect()
}

// ---- record fields -------------------------------------------------------------------------------

/// A JSON object read through `fieldOf` / `optionalFieldOf`.
pub struct Fields<'a> {
    map: &'a Map<String, Value>,
}

impl<'a> Fields<'a> {
    pub fn new(value: &'a Value) -> LoadResult<Fields<'a>> {
        value
            .as_object()
            .map(|map| Fields { map })
            .ok_or_else(|| LoadError::new("expected object"))
    }

    /// `JsonOps`' `MapLike.get`: a JSON `null` field counts as absent.
    pub fn get(&self, name: &str) -> Option<&'a Value> {
        self.map.get(name).filter(|value| !value.is_null())
    }

    /// `fieldOf(name)`.
    pub fn required<T>(
        &self,
        name: &str,
        parse: impl FnOnce(&'a Value) -> LoadResult<T>,
    ) -> LoadResult<T> {
        match self.get(name) {
            Some(value) => parse(value).map_err(|e| e.at(name)),
            None => Err(LoadError::new("missing required field").at(name)),
        }
    }

    /// `optionalFieldOf(name)`: a present but malformed value is an error.
    pub fn optional<T>(
        &self,
        name: &str,
        parse: impl FnOnce(&'a Value) -> LoadResult<T>,
    ) -> LoadResult<Option<T>> {
        self.get(name)
            .map(|value| parse(value).map_err(|e| e.at(name)))
            .transpose()
    }

    fn df(&self, name: &str, registries: &WorldgenRegistries) -> LoadResult<DensityFunction> {
        self.required(name, |v| density_function_from_json(v, registries))
    }
}

// ---- DensityFunction.CODEC -------------------------------------------------------------------------

/// `DensityFunction.CODEC`: a string is a reference to a registered function, anything else is parsed
/// by [`density_function_direct_from_json`].
pub fn density_function_from_json(
    value: &Value,
    registries: &WorldgenRegistries,
) -> LoadResult<DensityFunction> {
    match value {
        Value::String(_) => {
            let id = parse_identifier(value)?;
            let function = registries.density_function(&id)?;
            Ok(DensityFunction::new(DfNode::HolderHolder(HolderHolder {
                function,
            })))
        }
        Value::Number(_) | Value::Object(_) => density_function_direct_from_json(value, registries),
        _ => Err(LoadError::new("expected number, string or object")),
    }
}

/// `DensityFunctions.DIRECT_CODEC`: a number is a constant, an object is dispatched on its `type`.
pub fn density_function_direct_from_json(
    value: &Value,
    registries: &WorldgenRegistries,
) -> LoadResult<DensityFunction> {
    match value {
        Value::Number(_) => Ok(DensityFunction::constant(parse_noise_value(value)?)),
        Value::Object(_) => {
            let fields = Fields::new(value)?;
            let ty = fields.required("type", parse_identifier)?;
            if ty.namespace() != "minecraft" {
                return Err(LoadError::new(format!("Unknown registry key: {ty}")).at("type"));
            }
            typed_from_json(ty.path(), &fields, registries)
        }
        _ => Err(LoadError::new("expected number or object")),
    }
}

fn noise_holder(value: &Value, registries: &WorldgenRegistries) -> LoadResult<NoiseHolder> {
    match value {
        Value::String(_) => registries.noise(&parse_identifier(value)?),
        // Vanilla stores an unregistered `Holder.Direct` that fails when used (no key to seed from).
        Value::Object(_) => Err(LoadError::new("inline noise definitions are not supported")),
        _ => Err(LoadError::new("expected string")),
    }
}

/// The `MapCodec` registered under `minecraft:<path>` in `DensityFunctionType`.
fn typed_from_json(
    path: &str,
    f: &Fields,
    registries: &WorldgenRegistries,
) -> LoadResult<DensityFunction> {
    let node = match path {
        "constant" => DfNode::Constant(ConstantFunction {
            value: f.required("value", parse_noise_value)?,
        }),
        "noise" => DfNode::Noise(NoiseFunction {
            noise: f.required("noise", |v| noise_holder(v, registries))?,
            xz_scale: f.required("xz_scale", parse_f64)?,
            y_scale: f.required("y_scale", parse_f64)?,
            shift_x: optional_df(f, "shift_x", registries, DensityFunction::zero)?,
            shift_y: optional_df(f, "shift_y", registries, DensityFunction::zero)?,
            shift_z: optional_df(f, "shift_z", registries, DensityFunction::zero)?,
        }),
        "end_outer_islands" => DfNode::EndIslands(EndIslandFunction),
        "distance_to_point" => DfNode::DistanceToPoint(DistanceToPointFunction {
            point: f.required("point", parse_vec3i)?,
            metric: f.required("metric", |v| {
                let name = parse_str(v)?;
                DistanceMetric::from_name(name)
                    .ok_or_else(|| LoadError::new(format!("Unknown element name: {name}")))
            })?,
        }),
        "gradient" => {
            let tiling = match f.optional("tiling", parse_str)? {
                None => TilingMode::ClampToEdge,
                Some(name) => TilingMode::from_name(name).ok_or_else(|| {
                    LoadError::new(format!("Unknown element name: {name}")).at("tiling")
                })?,
            };
            let from_coordinate = f.required("from_coordinate", parse_int)?;
            let to_coordinate = f.required("to_coordinate", parse_int)?;
            if from_coordinate == to_coordinate {
                return Err(LoadError::new(
                    "from_coordinate cannot be equal to to_coordinate",
                ));
            }
            DfNode::Gradient(GradientFunction {
                axis: f.required("axis", parse_axis)?,
                tiling,
                from_coordinate,
                to_coordinate,
                from_value: f.required("from_value", parse_noise_value)?,
                to_value: f.required("to_value", parse_noise_value)?,
            })
        }
        "shift_a" | "shift_b" | "shift" => DfNode::Shift(ShiftNoiseFunction {
            kind: match path {
                "shift_a" => ShiftKind::ShiftA,
                "shift_b" => ShiftKind::ShiftB,
                _ => ShiftKind::Shift,
            },
            offset_noise: f.required("noise", |v| noise_holder(v, registries))?,
        }),
        "pow" => DfNode::Pow(PowFunction {
            base: f.df("base", registries)?,
            exponent: f.df("exponent", registries)?,
        }),
        "spline" => DfNode::Spline(SplineFunction {
            spline: f.required("spline", |v| spline_from_json(v, registries))?,
        }),
        "lerp" => DfNode::Lerp(LerpFunction {
            alpha: f.df("alpha", registries)?,
            first: f.df("first", registries)?,
            second: f.df("second", registries)?,
        }),
        "clamp" => {
            let input = f.df("input", registries)?;
            let min = f.required("min", parse_noise_value)?;
            let max = f.required("max", parse_noise_value)?;
            if max < min {
                return Err(LoadError::new(format!(
                    "min ({min:?}) must be less than or equal to max ({max:?})"
                )));
            }
            DfNode::Clamp(ClampFunction { input, min, max })
        }
        "range_choice" => DfNode::RangeChoice(RangeChoiceFunction {
            input: f.df("input", registries)?,
            min_inclusive: f.required("min_inclusive", parse_noise_value)?,
            max_exclusive: f.required("max_exclusive", parse_noise_value)?,
            when_in_range: f.df("when_in_range", registries)?,
            when_out_of_range: f.df("when_out_of_range", registries)?,
        }),
        "interval_select" => interval_select_from_json(f, registries)?,
        "cache" => DfNode::Cache(CacheFunction {
            input: f.df("input", registries)?,
        }),
        "blend_density" => DfNode::BlendDensity(BlendDensityFunction {
            input: f.df("input", registries)?,
        }),
        "interpolated" => DfNode::Interpolated(InterpolatedFunction {
            input: f.df("input", registries)?,
            cell_size_xz: f.required("cell_size_xz", parse_positive_int)?,
            cell_size_y: f.required("cell_size_y", parse_positive_int)?,
        }),
        "slice" => DfNode::Slice(SliceFunction {
            axis: f.required("axis", parse_axis)?,
            coordinate: f.required("coordinate", parse_int)?,
            input: f.df("input", registries)?,
        }),
        "find_top_surface" => DfNode::FindTopSurface(FindTopSurfaceFunction {
            density: f.df("density", registries)?,
            upper_bound: f.df("upper_bound", registries)?,
            lower_bound: f.required("lower_bound", |v| {
                parse_int_range(v, DIMENSION_MIN_Y * 2, DIMENSION_MAX_Y * 2)
            })?,
            cell_height: f.required("cell_height", parse_positive_int)?,
        }),
        "old_blended_noise" => {
            let scale = |v: &Value| parse_double_range(v, 0.001, 1000.0);
            DfNode::OldBlendedNoise(BlendedNoise {
                xz_scale: f.required("xz_scale", scale)?,
                y_scale: f.required("y_scale", scale)?,
                xz_factor: f.required("xz_factor", scale)?,
                y_factor: f.required("y_factor", scale)?,
                smear_scale_multiplier: f.required("smear_scale_multiplier", |v| {
                    parse_double_range(v, 1.0, 8.0)
                })?,
            })
        }
        _ => return simple_unary_round_binary(path, f, registries),
    };
    Ok(DensityFunction::new(node))
}

/// The types vanilla registers by iterating enum values: `SimpleDensityFunction`, `UnaryFunction.Type`,
/// `RoundFunction.Type`, `BinaryFunction.Type`.
fn simple_unary_round_binary(
    path: &str,
    f: &Fields,
    registries: &WorldgenRegistries,
) -> LoadResult<DensityFunction> {
    let node = if let Some(simple) = SimpleDensityFunction::ALL
        .into_iter()
        .find(|s| s.id() == path)
    {
        DfNode::Simple(simple)
    } else if let Some(ty) = UnaryType::ALL.into_iter().find(|t| t.id() == path) {
        DfNode::Unary(UnaryFunction {
            ty,
            input: f.df("input", registries)?,
        })
    } else if let Some(ty) = RoundType::ALL.into_iter().find(|t| t.id() == path) {
        DfNode::Round(RoundFunction {
            ty,
            input: f.df("input", registries)?,
            multiple: optional_df(f, "multiple", registries, || DensityFunction::constant(1.0))?,
        })
    } else if let Some(ty) = BinaryType::ALL.into_iter().find(|t| t.id() == path) {
        DfNode::Binary(BinaryFunction {
            ty,
            left: f.df("left", registries)?,
            right: f.df("right", registries)?,
        })
    } else {
        return Err(LoadError::new(format!("Unknown registry key: minecraft:{path}")).at("type"));
    };
    Ok(DensityFunction::new(node))
}

fn optional_df(
    f: &Fields,
    name: &str,
    registries: &WorldgenRegistries,
    default: impl FnOnce() -> DensityFunction,
) -> LoadResult<DensityFunction> {
    Ok(
        f.optional(name, |v| density_function_from_json(v, registries))?
            .unwrap_or_else(default),
    )
}

fn parse_axis(value: &Value) -> LoadResult<Axis> {
    let name = parse_str(value)?;
    Axis::from_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name: {name}")))
}

/// `Vec3i.CODEC`: a list of exactly three ints.
fn parse_vec3i(value: &Value) -> LoadResult<[i32; 3]> {
    parse_list(value, parse_int)?
        .try_into()
        .map_err(|_| LoadError::new("Input is not a list of 3 ints"))
}

fn interval_select_from_json(f: &Fields, registries: &WorldgenRegistries) -> LoadResult<DfNode> {
    let input = f.df("input", registries)?;
    let thresholds = f.required("thresholds", |v| parse_list(v, parse_noise_value))?;
    let functions = f.required("functions", |v| {
        let functions = parse_list(v, |e| density_function_from_json(e, registries))?;
        if functions.len() < 2 {
            return Err(LoadError::new(format!(
                "List is too short: {}, expected at least 2",
                functions.len()
            )));
        }
        Ok(functions)
    })?;
    if thresholds.len() != functions.len() - 1 {
        return Err(LoadError::new(format!(
            "Expected {} thresholds for {} functions, but got {}",
            functions.len() - 1,
            functions.len(),
            thresholds.len()
        )));
    }
    if !thresholds.windows(2).all(|w| w[0].total_cmp(&w[1]).is_le()) {
        return Err(LoadError::new(
            "Threshold values must be ordered from smallest to largest",
        ));
    }
    Ok(DfNode::IntervalSelect(IntervalSelectFunction {
        input,
        thresholds,
        functions,
    }))
}

// ---- CubicSpline.codec -----------------------------------------------------------------------------

/// `CubicSpline.codec(SplineFunction.Coordinate.CODEC)`: a float (constant) or `{coordinate, points}`.
fn spline_from_json(
    value: &Value,
    registries: &WorldgenRegistries,
) -> LoadResult<CubicSpline<DensityFunction>> {
    match value {
        Value::Number(_) => Ok(CubicSpline::Constant(parse_f32(value)?)),
        Value::Object(_) => {
            let fields = Fields::new(value)?;
            let coordinate = fields.df("coordinate", registries)?;
            let points = fields.required("points", |v| {
                let points = parse_list(v, |p| point_from_json(p, registries))?;
                if points.is_empty() {
                    Err(LoadError::new("List must have contents"))
                } else {
                    Ok(points)
                }
            })?;
            let mut locations = Vec::with_capacity(points.len());
            let mut values = Vec::with_capacity(points.len());
            let mut derivatives = Vec::with_capacity(points.len());
            for (location, value, derivative) in points {
                locations.push(location);
                values.push(value);
                derivatives.push(derivative);
            }
            Ok(CubicSpline::Multipoint(Multipoint::new(
                coordinate,
                locations,
                values,
                derivatives,
            )))
        }
        _ => Err(LoadError::new("expected number or object")),
    }
}

/// `CubicSpline.Multipoint.Point.codec`: `(location, value, derivative)`.
fn point_from_json(
    value: &Value,
    registries: &WorldgenRegistries,
) -> LoadResult<(f32, CubicSpline<DensityFunction>, f32)> {
    let fields = Fields::new(value)?;
    Ok((
        fields.required("location", parse_f32)?,
        fields.required("value", |v| spline_from_json(v, registries))?,
        fields.required("derivative", parse_f32)?,
    ))
}
