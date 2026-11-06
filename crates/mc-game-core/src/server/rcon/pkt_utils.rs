//! `PktUtils` and `NetworkDataOutputStream`.

/// `PktUtils.MAX_PACKET_SIZE`.
pub const MAX_PACKET_SIZE: usize = 1460;

/// `PktUtils.stringFromByteArray`: the UTF-8 text from `offset` up to the first NUL (or `length - 1`). `None` where
/// Java throws (`offset` past `length - 1`).
pub fn string_from_byte_array(b: &[u8], offset: usize, length: usize) -> Option<String> {
    let max = length.checked_sub(1)?;
    if offset > max || max >= b.len() {
        return None;
    }
    let mut i = offset;
    while b[i] != 0 && i < max {
        i += 1;
    }
    Some(String::from_utf8_lossy(&b[offset..i]).into_owned())
}

/// `PktUtils.intFromByteArray`: little endian, `0` when fewer than four bytes remain before `length`.
pub fn int_from_byte_array(b: &[u8], offset: usize, length: usize) -> i32 {
    if length < offset + 4 || b.len() < offset + 4 {
        return 0;
    }
    i32::from_le_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]])
}

/// `PktUtils.intFromNetworkByteArray`: big endian.
pub fn int_from_network_byte_array(b: &[u8], offset: usize, length: usize) -> i32 {
    if length < offset + 4 || b.len() < offset + 4 {
        return 0;
    }
    i32::from_be_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]])
}

/// `PktUtils.toHexString`: two lowercase hex digits.
pub fn to_hex_string(b: u8) -> String {
    format!("{b:02x}")
}

/// `NetworkDataOutputStream`: little-endian primitives, NUL-terminated strings.
#[derive(Default)]
pub struct NetworkDataOutputStream {
    buf: Vec<u8>,
}

impl NetworkDataOutputStream {
    pub fn new(size: usize) -> Self {
        NetworkDataOutputStream {
            buf: Vec::with_capacity(size),
        }
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn write_string(&mut self, data: &str) {
        self.buf.extend_from_slice(data.as_bytes());
        self.buf.push(0);
    }

    /// `write(int)`: the low byte.
    pub fn write(&mut self, data: i32) {
        self.buf.push(data as u8);
    }

    pub fn write_short(&mut self, data: i16) {
        self.buf.extend_from_slice(&data.to_le_bytes());
    }

    pub fn write_int(&mut self, data: i32) {
        self.buf.extend_from_slice(&data.to_le_bytes());
    }

    pub fn write_float(&mut self, data: f32) {
        self.buf.extend_from_slice(&data.to_bits().to_le_bytes());
    }

    pub fn to_byte_array(&self) -> Vec<u8> {
        self.buf.clone()
    }

    pub fn reset(&mut self) {
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkt_utils_semantics() {
        assert_eq!(int_from_byte_array(&[1, 0, 0, 0, 9], 0, 5), 1);
        assert_eq!(int_from_byte_array(&[1, 0, 0], 0, 3), 0);
        assert_eq!(int_from_byte_array(&[0xff, 0xff, 0xff, 0xff], 0, 4), -1);
        assert_eq!(int_from_network_byte_array(&[0, 0, 1, 0], 0, 4), 256);
        assert_eq!(
            string_from_byte_array(b"ab\0cd", 0, 5).as_deref(),
            Some("ab")
        );
        assert_eq!(
            string_from_byte_array(b"abcd", 0, 4).as_deref(),
            Some("abc")
        ); // `i < length - 1`
        assert_eq!(string_from_byte_array(b"\0\0", 0, 2).as_deref(), Some(""));
        assert_eq!(string_from_byte_array(b"abcd", 5, 4), None);
        assert_eq!(to_hex_string(0x0a), "0a");
    }

    #[test]
    fn output_stream_layout() {
        let mut o = NetworkDataOutputStream::new(16);
        o.write(0);
        o.write_string("hi");
        o.write_short(0x1234);
        o.write_int(1);
        assert_eq!(
            o.to_byte_array(),
            [0, b'h', b'i', 0, 0x34, 0x12, 1, 0, 0, 0]
        );
    }
}
