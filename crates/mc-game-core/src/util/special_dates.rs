//! `net.minecraft.util.SpecialDates` (the dates are read in the server's local time zone like `ZonedDateTime.now()`).

use chrono::{Datelike, Local};

/// `isHalloween()`: October 31st.
pub fn is_halloween() -> bool {
    let now = Local::now();
    now.month() == 10 && now.day() == 31
}
