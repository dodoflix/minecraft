//! `TextComponentHoverAndClickEventFix`: renames `hoverEvent` / `clickEvent` to `hover_event` / `click_event` and
//! flattens their payloads into the new field layout, dropping click events whose payload is not valid.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::{cast_value, patch_sub_type};
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use crate::value::{Value, mismatch};
use mc_core::math::java::parse_int;
use mc_dfu::Dynamic;

/// `new TextComponentHoverAndClickEventFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "TextComponentHoverAndClickEventFix",
        output_schema,
        true,
        |df| {
            let input_schema = df.input_schema();
            let input_text_component_type = input_schema.get_type(&References::TEXT_COMPONENT);
            let hover_event_type = input_text_component_type.find_field_type("hoverEvent");
            let old_raw_text_component_type =
                input_schema.get_type_raw(&References::TEXT_COMPONENT);
            let new_text_component_type = df.output_schema().get_type(&References::TEXT_COMPONENT);
            let old_text_component_type = DSL::named_type(
                References::TEXT_COMPONENT.type_name(),
                &DSL::or_type(
                    &DSL::or_type(
                        &DSL::string(),
                        &DSL::list_type(&old_raw_text_component_type),
                    ),
                    &DSL::and_type4(
                        &DSL::optional_type(&DSL::field_type(
                            "extra",
                            &DSL::list_type(&old_raw_text_component_type),
                        )),
                        &DSL::optional_type(&DSL::field_type(
                            "separator",
                            &old_raw_text_component_type,
                        )),
                        &DSL::optional_type(&DSL::field_type("hoverEvent", &hover_event_type)),
                        &DSL::remainder_type(),
                    ),
                ),
            );
            if old_text_component_type != input_text_component_type {
                panic!(
                    "java.lang.IllegalStateException: Text component type did not match, expected {old_text_component_type} but got {input_text_component_type}"
                );
            }
            let patched_input_type = patch_sub_type(
                &old_text_component_type,
                &old_text_component_type,
                &new_text_component_type,
            );
            let new_type = new_text_component_type.clone();
            df.fix_type_everywhere_new(
                "TextComponentHoverAndClickEventFix",
                &old_text_component_type,
                &new_text_component_type,
                move |ops, text_component| {
                    let has_hover_or_click = match text_component.second() {
                        Value::Left(_) => false,
                        Value::Right(full) => {
                            let hover_and_remainder = full.second().second();
                            let has_hover = hover_and_remainder.first().is_left();
                            let has_click = hover_and_remainder
                                .second()
                                .as_dynamic()
                                .get("clickEvent")
                                .is_present();
                            has_hover || has_click
                        }
                        other => mismatch("Either", other),
                    };
                    if !has_hover_or_click {
                        text_component
                    } else {
                        write_and_read_typed_or_throw(
                            &cast_value(&patched_input_type, text_component, *ops),
                            &new_type,
                            fix_text_component,
                        )
                        .value
                    }
                },
            )
        },
    )
}

fn fix_text_component(dynamic: Dynamic) -> Dynamic {
    let dynamic = rename_and_fix_field(&dynamic, "hoverEvent", "hover_event", |d| {
        Some(fix_hover_event(d))
    });
    rename_and_fix_field(&dynamic, "clickEvent", "click_event", fix_click_event)
}

/// `renameAndFixField` whose fixer may return `null` (an empty `Optional.map` result drops the field).
fn rename_and_fix_field(
    dynamic: &Dynamic,
    old_field_name: &str,
    new_field_name: &str,
    fixer: impl FnOnce(Dynamic) -> Option<Dynamic>,
) -> Dynamic {
    dynamic.remove(old_field_name).set_field_if_present(
        new_field_name,
        dynamic.get(old_field_name).result().and_then(fixer),
    )
}

fn copy_fields(mut target: Dynamic, source: &Dynamic, fields: &[&str]) -> Dynamic {
    for field in fields {
        target = Dynamic::copy_field(source, field, &target, field);
    }
    target
}

fn fix_hover_event(dynamic: Dynamic) -> Dynamic {
    let action = dynamic.get("action").as_string_or("");
    match action.as_lossy_str() {
        "show_text" => dynamic.rename_field("contents", "value"),
        "show_item" => {
            let contents = dynamic.get("contents").or_else_empty_map();
            copy_fields(
                dynamic.remove("contents"),
                &contents,
                &["id", "count", "components"],
            )
        }
        "show_entity" => {
            let contents = dynamic.get("contents").or_else_empty_map();
            copy_fields(
                dynamic.remove("contents"),
                &contents,
                &["id", "type", "name"],
            )
            .rename_field("id", "uuid")
            .rename_field("type", "id")
        }
        _ => dynamic,
    }
}

fn fix_click_event(dynamic: Dynamic) -> Option<Dynamic> {
    let action = dynamic.get("action").as_string_or("");
    let value = dynamic.get("value").as_string_or("");
    match action.as_lossy_str() {
        "open_url" => {
            validate_uri(value.as_lossy_str()).then(|| dynamic.rename_field("value", "url"))
        }
        "open_file" => Some(dynamic.rename_field("value", "path")),
        "run_command" | "suggest_command" => {
            validate_chat(value.as_lossy_str()).then(|| dynamic.rename_field("value", "command"))
        }
        "change_page" => {
            let old_page = dynamic.get("value").result().and_then(parse_old_page)?;
            let page = old_page.max(1);
            Some(
                dynamic
                    .remove("value")
                    .set("page", dynamic.create_int(page)),
            )
        }
        _ => Some(dynamic),
    }
}

fn parse_old_page(value: Dynamic) -> Option<i32> {
    if let Some(number_value) = value.as_number().result() {
        return Some(number_value.int_value());
    }
    // `Integer.parseInt` failing is caught
    parse_int(value.as_string_or("").as_lossy_str())
}

fn validate_uri(uri: &str) -> bool {
    match UriParser::new(uri).parse() {
        Ok(Some(scheme)) => {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        }
        Ok(None) | Err(UriSyntaxError) => false,
    }
}

fn validate_chat(string: &str) -> bool {
    string
        .encode_utf16()
        .all(|c| !(c == 167 || c < u16::from(b' ') || c == 127))
}

// ---- `java.net.URI` syntax (`new URI(String)`) ----------------------------------------------------------------------

// Character-class masks of `java.net.URI`: bit `c` of the low mask for `c < 64`, bit `c - 64` of the high mask for
// `c < 128`.
const L_DIGIT: u64 = 0x03FF_0000_0000_0000;
const H_DIGIT: u64 = 0;
const L_ALPHA: u64 = 0;
const H_ALPHA: u64 = 0x7FF_FFFE_0000_0000 | 0x7FF_FFFE;
const L_ALPHANUM: u64 = L_DIGIT | L_ALPHA;
const H_ALPHANUM: u64 = H_DIGIT | H_ALPHA;
const L_HEX: u64 = L_DIGIT;
const H_HEX: u64 = 0x7E_0000_007E;
const L_MARK: u64 = 0x6782_0000_0000;
const H_MARK: u64 = 0x4000_0000_8000_0000;
const L_UNRESERVED: u64 = L_ALPHANUM | L_MARK;
const H_UNRESERVED: u64 = H_ALPHANUM | H_MARK;
const L_RESERVED: u64 = 0xAC00_9850_0000_0000;
const H_RESERVED: u64 = 0x2800_0001;
const L_ESCAPED: u64 = 1;
const H_ESCAPED: u64 = 0;
const L_URIC: u64 = L_RESERVED | L_UNRESERVED | L_ESCAPED;
const H_URIC: u64 = H_RESERVED | H_UNRESERVED | H_ESCAPED;
const L_PCHAR: u64 = L_UNRESERVED | L_ESCAPED | 0x2400_1850_0000_0000;
const H_PCHAR: u64 = H_UNRESERVED | H_ESCAPED | 0x1;
const L_PATH: u64 = L_PCHAR | 0x0800_8000_0000_0000;
const H_PATH: u64 = H_PCHAR;
const L_DASH: u64 = 0x2000_0000_0000;
const H_DASH: u64 = 0;
const L_DOT: u64 = 0x4000_0000_0000;
const H_DOT: u64 = 0;
const L_USERINFO: u64 = L_UNRESERVED | L_ESCAPED | 0x2C00_1850_0000_0000;
const H_USERINFO: u64 = H_UNRESERVED | H_ESCAPED;
const L_REG_NAME: u64 = L_UNRESERVED | L_ESCAPED | 0x2C00_1850_0000_0000;
const H_REG_NAME: u64 = H_UNRESERVED | H_ESCAPED | 0x1;
const L_SERVER: u64 = L_USERINFO | L_ALPHANUM | L_DASH | 0x0400_4000_0000_0000;
const H_SERVER: u64 = H_USERINFO | H_ALPHANUM | H_DASH | 0x2800_0001;
const L_SERVER_PERCENT: u64 = L_SERVER | 0x20_0000_0000;
const H_SERVER_PERCENT: u64 = H_SERVER;
const L_SCHEME: u64 = L_ALPHA | L_DIGIT | 0x6800_0000_0000;
const H_SCHEME: u64 = H_ALPHA | H_DIGIT;
const L_SCOPE_ID: u64 = L_ALPHANUM | 0x4000_0000_0000;
const H_SCOPE_ID: u64 = H_ALPHANUM | 0x8000_0000;

/// `URI.match(c, lowMask, highMask)`.
fn match_mask(c: u16, low_mask: u64, high_mask: u64) -> bool {
    if c == 0 {
        false
    } else if c < 64 {
        (1u64 << c) & low_mask != 0
    } else if c < 128 {
        (1u64 << (c - 64)) & high_mask != 0
    } else {
        false
    }
}

/// `Character.isSpaceChar` of a UTF-16 code unit (`Zs`, `Zl`, `Zp`).
fn is_space_char(c: u16) -> bool {
    matches!(
        c,
        0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000
    )
}

/// `Character.isISOControl`.
fn is_iso_control(c: u16) -> bool {
    c <= 0x1F || (0x7F..=0x9F).contains(&c)
}

/// A `URISyntaxException`.
struct UriSyntaxError;

type UriResult<T> = Result<T, UriSyntaxError>;

/// `java.net.URI.Parser` with `requireServerAuthority == false`; only the scheme is kept.
struct UriParser {
    input: Vec<u16>,
    ipv6_byte_count: i32,
}

impl UriParser {
    fn new(input: &str) -> UriParser {
        UriParser {
            input: input.encode_utf16().collect(),
            ipv6_byte_count: 0,
        }
    }

    fn at(&self, start: usize, end: usize, c: u8) -> bool {
        start < end && self.input[start] == u16::from(c)
    }

    fn at_str(&self, start: usize, end: usize, s: &str) -> bool {
        let s = s.as_bytes();
        start <= end
            && s.len() <= end - start
            && s.iter()
                .enumerate()
                .all(|(i, &b)| self.input[start + i] == u16::from(b))
    }

    fn scan_char(&self, start: usize, end: usize, c: u8) -> usize {
        if self.at(start, end, c) {
            start + 1
        } else {
            start
        }
    }

    /// `scan(start, end, err, stop)`: `None` is `-1`.
    fn scan_err(&self, start: usize, end: usize, err: &str, stop: &str) -> Option<usize> {
        let in_set = |set: &str, c: u16| set.bytes().any(|b| u16::from(b) == c);
        let mut p = start;
        while p < end {
            let c = self.input[p];
            if in_set(err, c) {
                return None;
            }
            if in_set(stop, c) {
                break;
            }
            p += 1;
        }
        Some(p)
    }

    /// `scan(start, end, stop)`.
    fn scan_stop(&self, start: usize, end: usize, stop: &str) -> usize {
        let mut p = start;
        while p < end && !stop.bytes().any(|b| u16::from(b) == self.input[p]) {
            p += 1;
        }
        p
    }

    fn scan_escape(&self, start: usize, n: usize, first: u16) -> UriResult<usize> {
        if first == u16::from(b'%') {
            if start + 3 <= n
                && match_mask(self.input[start + 1], L_HEX, H_HEX)
                && match_mask(self.input[start + 2], L_HEX, H_HEX)
            {
                return Ok(start + 3);
            }
            return Err(UriSyntaxError);
        } else if first > 128 && !is_space_char(first) && !is_iso_control(first) {
            // unescaped but visible non-US-ASCII chars
            return Ok(start + 1);
        }
        Ok(start)
    }

    /// `scan(start, n, lowMask, highMask)`.
    fn scan_mask(&self, start: usize, n: usize, low_mask: u64, high_mask: u64) -> UriResult<usize> {
        let mut p = start;
        while p < n {
            let c = self.input[p];
            if match_mask(c, low_mask, high_mask) {
                p += 1;
                continue;
            }
            if low_mask & L_ESCAPED != 0 {
                let q = self.scan_escape(p, n, c)?;
                if q > p {
                    p = q;
                    continue;
                }
            }
            break;
        }
        Ok(p)
    }

    fn check_chars(
        &self,
        start: usize,
        end: usize,
        low_mask: u64,
        high_mask: u64,
    ) -> UriResult<()> {
        let p = self.scan_mask(start, end, low_mask, high_mask)?;
        if p < end { Err(UriSyntaxError) } else { Ok(()) }
    }

    /// `[<scheme>:]<scheme-specific-part>[#<fragment>]`; the scheme, if any.
    fn parse(&mut self) -> UriResult<Option<String>> {
        let n = self.input.len();
        let mut p;
        let scheme;
        match self.scan_err(0, n, "/?#", ":") {
            Some(colon) if self.at(colon, n, b':') => {
                if colon == 0 {
                    return Err(UriSyntaxError);
                }
                self.check_chars(0, 1, L_ALPHA, H_ALPHA)?;
                self.check_chars(1, colon, L_SCHEME, H_SCHEME)?;
                scheme = Some(String::from_utf16_lossy(&self.input[..colon]));
                p = colon + 1;
                if self.at(p, n, b'/') {
                    p = self.parse_hierarchical(p, n)?;
                } else {
                    let q = self.scan_stop(p, n, "#");
                    if q <= p {
                        return Err(UriSyntaxError);
                    }
                    self.check_chars(p, q, L_URIC, H_URIC)?;
                    p = q;
                }
            }
            _ => {
                scheme = None;
                p = self.parse_hierarchical(0, n)?;
            }
        }
        if self.at(p, n, b'#') {
            self.check_chars(p + 1, n, L_URIC, H_URIC)?;
            p = n;
        }
        if p < n {
            return Err(UriSyntaxError);
        }
        Ok(scheme)
    }

    /// `[//authority]<path>[?<query>]`
    fn parse_hierarchical(&mut self, start: usize, n: usize) -> UriResult<usize> {
        let mut p = start;
        if self.at(p, n, b'/') && self.at(p + 1, n, b'/') {
            p += 2;
            let q = self.scan_stop(p, n, "/?#");
            if q > p {
                p = self.parse_authority(p, q)?;
            } else if q >= n {
                return Err(UriSyntaxError);
            }
        }
        let q = self.scan_stop(p, n, "?#");
        self.check_chars(p, q, L_PATH, H_PATH)?;
        p = q;
        if self.at(p, n, b'?') {
            p += 1;
            let q = self.scan_stop(p, n, "#");
            self.check_chars(p, q, L_URIC, H_URIC)?;
            p = q;
        }
        Ok(p)
    }

    /// `authority = server | reg_name`
    fn parse_authority(&mut self, start: usize, n: usize) -> UriResult<usize> {
        let p = start;
        let mut q = p;

        let server_chars = if self.scan_stop(p, n, "]") > p {
            self.scan_mask(p, n, L_SERVER_PERCENT, H_SERVER_PERCENT)? == n
        } else {
            self.scan_mask(p, n, L_SERVER, H_SERVER)? == n
        };
        let reg_chars = self.scan_mask(p, n, L_REG_NAME, H_REG_NAME)? == n;

        if reg_chars && !server_chars {
            return Ok(n);
        }

        // a failed server parse falls back to the registry-name reading
        if server_chars {
            q = match self.parse_server(p, n, reg_chars) {
                Ok(parsed) if parsed >= n => parsed,
                _ => p,
            };
        }

        if q < n && !reg_chars {
            return Err(UriSyntaxError);
        }
        Ok(n)
    }

    /// `[<userinfo>@]<host>[:<port>]`
    fn parse_server(
        &mut self,
        start: usize,
        n: usize,
        skip_parse_exception: bool,
    ) -> UriResult<usize> {
        let mut p = start;

        if let Some(q) = self.scan_err(p, n, "/?#", "@")
            && self.at(q, n, b'@')
        {
            self.check_chars(p, q, L_USERINFO, H_USERINFO)?;
            p = q + 1;
        }

        if self.at(p, n, b'[') {
            p += 1;
            match self.scan_err(p, n, "/?#", "]") {
                Some(q) if q > p && self.at(q, n, b']') => {
                    let r = self.scan_stop(p, q, "%");
                    if r > p {
                        self.parse_ipv6_reference(p, r)?;
                        if r + 1 == q {
                            return Err(UriSyntaxError);
                        }
                        self.check_chars(r + 1, q, L_SCOPE_ID, H_SCOPE_ID)?;
                    } else {
                        self.parse_ipv6_reference(p, q)?;
                    }
                    p = q + 1;
                }
                _ => return Err(UriSyntaxError),
            }
        } else {
            p = match self.parse_ipv4_address(p, n) {
                Some(q) if q > p => q,
                _ => self.parse_hostname(p, n, skip_parse_exception)?,
            };
        }

        if self.at(p, n, b':') {
            p += 1;
            let q = self.scan_stop(p, n, "/");
            if q > p {
                self.check_chars(p, q, L_DIGIT, H_DIGIT)?;
                // `Integer.parseInt(input, p, q, 10)`
                let mut port: u64 = 0;
                for &digit in &self.input[p..q] {
                    port = port * 10 + u64::from(digit - u16::from(b'0'));
                    if port > i32::MAX as u64 {
                        return Err(UriSyntaxError);
                    }
                }
                p = q;
            }
        } else if p < n && skip_parse_exception {
            return Ok(p);
        }

        if p < n {
            return Err(UriSyntaxError);
        }
        Ok(p)
    }

    /// A string of decimal digits whose value fits in a byte.
    fn scan_byte(&self, start: usize, n: usize) -> UriResult<usize> {
        let p = start;
        let q = self.scan_mask(p, n, L_DIGIT, H_DIGIT)?;
        if q <= p {
            return Ok(q);
        }
        // leading zeros
        let mut i = p;
        loop {
            let j = self.scan_char(i, q, b'0');
            if j > i {
                i = j;
            } else {
                break;
            }
        }
        let significant_digits = q - i;
        if significant_digits < 3 {
            return Ok(q);
        }
        if significant_digits > 3 {
            return Ok(p);
        }
        let value = self.input[i..q]
            .iter()
            .fold(0u32, |acc, &d| acc * 10 + u32::from(d - u16::from(b'0')));
        Ok(if value > 255 { p } else { q })
    }

    /// `scanIPv4Address`: `None` is `-1`.
    fn scan_ipv4_address(&self, start: usize, n: usize, strict: bool) -> UriResult<Option<usize>> {
        let mut p = start;
        let m = self.scan_mask(p, n, L_DIGIT | L_DOT, H_DIGIT | H_DOT)?;
        if m <= p || (strict && m != n) {
            return Ok(None);
        }
        // `do { .. } while (false)`
        'scan: {
            let mut q = self.scan_byte(p, m)?;
            if q <= p {
                break 'scan;
            }
            p = q;
            let mut broke = false;
            for _ in 0..3 {
                q = self.scan_char(p, m, b'.');
                if q <= p {
                    broke = true;
                    break;
                }
                p = q;
                q = self.scan_byte(p, m)?;
                if q <= p {
                    broke = true;
                    break;
                }
                p = q;
            }
            if broke || q < m {
                break 'scan;
            }
            return Ok(Some(q));
        }
        if strict {
            return Err(UriSyntaxError);
        }
        Ok(None)
    }

    fn take_ipv4_address(&self, start: usize, n: usize) -> UriResult<usize> {
        match self.scan_ipv4_address(start, n, true)? {
            Some(p) if p > start => Ok(p),
            _ => Err(UriSyntaxError),
        }
    }

    /// `parseIPv4Address`: `None` is `-1`; the address may be followed by `:...`.
    fn parse_ipv4_address(&self, start: usize, n: usize) -> Option<usize> {
        let p = self.scan_ipv4_address(start, n, false).ok()??;
        if p > start && p < n && self.input[p] != u16::from(b':') {
            return None;
        }
        Some(p)
    }

    /// `hostname = domainlabel [ "." ] | 1*( domainlabel "." ) toplabel [ "." ]`
    fn parse_hostname(
        &self,
        start: usize,
        n: usize,
        skip_parse_exception: bool,
    ) -> UriResult<usize> {
        let mut p = start;
        let mut last_label: Option<usize> = None;

        loop {
            let q = self.scan_mask(p, n, L_ALPHANUM, H_ALPHANUM)?;
            if q <= p {
                break;
            }
            last_label = Some(p);
            p = q;
            let q = self.scan_mask(p, n, L_ALPHANUM | L_DASH, H_ALPHANUM | H_DASH)?;
            if q > p {
                if self.input[q - 1] == u16::from(b'-') {
                    return Err(UriSyntaxError);
                }
                p = q;
            }
            let q = self.scan_char(p, n, b'.');
            if q <= p {
                break;
            }
            p = q;
            if p >= n {
                break;
            }
        }

        if p < n && !self.at(p, n, b':') {
            if skip_parse_exception {
                return Ok(p);
            }
            return Err(UriSyntaxError);
        }
        let Some(l) = last_label else {
            return Err(UriSyntaxError);
        };
        // the rightmost label of a fully qualified hostname starts with an alpha character
        if l > start && !match_mask(self.input[l], L_ALPHA, H_ALPHA) {
            return Err(UriSyntaxError);
        }
        Ok(p)
    }

    fn parse_ipv6_reference(&mut self, start: usize, n: usize) -> UriResult<usize> {
        let mut p = start;
        let mut compressed_zeros = false;

        match self.scan_hex_seq(p, n)? {
            Some(q) => {
                p = q;
                if self.at_str(p, n, "::") {
                    compressed_zeros = true;
                    p = self.scan_hex_post(p + 2, n)?;
                } else if self.at(p, n, b':') {
                    p = self.take_ipv4_address(p + 1, n)?;
                    self.ipv6_byte_count += 4;
                }
            }
            None => {
                if self.at_str(p, n, "::") {
                    compressed_zeros = true;
                    p = self.scan_hex_post(p + 2, n)?;
                }
            }
        }
        if p < n
            || self.ipv6_byte_count > 16
            || (!compressed_zeros && self.ipv6_byte_count < 16)
            || (compressed_zeros && self.ipv6_byte_count == 16)
        {
            return Err(UriSyntaxError);
        }
        Ok(p)
    }

    fn scan_hex_post(&mut self, start: usize, n: usize) -> UriResult<usize> {
        let mut p = start;
        if p == n {
            return Ok(p);
        }
        match self.scan_hex_seq(p, n)? {
            Some(q) => {
                p = q;
                if self.at(p, n, b':') {
                    p += 1;
                    p = self.take_ipv4_address(p, n)?;
                    self.ipv6_byte_count += 4;
                }
            }
            None => {
                p = self.take_ipv4_address(p, n)?;
                self.ipv6_byte_count += 4;
            }
        }
        Ok(p)
    }

    /// A hex sequence; `None` when none could be scanned.
    fn scan_hex_seq(&mut self, start: usize, n: usize) -> UriResult<Option<usize>> {
        let mut p = start;
        let q = self.scan_mask(p, n, L_HEX, H_HEX)?;
        if q <= p {
            return Ok(None);
        }
        if self.at(q, n, b'.') {
            // beginning of an IPv4 address
            return Ok(None);
        }
        if q > p + 4 {
            return Err(UriSyntaxError);
        }
        self.ipv6_byte_count += 2;
        p = q;
        while p < n {
            if !self.at(p, n, b':') {
                break;
            }
            if self.at(p + 1, n, b':') {
                break;
            }
            p += 1;
            let q = self.scan_mask(p, n, L_HEX, H_HEX)?;
            if q <= p {
                return Err(UriSyntaxError);
            }
            if self.at(q, n, b'.') {
                p -= 1;
                break;
            }
            if q > p + 4 {
                return Err(UriSyntaxError);
            }
            self.ipv6_byte_count += 2;
            p = q;
        }
        Ok(Some(p))
    }
}
