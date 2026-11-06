//! `LastSeenMessages`, `LastSeenTrackedEntry`, `LastSeenMessagesTracker`, `LastSeenMessagesValidator`.

use super::message_signature::{self, MessageSignature};
use super::message_signature_cache::{IdOutOfRange, MessageSignatureCache};
use mc_protocol::codec::BitSet;
use mc_protocol::packets::play::{LastSeenMessagesPacked, LastSeenMessagesUpdate};

/// `LastSeenMessages.LAST_SEEN_MESSAGES_MAX_LENGTH`.
pub const LAST_SEEN_MESSAGES_MAX_LENGTH: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LastSeenMessages {
    pub entries: Vec<MessageSignature>,
}

impl LastSeenMessages {
    /// `LastSeenMessages.EMPTY`.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn update_signature(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.entries.len() as i32).to_be_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&entry.0);
        }
    }

    pub fn pack(&self, cache: &MessageSignatureCache) -> LastSeenMessagesPacked {
        LastSeenMessagesPacked {
            entries: self
                .entries
                .iter()
                .map(|e| message_signature::pack(e, cache))
                .collect(),
        }
    }

    /// `computeChecksum`: `31 * checksum + entry.checksum()` over the entries, as a byte (0 becomes 1).
    pub fn compute_checksum(&self) -> i8 {
        let checksum = self.entries.iter().fold(1i32, |c, e| {
            c.wrapping_mul(31)
                .wrapping_add(message_signature::checksum(e))
        });
        match checksum as i8 {
            0 => 1,
            b => b,
        }
    }
}

/// `LastSeenMessages.Packed.unpack`: `Ok(None)` when an entry is not in the cache.
pub fn unpack(
    packed: &LastSeenMessagesPacked,
    cache: &MessageSignatureCache,
) -> Result<Option<LastSeenMessages>, IdOutOfRange> {
    let mut entries = Vec::with_capacity(packed.entries.len());
    for entry in &packed.entries {
        match message_signature::unpack(entry, cache)? {
            Some(signature) => entries.push(signature),
            None => return Ok(None),
        }
    }
    Ok(Some(LastSeenMessages { entries }))
}

/// `LastSeenMessages.Update.verifyChecksum`.
pub fn verify_checksum(update: &LastSeenMessagesUpdate, last_seen: &LastSeenMessages) -> bool {
    update.checksum == 0 || update.checksum == last_seen.compute_checksum()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastSeenTrackedEntry {
    pub signature: MessageSignature,
    pub pending: bool,
}

impl LastSeenTrackedEntry {
    pub fn acknowledge(&self) -> Self {
        if self.pending {
            Self {
                signature: self.signature.clone(),
                pending: false,
            }
        } else {
            self.clone()
        }
    }
}

/// `LastSeenMessagesTracker.Update`.
pub struct TrackerUpdate {
    pub last_seen: LastSeenMessages,
    pub update: LastSeenMessagesUpdate,
}

/// Client side of the window; the server only needs it for `ServerPlayer`-signed commands/tests, kept for parity.
pub struct LastSeenMessagesTracker {
    tracked_messages: Vec<Option<LastSeenTrackedEntry>>,
    tail: usize,
    offset: i32,
    last_tracked_message: Option<MessageSignature>,
}

impl LastSeenMessagesTracker {
    pub fn new(last_seen_count: usize) -> Self {
        Self {
            tracked_messages: vec![None; last_seen_count],
            tail: 0,
            offset: 0,
            last_tracked_message: None,
        }
    }

    pub fn add_pending(&mut self, message: &MessageSignature, was_shown: bool) -> bool {
        if self.last_tracked_message.as_ref() == Some(message) {
            return false;
        }
        self.last_tracked_message = Some(message.clone());
        self.add_entry(was_shown.then(|| LastSeenTrackedEntry {
            signature: message.clone(),
            pending: true,
        }));
        true
    }

    fn add_entry(&mut self, entry: Option<LastSeenTrackedEntry>) {
        let index = self.tail;
        self.tail = (index + 1) % self.tracked_messages.len();
        self.offset += 1;
        self.tracked_messages[index] = entry;
    }

    pub fn ignore_pending(&mut self, pending: &MessageSignature) {
        for slot in &mut self.tracked_messages {
            if slot
                .as_ref()
                .is_some_and(|e| e.pending && &e.signature == pending)
            {
                *slot = None;
                break;
            }
        }
    }

    pub fn get_and_clear_offset(&mut self) -> i32 {
        std::mem::take(&mut self.offset)
    }

    pub fn generate_and_apply_update(&mut self) -> TrackerUpdate {
        let offset = self.get_and_clear_offset();
        let mut acknowledged = BitSet::new();
        let mut entries = Vec::with_capacity(self.tracked_messages.len());
        let n = self.tracked_messages.len();
        for i in 0..n {
            let index = (self.tail + i) % n;
            if let Some(message) = &self.tracked_messages[index] {
                acknowledged.set(i);
                entries.push(message.signature.clone());
                self.tracked_messages[index] = Some(message.acknowledge());
            }
        }
        let last_seen = LastSeenMessages { entries };
        let update = LastSeenMessagesUpdate {
            offset,
            acknowledged: mc_protocol::codec::FixedBitSet(acknowledged),
            checksum: last_seen.compute_checksum(),
        };
        TrackerUpdate { last_seen, update }
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }
}

/// `LastSeenMessagesValidator.ValidationException` (message = the kick reason text).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ValidationException(pub String);

pub struct LastSeenMessagesValidator {
    last_seen_count: usize,
    tracked_messages: Vec<Option<LastSeenTrackedEntry>>,
    last_pending_message: Option<MessageSignature>,
}

impl LastSeenMessagesValidator {
    pub fn new(last_seen_count: usize) -> Self {
        Self {
            last_seen_count,
            tracked_messages: vec![None; last_seen_count],
            last_pending_message: None,
        }
    }

    pub fn add_pending(&mut self, message: &MessageSignature) {
        if self.last_pending_message.as_ref() != Some(message) {
            self.tracked_messages.push(Some(LastSeenTrackedEntry {
                signature: message.clone(),
                pending: true,
            }));
            self.last_pending_message = Some(message.clone());
        }
    }

    pub fn tracked_messages_count(&self) -> usize {
        self.tracked_messages.len()
    }

    pub fn apply_offset(&mut self, offset: i32) -> Result<(), ValidationException> {
        let max_offset = (self.tracked_messages.len() - self.last_seen_count) as i32;
        if offset >= 0 && offset <= max_offset {
            self.tracked_messages.drain(..offset as usize);
            Ok(())
        } else {
            Err(ValidationException(format!(
                "Advanced last seen window by {offset} messages, but expected at most {max_offset}"
            )))
        }
    }

    pub fn apply_update(
        &mut self,
        update: &LastSeenMessagesUpdate,
    ) -> Result<LastSeenMessages, ValidationException> {
        self.apply_offset(update.offset)?;
        let acknowledged = &update.acknowledged.0;
        let mut entries = Vec::with_capacity(acknowledged.cardinality());
        if acknowledged.length() > self.last_seen_count {
            return Err(ValidationException(format!(
                "Last seen update contained {} messages, but maximum window size is {}",
                acknowledged.length(),
                self.last_seen_count
            )));
        }
        for i in 0..self.last_seen_count {
            let message = self.tracked_messages[i].clone();
            if acknowledged.get(i) {
                let Some(message) = message else {
                    return Err(ValidationException(format!(
                        "Last seen update acknowledged unknown or previously ignored message at index {i}"
                    )));
                };
                self.tracked_messages[i] = Some(message.acknowledge());
                entries.push(message.signature);
            } else {
                if let Some(message) = &message
                    && !message.pending
                {
                    return Err(ValidationException(format!(
                        "Last seen update ignored previously acknowledged message at index {i} and signature {}",
                        message_signature::to_base64(&message.signature)
                    )));
                }
                self.tracked_messages[i] = None;
            }
        }
        let last_seen = LastSeenMessages { entries };
        if !verify_checksum(update, &last_seen) {
            return Err(ValidationException(
                "Checksum mismatch on last seen update: the client and server must have desynced"
                    .to_owned(),
            ));
        }
        Ok(last_seen)
    }
}
