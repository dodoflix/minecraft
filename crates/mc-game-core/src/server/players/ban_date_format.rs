//! `BanListEntry.DATE_FORMAT`: `new SimpleDateFormat("yyyy-MM-dd HH:mm:ss Z", Locale.ROOT)`, lenient, of JDK 25.
//!
//! Parsing follows `SimpleDateFormat.parse(String, ParsePosition)` for this pattern: the text after the zone is
//! ignored, a number field skips spaces and tabs and is read by `DecimalFormat` (a sign, any Unicode digit, `NaN`,
//! infinity, a fraction and an exponent included), the lenient `GregorianCalendar` rolls out-of-range fields over (the
//! Julian calendar before 1582-10-15) and a zone is `+hhmm`, `GMT`, `GMT+hh:mm` or a name of the zone table. Formatting
//! writes the date of the same calendar.
//!
//! The format object of vanilla is one shared static ([`DATE_FORMAT`]): parsing a zone name makes that zone the time
//! zone of the format for every later parse and format, which [`DateFormat`] keeps. A zone name is placed in the zone's
//! history (`java_time_zone`), as `GregorianCalendar` does for the zone of the format.

use std::sync::{Arc, LazyLock, Mutex};

use mc_core::java_case::region_matches_ignore_case;

use super::ban_list_entry::Date;
use super::java_time_zone::{RuleCache, ZoneInfo, zone_info};

/// `GregorianCalendar.EPOCH_OFFSET`: the fixed date of 1970-01-01.
const EPOCH_OFFSET: i64 = 719_163;
/// The fixed date of the Gregorian cutover 1582-10-15.
const CUTOVER_FIXED_DATE: i64 = 577_736;
const CUTOVER_YEAR: i32 = 1582;
const ONE_DAY: i64 = 86_400_000;
/// `JulianCalendar.JULIAN_EPOCH`.
const JULIAN_EPOCH: i64 = -1;

/// The first code point of each run of ten BMP decimal digits (`Character.digit(c, 10)`), JDK 25.
const DIGIT_ZEROS: [u16; 37] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10,
];

/// `Character.digit(unit, 10)`: the digit value, or -1.
pub fn java_digit(unit: u16) -> i32 {
    DIGIT_ZEROS
        .iter()
        .find(|&&zero| (zero..zero + 10).contains(&unit))
        .map_or(-1, |&zero| i32::from(unit - zero))
}

/// `DigitList` of `DecimalFormat.parse`: the significant digits and the position of the decimal point.
#[derive(Default)]
struct DigitList {
    digits: Vec<u8>,
    decimal_at: i32,
}

impl DigitList {
    /// `DigitList.fitsIntoLong` (trims the trailing zeros like the JDK does).
    fn fits_into_long(&mut self, is_positive: bool, ignore_negative_zero: bool) -> bool {
        const LONG_MIN_REP: &[u8] = b"9223372036854775808";
        while self.digits.last() == Some(&b'0') {
            self.digits.pop();
        }
        let count = self.digits.len() as i32;
        if count == 0 {
            return is_positive || ignore_negative_zero;
        }
        if self.decimal_at < count || self.decimal_at > 19 {
            return false;
        }
        if self.decimal_at < 19 {
            return true;
        }
        for (&d, &max) in self.digits.iter().zip(LONG_MIN_REP) {
            if d != max {
                return d < max;
            }
        }
        if count < self.decimal_at {
            return true;
        }
        !is_positive
    }

    /// `DigitList.getLong` (after `fits_into_long`).
    fn get_long(&self) -> i64 {
        if self.digits.is_empty() {
            return 0;
        }
        if self.decimal_at == 19 && self.digits == b"9223372036854775808" {
            return i64::MIN;
        }
        let mut text = String::from_utf8_lossy(&self.digits).into_owned();
        text.extend(std::iter::repeat_n(
            '0',
            (self.decimal_at - self.digits.len() as i32).max(0) as usize,
        ));
        text.parse().expect("at most 19 digits that fit a long")
    }

    /// `DigitList.getDouble`.
    fn get_double(&self) -> f64 {
        if self.digits.is_empty() {
            return 0.0;
        }
        format!(
            ".{}E{}",
            String::from_utf8_lossy(&self.digits),
            self.decimal_at
        )
        .parse()
        .expect("a decimal fraction with an exponent")
    }
}

/// `DecimalFormat.shiftDecimalAt`.
fn shift_decimal_at(decimal_at: i32, exponent: i64) -> i32 {
    match i64::from(decimal_at).checked_add(exponent) {
        None if exponent > 0 && decimal_at > 0 => i32::MAX,
        None => i32::MIN,
        Some(sum) => i32::try_from(sum).unwrap_or(if sum > 0 { i32::MAX } else { i32::MIN }),
    }
}

/// `DecimalFormat.subparseNumber` of the integer instance of the root locale: returns the position after the number
/// and the position of its decimal point, or `None` when there is no number. `infinite` is set for an infinity.
fn subparse_number(
    text: &[u16],
    mut position: usize,
    digits: &mut DigitList,
    is_exponent: bool,
    infinite: &mut bool,
) -> Option<(usize, Option<usize>)> {
    *infinite = false;
    let mut int_index = None;
    if !is_exponent && text.get(position) == Some(&0x221E) {
        *infinite = true;
        return Some((position + 1, int_index));
    }
    digits.decimal_at = 0;
    digits.digits.clear();
    let (mut saw_decimal, mut saw_digit) = (false, false);
    let mut exponent = 0i64;
    let mut digit_count = 0i32;
    let start_pos = position;
    while position < text.len() {
        let ch = text[position];
        let mut digit = i32::from(ch) - i32::from(b'0');
        if !(0..=9).contains(&digit) {
            digit = java_digit(ch);
        }
        if digit == 0 {
            saw_digit = true;
            if digits.digits.is_empty() {
                if !saw_decimal {
                    position += 1;
                    continue;
                }
                digits.decimal_at -= 1;
            } else {
                digit_count += 1;
                if !saw_decimal {
                    digits.digits.push(b'0');
                }
            }
        } else if (1..=9).contains(&digit) {
            saw_digit = true;
            digit_count += 1;
            if !saw_decimal {
                digits.digits.push(b'0' + digit as u8);
            }
        } else if !is_exponent && ch == u16::from(b'.') {
            if start_pos == position {
                return None;
            }
            if saw_decimal {
                break;
            }
            int_index = Some(position);
            digits.decimal_at = digit_count;
            saw_decimal = true;
        } else if !is_exponent && ch == u16::from(b'E') {
            let mut exponent_digits = DigitList::default();
            if let Some(Subparsed {
                end,
                positive: exponent_positive,
                ..
            }) = subparse(text, position + 1, &mut exponent_digits, true)
            {
                exponent = if exponent_digits.fits_into_long(exponent_positive, true) {
                    let value = exponent_digits.get_long();
                    if exponent_positive {
                        value
                    } else {
                        value.wrapping_neg()
                    }
                } else if exponent_positive {
                    i64::MAX
                } else {
                    i64::MIN
                };
                position = end;
            }
            break;
        } else {
            break;
        }
        position += 1;
    }
    if !saw_decimal {
        digits.decimal_at = digit_count;
        digits.decimal_at = shift_decimal_at(digits.decimal_at, exponent);
    }
    if !saw_digit && digit_count == 0 {
        return None;
    }
    Some((position, int_index))
}

/// What `DecimalFormat.subparse` read: the end of the number (the decimal point when there is one, a number is read
/// as an integer), its sign and whether it is an infinity.
struct Subparsed {
    end: usize,
    positive: bool,
    infinite: bool,
}

/// `DecimalFormat.subparse` with the prefixes `""` (positive) and `"-"` (negative) and no suffixes.
fn subparse(
    text: &[u16],
    position: usize,
    digits: &mut DigitList,
    is_exponent: bool,
) -> Option<Subparsed> {
    let negative = text.get(position) == Some(&u16::from(b'-'));
    let mut infinite = false;
    let (end, int_pos) = subparse_number(
        text,
        position + usize::from(negative),
        digits,
        is_exponent,
        &mut infinite,
    )?;
    let end = if is_exponent {
        end
    } else {
        int_pos.unwrap_or(end)
    };
    (end != position).then_some(Subparsed {
        end,
        positive: !negative,
        infinite,
    })
}

/// `NumberFormat.getIntegerInstance(Locale.ROOT).parse(text, position).intValue()`: the value and the end position.
fn parse_int(text: &[u16], position: usize) -> Option<(i32, usize)> {
    // `NaN` is a Double: intValue is 0
    if text[position.min(text.len())..].starts_with(&[0x4E, 0x61, 0x4E]) {
        return Some((0, position + 3));
    }
    let mut digits = DigitList::default();
    let Subparsed {
        end,
        positive,
        infinite,
    } = subparse(text, position, &mut digits, false)?;
    if infinite {
        // `Double.POSITIVE_INFINITY` / `NEGATIVE_INFINITY`: intValue saturates
        return Some((if positive { i32::MAX } else { i32::MIN }, end));
    }
    let value = if digits.fits_into_long(positive, true) {
        let long = digits.get_long();
        // Long.MIN_VALUE stays as it is when negated
        let long = if !positive && long != i64::MIN {
            -long
        } else {
            long
        };
        long as i32
    } else {
        let double = digits.get_double();
        (if positive { double } else { -double }) as i32
    };
    Some((value, end))
}

/// A zone of `DateFormatSymbols.getZoneStrings()` of the root locale.
struct ZoneStrings {
    id: &'static str,
    /// Long and short standard name, long and short daylight name.
    names: [Vec<u16>; 4],
}

static ZONE_STRINGS: LazyLock<Vec<ZoneStrings>> = LazyLock::new(|| {
    include_str!("java_zone_strings.tsv")
        .lines()
        .map(|line| {
            let f: Vec<&'static str> = line.split('\t').collect();
            let names = |i: usize| f[i].encode_utf16().collect::<Vec<u16>>();
            ZoneStrings {
                id: f[0],
                names: [names(1), names(2), names(3), names(4)],
            }
        })
        .collect()
});

/// `TimeZone.getDefault().getID()` as far as the zone table needs it: the `TZ` variable, else the link of
/// `/etc/localtime`.
fn default_zone_id() -> Option<String> {
    if let Some(tz) = std::env::var("TZ").ok().filter(|tz| !tz.is_empty()) {
        return Some(tz.trim_start_matches(':').to_owned());
    }
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let link = link.to_string_lossy();
    link.rsplit_once("zoneinfo/").map(|(_, id)| id.to_owned())
}

/// The default time zone when its id is a zone of the JDK: its history and the short names `z` prints.
pub(super) struct DefaultZone {
    pub id: &'static str,
    pub rules: Arc<ZoneInfo>,
    /// `getDisplayName(false, SHORT, Locale.ROOT)` and `getDisplayName(true, SHORT, Locale.ROOT)`.
    short_names: [String; 2],
}

impl DefaultZone {
    pub fn short_name(&self, daylight: bool) -> &str {
        &self.short_names[usize::from(daylight)]
    }
}

/// `TimeZone.getDefault()`, `None` when the process zone is not a named JDK zone (then only the C library knows it).
pub(super) fn default_zone() -> Option<DefaultZone> {
    let id = default_zone_id()?;
    let strings = ZONE_STRINGS.iter().find(|z| z.id == id)?;
    Some(DefaultZone {
        id: strings.id,
        rules: zone_info(strings.id)?,
        short_names: [1, 3].map(|i| String::from_utf16_lossy(&strings.names[i])),
    })
}

/// A zone named in the text: the zone, and the offsets `subParseZoneString` sets in the calendar
/// (`None` leaves the zone to decide).
struct NamedZone {
    zone: Arc<ZoneInfo>,
    id: &'static str,
    zone_offset: Option<i32>,
    dst_offset: Option<i32>,
}

/// The zone whose name is at `start` of `text` (`SimpleDateFormat.subParseZoneString`): the zone of the format first,
/// then the default zone, then every zone of the table in its order.
fn parse_zone_name(
    text: &[u16],
    start: usize,
    default_zone: Option<&str>,
    current: Option<&str>,
) -> Option<NamedZone> {
    let zones = &*ZONE_STRINGS;
    let match_zone = |zone: &ZoneStrings| {
        (0..4).find(|&i| {
            region_matches_ignore_case(text, start, &zone.names[i], 0, zone.names[i].len())
        })
    };
    let by_id = |id: &str| zones.iter().find(|z| z.id == id);
    let (zone, index) = current
        .and_then(by_id)
        .into_iter()
        .chain(default_zone.and_then(by_id))
        .chain(zones.iter())
        .find_map(|zone| Some((zone, match_zone(zone)?)))?;
    let tz = zone_info(zone.id)?;
    // names 0 and 1 are the long and short standard names, 2 and 3 the daylight ones
    let daylight = index >= 2;
    let use_same_name = !daylight && {
        let (standard, daylight) = (&zone.names[index], &zone.names[index + 2]);
        standard.len() == daylight.len()
            && region_matches_ignore_case(standard, 0, daylight, 0, standard.len())
    };
    let dst_amount = if daylight { tz.dst_savings } else { 0 };
    // the name of both standard and daylight time, or a daylight name of a zone without DST: the zone decides
    let zone_decides = use_same_name || (daylight && dst_amount == 0);
    Some(NamedZone {
        zone: tz,
        id: zone.id,
        zone_offset: None,
        dst_offset: (!zone_decides).then_some(dst_amount),
    })
}

/// `SimpleDateFormat.subParseNumericZone`: `hh:mm` (colon required) or `hhmm`; returns the end and the offset.
fn parse_numeric_zone(text: &[u16], start: usize, sign: i32, colon: bool) -> Option<(usize, i32)> {
    let digit = |i: usize| match text.get(i) {
        Some(&c @ 0x30..=0x39) => Some(i32::from(c - 0x30)),
        _ => None,
    };
    let mut index = start;
    let mut hours = digit(index)?;
    index += 1;
    match digit(index) {
        Some(d) => {
            hours = hours * 10 + d;
            index += 1;
        }
        None => {
            // the character after one digit must exist, the colon form accepts it as the separator
            text.get(index)?;
            if !colon {
                return None;
            }
        }
    }
    if hours > 23 {
        return None;
    }
    let mut c = *text.get(index)?;
    index += 1;
    if colon {
        if c != u16::from(b':') {
            return None;
        }
        c = *text.get(index)?;
        index += 1;
    }
    let tens = match c {
        0x30..=0x39 => i32::from(c - 0x30),
        _ => return None,
    };
    let ones = digit(index)?;
    index += 1;
    let minutes = tens * 10 + ones;
    if minutes > 59 {
        return None;
    }
    Some((index, (hours * 60 + minutes) * 60_000 * sign))
}

/// `Character.getType(c) == SPACE_SEPARATOR`.
fn is_space_separator(c: u16) -> bool {
    matches!(
        c,
        0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x202F | 0x205F | 0x3000
    )
}

/// `SimpleDateFormat.charEquals` of the lenient format.
fn char_equals(text: u16, pattern: u16) -> bool {
    text == pattern || (is_space_separator(text) && is_space_separator(pattern))
}

/// The fields `CalendarBuilder` collects.
struct Fields {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    /// The zone offset and DST offset the text set, `None` for the zone of the calendar to give.
    zone_offset: Option<i32>,
    dst_offset: Option<i32>,
    zone: Option<Arc<ZoneInfo>>,
}

/// A field `subParse` reads as a number: skips spaces and tabs, then reads an integer.
fn parse_field(text: &[u16], start: usize) -> Option<(i32, usize)> {
    let mut pos = start;
    loop {
        match text.get(pos) {
            None => return None,
            Some(0x20 | 0x09) => pos += 1,
            Some(_) => break,
        }
    }
    parse_int(text, pos)
}

/// What a parse leaves behind: the epoch milliseconds, and the zone a name in the text made the zone of the format
/// (`Some(None)` for the default zone itself).
struct Parsed {
    millis: i64,
    named: Option<Option<&'static str>>,
}

/// `SimpleDateFormat.parse(text, new ParsePosition(0))` of the pattern, as epoch milliseconds. `current` is the id of
/// the named zone the format is in (the default zone when `None`).
fn parse_millis(
    text: &[u16],
    default_zone: Option<&str>,
    current: Option<&str>,
    cache: &mut Option<RuleCache>,
) -> Option<Parsed> {
    let mut start = 0;
    let mut numbers = [0i32; 6];
    // yyyy-MM-dd HH:mm:ss Z: each number is followed by its literal
    let literals = b"-- :: ";
    for (number, &literal) in numbers.iter_mut().zip(literals) {
        let (value, end) = parse_field(text, start)?;
        *number = value;
        start = end;
        if !text
            .get(start)
            .is_some_and(|&c| char_equals(c, u16::from(literal)))
        {
            return None;
        }
        start += 1;
    }
    let zone = parse_zone(text, start, default_zone, current)?;
    // a zone other than the format's own is a new `TimeZone` object, with an empty rule cache
    if zone
        .id
        .is_some_and(|id| Some(id) != current.or(default_zone))
    {
        *cache = None;
    }
    let fields = Fields {
        year: numbers[0],
        month: numbers[1].wrapping_sub(1),
        day: numbers[2],
        hour: numbers[3],
        minute: numbers[4],
        second: numbers[5],
        zone_offset: zone.zone_offset,
        dst_offset: zone.dst_offset,
        zone: zone.zone,
    };
    Some(Parsed {
        millis: compute_millis(&fields, cache),
        named: zone.named,
    })
}

/// The `Z` field: the offsets it sets in the calendar, and the zone it names.
struct ParsedZone {
    zone_offset: Option<i32>,
    dst_offset: Option<i32>,
    zone: Option<Arc<ZoneInfo>>,
    /// The id of the zone the text names.
    id: Option<&'static str>,
    named: Option<Option<&'static str>>,
}

impl ParsedZone {
    /// A numeric zone: both offsets are set.
    fn fixed(offset: i32) -> ParsedZone {
        ParsedZone {
            zone_offset: Some(offset),
            dst_offset: Some(0),
            zone: None,
            id: None,
            named: None,
        }
    }
}

fn parse_zone(
    text: &[u16],
    start: usize,
    default_zone: Option<&str>,
    current: Option<&str>,
) -> Option<ParsedZone> {
    // `subParse` skips the spaces and tabs first; the `GMT` check looks at `start`, the name at the skipped position
    let mut pos = start;
    loop {
        match text.get(pos) {
            None => return None,
            Some(0x20 | 0x09) => pos += 1,
            Some(_) => break,
        }
    }
    let c = text[pos];
    let sign = match c {
        0x2B => 1,
        0x2D => -1,
        _ => 0,
    };
    if sign != 0 {
        return parse_numeric_zone(text, pos + 1, sign, false)
            .map(|(_, offset)| ParsedZone::fixed(offset));
    }
    let gmt = [0x47u16, 0x4D, 0x54];
    if (c == 0x47 || c == 0x67)
        && text.len() - start >= 3
        && region_matches_ignore_case(text, start, &gmt, 0, 3)
    {
        let after = start + 3;
        let sign = match text.get(after) {
            Some(0x2B) => 1,
            Some(0x2D) => -1,
            _ => return Some(ParsedZone::fixed(0)),
        };
        return parse_numeric_zone(text, after + 1, sign, true)
            .map(|(_, offset)| ParsedZone::fixed(offset));
    }
    let named = parse_zone_name(text, pos, default_zone, current)?;
    Some(ParsedZone {
        zone_offset: named.zone_offset,
        dst_offset: named.dst_offset,
        // `setTimeZone` of the matched zone, which is the default zone itself when the default zone matched
        named: Some((Some(named.id) != default_zone).then_some(named.id)),
        zone: Some(named.zone),
        id: Some(named.id),
    })
}

/// `floorDiv` and `floorMod` for the month roll-over.
fn floor_div_mod(a: i32, b: i32) -> (i32, i32) {
    (a.div_euclid(b), a.rem_euclid(b))
}

/// `BaseCalendar.getFixedDate` (Gregorian): the fixed date of `month` (1 to 12) `day` of `year`.
pub(super) fn gregorian_fixed_date(year: i32, month: i32, day: i64) -> i64 {
    let prev = i64::from(year) - 1;
    let mut days = day + 365 * prev + prev.div_euclid(4) - prev.div_euclid(100)
        + prev.div_euclid(400)
        + i64::from((367 * month - 362).div_euclid(12));
    if month > 2 {
        days -= if is_gregorian_leap_year(year) { 1 } else { 2 };
    }
    days
}

/// The normalized year of a Julian fixed date.
pub(super) fn julian_year(fixed: i64) -> i32 {
    julian_date(fixed).0 as i32
}

/// The year of a Gregorian fixed date.
pub(super) fn gregorian_year(fixed: i64) -> i32 {
    civil_from_days(fixed - EPOCH_OFFSET).0 as i32
}

fn is_gregorian_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// `JulianCalendar.getFixedDate`.
pub(super) fn julian_fixed_date(year: i32, month: i32, day: i64) -> i64 {
    let y = i64::from(year);
    let mut days = JULIAN_EPOCH - 1 + 365 * (y - 1) + day;
    days += (y - 1).div_euclid(4);
    days += (367 * i64::from(month) - 362).div_euclid(12);
    if month > 2 {
        days -= if year % 4 == 0 { 1 } else { 2 };
    }
    days
}

/// `GregorianCalendar.getFixedDate`: the first day of the month plus the day, the month rolled over into the year.
fn calendar_fixed_date(julian: bool, year: i32, month: i32, day: i32) -> i64 {
    let (mut year, mut month) = (year, month);
    if month > 11 {
        year = year.wrapping_add(month / 12);
        month %= 12;
    } else if month < 0 {
        let (q, r) = floor_div_mod(month, 12);
        year = year.wrapping_add(q);
        month = r;
    }
    let first = if julian {
        julian_fixed_date(year, month + 1, 1)
    } else {
        gregorian_fixed_date(year, month + 1, 1)
    };
    first + i64::from(day) - 1
}

/// `GregorianCalendar.computeTime` in lenient mode for year, month, day, hour of day, minute, second, zone offset
/// and DST offset.
fn compute_millis(f: &Fields, cache: &mut Option<RuleCache>) -> i64 {
    let mut time_of_day = i64::from(f.hour);
    time_of_day = time_of_day * 60 + i64::from(f.minute);
    time_of_day = time_of_day * 60 + i64::from(f.second);
    time_of_day *= 1000;
    let mut fixed = time_of_day / ONE_DAY;
    time_of_day %= ONE_DAY;
    while time_of_day < 0 {
        time_of_day += ONE_DAY;
        fixed -= 1;
    }
    let year = f.year;
    let in_calendar = |julian: bool| fixed + calendar_fixed_date(julian, year, f.month, f.day);
    // A Julian date of the period the cutover leaves out is kept as the Julian calculation gives it (lenient).
    let fixed_date = if year > CUTOVER_YEAR {
        let gregorian = in_calendar(false);
        if gregorian >= CUTOVER_FIXED_DATE {
            gregorian
        } else {
            in_calendar(true)
        }
    } else if year < CUTOVER_YEAR {
        in_calendar(true)
    } else {
        let gregorian = in_calendar(false);
        if gregorian >= CUTOVER_FIXED_DATE {
            gregorian
        } else {
            in_calendar(true)
        }
    };
    // local wall-clock milliseconds, then the offsets set by the text, the zone giving the others for that wall time
    let wall = (fixed_date - EPOCH_OFFSET)
        .wrapping_mul(ONE_DAY)
        .wrapping_add(time_of_day);
    let (zone_offset, dst_offset) = match (f.zone_offset, f.dst_offset, &f.zone) {
        (Some(zone_offset), Some(dst_offset), _) => (zone_offset, dst_offset),
        (zone_offset, dst_offset, Some(zone)) => {
            let (raw, dst) = zone.offsets_at_wall(wall, cache);
            (zone_offset.unwrap_or(raw), dst_offset.unwrap_or(dst))
        }
        // a numeric zone sets both offsets
        (zone_offset, dst_offset, None) => (zone_offset.unwrap_or(0), dst_offset.unwrap_or(0)),
    };
    let millis = wall.wrapping_sub(i64::from(zone_offset) + i64::from(dst_offset));
    if let (None, Some(zone)) = (f.zone_offset.zip(f.dst_offset), &f.zone) {
        // `computeFields` of the parsed time asks the zone again, which moves the rule cache
        zone.offsets_at_utc(millis, cache);
    }
    millis
}

/// The calendar fields of `time` milliseconds in a zone `offset` milliseconds from UTC (`GregorianCalendar.computeFields`
/// adds the day count of the offset to that of the time, so the sum cannot overflow): the era-relative year, month
/// (1 to 12), day, hour, minute and second.
pub(super) fn calendar_fields(time: i64, offset: i32) -> (i64, u32, u32, u32, u32, u32) {
    let local = i128::from(time) + i128::from(offset);
    let days = local.div_euclid(i128::from(ONE_DAY)) as i64;
    let time = local.rem_euclid(i128::from(ONE_DAY)) as i64;
    let fixed = days + EPOCH_OFFSET;
    let (year, month, day) = if fixed >= CUTOVER_FIXED_DATE {
        civil_from_days(days)
    } else {
        julian_date(fixed)
    };
    let seconds = time / 1000;
    let year = if year > 0 { year } else { 1 - year };
    (
        year,
        month,
        day,
        (seconds / 3600) as u32,
        (seconds % 3600 / 60) as u32,
        (seconds % 60) as u32,
    )
}

/// The proleptic Gregorian year, month and day of the day `days` after 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

/// `JulianCalendar.getCalendarDateFromFixedDate`: normalized year, month and day.
fn julian_date(fixed: i64) -> (i64, u32, u32) {
    let fd = 4 * (fixed - JULIAN_EPOCH) + 1464;
    let year = fd.div_euclid(1461) as i32;
    let mut prior_days = (fixed - julian_fixed_date(year, 1, 1)) as i32;
    let leap = year % 4 == 0;
    if fixed >= julian_fixed_date(year, 3, 1) {
        prior_days += if leap { 1 } else { 2 };
    }
    let month = (12 * prior_days + 373).div_euclid(367);
    let day = (fixed - julian_fixed_date(year, month, 1)) as u32 + 1;
    (i64::from(year), month as u32, day)
}

/// A `SimpleDateFormat` of the pattern with its time zone: the default zone until a zone name was parsed.
pub struct DateFormat {
    zone: Mutex<FormatZone>,
}

/// The time zone of a format after a zone name was parsed.
struct FormatZone {
    /// The id of the zone a parsed name left in the format (the default zone when `None`).
    named: Option<&'static str>,
    /// The rule cache of that zone's `TimeZone` object (the default zone's while `named` is `None`).
    cache: Option<RuleCache>,
}

/// The default time zone of a [`DateFormat::format_in_default_zone`].
#[derive(Clone, Copy)]
pub enum DefaultOffset<'a> {
    /// A fixed offset from UTC in seconds (a `SimpleTimeZone`).
    Fixed(i32),
    /// The id of a zone of the JDK.
    Zone(&'a str),
    /// A zone only the C library knows (for example a POSIX `TZ` string).
    ///
    /// shortcut: its offsets come from `localtime_r` and it has no rule cache, so a date it looks up never changes the
    /// answer for the next one.
    Libc,
}

/// `BanListEntry.DATE_FORMAT`, shared by every ban list.
pub static DATE_FORMAT: DateFormat = DateFormat::new();

impl DateFormat {
    pub const fn new() -> Self {
        Self {
            zone: Mutex::new(FormatZone {
                named: None,
                cache: None,
            }),
        }
    }

    /// `parse(text)`: the date, `None` when the text does not start with a date.
    pub fn parse(&self, text: &str) -> Option<Date> {
        self.parse_in_default_zone(text, default_zone_id().as_deref())
    }

    /// [`parse`](Self::parse) when the id of the default time zone is `default_zone`.
    pub fn parse_in_default_zone(&self, text: &str, default_zone: Option<&str>) -> Option<Date> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut state = self.zone.lock().unwrap_or_else(|e| e.into_inner());
        let state = &mut *state;
        let parsed = parse_millis(&units, default_zone, state.named, &mut state.cache)?;
        if let Some(named) = parsed.named {
            state.named = named;
        }
        Some(Date::from_millis(parsed.millis))
    }

    /// `format(date)` in the default time zone (the zone of a parsed name after one was parsed).
    pub fn format(&self, date: &Date) -> String {
        let id = default_zone_id();
        let default = match id.as_deref() {
            Some(id) if zone_info(id).is_some() => DefaultOffset::Zone(id),
            _ => DefaultOffset::Libc,
        };
        self.format_in_default_zone(date, default)
    }

    /// [`format`](Self::format) when the default time zone is `default`.
    pub fn format_in_default_zone(&self, date: &Date, default: DefaultOffset) -> String {
        let mut state = self.zone.lock().unwrap_or_else(|e| e.into_inner());
        let state = &mut *state;
        let zone = match (state.named, default) {
            (Some(named), _) => zone_info(named),
            (None, DefaultOffset::Zone(id)) => zone_info(id),
            _ => None,
        };
        let offset = match (zone, default) {
            (Some(zone), _) => {
                let (raw, dst) = zone.offsets_at_utc(date.millis(), &mut state.cache);
                raw + dst
            }
            (None, DefaultOffset::Fixed(seconds)) => seconds * 1000,
            (None, _) => {
                super::player_list::local_offset_seconds(date.millis().div_euclid(1000)) * 1000
            }
        };
        let (year, month, day, hour, minute, second) = calendar_fields(date.millis(), offset);
        // `Z`: whole minutes, truncated toward zero
        let minutes = offset / 60_000;
        let sign = if minutes >= 0 { '+' } else { '-' };
        format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} {sign}{:02}{:02}",
            minutes.abs() / 60,
            minutes.abs() % 60
        )
    }
}

impl Default for DateFormat {
    fn default() -> Self {
        Self::new()
    }
}
