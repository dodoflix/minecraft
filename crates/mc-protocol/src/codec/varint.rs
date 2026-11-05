//! `VarInt` / `VarLong` (vanilla `net.minecraft.network.VarInt` / `VarLong`).
use crate::codec::EncodeResult;
use super::{take, Decode, DecodeError, DecodeResult, Encode, VarEncode};

pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    while v & !0x7F != 0 {
        out.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn read_varint(input: &mut &[u8]) -> DecodeResult<i32> {
    let mut out = 0u32;
    let mut bytes = 0;
    loop {
        let b = take(input, 1)?[0];
        out |= u32::from(b & 0x7F).wrapping_shl(bytes * 7); // Java masks the shift count too
        bytes += 1;
        if bytes > 5 {
            return Err(DecodeError::VarIntTooBig);
        }
        if b & 0x80 == 0 {
            return Ok(out as i32);
        }
    }
}

pub fn varint_size(value: i32) -> usize {
    let v = value as u32;
    (1..5).find(|i| v & (u32::MAX << (i * 7)) == 0).unwrap_or(5)
}

pub fn write_varlong(out: &mut Vec<u8>, value: i64) {
    let mut v = value as u64;
    while v & !0x7F != 0 {
        out.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn read_varlong(input: &mut &[u8]) -> DecodeResult<i64> {
    let mut out = 0u64;
    let mut bytes = 0;
    loop {
        let b = take(input, 1)?[0];
        out |= u64::from(b & 0x7F).wrapping_shl(bytes * 7);
        bytes += 1;
        if bytes > 10 {
            return Err(DecodeError::VarLongTooBig);
        }
        if b & 0x80 == 0 {
            return Ok(out as i64);
        }
    }
}

pub fn varlong_size(value: i64) -> usize {
    let v = value as u64;
    (1..10).find(|i| v & (u64::MAX << (i * 7)) == 0).unwrap_or(10)
}

/// A VarInt on the wire (`ByteBufCodecs.VAR_INT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarInt(pub i32);

/// A VarLong on the wire (`ByteBufCodecs.VAR_LONG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarLong(pub i64);

impl Encode for VarInt {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.0);
        Ok(())
    }
}
impl Decode for VarInt {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varint(input).map(VarInt)
    }
}
impl Encode for VarLong {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varlong(out, self.0);
        Ok(())
    }
}
impl Decode for VarLong {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varlong(input).map(VarLong)
    }
}

impl VarEncode for i32 {
    fn encode_var(&self, out: &mut Vec<u8>) {
        write_varint(out, *self)
    }
    fn decode_var(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varint(input)
    }
}
impl VarEncode for i64 {
    fn encode_var(&self, out: &mut Vec<u8>) {
        write_varlong(out, *self)
    }
    fn decode_var(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varlong(input)
    }
}
impl<T: VarEncode> VarEncode for Option<T> {
    fn encode_var(&self, out: &mut Vec<u8>) {
        match self {
            Some(v) => {
                out.push(1);
                v.encode_var(out)
            }
            None => out.push(0),
        }
    }
    fn decode_var(input: &mut &[u8]) -> DecodeResult<Self> {
        if bool::decode(input)? { T::decode_var(input).map(Some) } else { Ok(None) }
    }
}
impl<T: VarEncode> VarEncode for Vec<T> {
    fn encode_var(&self, out: &mut Vec<u8>) {
        write_varint(out, self.len() as i32);
        for v in self {
            v.encode_var(out);
        }
    }
    fn decode_var(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = super::read_count(input, i32::MAX as usize)?;
        let mut v = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            v.push(T::decode_var(input)?);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_edges() {
        for v in [0, 1, 127, 128, 255, 25565, 2097151, i32::MAX, -1, i32::MIN] {
            let mut b = Vec::new();
            write_varint(&mut b, v);
            assert_eq!(b.len(), varint_size(v));
            assert_eq!(read_varint(&mut &b[..]).unwrap(), v);
        }
        let mut b = Vec::new();
        write_varint(&mut b, -1);
        assert_eq!(b, [0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(read_varint(&mut &[0x80u8, 0x80, 0x80, 0x80, 0x80, 0x01][..]), Err(DecodeError::VarIntTooBig));
        assert!(matches!(read_varint(&mut &[0x80u8][..]), Err(DecodeError::Eof { .. })));
    }

    #[test]
    fn varlong_edges() {
        for v in [0, 1, i64::MAX, -1, i64::MIN] {
            let mut b = Vec::new();
            write_varlong(&mut b, v);
            assert_eq!(b.len(), varlong_size(v));
            assert_eq!(read_varlong(&mut &b[..]).unwrap(), v);
        }
        assert_eq!(read_varlong(&mut &[0xffu8; 11][..]), Err(DecodeError::VarLongTooBig));
    }
}
