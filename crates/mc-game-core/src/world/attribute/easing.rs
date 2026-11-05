//! `EasingType` + `Ease`: keyframe interpolation curves (float arithmetic as vanilla).

use mc_core::math::mth;
use serde_json::Value;
use std::f32::consts::PI;

#[derive(Clone, Debug, PartialEq)]
pub enum EasingType {
    Named(fn(f32) -> f32, &'static str),
    CubicBezier(CubicBezier),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    x: Curve,
    y: Curve,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Curve(f32, f32, f32);

impl Curve {
    fn from_controls(v1: f32, v2: f32) -> Curve {
        Curve(3.0 * v1 - 3.0 * v2 + 1.0, -6.0 * v1 + 3.0 * v2, 3.0 * v1)
    }
    fn sample(self, t: f32) -> f32 {
        ((self.0 * t + self.1) * t + self.2) * t
    }
    fn gradient(self, t: f32) -> f32 {
        (3.0 * self.0 * t + 2.0 * self.1) * t + self.2
    }
}

impl CubicBezier {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> CubicBezier {
        CubicBezier { x: Curve::from_controls(x1, x2), y: Curve::from_controls(y1, y2) }
    }

    fn apply(&self, x: f32) -> f32 {
        self.y.sample(self.solve_t(x))
    }

    fn solve_t(&self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..4 {
            let error = self.x.sample(t) - x;
            if error.abs() < 1.0E-5 {
                return t;
            }
            let gradient = self.x.gradient(t);
            if gradient < 1.0E-5 {
                break;
            }
            t -= mth::clamp_f32(error / gradient, -0.25, 0.25);
        }
        self.solve_t_bisect(x, t)
    }

    fn solve_t_bisect(&self, x: f32, initial_t: f32) -> f32 {
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        let mut t = initial_t;
        while t0 < t1 {
            let error = self.x.sample(t) - x;
            if error.abs() < 1.0E-5 {
                return t;
            }
            if error < 0.0 {
                t0 = t;
            } else {
                t1 = t;
            }
            t = (t1 + t0) / 2.0;
        }
        t
    }
}

fn sq(x: f32) -> f32 {
    x * x
}
fn cube(x: f32) -> f32 {
    x * x * x
}
fn pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

const C4: f32 = (std::f64::consts::PI * 2.0 / 3.0) as f32;

fn out_bounce(x: f32) -> f32 {
    if x < 0.36363637 {
        7.5625 * sq(x)
    } else if x < 0.72727275 {
        7.5625 * sq(x - 0.54545456) + 0.75
    } else if f64::from(x) < 0.9090909090909091 {
        7.5625 * sq(x - 0.8181818) + 0.9375
    } else {
        7.5625 * sq(x - 0.95454544) + 0.984375
    }
}

const SIMPLE: &[(&str, fn(f32) -> f32)] = &[
    ("constant", |_| 0.0),
    ("linear", |x| x),
    ("in_back", |x| sq(x) * (2.70158 * x - 1.70158)),
    ("in_bounce", |x| 1.0 - out_bounce(1.0 - x)),
    ("in_circ", |x| (-f64::from(1.0 - x * x).sqrt()) as f32 + 1.0),
    ("in_cubic", cube),
    ("in_elastic", |x| {
        if x == 0.0 {
            0.0
        } else if x == 1.0 {
            1.0
        } else {
            (-pow(2.0, f64::from(x) * 10.0 - 10.0) * ((f64::from(x) * 10.0 - 10.75) * f64::from(C4)).sin()) as f32
        }
    }),
    ("in_expo", |x| if x == 0.0 { 0.0 } else { pow(2.0, f64::from(x) * 10.0 - 10.0) as f32 }),
    ("in_quad", |x| x * x),
    ("in_quart", |x| sq(sq(x))),
    ("in_quint", |x| sq(sq(x)) * x),
    ("in_sine", |x| 1.0 - mth::cos(f64::from(x * (PI / 2.0)))),
    ("in_out_back", |x| {
        if x < 0.5 {
            4.0 * x * x * (7.189819 * x - 2.5949094) / 2.0
        } else {
            let dt = 2.0 * x - 2.0;
            (dt * dt * (3.5949094 * dt + 2.5949094) + 2.0) / 2.0
        }
    }),
    ("in_out_bounce", |x| if x < 0.5 { (1.0 - out_bounce(1.0 - 2.0 * x)) / 2.0 } else { (1.0 + out_bounce(2.0 * x - 1.0)) / 2.0 }),
    ("in_out_circ", |x| {
        if x < 0.5 {
            ((1.0 - (1.0 - pow(2.0 * f64::from(x), 2.0)).sqrt()) / 2.0) as f32
        } else {
            (((1.0 - pow(-2.0 * f64::from(x) + 2.0, 2.0)).sqrt() + 1.0) / 2.0) as f32
        }
    }),
    ("in_out_cubic", |x| if x < 0.5 { 4.0 * cube(x) } else { (1.0 - pow(-2.0 * f64::from(x) + 2.0, 3.0) / 2.0) as f32 }),
    ("in_out_elastic", |x| {
        if x == 0.0 {
            0.0
        } else if x == 1.0 {
            1.0
        } else {
            let sin = ((20.0 * f64::from(x) - 11.125) * f64::from(PI * 4.0 / 9.0)).sin();
            if x < 0.5 {
                (-(pow(2.0, 20.0 * f64::from(x) - 10.0) * sin) / 2.0) as f32
            } else {
                (pow(2.0, -20.0 * f64::from(x) + 10.0) * sin / 2.0 + 1.0) as f32
            }
        }
    }),
    ("in_out_expo", |x| {
        if x < 0.5 {
            if x == 0.0 { 0.0 } else { (pow(2.0, 20.0 * f64::from(x) - 10.0) / 2.0) as f32 }
        } else if x == 1.0 {
            1.0
        } else {
            ((2.0 - pow(2.0, -20.0 * f64::from(x) + 10.0)) / 2.0) as f32
        }
    }),
    ("in_out_quad", |x| if x < 0.5 { 2.0 * sq(x) } else { (1.0 - pow(-2.0 * f64::from(x) + 2.0, 2.0) / 2.0) as f32 }),
    ("in_out_quart", |x| if x < 0.5 { 8.0 * sq(sq(x)) } else { (1.0 - pow(-2.0 * f64::from(x) + 2.0, 4.0) / 2.0) as f32 }),
    ("in_out_quint", |x| if f64::from(x) < 0.5 { 16.0 * x * x * x * x * x } else { (1.0 - pow(-2.0 * f64::from(x) + 2.0, 5.0) / 2.0) as f32 }),
    ("in_out_sine", |x| -(mth::cos(f64::from(PI * x)) - 1.0) / 2.0),
    ("out_back", |x| 1.0 + 2.70158 * cube(x - 1.0) + 1.70158 * sq(x - 1.0)),
    ("out_bounce", out_bounce),
    ("out_circ", |x| f64::from(1.0 - sq(x - 1.0)).sqrt() as f32),
    ("out_cubic", |x| 1.0 - cube(1.0 - x)),
    ("out_elastic", |x| {
        if x == 0.0 {
            0.0
        } else if x == 1.0 {
            1.0
        } else {
            (pow(2.0, -10.0 * f64::from(x)) * ((f64::from(x) * 10.0 - 0.75) * f64::from(C4)).sin() + 1.0) as f32
        }
    }),
    ("out_expo", |x| if x == 1.0 { 1.0 } else { 1.0 - pow(2.0, -10.0 * f64::from(x)) as f32 }),
    ("out_quad", |x| 1.0 - sq(1.0 - x)),
    ("out_quart", |x| 1.0 - sq(sq(1.0 - x))),
    ("out_quint", |x| 1.0 - pow(1.0 - f64::from(x), 5.0) as f32),
    ("out_sine", |x| mth::sin(f64::from(x * (PI / 2.0)))),
];

impl EasingType {
    pub const LINEAR: EasingType = EasingType::Named(|x| x, "linear");

    /// `EasingType.CODEC`: a simple name or `{"cubic_bezier": [x1, y1, x2, y2]}`.
    pub fn from_json(value: &Value) -> Option<EasingType> {
        match value {
            Value::String(name) => SIMPLE.iter().find(|(n, _)| n == name).map(|(n, f)| EasingType::Named(*f, n)),
            Value::Object(o) => {
                let controls: Vec<f32> = o.get("cubic_bezier")?.as_array()?.iter().map(|v| v.as_f64().map(|f| f as f32)).collect::<Option<_>>()?;
                let [x1, y1, x2, y2] = controls[..] else { return None };
                if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
                    return None;
                }
                Some(EasingType::CubicBezier(CubicBezier::new(x1, y1, x2, y2)))
            }
            _ => None,
        }
    }

    pub fn apply(&self, x: f32) -> f32 {
        match self {
            EasingType::Named(f, _) => f(x),
            EasingType::CubicBezier(b) => b.apply(x),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_curves() {
        let get = |n: &str| EasingType::from_json(&Value::from(n)).unwrap();
        assert_eq!(get("constant").apply(0.7), 0.0);
        assert_eq!(get("linear").apply(0.25), 0.25);
        assert_eq!(get("in_quad").apply(0.5), 0.25);
        assert!((get("out_bounce").apply(1.0) - 1.0).abs() < 1e-6);
        assert!(EasingType::from_json(&Value::from("nope")).is_none());
    }

    #[test]
    fn bezier_is_monotone_for_ease_in_out() {
        let e = EasingType::from_json(&serde_json::json!({"cubic_bezier": [0.362, 0.241, 0.638, 0.759]})).unwrap();
        let mut last = -1.0;
        for i in 0..=20 {
            let v = e.apply(i as f32 / 20.0);
            assert!(v >= last);
            last = v;
        }
        assert!((e.apply(0.0)).abs() < 1e-4 && (e.apply(1.0) - 1.0).abs() < 1e-3);
    }
}
