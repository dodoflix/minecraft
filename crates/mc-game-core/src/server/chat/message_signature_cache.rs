//! `net.minecraft.network.chat.MessageSignatureCache`: the 128-entry signature cache both ends keep in sync.

use super::message_signature::MessageSignature;
use super::signed_message::SignedMessageBody;
use std::collections::{HashSet, VecDeque};

pub const NOT_FOUND: i32 = -1;
const DEFAULT_CAPACITY: usize = 128;

/// An unpack id outside `0..capacity` (vanilla: `ArrayIndexOutOfBoundsException` while handling the packet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdOutOfRange(pub i32);

pub struct MessageSignatureCache {
    entries: Vec<Option<MessageSignature>>,
}

impl MessageSignatureCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: vec![None; capacity],
        }
    }

    pub fn create_default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    pub fn pack(&self, signature: &MessageSignature) -> i32 {
        self.entries
            .iter()
            .position(|e| e.as_ref() == Some(signature))
            .map_or(NOT_FOUND, |i| i as i32)
    }

    pub fn unpack(&self, id: i32) -> Result<Option<MessageSignature>, IdOutOfRange> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.entries.get(i))
            .cloned()
            .ok_or(IdOutOfRange(id))
    }

    pub fn push_body(&mut self, body: &SignedMessageBody, signature: Option<&MessageSignature>) {
        let mut queue: VecDeque<MessageSignature> =
            body.last_seen.entries.iter().cloned().collect();
        if let Some(signature) = signature {
            queue.push_back(signature.clone());
        }
        self.push_queue(queue);
    }

    pub fn push(&mut self, entries: &[MessageSignature]) {
        self.push_queue(entries.iter().cloned().collect());
    }

    fn push_queue(&mut self, mut queue: VecDeque<MessageSignature>) {
        let new_entries: HashSet<MessageSignature> = queue.iter().cloned().collect();
        let mut i = 0;
        while !queue.is_empty() && i < self.entries.len() {
            let entry = self.entries[i].take();
            self.entries[i] = queue.pop_back();
            if let Some(entry) = entry
                && !new_entries.contains(&entry)
            {
                queue.push_front(entry);
            }
            i += 1;
        }
    }
}
