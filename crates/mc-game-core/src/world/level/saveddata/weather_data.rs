//! `WeatherData` (`data/minecraft/weather.dat`).

use std::sync::LazyLock;

use mc_core::ident::Identifier;
use mc_dfu::{BOOL, INT};
use mc_nbt::Tag;

use super::{SavedData, SavedDataType, decode_codec};
use crate::impl_saved_data;
use crate::util::codec::DecodeResult;

#[derive(Debug, Clone, Default)]
pub struct WeatherData {
    dirty: bool,
    clear_weather_time: i32,
    rain_time: i32,
    thunder_time: i32,
    raining: bool,
    thundering: bool,
}

impl_saved_data!(WeatherData);

impl WeatherData {
    pub fn new(
        clear_weather_time: i32,
        rain_time: i32,
        thunder_time: i32,
        raining: bool,
        thundering: bool,
    ) -> Self {
        WeatherData {
            dirty: false,
            clear_weather_time,
            rain_time,
            thunder_time,
            raining,
            thundering,
        }
    }

    pub fn get_clear_weather_time(&self) -> i32 {
        self.clear_weather_time
    }
    pub fn set_clear_weather_time(&mut self, v: i32) {
        self.clear_weather_time = v;
        self.set_dirty();
    }
    pub fn is_thundering(&self) -> bool {
        self.thundering
    }
    pub fn set_thundering(&mut self, v: bool) {
        self.thundering = v;
        self.set_dirty();
    }
    pub fn get_thunder_time(&self) -> i32 {
        self.thunder_time
    }
    pub fn set_thunder_time(&mut self, v: i32) {
        self.thunder_time = v;
        self.set_dirty();
    }
    pub fn is_raining(&self) -> bool {
        self.raining
    }
    pub fn set_raining(&mut self, v: bool) {
        self.raining = v;
        self.set_dirty();
    }
    pub fn get_rain_time(&self) -> i32 {
        self.rain_time
    }
    pub fn set_rain_time(&mut self, v: i32) {
        self.rain_time = v;
        self.set_dirty();
    }

    /// `WeatherData.CODEC`.
    pub fn codec() -> mc_dfu::Codec<WeatherData> {
        static CODEC: LazyLock<mc_dfu::Codec<WeatherData>> = LazyLock::new(|| {
            mc_dfu::record!(Packed {
                clear_weather_time: INT.field_of("clear_weather_time"),
                rain_time: INT.field_of("rain_time"),
                thunder_time: INT.field_of("thunder_time"),
                raining: BOOL.field_of("raining"),
                thundering: BOOL.field_of("thundering"),
            })
            .codec()
            .xmap(
                |p| {
                    WeatherData::new(
                        p.clear_weather_time,
                        p.rain_time,
                        p.thunder_time,
                        p.raining,
                        p.thundering,
                    )
                },
                |w: &WeatherData| Packed {
                    clear_weather_time: w.clear_weather_time,
                    rain_time: w.rain_time,
                    thunder_time: w.thunder_time,
                    raining: w.raining,
                    thundering: w.thundering,
                },
            )
        });
        CODEC.clone()
    }

    /// `WeatherData.CODEC` decode.
    pub fn decode(tag: &Tag) -> DecodeResult<WeatherData> {
        decode_codec(&WeatherData::codec(), tag)
    }

    /// `WeatherData.TYPE`.
    pub fn type_() -> SavedDataType<WeatherData> {
        SavedDataType::from_codec(
            Identifier::parse("minecraft:weather").expect("static id"),
            WeatherData::default,
            WeatherData::codec(),
            mc_datafix::DataFixTypes::SAVED_DATA_WEATHER,
        )
    }
}

/// The serialized shape of [`WeatherData`].
struct Packed {
    clear_weather_time: i32,
    rain_time: i32,
    thunder_time: i32,
    raining: bool,
    thundering: bool,
}
