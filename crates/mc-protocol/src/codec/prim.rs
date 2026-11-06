//! Primitive codecs: numbers, strings, options, collections.
use super::{
    Decode, DecodeBounded, DecodeError, DecodeResult, Encode, EncodeBounded, EncodeError,
    EncodeResult, read_count, read_varint, take, take_array, write_varint,
};

macro_rules! be_num {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            #[inline]
            fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> { out.extend_from_slice(&self.to_be_bytes()); Ok(()) }
        }
        impl Decode for $t {
            #[inline]
            fn decode(input: &mut &[u8]) -> DecodeResult<Self> { Ok(<$t>::from_be_bytes(take_array(input)?)) }
        }
    )*};
}
be_num!(i16, u16, i32, u32, i64, u64, f32, f64);

impl Encode for u8 {
    #[inline]
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        out.push(*self);
        Ok(())
    }
    fn encode_slice(items: &[Self], out: &mut Vec<u8>) -> EncodeResult<()> {
        out.extend_from_slice(items);
        Ok(())
    }
    /// `ByteBufCodecs.byteArray(max)` encode: the check, then `FriendlyByteBuf.writeByteArray`.
    fn encode_vec_bounded(items: &[Self], out: &mut Vec<u8>, max: usize) -> EncodeResult<()> {
        if items.len() > max {
            return Err(EncodeError(format!(
                "ByteArray with size {} is bigger than allowed {max}",
                items.len()
            )));
        }
        write_varint(out, items.len() as i32);
        out.extend_from_slice(items);
        Ok(())
    }
}
impl Decode for u8 {
    #[inline]
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(take(input, 1)?[0])
    }
    /// `FriendlyByteBuf.readByteArray(buf, max)`.
    fn decode_vec(input: &mut &[u8], max: usize) -> DecodeResult<Vec<Self>> {
        let max = if max == i32::MAX as usize {
            input.len()
        } else {
            max
        };
        let size = read_varint(input)?;
        if size < 0 {
            return Err(DecodeError::Message(size.to_string())); // NegativeArraySizeException
        }
        if size as usize > max {
            return Err(DecodeError::Message(format!(
                "ByteArray with size {size} is bigger than allowed {max}"
            )));
        }
        Ok(take(input, size as usize)?.to_vec())
    }
}
impl Encode for i8 {
    #[inline]
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        out.push(*self as u8);
        Ok(())
    }
}
impl Decode for i8 {
    #[inline]
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(take(input, 1)?[0] as i8)
    }
}
impl Encode for bool {
    #[inline]
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        out.push(*self as u8);
        Ok(())
    }
}
impl Decode for bool {
    #[inline]
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(take(input, 1)?[0] != 0)
    }
}
impl Encode for () {
    fn encode(&self, _: &mut Vec<u8>) -> EncodeResult<()> {
        Ok(())
    }
}
impl Decode for () {
    fn decode(_: &mut &[u8]) -> DecodeResult<Self> {
        Ok(())
    }
}

macro_rules! tuple {
    ($($n:ident),+) => {
        impl<$($n: Encode),+> Encode for ($($n,)+) {
            #[allow(non_snake_case)]
            fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> { let ($($n,)+) = self; $($n.encode(out)?;)+ Ok(()) }
        }
        impl<$($n: Decode),+> Decode for ($($n,)+) {
            fn decode(input: &mut &[u8]) -> DecodeResult<Self> { Ok(($($n::decode(input)?,)+)) }
        }
    };
}
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);

// ---- strings -------------------------------------------------------------------------------

/// Vanilla `STRING_UTF8` / `FriendlyByteBuf.readUtf()` default maximum, in UTF-16 code units.
pub const MAX_STRING_LENGTH: usize = 32767;

/// Length in UTF-16 code units (Java `String.length()`).
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `Utf8String.write`: `max_length` in UTF-16 code units; the encoded size is bounded by `3 * max_length` bytes
/// (`ByteBufUtil.utf8MaxBytes`). Nothing is written on error.
pub fn write_utf(out: &mut Vec<u8>, s: &str, max_length: usize) -> EncodeResult<()> {
    let chars = utf16_len(s);
    if chars > max_length {
        return Err(EncodeError(format!(
            "String too big (was {chars} characters, max {max_length})"
        )));
    }
    let max_encoded = max_length as i64 * 3;
    if s.len() as i64 > max_encoded {
        return Err(EncodeError(format!(
            "String too big (was {} bytes encoded, max {max_encoded})",
            s.len()
        )));
    }
    write_varint(out, s.len() as i32);
    out.extend_from_slice(s.as_bytes());
    Ok(())
}

/// `ByteBufCodecs.writeCount`: VarInt element count, rejected when above `max`.
pub fn write_count(out: &mut Vec<u8>, count: usize, max: usize) -> EncodeResult<()> {
    if count > max {
        return Err(EncodeError(format!(
            "{count} elements exceeded max size of: {max}"
        )));
    }
    write_varint(out, count as i32);
    Ok(())
}

/// `Utf8String.read`: `max_length` in UTF-16 code units.
pub fn read_utf(input: &mut &[u8], max_length: usize) -> DecodeResult<String> {
    let max_encoded = max_length as i64 * 3;
    let len = i64::from(read_varint(input)?);
    if len > max_encoded {
        return Err(DecodeError::Message(format!(
            "The received encoded string buffer length is longer than maximum allowed ({len} > {max_encoded})"
        )));
    }
    if len < 0 {
        return Err(DecodeError::msg(
            "The received encoded string buffer length is less than zero! Weird string!",
        ));
    }
    let available = input.len();
    if len as usize > available {
        return Err(DecodeError::Message(format!(
            "Not enough bytes in buffer, expected {len}, but got {available}"
        )));
    }
    let s = String::from_utf8_lossy(take(input, len as usize)?).into_owned();
    let chars = utf16_len(&s);
    if chars > max_length {
        return Err(DecodeError::Message(format!(
            "The received string length is longer than maximum allowed ({chars} > {max_length})"
        )));
    }
    Ok(s)
}

impl Encode for str {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_utf(out, self, MAX_STRING_LENGTH)
    }
}
impl Encode for String {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_utf(out, self, MAX_STRING_LENGTH)
    }
}
impl EncodeBounded for str {
    fn encode_bounded(&self, out: &mut Vec<u8>, max: usize) -> EncodeResult<()> {
        write_utf(out, self, max)
    }
}
impl EncodeBounded for String {
    fn encode_bounded(&self, out: &mut Vec<u8>, max: usize) -> EncodeResult<()> {
        write_utf(out, self, max)
    }
}
impl Decode for String {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_utf(input, MAX_STRING_LENGTH)
    }
}
impl DecodeBounded for String {
    fn decode_bounded(input: &mut &[u8], max: usize) -> DecodeResult<Self> {
        read_utf(input, max)
    }
}

// ---- option / collections ------------------------------------------------------------------

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match self {
            Some(v) => {
                out.push(1);
                v.encode(out)
            }
            None => {
                out.push(0);
                Ok(())
            }
        }
    }
}
impl<T: EncodeBounded> EncodeBounded for Option<T> {
    fn encode_bounded(&self, out: &mut Vec<u8>, max: usize) -> EncodeResult<()> {
        match self {
            Some(v) => {
                out.push(1);
                v.encode_bounded(out, max)
            }
            None => {
                out.push(0);
                Ok(())
            }
        }
    }
}
impl<T: Decode> Decode for Option<T> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if bool::decode(input)? {
            T::decode(input).map(Some)
        } else {
            Ok(None)
        }
    }
}
impl<T: DecodeBounded> DecodeBounded for Option<T> {
    fn decode_bounded(input: &mut &[u8], max: usize) -> DecodeResult<Self> {
        if bool::decode(input)? {
            T::decode_bounded(input, max).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl<T: Encode> Encode for [T] {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.len() as i32);
        T::encode_slice(self, out)
    }
}
impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.as_slice().encode(out)
    }
}
impl<T: Encode> EncodeBounded for [T] {
    fn encode_bounded(&self, out: &mut Vec<u8>, max: usize) -> EncodeResult<()> {
        T::encode_vec_bounded(self, out, max)
    }
}
impl<T: Encode> EncodeBounded for Vec<T> {
    fn encode_bounded(&self, out: &mut Vec<u8>, max: usize) -> EncodeResult<()> {
        T::encode_vec_bounded(self, out, max)
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        T::decode_vec(input, i32::MAX as usize)
    }
}
impl<T: Decode> DecodeBounded for Vec<T> {
    fn decode_bounded(input: &mut &[u8], max: usize) -> DecodeResult<Self> {
        T::decode_vec(input, max)
    }
}

/// `fixedSizeCollection`: no length prefix.
impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        T::encode_slice(self, out)
    }
}
impl<T: Decode, const N: usize> Decode for [T; N] {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let mut v = Vec::with_capacity(N);
        for _ in 0..N {
            v.push(T::decode(input)?);
        }
        Ok(v.try_into()
            .unwrap_or_else(|_| unreachable!("exactly N elements")))
    }
}

/// `FriendlyByteBuf.readLongArray`: VarInt count, bounded by the readable bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LongArray(pub Vec<i64>);

impl Encode for LongArray {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.0.len() as i32);
        for l in &self.0 {
            l.encode(out)?;
        }
        Ok(())
    }
}
impl Decode for LongArray {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let size = read_count(input, i32::MAX as usize)?;
        let max = input.len() / 8;
        if size > max {
            return Err(DecodeError::Message(format!(
                "LongArray with size {size} is bigger than allowed {max}"
            )));
        }
        (0..size)
            .map(|_| i64::decode(input))
            .collect::<DecodeResult<_>>()
            .map(LongArray)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{from_bytes, to_bytes};

    #[test]
    fn utf16_limits() {
        // U+1F600 is 2 UTF-16 units, 4 UTF-8 bytes.
        let s = "\u{1F600}\u{1F600}";
        let b = to_bytes(s).unwrap();
        assert_eq!(b[0], 8);
        assert_eq!(read_utf(&mut &b[..], 4).unwrap(), s);
        assert_eq!(
            read_utf(&mut &b[..], 3),
            Err(DecodeError::msg(
                "The received string length is longer than maximum allowed (4 > 3)"
            ))
        );
        assert_eq!(
            read_utf(&mut &b[..], 2),
            Err(DecodeError::msg(
                "The received encoded string buffer length is longer than maximum allowed (8 > 6)"
            ))
        );
        assert_eq!(
            read_utf(&mut &[0x05u8, b'a'][..], 10),
            Err(DecodeError::msg(
                "Not enough bytes in buffer, expected 5, but got 1"
            ))
        );
        assert_eq!(
            read_utf(&mut &[0xffu8, 0xff, 0xff, 0xff, 0x0f][..], 10),
            Err(DecodeError::msg(
                "The received encoded string buffer length is less than zero! Weird string!"
            ))
        );
    }

    #[test]
    fn collections() {
        let v: Vec<u8> = vec![1, 2, 3];
        assert_eq!(to_bytes(&v).unwrap(), [3, 1, 2, 3]);
        assert_eq!(from_bytes::<Vec<u8>>(&[3, 1, 2, 3]).unwrap(), v);
        assert_eq!(
            <Vec<u8>>::decode_bounded(&mut &[3u8, 1, 2, 3][..], 2),
            Err(DecodeError::msg(
                "ByteArray with size 3 is bigger than allowed 2"
            ))
        );
        assert_eq!(
            <Vec<i32>>::decode_bounded(&mut &[3u8, 0, 0, 0, 1][..], 2),
            Err(DecodeError::msg("3 elements exceeded max size of: 2"))
        );
        assert_eq!(from_bytes::<Option<i16>>(&[1, 0, 5]).unwrap(), Some(5));
        assert_eq!(to_bytes(&None::<i16>).unwrap(), [0]);
        assert_eq!(from_bytes::<[u8; 2]>(&[7, 8]).unwrap(), [7, 8]);
    }
}
