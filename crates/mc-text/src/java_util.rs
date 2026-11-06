//! Small re-implementations of the Java library behaviour the codecs depend on:
//! `java.net.URI` syntax validation, `UUID.fromString`, chat character filtering.

/// `StringUtil.isAllowedChatCharacter` for one UTF-16 code unit.
pub fn is_allowed_chat_character(c: u16) -> bool {
    c != 167 && c >= 32 && c != 127
}

/// `ExtraCodecs.CHAT_STRING`.
pub fn validate_chat_string(s: &str) -> Result<(), String> {
    for unit in s.encode_utf16() {
        if !is_allowed_chat_character(unit) {
            return Err(format!(
                "Disallowed chat character: '{}'",
                char::from_u32(unit as u32).unwrap_or('\u{fffd}')
            ));
        }
    }
    Ok(())
}

/// `StringUtil.isValidPlayerName`.
pub fn is_valid_player_name(name: &str) -> bool {
    name.encode_utf16().count() <= 16 && name.encode_utf16().all(|c| c > 32 && c < 127)
}

fn is_unreserved(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_.!~*'()".contains(c)
}

/// Java `URI` "other" characters: non-ASCII that are neither space separators nor ISO controls.
fn is_other(c: char) -> bool {
    !c.is_ascii() && !c.is_control() && !c.is_whitespace()
}

/// Checks `s` consists only of characters accepted by `allowed` plus `%XX` escapes.
fn check_chars(s: &str, what: &str, allowed: impl Fn(char) -> bool) -> Result<(), String> {
    let mut it = s.char_indices();
    while let Some((i, c)) = it.next() {
        if c == '%' {
            let rest = &s[i + 1..];
            if rest.len() < 2 || !rest.as_bytes()[..2].iter().all(u8::is_ascii_hexdigit) {
                return Err(format!("Malformed escape pair at index {i}: {s}"));
            }
            it.next();
            it.next();
        } else if !(allowed(c) || is_other(c)) {
            return Err(format!("Illegal character in {what} at index {i}: {s}"));
        }
    }
    Ok(())
}

fn is_ipv6_authority(auth: &str) -> bool {
    let host_part = auth.rsplit_once('@').map_or(auth, |(_, h)| h);
    let Some(rest) = host_part.strip_prefix('[') else {
        return false;
    };
    let Some((addr, tail)) = rest.split_once(']') else {
        return false;
    };
    let port_ok = tail.is_empty()
        || tail
            .strip_prefix(':')
            .is_some_and(|p| p.bytes().all(|b| b.is_ascii_digit()));
    !addr.is_empty()
        && addr
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
        && port_ok
}

/// `Util.parseAndValidateUntrustedUri` (`ExtraCodecs.UNTRUSTED_URI`): `new URI(s)` must parse and the
/// scheme must be `http`/`https`. The URI grammar follows `java.net.URI` (RFC 2396/2732).
pub fn validate_untrusted_uri(s: &str) -> Result<(), String> {
    let Some(colon) = s
        .find([':', '/', '?', '#'])
        .filter(|&i| s.as_bytes()[i] == b':')
    else {
        // Without a scheme `new URI` may still succeed but vanilla then rejects it.
        check_hierarchical(s)?;
        return Err(format!("Missing protocol in URI: {s}"));
    };
    if colon == 0 {
        return Err(format!("Expected scheme name at index 0: {s}"));
    }
    let scheme = &s[..colon];
    let mut sc = scheme.chars();
    let first_ok = sc.next().is_some_and(|c| c.is_ascii_alphabetic());
    if !first_ok || !sc.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) {
        return Err(format!("Illegal character in scheme name: {s}"));
    }
    let rest = &s[colon + 1..];
    if rest.is_empty() {
        return Err(format!(
            "Expected scheme-specific part at index {}: {s}",
            colon + 1
        ));
    }
    if rest.starts_with('/') {
        check_hierarchical(rest)?;
    } else {
        // Opaque URI: scheme-specific part (URIC) and optional fragment.
        let (ssp, frag) = rest.split_once('#').map_or((rest, ""), |(a, b)| (a, b));
        check_chars(ssp, "opaque part", |c| {
            is_unreserved(c) || ";/?:@&=+$,[]".contains(c)
        })?;
        check_chars(frag, "fragment", |c| {
            is_unreserved(c) || ";/?:@&=+$,[]".contains(c)
        })?;
    }
    let lower = scheme.to_ascii_lowercase();
    if lower != "http" && lower != "https" {
        return Err(format!("Unsupported protocol in URI: {s}"));
    }
    Ok(())
}

fn check_hierarchical(rest: &str) -> Result<(), String> {
    let tail = if let Some(after) = rest.strip_prefix("//") {
        let q = after.find(['/', '?', '#']).unwrap_or(after.len());
        let authority = &after[..q];
        if authority.is_empty() {
            if q == after.len() {
                return Err(format!("Expected authority: {rest}"));
            }
        } else if authority.contains(['[', ']']) {
            if !is_ipv6_authority(authority) {
                return Err(format!("Illegal character in authority: {rest}"));
            }
        } else {
            check_chars(authority, "authority", |c| {
                is_unreserved(c) || "$,;:@&=+".contains(c)
            })?;
        }
        &after[q..]
    } else {
        rest
    };
    let (before_frag, frag) = tail
        .split_once('#')
        .map_or((tail, None), |(a, b)| (a, Some(b)));
    let (path, query) = before_frag
        .split_once('?')
        .map_or((before_frag, None), |(a, b)| (a, Some(b)));
    check_chars(path, "path", |c| {
        is_unreserved(c) || ":@&=+$,;/".contains(c)
    })?;
    let uric = |c: char| is_unreserved(c) || ";/?:@&=+$,[]".contains(c);
    if let Some(q) = query {
        check_chars(q, "query", uric)?;
    }
    if let Some(f) = frag {
        check_chars(f, "fragment", uric)?;
    }
    Ok(())
}

/// `Long.parseLong(s, 16)` over the whole string (optional sign, no overflow).
fn parse_long_hex(s: &str) -> Option<i64> {
    let (neg, digits) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mag = u64::from_str_radix(digits, 16).ok()?;
    if neg {
        0i64.checked_sub_unsigned(mag)
    } else {
        i64::try_from(mag).ok()
    }
}

/// `UUID.fromString` (returns `most << 64 | least`).
pub fn java_uuid_from_string(name: &str) -> Option<u128> {
    let b = name.as_bytes();
    if name.len() > 36 {
        return None;
    }
    if name.len() == 36 {
        if !(b[8] == b'-' && b[13] == b'-' && b[18] == b'-' && b[23] == b'-') {
            return java_uuid_slow(name);
        }
        let hex = |r: std::ops::Range<usize>| -> Option<u64> {
            let s = name.get(r)?;
            s.bytes()
                .all(|c| c.is_ascii_hexdigit())
                .then(|| u64::from_str_radix(s, 16).ok())
                .flatten()
        };
        let msb = (hex(0..8)? << 32) | (hex(9..13)? << 16) | hex(14..18)?;
        let lsb = (hex(19..23)? << 48) | hex(24..36)?;
        return Some(((msb as u128) << 64) | lsb as u128);
    }
    java_uuid_slow(name)
}

fn java_uuid_slow(name: &str) -> Option<u128> {
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let p = |i: usize| parse_long_hex(parts[i]);
    let mut msb = (p(0)? as u64) & 0xffff_ffff;
    msb = (msb << 16) | ((p(1)? as u64) & 0xffff);
    msb = (msb << 16) | ((p(2)? as u64) & 0xffff);
    let mut lsb = (p(3)? as u64) & 0xffff;
    lsb = (lsb << 48) | ((p(4)? as u64) & 0xffff_ffff_ffff);
    Some(((msb as u128) << 64) | lsb as u128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_parsing() {
        assert_eq!(
            java_uuid_from_string("1-1-1-1-1"),
            Some((0x1_0001_0001u128 << 64) | (1u128 << 48) | 1)
        );
        assert!(java_uuid_from_string("zz").is_none());
        assert!(java_uuid_from_string("123e4567-e89b-12d3-a456-426614174000").is_some());
    }
}
