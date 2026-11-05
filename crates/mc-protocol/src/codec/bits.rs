//! `java.util.BitSet` and the vanilla ways of putting it on the wire.
use crate::codec::EncodeResult;
use std::marker::PhantomData;

use super::{take, Decode, DecodeResult, Encode, EncodeError, LongArray};

/// `java.util.BitSet`. Invariant: the last word is non-zero (so `words` is the `toLongArray()` form).
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub fn new() -> Self {
        Self::default()
    }

    fn trim(&mut self) {
        while self.words.last() == Some(&0) {
            self.words.pop();
        }
    }

    pub fn set(&mut self, i: usize) {
        self.set_to(i, true)
    }

    pub fn set_to(&mut self, i: usize, v: bool) {
        let w = i / 64;
        if v {
            if self.words.len() <= w {
                self.words.resize(w + 1, 0);
            }
            self.words[w] |= 1 << (i % 64);
        } else if w < self.words.len() {
            self.words[w] &= !(1 << (i % 64));
            self.trim();
        }
    }

    pub fn get(&self, i: usize) -> bool {
        self.words.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 != 0)
    }

    /// Highest set bit + 1 (`BitSet.length()`).
    pub fn length(&self) -> usize {
        self.words.last().map_or(0, |w| self.words.len() * 64 - w.leading_zeros() as usize)
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn cardinality(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// `BitSet.valueOf(long[])`.
    pub fn from_longs(longs: &[i64]) -> Self {
        let mut b = Self { words: longs.iter().map(|&l| l as u64).collect() };
        b.trim();
        b
    }

    /// `BitSet.toLongArray()`.
    pub fn to_longs(&self) -> Vec<i64> {
        self.words.iter().map(|&w| w as i64).collect()
    }

    /// `BitSet.valueOf(byte[])` (little-endian bit order).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut b = Self { words: bytes.chunks(8).map(|c| c.iter().rev().fold(0u64, |a, &x| a << 8 | u64::from(x))).collect() };
        b.trim();
        b
    }

    /// `BitSet.toByteArray()`: just enough bytes for `length()`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.length().div_ceil(8);
        let mut out: Vec<u8> = self.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        out.truncate(n);
        out
    }

    pub fn iter_ones(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.length()).filter(|&i| self.get(i))
    }
}

/// `FriendlyByteBuf.writeBitSet`: `long[]` form.
impl Encode for BitSet {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        LongArray(self.to_longs()).encode(out)?;
        Ok(())
    }
}
impl Decode for BitSet {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self::from_longs(&LongArray::decode(input)?.0))
    }
}

/// `FriendlyByteBuf.writeFixedBitSet`: `ceil(size/8)` bytes; a set longer than `size` is rejected.
pub(crate) fn write_fixed_bit_set(out: &mut Vec<u8>, bits: &BitSet, size: usize) -> EncodeResult<()> {
    if bits.length() > size {
        return Err(EncodeError(format!("BitSet is larger than expected size ({}>{size})", bits.length())));
    }
    let mut b = bits.to_bytes();
    b.resize(size.div_ceil(8), 0);
    out.extend_from_slice(&b);
    Ok(())
}

/// `ByteBufCodecs.BIT_SET`: `byte[]` form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BitSetBytes(pub BitSet);

impl Encode for BitSetBytes {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.0.to_bytes().encode(out)?;
        Ok(())
    }
}
impl Decode for BitSetBytes {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self(BitSet::from_bytes(&Vec::<u8>::decode(input)?)))
    }
}

/// `ByteBufCodecs.fixedBitSet(N)`: exactly `ceil(N/8)` bytes, no prefix. Encoding bits at or above `N`
/// is a vanilla `EncoderException`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixedBitSet<const N: usize>(pub BitSet);

impl<const N: usize> Encode for FixedBitSet<N> {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_fixed_bit_set(out, &self.0, N)
    }
}
impl<const N: usize> Decode for FixedBitSet<N> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self(BitSet::from_bytes(take(input, N.div_ceil(8))?)))
    }
}

/// Enums usable in an [`EnumSet`]: declaration-order ordinals.
pub trait EnumValues: Copy {
    const COUNT: usize;
    fn ordinal(self) -> usize;
    fn from_ordinal(i: usize) -> Self;
}

/// `FriendlyByteBuf.writeEnumSet`: fixed bit set over the enum's ordinals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumSet<E: EnumValues> {
    bits: BitSet,
    _e: PhantomData<E>,
}

impl<E: EnumValues> Default for EnumSet<E> {
    fn default() -> Self {
        Self { bits: BitSet::new(), _e: PhantomData }
    }
}

impl<E: EnumValues> EnumSet<E> {
    pub fn insert(&mut self, e: E) {
        self.bits.set(e.ordinal())
    }
    pub fn contains(&self, e: E) -> bool {
        self.bits.get(e.ordinal())
    }
    pub fn iter(&self) -> impl Iterator<Item = E> + '_ {
        self.bits.iter_ones().map(E::from_ordinal)
    }
}

impl<E: EnumValues> FromIterator<E> for EnumSet<E> {
    fn from_iter<I: IntoIterator<Item = E>>(it: I) -> Self {
        let mut s = Self::default();
        it.into_iter().for_each(|e| s.insert(e));
        s
    }
}

impl<E: EnumValues> Encode for EnumSet<E> {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_fixed_bit_set(out, &self.bits, E::COUNT)
    }
}
impl<E: EnumValues> Decode for EnumSet<E> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let mut bits = BitSet::from_bytes(take(input, E::COUNT.div_ceil(8))?);
        // `readEnumSet` only looks at the first COUNT bits.
        for i in E::COUNT..bits.length() {
            bits.set_to(i, false);
        }
        Ok(Self { bits, _e: PhantomData })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{from_bytes, to_bytes};

    #[test]
    fn bitset_forms() {
        let mut b = BitSet::new();
        b.set(1);
        b.set(70);
        assert_eq!(b.length(), 71);
        assert_eq!(to_bytes(&b).unwrap(), [2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0x40]);
        assert_eq!(BitSet::from_bytes(&b.to_bytes()), b);
        assert_eq!(b.to_bytes().len(), 9);
        assert_eq!(from_bytes::<BitSet>(&to_bytes(&b).unwrap()).unwrap(), b);
        let f = FixedBitSet::<20>(BitSet::from_bytes(&[1, 0, 8]));
        assert_eq!(to_bytes(&f).unwrap(), [1, 0, 8]);
        assert_eq!(from_bytes::<FixedBitSet<20>>(&[1, 0, 8]).unwrap(), f);
    }
}
