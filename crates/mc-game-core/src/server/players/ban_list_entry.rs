//! `BanListEntry`, `UserBanListEntry`, `IpBanListEntry`.

use chrono::{DateTime, Utc};
use mc_text::Component;

use super::ban_date_format::DATE_FORMAT;
use super::gson::{JsonElement, JsonError, JsonObject};
use super::name_and_id::NameAndId;
use super::stored_user_list::StoredUserEntry;

/// `java.util.Date`: epoch milliseconds of any `long`, which `chrono::DateTime` (years up to 262142) cannot hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i64);

impl Date {
    pub const fn from_millis(millis: i64) -> Date {
        Date(millis)
    }

    pub const fn millis(self) -> i64 {
        self.0
    }

    /// The date as a `chrono` value, `None` outside its range.
    pub fn to_utc(self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.0)
    }
}

impl From<DateTime<Utc>> for Date {
    fn from(date: DateTime<Utc>) -> Date {
        Date(date.timestamp_millis())
    }
}

pub const EXPIRES_NEVER: &str = "forever";

/// `new Date()` (millisecond precision like `java.util.Date`).
pub fn now() -> Date {
    Date(Utc::now().timestamp_millis())
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

/// `getAsString` of a ban list field, which the entries keep as text (a lone surrogate shows as U+FFFD).
fn lossy_string(e: &JsonElement) -> Result<String, JsonError> {
    e.as_string()
        .map(mc_core::java_string::JavaString::into_string_lossy)
}

impl BanUser for String {
    fn key(&self) -> String {
        self.clone()
    }

    /// `IpBanListEntry.createIpInfo`.
    fn from_json(object: &JsonObject) -> Result<Option<Self>, JsonError> {
        object.get("ip").map(lossy_string).transpose()
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
    pub fn new(
        user: Option<T>,
        created: Option<Date>,
        source: Option<&str>,
        expires: Option<Date>,
        reason: Option<&str>,
    ) -> Self {
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
            Some(c) => DATE_FORMAT.parse(&lossy_string(c)?).unwrap_or_else(now),
            None => now(),
        };
        let source = match object.get("source") {
            Some(s) => lossy_string(s)?,
            None => "(Unknown)".to_owned(),
        };
        let expires = match object.get("expires") {
            Some(e) => DATE_FORMAT.parse(&lossy_string(e)?),
            None => None,
        };
        let reason = object.get("reason").map(lossy_string).transpose()?;
        Ok(BanListEntry {
            user,
            created,
            source,
            expires,
            reason,
        })
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
        object.add_property_str("created", Some(&DATE_FORMAT.format(&self.created)));
        object.add_property_str("source", Some(&self.source));
        object.add_property_str(
            "expires",
            Some(
                &self
                    .expires
                    .as_ref()
                    .map_or(EXPIRES_NEVER.to_owned(), |expires| {
                        DATE_FORMAT.format(expires)
                    }),
            ),
        );
        object.add_property_str("reason", self.reason.as_deref());
    }

    fn from_json(object: &JsonObject) -> Result<Self, JsonError> {
        Self::from_object(T::from_json(object)?, object)
    }

    fn equals(&self, other: &Self) -> bool {
        self.source == other.source
            && self.expires == other.expires
            && self.reason == other.reason
            && self.user == other.user
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
