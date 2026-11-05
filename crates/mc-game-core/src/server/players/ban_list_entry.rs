//! `BanListEntry`, `UserBanListEntry`, `IpBanListEntry`.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, SubsecRound, Utc};
use mc_text::Component;

use super::gson::{JsonError, JsonObject};
use super::name_and_id::NameAndId;
use super::stored_user_list::StoredUserEntry;

pub type Date = DateTime<Utc>;

pub const EXPIRES_NEVER: &str = "forever";

/// `new Date()` (millisecond precision like `java.util.Date`).
pub fn now() -> Date {
    Utc::now().trunc_subsecs(3)
}

/// `BanListEntry.DATE_FORMAT.format` (`yyyy-MM-dd HH:mm:ss Z`, default time zone).
pub fn format_date(date: &Date) -> String {
    date.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S %z").to_string()
}

/// `BanListEntry.DATE_FORMAT.parse`: `yyyy-MM-dd HH:mm:ss` plus a zone (`+hhmm`, `+hh:mm`, `GMT+hh:mm`, `GMT`, `UTC`).
/// shortcut: SimpleDateFormat's lenient field roll-over (e.g. month 13) is not reproduced and rejects the string.
pub fn parse_date(s: &str) -> Option<Date> {
    let mut parts = s.split_whitespace();
    let (date, time, zone) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let mut d = date.splitn(3, '-').map(|p| p.parse::<u32>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.splitn(3, ':').map(|p| p.parse::<u32>().ok());
    let (h, mi, sec) = (t.next()??, t.next()??, t.next()??);
    let naive: NaiveDateTime = NaiveDate::from_ymd_opt(y as i32, m, day)?.and_hms_opt(h, mi, sec)?;
    let zone = zone.strip_prefix("GMT").or_else(|| zone.strip_prefix("UTC")).unwrap_or(zone);
    let offset_secs = if zone.is_empty() {
        0
    } else {
        let sign = match zone.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let digits: String = zone[1..].chars().filter(|c| *c != ':').collect();
        if !digits.bytes().all(|b| b.is_ascii_digit()) || digits.is_empty() || digits.len() > 4 {
            return None;
        }
        let n: i32 = digits.parse().ok()?;
        let (hh, mm) = if digits.len() <= 2 { (n, 0) } else { (n / 100, n % 100) };
        sign * (hh * 3600 + mm * 60)
    };
    Some(DateTime::from_naive_utc_and_offset(naive - chrono::Duration::seconds(offset_secs as i64), Utc))
}

/// The user type of a ban list (`NameAndId` or an ip `String`).
pub trait BanUser: Clone + PartialEq + Sized {
    fn key(&self) -> String;
    fn from_json(object: &JsonObject) -> Result<Option<Self>, JsonError>;
    fn append_to(&self, object: &mut JsonObject);
}

impl BanUser for NameAndId {
    fn key(&self) -> String {
        self.id.to_string()
    }

    fn from_json(object: &JsonObject) -> Result<Option<Self>, JsonError> {
        NameAndId::from_json(object)
    }

    fn append_to(&self, object: &mut JsonObject) {
        NameAndId::append_to(self, object)
    }
}

impl BanUser for String {
    fn key(&self) -> String {
        self.clone()
    }

    /// `IpBanListEntry.createIpInfo`.
    fn from_json(object: &JsonObject) -> Result<Option<Self>, JsonError> {
        object.get("ip").map(|ip| ip.as_string()).transpose()
    }

    fn append_to(&self, object: &mut JsonObject) {
        object.add_property_str("ip", Some(self));
    }
}

/// `BanListEntry<T>`.
#[derive(Clone, Debug)]
pub struct BanListEntry<T: BanUser> {
    user: Option<T>,
    pub created: Date,
    pub source: String,
    pub expires: Option<Date>,
    pub reason: Option<String>,
}

pub type UserBanListEntry = BanListEntry<NameAndId>;
pub type IpBanListEntry = BanListEntry<String>;

impl<T: BanUser> BanListEntry<T> {
    /// Full constructor (`created`/`source` default like vanilla when `None`).
    pub fn new(user: Option<T>, created: Option<Date>, source: Option<&str>, expires: Option<Date>, reason: Option<&str>) -> Self {
        BanListEntry {
            user,
            created: created.unwrap_or_else(now),
            source: source.unwrap_or("(Unknown)").to_owned(),
            expires,
            reason: reason.map(str::to_owned),
        }
    }

    /// `new UserBanListEntry(user)` / `new IpBanListEntry(address)`.
    pub fn of(user: T) -> Self {
        Self::new(Some(user), None, None, None, None)
    }

    fn from_object(user: Option<T>, object: &JsonObject) -> Result<Self, JsonError> {
        let created = match object.get("created") {
            Some(c) => parse_date(&c.as_string()?).unwrap_or_else(now),
            None => now(),
        };
        let source = match object.get("source") {
            Some(s) => s.as_string()?,
            None => "(Unknown)".to_owned(),
        };
        let expires = match object.get("expires") {
            Some(e) => parse_date(&e.as_string()?),
            None => None,
        };
        let reason = object.get("reason").map(|r| r.as_string()).transpose()?;
        Ok(BanListEntry { user, created, source, expires, reason })
    }
}

impl<T: BanUser> StoredUserEntry for BanListEntry<T> {
    type User = T;

    fn user(&self) -> Option<&T> {
        self.user.as_ref()
    }

    fn key_for_user(user: &T) -> String {
        user.key()
    }

    fn has_expired(&self) -> bool {
        self.expires.is_some_and(|e| e < now())
    }

    fn serialize(&self, object: &mut JsonObject) {
        let Some(user) = &self.user else { return };
        user.append_to(object);
        object.add_property_str("created", Some(&format_date(&self.created)));
        object.add_property_str("source", Some(&self.source));
        object.add_property_str("expires", Some(&self.expires.as_ref().map_or(EXPIRES_NEVER.to_owned(), format_date)));
        object.add_property_str("reason", self.reason.as_deref());
    }

    fn from_json(object: &JsonObject) -> Result<Self, JsonError> {
        Self::from_object(T::from_json(object)?, object)
    }

    fn equals(&self, other: &Self) -> bool {
        self.source == other.source && self.expires == other.expires && self.reason == other.reason && self.user == other.user
    }
}

impl UserBanListEntry {
    /// `getDisplayName`.
    pub fn get_display_name(&self) -> Component {
        match &self.user {
            Some(user) => Component::literal(user.name.clone()),
            None => Component::translatable("commands.banlist.entry.unknown", vec![]),
        }
    }
}

impl IpBanListEntry {
    /// `getDisplayName` (`String.valueOf(user)`).
    pub fn get_display_name(&self) -> Component {
        Component::literal(self.user.clone().unwrap_or_else(|| "null".to_owned()))
    }
}

impl<T: BanUser> BanListEntry<T> {
    /// `getReasonMessage`.
    pub fn get_reason_message(&self) -> Component {
        match &self.reason {
            Some(reason) => Component::literal(reason.clone()),
            None => Component::translatable("multiplayer.disconnect.banned.reason.default", vec![]),
        }
    }
}
