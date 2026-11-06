//! `LegacyQueryHandler` + `LegacyProtocolUtils`: the pre-1.7 server list ping (`0xFE` forms), answered before the
//! first buffer reaches the frame decoder.

use super::connection::java_socket_address_string;
use super::connection_task::ConnectionContext;
use super::hooks::LegacyServerInfo;

/// `LegacyProtocolUtils.FAKE_PROTOCOL_VERSION`.
const FAKE_PROTOCOL_VERSION: i32 = 127;

pub(super) enum Sniffed {
    /// The first buffer was a legacy ping; it was answered and the connection closed.
    Answered,
    /// Not a legacy ping; the buffer went to the frame decoder.
    Normal,
}

/// Reads the first socket buffer and answers it if it is a legacy ping.
pub(super) async fn sniff(ctx: &mut ConnectionContext) -> Sniffed {
    let first = match ctx.reader.read_first().await {
        Ok(first) => first,
        Err(e) => {
            ctx.fault(e);
            return Sniffed::Answered;
        }
    };
    match parse(&first) {
        Some(kind) => {
            let socket = ctx
                .handle
                .remote_address()
                .map(|a| java_socket_address_string(&a))
                .unwrap_or_default();
            let info = ctx.hooks.legacy_info();
            let body = match kind {
                Kind::PreFourteen => {
                    log::debug!("Ping: (<1.3.x) from {socket}");
                    create_version0_response(&info)
                }
                Kind::OneFour => {
                    log::debug!("Ping: (1.4-1.5.x) from {socket}");
                    create_version1_response(&info)
                }
                Kind::OneSix => {
                    log::debug!("Ping: (1.6) from {socket}");
                    create_version1_response(&info)
                }
            };
            ctx.sender.send_raw(create_legacy_disconnect_packet(&body));
            Sniffed::Answered
        }
        None => {
            ctx.reader.push_first(&first);
            Sniffed::Normal
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    PreFourteen,
    OneFour,
    OneSix,
}

/// `LegacyQueryHandler.channelRead` on the first buffer; `None` = connect normally (including every
/// `RuntimeException` the Java code swallows: a too-short buffer, a bad length).
fn parse(first: &[u8]) -> Option<Kind> {
    let mut input = Reader(first);
    if input.u8()? != 254 {
        return None;
    }
    if input.remaining() == 0 {
        return Some(Kind::PreFourteen);
    }
    if input.u8()? != 1 {
        return None;
    }
    if input.remaining() > 0 {
        read_custom_payload_packet(&mut input)?;
        Some(Kind::OneSix)
    } else {
        Some(Kind::OneFour)
    }
}

/// `readCustomPayloadPacket`: `Some(())` when it is a well-formed `MC|PingHost` packet.
fn read_custom_payload_packet(input: &mut Reader) -> Option<()> {
    if input.u8()? != 250 {
        return None;
    }
    if input.legacy_string()? != "MC|PingHost" {
        return None;
    }
    let payload_size = usize::from(input.u16()?);
    if input.remaining() != payload_size {
        return None;
    }
    if input.u8()? < 73 {
        return None;
    }
    let _host = input.legacy_string()?;
    let port = input.i32()?;
    (port <= 65535).then_some(())
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.0.len()
    }
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let (head, tail) = self.0.split_at_checked(n)?;
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// `LegacyProtocolUtils.readLegacyString`: a signed short char count (negative = exception in Java) then UTF-16BE.
    fn legacy_string(&mut self) -> Option<String> {
        let char_count = i16::from_be_bytes(self.take(2)?.try_into().ok()?);
        let byte_count = usize::try_from(char_count).ok()? * 2;
        let bytes = self.take(byte_count)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        Some(String::from_utf16_lossy(&units))
    }
}

/// `createVersion0Response`: `motd§players§max`.
fn create_version0_response(info: &LegacyServerInfo) -> String {
    format!(
        "{}\u{a7}{}\u{a7}{}",
        info.motd, info.player_count, info.max_players
    )
}

/// `createVersion1Response`: `§1\0127\0version\0motd\0players\0max`.
fn create_version1_response(info: &LegacyServerInfo) -> String {
    format!(
        "\u{a7}1\0{FAKE_PROTOCOL_VERSION}\0{}\0{}\0{}\0{}",
        info.server_version, info.motd, info.player_count, info.max_players
    )
}

/// `createLegacyDisconnectPacket`: `0xFF` + `writeLegacyString` (UTF-16 unit count as a short, UTF-16BE chars).
fn create_legacy_disconnect_packet(reason: &str) -> Vec<u8> {
    let units: Vec<u16> = reason.encode_utf16().collect();
    let mut out = Vec::with_capacity(3 + units.len() * 2);
    out.push(255);
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for u in units {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping_host_packet(protocol: u8, host: &str, port: i32) -> Vec<u8> {
        let host16: Vec<u8> = host.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let mut payload = vec![protocol];
        payload.extend((host.encode_utf16().count() as u16).to_be_bytes());
        payload.extend(&host16);
        payload.extend(port.to_be_bytes());
        let mut out = vec![254, 1, 250];
        let channel: Vec<u8> = "MC|PingHost"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        out.extend(11u16.to_be_bytes());
        out.extend(channel);
        out.extend((payload.len() as u16).to_be_bytes());
        out.extend(payload);
        out
    }

    #[test]
    fn detects_the_three_legacy_forms() {
        assert_eq!(parse(&[254]), Some(Kind::PreFourteen));
        assert_eq!(parse(&[254, 1]), Some(Kind::OneFour));
        assert_eq!(
            parse(&ping_host_packet(74, "localhost", 25565)),
            Some(Kind::OneSix)
        );
    }

    #[test]
    fn rejects_everything_else_like_vanilla() {
        // Not 0xFE, wrong second byte, wrong packet id, old protocol, bad size, port > 65535, truncated.
        assert_eq!(parse(&[0]), None);
        assert_eq!(parse(&[254, 2]), None);
        assert_eq!(parse(&ping_host_packet(72, "localhost", 25565)), None);
        assert_eq!(parse(&ping_host_packet(74, "localhost", 70000)), None);
        let mut bad_size = ping_host_packet(74, "localhost", 25565);
        bad_size.push(0);
        assert_eq!(parse(&bad_size), None);
        assert_eq!(parse(&ping_host_packet(74, "localhost", 25565)[..10]), None);
        // Handshake packets of the modern protocol (first byte is a length) pass through.
        assert_eq!(parse(&[0x10, 0x00, 0xfa, 0x05]), None);
    }

    #[test]
    fn response_format() {
        let info = LegacyServerInfo {
            motd: "A Minecraft Server".into(),
            player_count: 1,
            max_players: 20,
            server_version: "26.3".into(),
        };
        assert_eq!(
            create_version0_response(&info),
            "A Minecraft Server\u{a7}1\u{a7}20"
        );
        assert_eq!(
            create_version1_response(&info),
            "\u{a7}1\x00127\x0026.3\0A Minecraft Server\x001\x0020"
        );
        let packet = create_legacy_disconnect_packet("ab\u{a7}");
        assert_eq!(packet, [255, 0, 3, 0, b'a', 0, b'b', 0, 0xa7]);
    }
}
