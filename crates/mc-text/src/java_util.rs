//! Small re-implementations of the Java library behaviour the codecs depend on:
//! `java.net.URI` syntax validation, chat character filtering.

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

/// The UTF-16 index in `full` of byte `byte` of `part`, a sub-slice of `full` (Java `URISyntaxException` indexes are
/// into the whole input).
fn at(full: &str, part: &str, byte: usize) -> usize {
    let start = part.as_ptr() as usize - full.as_ptr() as usize + byte;
    full[..start].encode_utf16().count()
}

/// Checks `s`, a sub-slice of `full`, consists only of characters accepted by `allowed` plus `%XX` escapes.
fn check_chars(
    full: &str,
    s: &str,
    what: &str,
    allowed: impl Fn(char) -> bool,
) -> Result<(), String> {
    let mut it = s.char_indices();
    while let Some((i, c)) = it.next() {
        if c == '%' {
            let rest = &s[i + 1..];
            if rest.len() < 2 || !rest.as_bytes()[..2].iter().all(u8::is_ascii_hexdigit) {
                return Err(format!(
                    "Malformed escape pair at index {}: {full}",
                    at(full, s, i)
                ));
            }
            it.next();
            it.next();
        } else if !(allowed(c) || is_other(c)) {
            return Err(format!(
                "Illegal character in {what} at index {}: {full}",
                at(full, s, i)
            ));
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
        check_hierarchical(s, s)?;
        return Err(format!("Missing protocol in URI: {s}: {s}"));
    };
    if colon == 0 {
        return Err(format!("Expected scheme name at index 0: {s}"));
    }
    let scheme = &s[..colon];
    let illegal = scheme.char_indices().find(|&(i, c)| {
        !(c.is_ascii_alphabetic() || (i > 0 && (c.is_ascii_digit() || "+-.".contains(c))))
    });
    if let Some((i, _)) = illegal {
        return Err(format!(
            "Illegal character in scheme name at index {}: {s}",
            at(s, s, i)
        ));
    }
    let rest = &s[colon + 1..];
    if rest.is_empty() {
        return Err(format!(
            "Expected scheme-specific part at index {}: {s}",
            at(s, s, colon + 1)
        ));
    }
    if rest.starts_with('/') {
        check_hierarchical(s, rest)?;
    } else {
        // Opaque URI: scheme-specific part (URIC) and optional fragment.
        let (ssp, frag) = rest.split_once('#').map_or((rest, ""), |(a, b)| (a, b));
        check_chars(s, ssp, "opaque part", |c| {
            is_unreserved(c) || ";/?:@&=+$,[]".contains(c)
        })?;
        check_chars(s, frag, "fragment", |c| {
            is_unreserved(c) || ";/?:@&=+$,[]".contains(c)
        })?;
    }
    let lower = scheme.to_ascii_lowercase();
    if lower != "http" && lower != "https" {
        return Err(format!("Unsupported protocol in URI: {s}: {s}"));
    }
    Ok(())
}

/// `rest` is the part of `full` after the scheme (or all of it).
fn check_hierarchical(full: &str, rest: &str) -> Result<(), String> {
    let tail = if let Some(after) = rest.strip_prefix("//") {
        let q = after.find(['/', '?', '#']).unwrap_or(after.len());
        let authority = &after[..q];
        if authority.is_empty() {
            if q == after.len() {
                return Err(format!(
                    "Expected authority at index {}: {full}",
                    at(full, rest, 2)
                ));
            }
        } else if authority.contains(['[', ']']) {
            if !is_ipv6_authority(authority) {
                return Err(format!("Illegal character in authority: {full}"));
            }
        } else {
            check_chars(full, authority, "authority", |c| {
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
    check_chars(full, path, "path", |c| {
        is_unreserved(c) || ":@&=+$,;/".contains(c)
    })?;
    let uric = |c: char| is_unreserved(c) || ";/?:@&=+$,[]".contains(c);
    if let Some(q) = query {
        check_chars(full, q, "query", uric)?;
    }
    if let Some(f) = frag {
        check_chars(full, f, "fragment", uric)?;
    }
    Ok(())
}
