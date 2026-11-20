//! `String.toLowerCase(Locale.ROOT)` and `String.equalsIgnoreCase` of JDK 25 (the Java the vanilla server runs on).
//!
//! The mapping is `Character.toLowerCase(int)` per code point, except for two characters:
//! * `U+0130` becomes `i` followed by `U+0307`.
//! * `U+03A3` (capital sigma) becomes the final sigma `U+03C2` when `ConditionalSpecialCasing.isFinalCased` holds
//!   (a cased letter before it and none after it, within the word boundaries that surround it), else `U+03C3`.
//!   Rust's `str::to_lowercase` decides that by case-ignorable characters instead of the boundaries of
//!   `BreakIterator.getWordInstance(Locale.ROOT)`, so `A-Σ` and `A1Σ` end in different sigmas.
//!
//! The word boundaries come from a port of the JDK's `sun.text.RuleBasedBreakIterator` on the compiled word rules
//! (`word_break_data.bin`, the JDK's `WordBreakIteratorData`). The port keeps the iterator's quirks (it asks for the
//! boundary at `index` by iterating forward from a position found by its backwards state table, which does not know
//! surrogate pairs). `tests/golden/java_case.rs` checks the result against the real `String` method.

use crate::java_cased_letters::CASED_LETTERS;
use std::sync::LazyLock;

/// `CharacterIterator.DONE`.
const DONE: u16 = 0xFFFF;
/// `BreakIterator.DONE`.
const NO_BREAK: isize = -1;
/// `RuleBasedBreakIterator.START_STATE` / `STOP_STATE`.
const START_STATE: usize = 1;
const STOP_STATE: usize = 0;
/// Category value of an ignored character (a byte of -1 in the tables).
const IGNORE: i32 = -1;

/// `ConditionalSpecialCasing.isCased`: lower, upper and title case letters and the Other_Lowercase/Other_Uppercase
/// characters it lists.
fn is_cased(cp: u32) -> bool {
    CASED_LETTERS
        .binary_search_by(|&(start, end)| {
            if cp < start {
                std::cmp::Ordering::Greater
            } else if cp > end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
        || matches!(
            cp,
            0x02B0..=0x02B8
                | 0x02C0..=0x02C1
                | 0x02E0..=0x02E4
                | 0x0345
                | 0x037A
                | 0x1D2C..=0x1D61
                | 0x2160..=0x217F
                | 0x24B6..=0x24E9
        )
}

/// The tables of `RuleBasedBreakIterator.setupTables` (`BreakIteratorData` file format of the JDK).
struct WordBreakTables {
    state: Vec<u16>,
    backwards_state: Vec<u16>,
    end_states: Vec<bool>,
    lookahead_states: Vec<bool>,
    /// `CompactByteArray` of the BMP characters: block indices and block data.
    bmp_indices: Vec<u16>,
    bmp_data: Vec<i8>,
    /// `SupplementaryCharacterData`: `start << 8 | value` entries.
    supplementary: Vec<u32>,
    num_categories: usize,
}

static WORD_BREAK: LazyLock<WordBreakTables> = LazyLock::new(|| {
    /// Big-endian reader of the data file.
    struct Reader<'a>(&'a [u8]);
    impl Reader<'_> {
        fn take(&mut self, n: usize) -> &[u8] {
            let (head, tail) = self.0.split_at(n);
            self.0 = tail;
            head
        }
        fn u16(&mut self) -> u16 {
            u16::from_be_bytes(self.take(2).try_into().expect("two bytes"))
        }
        fn u32(&mut self) -> u32 {
            u32::from_be_bytes(self.take(4).try_into().expect("four bytes"))
        }
    }
    let mut r = Reader(include_bytes!("word_break_data.bin"));
    assert_eq!(r.take(7), b"BIdata\0", "break iterator data magic");
    assert_eq!(r.take(1), [1], "break iterator data version");
    let _total = r.u32();
    let lengths: [usize; 7] = std::array::from_fn(|_| r.u32() as usize);
    let [
        state,
        backwards,
        end_states,
        lookahead,
        bmp_data,
        supplementary,
        additional,
    ] = lengths;
    r.take(8); // checksum
    let state: Vec<u16> = (0..state).map(|_| r.u16()).collect();
    let backwards_state = (0..backwards).map(|_| r.u16()).collect();
    let end_states: Vec<bool> = r.take(end_states).iter().map(|&b| b == 1).collect();
    let lookahead_states = r.take(lookahead).iter().map(|&b| b == 1).collect();
    let bmp_indices = (0..512).map(|_| r.u16()).collect();
    let bmp_data = r.take(bmp_data).iter().map(|&b| b as i8).collect();
    let supplementary = (0..supplementary).map(|_| r.u32()).collect();
    r.take(additional);
    assert!(r.0.is_empty(), "break iterator data length");
    WordBreakTables {
        num_categories: state.len() / end_states.len(),
        state,
        backwards_state,
        end_states,
        lookahead_states,
        bmp_indices,
        bmp_data,
        supplementary,
    }
});

impl WordBreakTables {
    /// `lookupCategory`.
    fn category(&self, c: u32) -> i32 {
        if c < 0x10000 {
            let i = usize::from(self.bmp_indices[(c >> 7) as usize]) + (c & 127) as usize;
            return i32::from(self.bmp_data[i]);
        }
        // `SupplementaryCharacterData.getValue`
        let (mut i, mut j) = (0, self.supplementary.len() - 1);
        loop {
            let k = (i + j) / 2;
            let start = self.supplementary[k] >> 8;
            let end = self.supplementary[k + 1] >> 8;
            if c < start {
                j = k;
            } else if c > end - 1 {
                i = k;
            } else {
                let v = self.supplementary[k] & 0xFF;
                return if v == 0xFF { IGNORE } else { v as i32 };
            }
        }
    }
}

/// `BreakIterator.getWordInstance(Locale.ROOT)` over a `StringCharacterIterator` of `text`: the iterator's position and
/// its `cachedLastKnownBreak`.
struct WordBreakIterator<'a> {
    tables: &'a WordBreakTables,
    text: &'a [u16],
    index: usize,
    cached_last_known_break: isize,
}

impl<'a> WordBreakIterator<'a> {
    fn new(text: &'a [u16]) -> Self {
        WordBreakIterator {
            tables: &WORD_BREAK,
            text,
            index: 0,
            cached_last_known_break: NO_BREAK,
        }
    }

    // `StringCharacterIterator`
    fn current(&self) -> u16 {
        self.text.get(self.index).copied().unwrap_or(DONE)
    }

    fn next_unit(&mut self) -> u16 {
        if self.index + 1 < self.text.len() {
            self.index += 1;
            self.text[self.index]
        } else {
            self.index = self.text.len();
            DONE
        }
    }

    fn previous_unit(&mut self) -> u16 {
        if self.index > 0 {
            self.index -= 1;
            self.text[self.index]
        } else {
            DONE
        }
    }

    // `RuleBasedBreakIterator.getPrevious/getCurrent/getCurrentCodePointCount/getNext/getNextIndex`
    fn get_previous(&mut self) -> i32 {
        let c2 = self.previous_unit();
        if (0xDC00..0xE000).contains(&c2) && self.index > 0 {
            let c1 = self.previous_unit();
            if (0xD800..0xDC00).contains(&c1) {
                return to_code_point(c1, c2);
            }
            self.next_unit();
        }
        i32::from(c2)
    }

    fn get_current(&mut self) -> i32 {
        let c1 = self.current();
        if (0xD800..0xDC00).contains(&c1) && self.index < self.text.len() {
            let c2 = self.next_unit();
            self.previous_unit();
            if (0xDC00..0xE000).contains(&c2) {
                return to_code_point(c1, c2);
            }
        }
        i32::from(c1)
    }

    fn current_code_point_count(&mut self) -> usize {
        let c1 = self.current();
        if (0xD800..0xDC00).contains(&c1) && self.index < self.text.len() {
            let c2 = self.next_unit();
            self.previous_unit();
            if (0xDC00..0xE000).contains(&c2) {
                return 2;
            }
        }
        1
    }

    fn get_next(&mut self) -> i32 {
        let end = self.text.len();
        if self.index == end {
            return i32::from(DONE);
        }
        let index = self.index + self.current_code_point_count();
        if index >= end {
            return i32::from(DONE);
        }
        self.index = index;
        self.get_current()
    }

    fn get_next_index(&mut self) -> usize {
        (self.index + self.current_code_point_count()).min(self.text.len())
    }

    /// `handleNext`.
    fn handle_next(&mut self) -> isize {
        if self.index == self.text.len() {
            return NO_BREAK;
        }
        let t = self.tables;
        let mut result = self.get_next_index();
        let mut lookahead_result = 0;
        let mut state = START_STATE;
        let mut c = self.get_current();
        while c != i32::from(DONE) && state != STOP_STATE {
            let category = t.category(c as u32);
            if category != IGNORE {
                state = usize::from(t.state[state * t.num_categories + category as usize]);
            }
            if t.lookahead_states[state] {
                if t.end_states[state] {
                    result = lookahead_result;
                } else {
                    lookahead_result = self.get_next_index();
                }
            } else if t.end_states[state] {
                result = self.get_next_index();
            }
            c = self.get_next();
        }
        if c == i32::from(DONE) && lookahead_result == self.text.len() {
            result = lookahead_result;
        }
        self.index = result;
        result as isize
    }

    /// `handlePrevious`.
    fn handle_previous(&mut self) -> isize {
        let t = self.tables;
        let mut state = START_STATE;
        let mut category = 0;
        let mut last_category = 0;
        let mut c = self.get_current();
        while c != i32::from(DONE) && state != STOP_STATE {
            last_category = category;
            category = t.category(c as u32);
            if category != IGNORE {
                state =
                    usize::from(t.backwards_state[state * t.num_categories + category as usize]);
            }
            c = self.get_previous();
        }
        if c != i32::from(DONE) {
            if last_category != IGNORE {
                self.get_next();
                self.get_next();
            } else {
                self.get_next();
            }
        }
        self.index as isize
    }

    /// `following(offset)`.
    fn following(&mut self, offset: usize) -> isize {
        self.index = offset;
        if offset == 0 {
            self.cached_last_known_break = self.handle_next();
            return self.cached_last_known_break;
        }
        let mut result = self.cached_last_known_break;
        if result >= offset as isize || result <= NO_BREAK {
            result = self.handle_previous();
        } else {
            self.index = result as usize;
        }
        while result != NO_BREAK && result <= offset as isize {
            result = self.handle_next();
        }
        self.cached_last_known_break = result;
        result
    }

    /// `isBoundary(offset)`.
    fn is_boundary(&mut self, offset: usize) -> bool {
        offset == 0 || self.following(offset - 1) == offset as isize
    }
}

fn to_code_point(high: u16, low: u16) -> i32 {
    0x10000 + ((i32::from(high) - 0xD800) << 10) + (i32::from(low) - 0xDC00)
}

/// `ConditionalSpecialCasing.isFinalCased(src, index, Locale.ROOT)` for the sigma at UTF-16 position `index`.
fn is_final_cased(units: &[u16], index: usize) -> bool {
    let mut breaks = WordBreakIterator::new(units);
    // `src.codePointBefore(i)` / `src.codePointAt(i)`
    let before = |i: usize| -> (u32, usize) {
        let low = units[i - 1];
        if (0xDC00..0xE000).contains(&low) && i >= 2 && (0xD800..0xDC00).contains(&units[i - 2]) {
            (to_code_point(units[i - 2], low) as u32, 2)
        } else {
            (u32::from(low), 1)
        }
    };
    let at = |i: usize| -> (u32, usize) {
        let high = units[i];
        if (0xD800..0xDC00).contains(&high)
            && i + 1 < units.len()
            && (0xDC00..0xE000).contains(&units[i + 1])
        {
            (to_code_point(high, units[i + 1]) as u32, 2)
        } else {
            (u32::from(high), 1)
        }
    };
    let mut i = index;
    while !breaks.is_boundary(i) {
        let (ch, width) = before(i);
        if is_cased(ch) {
            let mut j = index + at(index).1;
            while j < units.len() && !breaks.is_boundary(j) {
                let (ch, width) = at(j);
                if is_cased(ch) {
                    return false;
                }
                j += width;
            }
            return true;
        }
        i -= width;
    }
    false
}

/// `Character.toLowerCase(int)`. Rust's tables are newer than JDK 25's Unicode 16: the characters added since are not
/// lower-cased by the JDK.
pub fn simple_lower(cp: u32) -> u32 {
    let Some(c) = char::from_u32(cp) else {
        return cp;
    };
    if cp == 0x130 {
        return 0x69;
    }
    if matches!(cp, 0xA7CE | 0xA7D2 | 0xA7D4 | 0x16EA0..=0x16EB8) {
        return cp;
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l as u32,
        _ => cp,
    }
}

/// `Character.toUpperCase(int)`: the one-character mapping of `UnicodeData.txt`. Rust's `to_uppercase` is the full
/// mapping, which is longer than one character for `U+00DF` and the like (no one-character mapping: the character
/// stays) and for the Greek letters with iota subscript, whose one-character mapping is the title-case letter.
/// Rust's tables are newer than JDK 25's Unicode 16 here too.
pub fn simple_upper(cp: u32) -> u32 {
    let Some(c) = char::from_u32(cp) else {
        return cp;
    };
    if matches!(cp, 0xA7CF | 0xA7D3 | 0xA7D5 | 0x16EBB..=0x16ED3) {
        return cp;
    }
    match cp {
        0x1F80..=0x1F87 | 0x1F90..=0x1F97 | 0x1FA0..=0x1FA7 => return cp + 8,
        0x1FB3 => return 0x1FBC,
        0x1FC3 => return 0x1FCC,
        0x1FF3 => return 0x1FFC,
        _ => {}
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u as u32,
        _ => cp,
    }
}

/// `Character.toUpperCase` then `Character.toLowerCase` comparison of `StringUTF16.compareCodePointCI`.
fn code_points_equal_ignore_case(cp1: u32, cp2: u32) -> bool {
    let (u1, u2) = (simple_upper(cp1), simple_upper(cp2));
    u1 == u2 || simple_lower(u1) == simple_lower(u2)
}

/// `CharacterDataLatin1.equalsIgnoreCase`.
fn latin1_equals_ignore_case(b1: u8, b2: u8) -> bool {
    if b1 == b2 {
        return true;
    }
    let upper = b1 & 0xDF;
    upper >= b'A'
        && (upper <= b'Z' || ((0xC0..=0xDE).contains(&upper) && upper != 0xD7))
        && upper == b2 & 0xDF
}

/// `StringUTF16.codePointIncluding`: the code point the unit at `index` belongs to (looking within `start..end`),
/// negated when the unit is the high surrogate of a pair (the pair's low surrogate follows).
fn code_point_including(units: &[u16], cp: u32, index: usize, start: usize, end: usize) -> i64 {
    match char::from_u32(cp) {
        Some(_) => i64::from(cp),
        None if (0xDC00..0xE000).contains(&cp) => match units.get(index.wrapping_sub(1)) {
            Some(&high) if index > start && (0xD800..0xDC00).contains(&high) => {
                i64::from(to_code_point(high, cp as u16))
            }
            _ => i64::from(cp),
        },
        None => match units.get(index + 1) {
            Some(&low) if index + 1 < end && (0xDC00..0xE000).contains(&low) => {
                -i64::from(to_code_point(cp as u16, low))
            }
            _ => i64::from(cp),
        },
    }
}

/// `String.regionMatches(true, a_offset, b, b_offset, len)` (JDK 25) of the strings with UTF-16 units `a` and `b`.
/// The comparison depends on how the two strings are stored (compact strings: Latin-1 when every character is at most
/// `U+00FF`): two Latin-1 strings compare byte by byte, a Latin-1 and a UTF-16 string unit by unit, two UTF-16 strings
/// by code point where a surrogate pair is involved.
pub fn region_matches_ignore_case(
    a: &[u16],
    a_offset: usize,
    b: &[u16],
    b_offset: usize,
    len: usize,
) -> bool {
    if a_offset.checked_add(len).is_none_or(|end| end > a.len())
        || b_offset.checked_add(len).is_none_or(|end| end > b.len())
    {
        return false;
    }
    let latin1 = |s: &[u16]| s.iter().all(|&u| u <= 0xFF);
    let (ra, rb) = (&a[a_offset..a_offset + len], &b[b_offset..b_offset + len]);
    match (latin1(a), latin1(b)) {
        (true, true) => ra
            .iter()
            .zip(rb)
            .all(|(&x, &y)| latin1_equals_ignore_case(x as u8, y as u8)),
        (true, false) | (false, true) => ra.iter().zip(rb).all(|(&x, &y)| {
            let (u1, u2) = (
                simple_upper(u32::from(x)) as u16,
                simple_upper(u32::from(y)) as u16,
            );
            x == y || u1 == u2 || simple_lower(u32::from(u1)) == simple_lower(u32::from(u2))
        }),
        (false, false) => {
            let (mut k1, mut k2) = (a_offset, b_offset);
            let (end1, end2) = (a_offset + len, b_offset + len);
            while k1 < end1 && k2 < end2 {
                let (cp1, cp2) = (u32::from(a[k1]), u32::from(b[k2]));
                if cp1 != cp2 && !code_points_equal_ignore_case(cp1, cp2) {
                    let mut c1 = code_point_including(a, cp1, k1, a_offset, end1);
                    let mut c2 = code_point_including(b, cp2, k2, b_offset, end2);
                    if c1 < 0 {
                        k1 += 1;
                        c1 = -c1;
                    }
                    if c2 < 0 {
                        k2 += 1;
                        c2 = -c2;
                    }
                    if !code_points_equal_ignore_case(c1 as u32, c2 as u32) {
                        return false;
                    }
                }
                k1 += 1;
                k2 += 1;
            }
            true
        }
    }
}

/// `String.equalsIgnoreCase` (JDK 25), see [`region_matches_ignore_case`].
pub fn equals_ignore_case(a: &str, b: &str) -> bool {
    let a: Vec<u16> = a.encode_utf16().collect();
    let b: Vec<u16> = b.encode_utf16().collect();
    a.len() == b.len() && region_matches_ignore_case(&a, 0, &b, 0, a.len())
}

/// `String.toLowerCase(Locale.ROOT)` of the UTF-16 units `units` (lone surrogates stay as they are).
pub fn to_lower_case_root_units(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len() + 1);
    let mut i = 0;
    for r in char::decode_utf16(units.iter().copied()) {
        let (cp, width) = match r {
            Ok(c) => (u32::from(c), c.len_utf16()),
            Err(e) => (u32::from(e.unpaired_surrogate()), 1),
        };
        match cp {
            0x03A3 => out.push(if is_final_cased(units, i) {
                0x03C2
            } else {
                0x03C3
            }),
            0x0130 => out.extend([0x69, 0x0307]),
            _ => match char::from_u32(simple_lower(cp)) {
                Some(c) => out.extend_from_slice(c.encode_utf16(&mut [0; 2])),
                None => out.push(cp as u16),
            },
        }
        i += width;
    }
    out
}

/// `String.toLowerCase(Locale.ROOT)`.
pub fn to_lower_case_root(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    String::from_utf16(&to_lower_case_root_units(&units)).expect("a lower-cased string is valid")
}
