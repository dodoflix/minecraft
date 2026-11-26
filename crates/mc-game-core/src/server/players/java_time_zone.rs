//! `sun.util.calendar.ZoneInfo`: the history of a time zone of the JDK (its transitions up to 2100 and the
//! `SimpleTimeZone` rule that follows), which `SimpleDateFormat` uses to place a local time in a zone it parsed by name and
//! to print an instant in the zone it left in the format. The data is `java_zone_rules.tsv.gz`, written by
//! `ZoneRulesGen.java` from the JDK the vanilla server runs on (Temurin 25.0.5.1, tzdata 2026f). A JDK with other tz
//! data changes the transitions and names: regenerate this file, `java_zone_strings.tsv` and the date golden together.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, LazyLock};

const ONE_DAY: i64 = 86_400_000;
/// `GregorianCalendar.DEFAULT_GREGORIAN_CUTOVER`: 1582-10-15 in epoch milliseconds.
const CUTOVER_MILLIS: i64 = -12_219_292_800_000;
/// `GregorianCalendar.EPOCH_OFFSET`: the fixed date of 1970-01-01.
const EPOCH_OFFSET: i64 = 719_163;
const OFFSET_MASK: i64 = 0x0f;
const DST_NSHIFT: u32 = 4;
const TRANSITION_NSHIFT: u32 = 12;

/// How `ZoneInfo.getOffsets` reads its date.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Time {
    Utc,
    Wall,
}

/// The mode of the start or end rule of a `SimpleTimeZone`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    DayOfMonth,
    DayOfWeekInMonth,
    DayOfWeekAfterDayOfMonth,
    DayOfWeekBeforeDayOfMonth,
}

/// The `WALL_TIME`, `STANDARD_TIME` and `UTC_TIME` of `SimpleTimeZone`.
const STANDARD_TIME: i32 = 1;
const UTC_TIME: i32 = 2;

#[derive(Clone)]
struct Rule {
    month: i32,
    day: i32,
    day_of_week: i32,
    time: i32,
    time_mode: i32,
    mode: Mode,
}

impl Rule {
    /// `SimpleTimeZone.decodeStartRule` / `decodeEndRule` of rules the JDK built itself (always valid).
    fn decode(month: i32, day: i32, day_of_week: i32, time: i32, time_mode: i32) -> Rule {
        let (mode, day, day_of_week) = if day_of_week == 0 {
            (Mode::DayOfMonth, day, 0)
        } else if day_of_week > 0 {
            (Mode::DayOfWeekInMonth, day, day_of_week)
        } else if day > 0 {
            (Mode::DayOfWeekAfterDayOfMonth, day, -day_of_week)
        } else {
            (Mode::DayOfWeekBeforeDayOfMonth, -day, -day_of_week)
        };
        Rule {
            month,
            day,
            day_of_week,
            time,
            time_mode,
            mode,
        }
    }

    /// `SimpleTimeZone.getTransition`: the UTC time of the rule in `year` of the Julian or Gregorian calendar,
    /// `time_of_day` being its time in the day.
    fn transition(&self, julian: bool, year: i32, time_of_day: i32) -> i64 {
        let fixed = |day: i64| {
            if julian {
                super::ban_date_format::julian_fixed_date(year, self.month + 1, day)
            } else {
                super::ban_date_format::gregorian_fixed_date(year, self.month + 1, day)
            }
        };
        let nth_day_of_week = |nth: i64, fd: i64| {
            let on_or_before = |fd: i64| {
                let from = fd - (i64::from(self.day_of_week) - 1);
                fd - from.rem_euclid(7)
            };
            if nth > 0 {
                7 * nth + on_or_before(fd - 1)
            } else {
                7 * nth + on_or_before(fd + 7)
            }
        };
        let day = i64::from(self.day);
        let fixed_date = match self.mode {
            Mode::DayOfMonth => fixed(day),
            Mode::DayOfWeekInMonth => {
                let first = if self.day < 0 {
                    fixed(i64::from(month_length(julian, year, self.month)))
                } else {
                    fixed(1)
                };
                nth_day_of_week(day, first)
            }
            Mode::DayOfWeekAfterDayOfMonth => nth_day_of_week(1, fixed(day)),
            Mode::DayOfWeekBeforeDayOfMonth => nth_day_of_week(-1, fixed(day)),
        };
        (fixed_date - EPOCH_OFFSET)
            .wrapping_mul(ONE_DAY)
            .wrapping_add(i64::from(time_of_day))
    }
}

/// The days of `month` (0 to 11) of a Julian or Gregorian `year`.
fn month_length(julian: bool, year: i32, month: i32) -> i32 {
    const DAYS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = year % 4 == 0 && (julian || year % 100 != 0 || year % 400 == 0);
    DAYS[month as usize] + i32::from(month == 1 && leap)
}

/// The `SimpleTimeZone` of `ZoneInfo.getLastRuleInstance`.
struct SimpleTimeZone {
    raw_offset: i32,
    dst_savings: i32,
    start: Rule,
    end: Rule,
    use_daylight: bool,
}

/// `SimpleTimeZone.Cache`: the last year whose transitions were computed. It stays in the `TimeZone` object, so the
/// dates a format looked up before change the answer for the later ones (the transitions of an extreme year wrap and
/// make the cached range cover almost every date).
#[derive(Clone, Copy)]
pub struct RuleCache {
    year: i64,
    start: i64,
    end: i64,
}

impl SimpleTimeZone {
    /// `SimpleTimeZone.getOffset(long)` for the dates after the last transition of a zone (a date before the Gregorian
    /// cutover is a wrapped one and reads the Julian calendar).
    fn offset(&self, date: i64, cache: &mut Option<RuleCache>) -> i32 {
        let raw = self.raw_offset;
        if !self.use_daylight {
            return raw;
        }
        if let Some(c) = cache
            && date >= c.start
            && date < c.end
        {
            return raw + self.dst_savings;
        }
        let local = date.wrapping_add(i64::from(raw));
        let julian = date < CUTOVER_MILLIS;
        let fixed = local.div_euclid(ONE_DAY) + EPOCH_OFFSET;
        let year = if julian {
            super::ban_date_format::julian_year(fixed)
        } else {
            super::ban_date_format::gregorian_year(fixed)
        };
        if year < 0 {
            return raw;
        }
        if let Some(c) = cache
            && i64::from(year) == c.year
        {
            return raw;
        }
        let start = |year: i32| {
            let time = if self.start.time_mode == UTC_TIME {
                self.start.time
            } else {
                self.start.time - raw
            };
            self.start.transition(julian, year, time)
        };
        let end = |year: i32| {
            let mut time = self.end.time;
            if self.end.time_mode != UTC_TIME {
                time -= raw;
            }
            if self.end.time_mode != UTC_TIME && self.end.time_mode != STANDARD_TIME {
                time -= self.dst_savings;
            }
            self.end.transition(julian, year, time)
        };
        let (mut s, mut e) = (start(year), end(year));
        let mut in_dst = false;
        if s <= e {
            in_dst = date >= s && date < e;
            *cache = Some(RuleCache {
                year: i64::from(year),
                start: s,
                end: e,
            });
        } else {
            if date < e {
                s = start(year - 1);
                in_dst = date >= s;
            } else if date >= s {
                e = end(year + 1);
                in_dst = date < e;
            }
            if s <= e {
                // `startYear - 1`
                *cache = Some(RuleCache {
                    year: -1,
                    start: s,
                    end: e,
                });
            }
        }
        raw + if in_dst { self.dst_savings } else { 0 }
    }
}

/// A zone of the JDK.
pub struct ZoneInfo {
    raw_offset: i32,
    pub dst_savings: i32,
    offsets: Vec<i32>,
    /// `utc millis << 12 | dst index << 4 | offset index`; none for a zone without transitions.
    transitions: Option<Vec<i64>>,
    last_rule: Option<SimpleTimeZone>,
}

impl ZoneInfo {
    /// `ZoneInfo.getOffsets(utc, offsets)`: the raw offset and the DST saving at an instant.
    pub fn offsets_at_utc(&self, utc: i64, cache: &mut Option<RuleCache>) -> (i32, i32) {
        self.offsets_of(utc, Time::Utc, cache)
    }

    /// `ZoneInfo.getOffsetsByWall(wall, offsets)`: the raw offset and the DST saving of a local time.
    pub fn offsets_at_wall(&self, wall: i64, cache: &mut Option<RuleCache>) -> (i32, i32) {
        self.offsets_of(wall, Time::Wall, cache)
    }

    fn of_transition(&self, val: i64) -> (i32, i32) {
        let offset = self.offsets[(val & OFFSET_MASK) as usize];
        let dst = ((val >> DST_NSHIFT) & 0xf) as usize;
        let save = if dst == 0 { 0 } else { self.offsets[dst] };
        (offset - save, save)
    }

    fn offsets_of(&self, date: i64, kind: Time, cache: &mut Option<RuleCache>) -> (i32, i32) {
        let Some(transitions) = &self.transitions else {
            return (self.raw_offset, 0);
        };
        let index = self.transition_index(transitions, date, kind);
        // prior to the transition table the raw offset (no LMT)
        let Some(index) = index else {
            return (self.raw_offset, 0);
        };
        if let Some(&val) = transitions.get(index) {
            return self.of_transition(val);
        }
        // beyond the transitions the rule that follows, else the last transition
        let Some(rule) = &self.last_rule else {
            return self.of_transition(transitions[transitions.len() - 1]);
        };
        let mut millis = date;
        if kind != Time::Utc {
            millis = millis.wrapping_sub(i64::from(self.raw_offset));
        }
        let mut dst = rule.offset(millis, cache) - rule.raw_offset;
        // in a standard-to-daylight transition
        if dst > 0
            && rule.offset(millis.wrapping_sub(i64::from(dst)), cache) == rule.raw_offset
            && kind == Time::Wall
        {
            dst = 0;
        }
        (rule.raw_offset, dst)
    }

    /// `ZoneInfo.getTransitionIndex`: `None` before the first transition, the table length after the last.
    fn transition_index(&self, transitions: &[i64], date: i64, kind: Time) -> Option<usize> {
        let (mut low, mut high) = (0i64, transitions.len() as i64 - 1);
        while low <= high {
            let mid = (low + high) / 2;
            let val = transitions[mid as usize];
            let mut mid_val = val >> TRANSITION_NSHIFT;
            if kind != Time::Utc {
                mid_val += i64::from(self.offsets[(val & OFFSET_MASK) as usize]);
            }
            match mid_val.cmp(&date) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid - 1,
                std::cmp::Ordering::Equal => return Some(mid as usize),
            }
        }
        usize::try_from(low - 1).ok().map(|before| {
            if low as usize >= transitions.len() {
                low as usize
            } else {
                before
            }
        })
    }
}

fn numbers<T: std::str::FromStr>(field: &str) -> Option<Vec<T>> {
    match field {
        "-" => None,
        "." => Some(Vec::new()),
        _ => Some(field.split(',').filter_map(|n| n.parse().ok()).collect()),
    }
}

fn parse_zone(fields: &[&str]) -> ZoneInfo {
    let raw_offset: i32 = fields[0].parse().expect("raw offset");
    let dst_savings: i32 = fields[1].parse().expect("DST saving");
    let params = numbers::<i32>(fields[4]).map(|p| {
        // 8 parameters: the times are wall times; 10: the time modes follow the times
        let (start, end) = if p.len() == 10 {
            (
                Rule::decode(p[0], p[1], p[2], p[3], p[4]),
                Rule::decode(p[5], p[6], p[7], p[8], p[9]),
            )
        } else {
            (
                Rule::decode(p[0], p[1], p[2], p[3], 0),
                Rule::decode(p[4], p[5], p[6], p[7], 0),
            )
        };
        SimpleTimeZone {
            raw_offset,
            dst_savings,
            use_daylight: start.day != 0 && end.day != 0,
            start,
            end,
        }
    });
    ZoneInfo {
        raw_offset,
        dst_savings,
        offsets: numbers(fields[2]).unwrap_or_default(),
        transitions: numbers(fields[3]),
        last_rule: params,
    }
}

static ZONES: LazyLock<HashMap<String, Arc<ZoneInfo>>> = LazyLock::new(|| {
    const GZ: &[u8] = include_bytes!("java_zone_rules.tsv.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(GZ)
        .read_to_string(&mut text)
        .expect("zone rules");
    let mut zones: HashMap<String, Arc<ZoneInfo>> = HashMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let zone = if fields[1] == "=" {
            zones[fields[2]].clone()
        } else {
            Arc::new(parse_zone(&fields[1..]))
        };
        zones.insert(fields[0].to_owned(), zone);
    }
    zones
});

/// `TimeZone.getTimeZone(id)` for the zones of the JDK's zone strings.
pub fn zone_info(id: &str) -> Option<Arc<ZoneInfo>> {
    ZONES.get(id).cloned()
}
