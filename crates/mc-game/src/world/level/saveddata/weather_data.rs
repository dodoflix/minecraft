//! `WeatherData` (`data/minecraft/weather.dat`).

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};

use super::SavedDataType;
use crate::impl_saved_data;
use crate::util::codec::{self, DataResult, DecodeResult};

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
    pub fn new(clear_weather_time: i32, rain_time: i32, thunder_time: i32, raining: bool, thundering: bool) -> Self {
        WeatherData { dirty: false, clear_weather_time, rain_time, thunder_time, raining, thundering }
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

    /// `WeatherData.CODEC` decode.
    pub fn decode(tag: &Tag) -> DecodeResult<WeatherData> {
        let m = codec::compound(tag)?;
        Ok(WeatherData::new(
            codec::int(codec::field(m, "clear_weather_time")?)?,
            codec::int(codec::field(m, "rain_time")?)?,
            codec::int(codec::field(m, "thunder_time")?)?,
            codec::boolean(codec::field(m, "raining")?)?,
            codec::boolean(codec::field(m, "thundering")?)?,
        ))
    }

    /// `WeatherData.CODEC` encode.
    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put_int("clear_weather_time", self.clear_weather_time);
        m.put_int("rain_time", self.rain_time);
        m.put_int("thunder_time", self.thunder_time);
        m.put_boolean("raining", self.raining);
        m.put_boolean("thundering", self.thundering);
        Ok(Tag::Compound(m))
    }

    /// `WeatherData.TYPE`.
    pub fn type_() -> SavedDataType<WeatherData> {
        SavedDataType::new(Identifier::parse("minecraft:weather").expect("static id"), WeatherData::default, WeatherData::decode, WeatherData::encode)
    }
}
