//! The `net.minecraft.util.Util` pieces the state slices need.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use mc_core::ident::Identifier;
use parking_lot::RwLock;
use uuid::Uuid;

/// `Util.NIL_UUID`.
pub const NIL_UUID: Uuid = Uuid::nil();

/// `Util.timeSource` override (`setTimeSource`/`shutdownTimeSource`); `None` = `System.nanoTime`.
static TIME_SOURCE: RwLock<Option<fn() -> i64>> = RwLock::new(None);

fn system_nano_time() -> i64 {
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_nanos() as i64
}

/// `Util.setTimeSource`.
pub fn set_time_source(source: fn() -> i64) {
    *TIME_SOURCE.write() = Some(source);
}

/// `Util.shutdownTimeSource`: freezes the clock at its current value is not representable with a plain `fn`, so this
/// restores the system source (callers only use it on shutdown, after which nothing reads time).
pub fn reset_time_source() {
    *TIME_SOURCE.write() = None;
}

/// `Util.getNanos` (`System.nanoTime`: arbitrary origin, monotonic).
pub fn get_nanos() -> i64 {
    match *TIME_SOURCE.read() {
        Some(source) => source(),
        None => system_nano_time(),
    }
}

/// `Util.getMillis`.
pub fn get_millis() -> i64 {
    get_nanos() / 1_000_000
}

/// `Util.getEpochMillis`.
pub fn get_epoch_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// `Util.toMillis(double seconds)`.
pub fn to_millis(seconds: f64) -> i64 {
    (seconds * 1000.0).floor() as i32 as i64
}

/// `Util.makeDescriptionId(prefix, location)`.
pub fn make_description_id(prefix: &str, location: Option<&Identifier>) -> String {
    match location {
        None => format!("{prefix}.unregistered_sadface"),
        Some(id) => format!("{prefix}.{}.{}", id.namespace(), id.path().replace('/', ".")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_id() {
        let id = Identifier::parse("minecraft:a/b").unwrap();
        assert_eq!(make_description_id("gamerule", Some(&id)), "gamerule.minecraft.a.b");
        assert_eq!(make_description_id("gamerule", None), "gamerule.unregistered_sadface");
    }

    #[test]
    fn nanos_monotonic() {
        let a = get_nanos();
        assert!(get_nanos() >= a);
    }
}
