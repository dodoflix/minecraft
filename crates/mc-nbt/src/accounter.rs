//! `net.minecraft.nbt.NbtAccounter`: byte quota and depth limit while reading NBT.

use crate::error::{NbtError, Result};

pub const DEFAULT_NBT_QUOTA: i64 = 2_097_152;
pub const UNCOMPRESSED_NBT_QUOTA: i64 = 104_857_600;
pub const MAX_STACK_DEPTH: i32 = 512;

#[derive(Debug, Clone)]
pub struct NbtAccounter {
    quota: i64,
    usage: i64,
    max_depth: i32,
    depth: i32,
}

impl NbtAccounter {
    pub fn new(quota: i64, max_depth: i32) -> Self {
        Self {
            quota,
            usage: 0,
            max_depth,
            depth: 0,
        }
    }

    pub fn create(quota: i64) -> Self {
        Self::new(quota, MAX_STACK_DEPTH)
    }

    pub fn default_quota() -> Self {
        Self::new(DEFAULT_NBT_QUOTA, MAX_STACK_DEPTH)
    }

    pub fn uncompressed_quota() -> Self {
        Self::new(UNCOMPRESSED_NBT_QUOTA, MAX_STACK_DEPTH)
    }

    pub fn unlimited_heap() -> Self {
        Self::new(i64::MAX, MAX_STACK_DEPTH)
    }

    /// `accountBytes(bytesPerEntry, count)`.
    pub fn account_bytes_n(&mut self, bytes_per_entry: i64, count: i64) -> Result<()> {
        self.account_bytes(bytes_per_entry.wrapping_mul(count))
    }

    pub fn account_bytes(&mut self, size: i64) -> Result<()> {
        if size < 0 {
            return Err(NbtError::IllegalArgument(format!(
                "Tried to account NBT tag with negative size: {size}"
            )));
        }
        // Java long arithmetic wraps.
        if self.usage.wrapping_add(size) > self.quota {
            return Err(NbtError::Accounter(format!(
                "Tried to read NBT tag that was too big; tried to allocate: {} + {} bytes where max allowed: {}",
                self.usage, size, self.quota
            )));
        }
        self.usage = self.usage.wrapping_add(size);
        Ok(())
    }

    pub fn push_depth(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(NbtError::Accounter(format!(
                "Tried to read NBT tag with too high complexity, depth > {}",
                self.max_depth
            )));
        }
        self.depth += 1;
        Ok(())
    }

    pub fn pop_depth(&mut self) -> Result<()> {
        if self.depth <= 0 {
            return Err(NbtError::Accounter(
                "NBT-Accounter tried to pop stack-depth at top-level".to_string(),
            ));
        }
        self.depth -= 1;
        Ok(())
    }

    pub fn quota(&self) -> i64 {
        self.quota
    }

    pub fn usage(&self) -> i64 {
        self.usage
    }

    pub fn depth(&self) -> i32 {
        self.depth
    }
}
