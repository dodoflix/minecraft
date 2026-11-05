//! `net.minecraft.stats.StatFormatter`.
use mc_core::math::java::java_double_to_string;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatFormatter {
    /// `NumberFormat.getIntegerInstance(Locale.US)`: grouping with `,`.
    Default,
    DivideByTen,
    Distance,
    Time,
}

/// `DECIMAL_FORMAT = new DecimalFormat("########0.00", DecimalFormatSymbols.getInstance(Locale.ROOT))` (HALF_EVEN on the
/// exact decimal expansion, which is what Rust's `{:.2}` does).
fn decimal_format(value: f64) -> String {
    format!("{value:.2}")
}

impl StatFormatter {
    pub fn format(self, value: i32) -> String {
        match self {
            StatFormatter::Default => {
                let digits = value.unsigned_abs().to_string();
                let mut out = String::new();
                for (i, c) in digits.chars().enumerate() {
                    if i > 0 && (digits.len() - i) % 3 == 0 {
                        out.push(',');
                    }
                    out.push(c);
                }
                if value < 0 { format!("-{out}") } else { out }
            }
            StatFormatter::DivideByTen => decimal_format(f64::from(value) * 0.1),
            StatFormatter::Distance => {
                let meters = f64::from(value) / 100.0;
                let kilometers = meters / 1000.0;
                if kilometers > 0.5 {
                    format!("{} km", decimal_format(kilometers))
                } else if meters > 0.5 {
                    format!("{} m", decimal_format(meters))
                } else {
                    format!("{value} cm")
                }
            }
            StatFormatter::Time => {
                let seconds = f64::from(value) / 20.0;
                let minutes = seconds / 60.0;
                let hours = minutes / 60.0;
                let days = hours / 24.0;
                let years = days / 365.0;
                if years > 0.5 {
                    format!("{} y", decimal_format(years))
                } else if days > 0.5 {
                    format!("{} d", decimal_format(days))
                } else if hours > 0.5 {
                    format!("{} h", decimal_format(hours))
                } else if minutes > 0.5 {
                    format!("{} min", decimal_format(minutes))
                } else {
                    format!("{} s", java_double_to_string(seconds))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_vanilla() {
        assert_eq!(StatFormatter::Default.format(1234567), "1,234,567");
        assert_eq!(StatFormatter::Default.format(-1000), "-1,000");
        assert_eq!(StatFormatter::DivideByTen.format(15), "1.50");
        assert_eq!(StatFormatter::Distance.format(40), "40 cm");
        assert_eq!(StatFormatter::Distance.format(250), "2.50 m");
        assert_eq!(StatFormatter::Distance.format(100_000), "1.00 km");
        assert_eq!(StatFormatter::Time.format(10), "0.5 s");
        assert_eq!(StatFormatter::Time.format(1200), "1.00 min");
    }
}
