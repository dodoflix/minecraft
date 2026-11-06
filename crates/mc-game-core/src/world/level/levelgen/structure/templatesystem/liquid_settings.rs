//! `LiquidSettings`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiquidSettings {
    IgnoreWaterlogging,
    ApplyWaterlogging,
}

impl LiquidSettings {
    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            LiquidSettings::IgnoreWaterlogging => "ignore_waterlogging",
            LiquidSettings::ApplyWaterlogging => "apply_waterlogging",
        }
    }

    /// `LiquidSettings.CODEC` (`StringRepresentable.fromValues`).
    pub fn by_name(name: &str) -> Option<LiquidSettings> {
        [
            LiquidSettings::IgnoreWaterlogging,
            LiquidSettings::ApplyWaterlogging,
        ]
        .into_iter()
        .find(|s| s.get_serialized_name() == name)
    }
}
