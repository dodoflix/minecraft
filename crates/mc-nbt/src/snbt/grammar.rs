//! `SnbtGrammar.createParser(NbtOps.INSTANCE)` and `SnbtOperations`, rule by rule.

use super::javanum;
use super::packrat::*;
use crate::{CompoundTag, ListTag, Tag};
use std::sync::LazyLock;

macro_rules! atoms {
    ($($name:ident),* $(,)?) => { atoms!(@c 0usize; $($name,)*); };
    (@c $n:expr; $name:ident, $($rest:ident,)*) => { const $name: Atom = $n; atoms!(@c $n + 1usize; $($rest,)*); };
    (@c $n:expr;) => { const ATOM_COUNT: usize = $n; };
}

atoms! {
    SIGN, INTEGER_SUFFIX, BINARY_NUMERAL, DECIMAL_NUMERAL, HEX_NUMERAL, INTEGER_LITERAL, FLOAT_TYPE_SUFFIX,
    FLOAT_EXPONENT_PART, FLOAT_WHOLE_PART, FLOAT_FRACTION_PART, FLOAT_LITERAL, STRING_HEX_2, STRING_HEX_4,
    STRING_HEX_8, STRING_UNICODE_NAME, STRING_ESCAPE_SEQUENCE, STRING_PLAIN_CONTENTS, STRING_CHUNKS,
    STRING_CONTENTS, SINGLE_QUOTED_STRING_CHUNK, SINGLE_QUOTED_STRING_CONTENTS, DOUBLE_QUOTED_STRING_CHUNK,
    DOUBLE_QUOTED_STRING_CONTENTS, QUOTED_STRING_LITERAL, UNQUOTED_STRING, LITERAL, ARGUMENTS,
    UNQUOTED_STRING_OR_BUILTIN, MAP_KEY, MAP_ENTRY, MAP_ENTRIES, MAP_LITERAL, LIST_ENTRIES, ARRAY_PREFIX,
    INT_ARRAY_ENTRIES, LIST_LITERAL,
}

fn msg(key: &'static str) -> Reason {
    Reason::Msg(key, None)
}

// ---- term helpers

fn ch(s: &'static str) -> Term {
    Term::Chars(CharSet::One(s.as_bytes()[0] as u16), s)
}
/// `characters(a, b)`; `s` is `"a|b"`.
fn chs(s: &'static str) -> Term {
    Term::Chars(CharSet::Two(s.as_bytes()[0] as u16, s.as_bytes()[2] as u16), s)
}
fn seq(v: Vec<Term>) -> Term {
    Term::Sequence(v)
}
fn alt(v: Vec<Term>) -> Term {
    Term::Alternative(v)
}
fn opt(t: Term) -> Term {
    Term::Maybe(Box::new(t))
}
fn named(rule: Atom) -> Term {
    Term::Reference { rule, store: rule }
}
fn alias(rule: Atom, store: Atom) -> Term {
    Term::Reference { rule, store }
}
fn marker(atom: Atom, v: Val) -> Term {
    Term::Marker(atom, v)
}
fn mstr(atom: Atom, s: &str) -> Term {
    Term::Marker(atom, Val::Str(s.encode_utf16().collect()))
}
fn rep_sep(element: Atom, list: Atom) -> Term {
    Term::RepeatedWithSeparator { element, list, separator: Box::new(ch(",")), min: 0, trailing: true }
}

// ---- scope accessors

fn sign_or_plus(ps: &Ps<'_>) -> Sign {
    match ps.scope.get(SIGN) {
        Some(Val::Sign(s)) => *s,
        _ => Sign::Plus,
    }
}

fn get_str(ps: &Ps<'_>, atom: Atom) -> Option<Vec<u16>> {
    match ps.scope.get(atom) {
        Some(Val::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn ascii(units: &[u16]) -> String {
    units.iter().map(|&u| u as u8 as char).collect()
}

fn to_string(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn needs_underscore_removal(s: &str) -> bool {
    s.contains('_')
}

fn clean_and_append(out: &mut String, contents: &str) {
    out.extend(contents.chars().filter(|&c| c != '_'));
}

fn is_allowed_to_start_unquoted_string(c: u16) -> bool {
    !matches!(c, 0x2b | 0x2d | 0x2e | 0x30..=0x39)
}

// ---- numbers

#[derive(Clone, Copy)]
enum Num {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
}

impl Num {
    fn tag(self) -> Tag {
        match self {
            Num::Byte(v) => Tag::Byte(v),
            Num::Short(v) => Tag::Short(v),
            Num::Int(v) => Tag::Int(v),
            Num::Long(v) => Tag::Long(v),
        }
    }
    fn byte_value(self) -> i8 {
        match self {
            Num::Byte(v) => v,
            Num::Short(v) => v as i8,
            Num::Int(v) => v as i8,
            Num::Long(v) => v as i8,
        }
    }
    fn int_value(self) -> i32 {
        match self {
            Num::Byte(v) => v as i32,
            Num::Short(v) => v as i32,
            Num::Int(v) => v,
            Num::Long(v) => v as i32,
        }
    }
    fn long_value(self) -> i64 {
        match self {
            Num::Byte(v) => v as i64,
            Num::Short(v) => v as i64,
            Num::Int(v) => v as i64,
            Num::Long(v) => v,
        }
    }
}

fn number_parse_failure(ps: &mut Ps<'_>, message: String) {
    ps.store_here(Reason::Msg("snbt.parser.number_parse_failure", Some(message)));
}

impl IntegerLiteral {
    fn signed_or_default(&self) -> SignedPrefix {
        match self.suffix.signed {
            Some(s) => s,
            None => match self.base {
                Base::Binary | Base::Hex => SignedPrefix::Unsigned,
                Base::Decimal => SignedPrefix::Signed,
            },
        }
    }

    fn cleanup_digits(&self) -> String {
        let needs = needs_underscore_removal(&self.digits);
        if self.sign != Sign::Minus && !needs {
            return self.digits.clone();
        }
        let mut out = String::new();
        if self.sign == Sign::Minus {
            out.push('-');
        }
        if needs {
            clean_and_append(&mut out, &self.digits);
        } else {
            out.push_str(&self.digits);
        }
        out
    }

    /// `IntegerLiteral.create(ops, type, state)` (JavaOps number result).
    fn create(&self, ty: TypeSuffix, ps: &mut Ps<'_>) -> Option<Num> {
        let is_signed = self.signed_or_default() == SignedPrefix::Signed;
        if !is_signed && self.sign == Sign::Minus {
            ps.store_here(msg("snbt.parser.expected_non_negative_number"));
            return None;
        }
        let digits = self.cleanup_digits();
        let radix = match self.base {
            Base::Binary => 2,
            Base::Decimal => 10,
            Base::Hex => 16,
        };
        let r: Result<Num, String> = match (is_signed, ty) {
            (true, TypeSuffix::Byte) => javanum::parse_byte(&digits, radix).map(Num::Byte),
            (true, TypeSuffix::Short) => javanum::parse_short(&digits, radix).map(Num::Short),
            (true, TypeSuffix::Int) => javanum::parse_int(&digits, radix).map(Num::Int),
            (true, TypeSuffix::Long) => javanum::parse_long(&digits, radix).map(Num::Long),
            (false, TypeSuffix::Byte) => javanum::parse_unsigned_byte(&digits, radix).map(Num::Byte),
            (false, TypeSuffix::Short) => javanum::parse_unsigned_short(&digits, radix).map(Num::Short),
            (false, TypeSuffix::Int) => javanum::parse_unsigned_int(&digits, radix).map(Num::Int),
            (false, TypeSuffix::Long) => javanum::parse_unsigned_long(&digits, radix).map(Num::Long),
            _ => {
                ps.store_here(msg("snbt.parser.expected_integer_type"));
                return None;
            }
        };
        match r {
            Ok(n) => Some(n),
            Err(m) => {
                number_parse_failure(ps, m);
                None
            }
        }
    }
}

impl ArrayPrefix {
    fn default_type(self) -> TypeSuffix {
        match self {
            ArrayPrefix::Byte => TypeSuffix::Byte,
            ArrayPrefix::Int => TypeSuffix::Int,
            ArrayPrefix::Long => TypeSuffix::Long,
        }
    }

    fn is_allowed(self, t: TypeSuffix) -> bool {
        t == self.default_type()
            || match self {
                ArrayPrefix::Byte => false,
                ArrayPrefix::Int => matches!(t, TypeSuffix::Byte | TypeSuffix::Short),
                ArrayPrefix::Long => matches!(t, TypeSuffix::Byte | TypeSuffix::Short | TypeSuffix::Int),
            }
    }

    fn build_number(self, entry: &IntegerLiteral, ps: &mut Ps<'_>) -> Option<Num> {
        let actual = match entry.suffix.ty {
            None => self.default_type(),
            Some(t) if self.is_allowed(t) => t,
            Some(_) => {
                ps.store_here(msg("snbt.parser.invalid_array_element_type"));
                return None;
            }
        };
        entry.create(actual, ps)
    }

    fn create(self, entries: &[IntegerLiteral], ps: &mut Ps<'_>) -> Option<Tag> {
        let mut nums = Vec::with_capacity(entries.len());
        for e in entries {
            nums.push(self.build_number(e, ps)?);
        }
        Some(match self {
            ArrayPrefix::Byte => Tag::ByteArray(nums.iter().map(|n| n.byte_value()).collect()),
            ArrayPrefix::Int => Tag::IntArray(nums.iter().map(|n| n.int_value()).collect()),
            ArrayPrefix::Long => Tag::LongArray(nums.iter().map(|n| n.long_value()).collect()),
        })
    }
}

fn create_float(ps: &mut Ps<'_>) -> Option<Val> {
    let sign = sign_or_plus(ps);
    let whole = get_str(ps, FLOAT_WHOLE_PART).map(|v| ascii(&v));
    let fraction = get_str(ps, FLOAT_FRACTION_PART).map(|v| ascii(&v));
    let exponent = match ps.scope.get(FLOAT_EXPONENT_PART) {
        Some(Val::Signed(s, v)) => Some((*s, ascii(v))),
        _ => None,
    };
    let type_suffix = match ps.scope.get(FLOAT_TYPE_SUFFIX) {
        Some(Val::TypeSuffix(t)) => Some(*t),
        _ => None,
    };
    let mut result = String::new();
    if sign == Sign::Minus {
        result.push('-');
    }
    if let Some(w) = &whole {
        clean_and_append(&mut result, w);
    }
    if let Some(f) = &fraction {
        result.push('.');
        clean_and_append(&mut result, f);
    }
    if let Some((s, e)) = &exponent {
        result.push('e');
        if *s == Sign::Minus {
            result.push('-');
        }
        clean_and_append(&mut result, e);
    }
    match type_suffix {
        Some(TypeSuffix::Float) => match result.parse::<f32>() {
            Ok(v) if v.is_finite() => Some(Val::Tag(Tag::float(v))),
            Ok(_) => {
                ps.store_here(msg("snbt.parser.infinity_not_allowed"));
                None
            }
            Err(_) => {
                number_parse_failure(ps, result);
                None
            }
        },
        None | Some(TypeSuffix::Double) => match result.parse::<f64>() {
            Ok(v) if v.is_finite() => Some(Val::Tag(Tag::double(v))),
            Ok(_) => {
                ps.store_here(msg("snbt.parser.infinity_not_allowed"));
                None
            }
            Err(_) => {
                number_parse_failure(ps, result);
                None
            }
        },
        Some(_) => {
            ps.store_here(msg("snbt.parser.expected_float_type"));
            None
        }
    }
}

// ---- rule actions

fn a_scope_atom(ps: &mut Ps<'_>, atom: Atom) -> Option<Val> {
    ps.scope.get(atom).cloned()
}

fn a_integer_literal(ps: &mut Ps<'_>) -> Option<Val> {
    let suffix = match ps.scope.get(INTEGER_SUFFIX) {
        Some(Val::IntSuffix(s)) => *s,
        _ => IntegerSuffix { signed: None, ty: None },
    };
    let sign = sign_or_plus(ps);
    let (base, digits) = if let Some(d) = get_str(ps, DECIMAL_NUMERAL) {
        (Base::Decimal, d)
    } else if let Some(h) = get_str(ps, HEX_NUMERAL) {
        (Base::Hex, h)
    } else {
        (Base::Binary, get_str(ps, BINARY_NUMERAL)?)
    };
    Some(Val::IntLit(IntegerLiteral { sign, base, digits: ascii(&digits), suffix }))
}

fn a_float_exponent_part(ps: &mut Ps<'_>) -> Option<Val> {
    let sign = sign_or_plus(ps);
    Some(Val::Signed(sign, get_str(ps, DECIMAL_NUMERAL)?))
}

fn a_string_escape_sequence(ps: &mut Ps<'_>) -> Option<Val> {
    if let Some(v) = ps.scope.get(STRING_ESCAPE_SEQUENCE) {
        return Some(v.clone());
    }
    if let Some(Val::Str(hex)) = ps.scope.get_any(&[STRING_HEX_2, STRING_HEX_4, STRING_HEX_8]) {
        // HexFormat.fromHexDigits: int arithmetic (8 digits may wrap negative).
        let mut cp: i32 = 0;
        for &u in hex {
            cp = cp.wrapping_shl(4).wrapping_add((u as u8 as char).to_digit(16).unwrap() as i32);
        }
        if !(0..=0x10FFFF).contains(&cp) {
            let arg = format!("U+{:08X}", cp as u32);
            ps.store_here(Reason::Msg("snbt.parser.invalid_codepoint", Some(arg)));
            return None;
        }
        return Some(Val::Str(code_point_units(cp as u32)));
    }
    let name = get_str(ps, STRING_UNICODE_NAME)?;
    match super::unicode_names::code_point_of(&ascii(&name)) {
        Some(cp) => Some(Val::Str(code_point_units(cp))),
        None => {
            ps.store_here(msg("snbt.parser.invalid_character_name"));
            None
        }
    }
}

/// `Character.toString(codePoint)` as UTF-16 units (a lone surrogate stays a lone unit).
fn code_point_units(cp: u32) -> Vec<u16> {
    if cp >= 0x10000 {
        let c = cp - 0x10000;
        vec![0xD800 + (c >> 10) as u16, 0xDC00 + (c & 0x3FF) as u16]
    } else {
        vec![cp as u16]
    }
}

fn join_list(ps: &Ps<'_>) -> Option<Val> {
    let Some(Val::List(chunks)) = ps.scope.get(STRING_CHUNKS) else { return None };
    let mut out = Vec::new();
    for c in chunks {
        if let Val::Str(s) = c {
            out.extend_from_slice(s);
        }
    }
    Some(Val::Str(out))
}

fn a_string_contents(ps: &mut Ps<'_>) -> Option<Val> {
    ps.scope.get(STRING_CONTENTS).cloned()
}

fn a_arguments_or_entries(ps: &mut Ps<'_>, atom: Atom) -> Option<Val> {
    ps.scope.get(atom).cloned()
}

fn a_unquoted_string_or_builtin(ps: &mut Ps<'_>) -> Option<Val> {
    let contents = get_str(ps, UNQUOTED_STRING)?;
    if !contents.is_empty() && is_allowed_to_start_unquoted_string(contents[0]) {
        if let Some(Val::List(args)) = ps.scope.get(ARGUMENTS) {
            let args: Vec<&Tag> = args.iter().map(|v| if let Val::Tag(t) = v { t } else { unreachable!() }).collect();
            let name = ascii(&contents);
            let key = format!("{}/{}", name, args.len());
            return match (name.as_str(), args.len()) {
                ("bool", 1) => {
                    // ops.getBooleanValue: any numeric tag; `doubleValue() != 0.0`.
                    match args[0].as_number() {
                        Some(n) => Some(Val::Tag(Tag::from_bool(n.double_value() != 0.0))),
                        None => {
                            ps.store_here(msg("snbt.parser.expected_number_or_boolean"));
                            None
                        }
                    }
                }
                ("uuid", 1) => {
                    let parsed = match args[0] {
                        Tag::String(s) => {
                            let units: Vec<u16> = s.encode_utf16().collect();
                            javanum::uuid_from_string(&units)
                        }
                        _ => None,
                    };
                    match parsed {
                        Some((m, l)) => Some(Val::Tag(Tag::IntArray(vec![(m >> 32) as i32, m as i32, (l >> 32) as i32, l as i32]))),
                        None => {
                            ps.store_here(msg("snbt.parser.expected_string_uuid"));
                            None
                        }
                    }
                }
                _ => {
                    ps.store_here(Reason::Msg("snbt.parser.no_such_operation", Some(key)));
                    None
                }
            };
        }
        let name = ascii(&contents);
        if name.eq_ignore_ascii_case("true") {
            Some(Val::Tag(Tag::Byte(1)))
        } else if name.eq_ignore_ascii_case("false") {
            Some(Val::Tag(Tag::Byte(0)))
        } else {
            Some(Val::Tag(Tag::String(name)))
        }
    } else {
        ps.store_here(msg("snbt.parser.invalid_unquoted_start"));
        None
    }
}

fn a_map_key(ps: &mut Ps<'_>) -> Option<Val> {
    ps.scope.get_any(&[QUOTED_STRING_LITERAL, UNQUOTED_STRING]).cloned()
}

fn a_map_entry(ps: &mut Ps<'_>) -> Option<Val> {
    let key = get_str(ps, MAP_KEY)?;
    if key.is_empty() {
        ps.store_here(msg("snbt.parser.empty_key"));
        return None;
    }
    match ps.scope.get(LITERAL) {
        Some(Val::Tag(t)) => Some(Val::Entry(key, t.clone())),
        _ => None,
    }
}

fn a_map_literal(ps: &mut Ps<'_>) -> Option<Val> {
    let Some(Val::List(entries)) = ps.scope.get(MAP_ENTRIES) else { return None };
    let mut c = CompoundTag::new();
    for e in entries {
        if let Val::Entry(k, t) = e {
            // ImmutableMap.buildKeepingLast: first position, last value.
            c.put(to_string(k), t.clone());
        }
    }
    Some(Val::Tag(Tag::Compound(c)))
}

fn a_list_literal(ps: &mut Ps<'_>) -> Option<Val> {
    if let Some(Val::ArrayPrefix(prefix)) = ps.scope.get(ARRAY_PREFIX).cloned() {
        let Some(Val::List(entries)) = ps.scope.get(INT_ARRAY_ENTRIES) else { return None };
        let lits: Vec<IntegerLiteral> = entries.iter().filter_map(|v| if let Val::IntLit(l) = v { Some(l.clone()) } else { None }).collect();
        if lits.is_empty() {
            return Some(Val::Tag(match prefix {
                ArrayPrefix::Byte => Tag::ByteArray(Vec::new()),
                ArrayPrefix::Int => Tag::IntArray(Vec::new()),
                ArrayPrefix::Long => Tag::LongArray(Vec::new()),
            }));
        }
        return prefix.create(&lits, ps).map(Val::Tag);
    }
    let Some(Val::List(entries)) = ps.scope.get(LIST_ENTRIES) else { return None };
    let tags: Vec<Tag> = entries.iter().filter_map(|v| if let Val::Tag(t) = v { Some(t.clone()) } else { None }).collect();
    Some(Val::Tag(Tag::List(ListTag::from(tags))))
}

fn a_literal(ps: &mut Ps<'_>) -> Option<Val> {
    if let Some(Val::Str(s)) = ps.scope.get(QUOTED_STRING_LITERAL) {
        return Some(Val::Tag(Tag::String(to_string(s))));
    }
    if let Some(Val::IntLit(lit)) = ps.scope.get(INTEGER_LITERAL).cloned() {
        let ty = lit.suffix.ty.unwrap_or(TypeSuffix::Int);
        return lit.create(ty, ps).map(|n| Val::Tag(n.tag()));
    }
    ps.scope.get(LITERAL).cloned()
}

// ---- accept predicates

fn is_binary(c: u16) -> bool {
    matches!(c, 0x30 | 0x31 | 0x5f)
}
fn is_decimal(c: u16) -> bool {
    matches!(c, 0x30..=0x39 | 0x5f)
}
fn is_hex_run(c: u16) -> bool {
    matches!(c, 0x30..=0x39 | 0x41..=0x46 | 0x61..=0x66 | 0x5f)
}
fn is_hex_digit(c: u16) -> bool {
    matches!(c, 0x30..=0x39 | 0x41..=0x46 | 0x61..=0x66)
}
fn is_plain_string_chunk(c: u16) -> bool {
    !matches!(c, 0x22 | 0x27 | 0x5c)
}
fn is_unicode_name(c: u16) -> bool {
    matches!(c, 0x2d | 0x20 | 0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a)
}

fn is(sign: Option<SignedPrefix>, ty: Option<TypeSuffix>) -> Val {
    Val::IntSuffix(IntegerSuffix { signed: sign, ty })
}

fn t(rules: &mut [Option<Rule>], atom: Atom, term: Term, action: Action) {
    rules[atom] = Some(Rule::Term { term, action });
}

fn build() -> Grammar {
    let mut rules: Vec<Option<Rule>> = (0..ATOM_COUNT).map(|_| None).collect();

    t(&mut rules, SIGN, alt(vec![seq(vec![ch("+"), marker(SIGN, Val::Sign(Sign::Plus))]), seq(vec![ch("-"), marker(SIGN, Val::Sign(Sign::Minus))])]), |ps| {
        a_scope_atom(ps, SIGN)
    });

    let suffix_alt = |signed: Option<SignedPrefix>, ty: TypeSuffix, c: &'static str| seq(vec![chs(c), marker(INTEGER_SUFFIX, is(signed, Some(ty)))]);
    use SignedPrefix::{Signed, Unsigned};
    t(&mut rules, 
        INTEGER_SUFFIX,
        alt(vec![
            seq(vec![
                chs("u|U"),
                alt(vec![
                    suffix_alt(Some(Unsigned), TypeSuffix::Byte, "b|B"),
                    suffix_alt(Some(Unsigned), TypeSuffix::Short, "s|S"),
                    suffix_alt(Some(Unsigned), TypeSuffix::Int, "i|I"),
                    suffix_alt(Some(Unsigned), TypeSuffix::Long, "l|L"),
                ]),
            ]),
            seq(vec![
                chs("s|S"),
                alt(vec![
                    suffix_alt(Some(Signed), TypeSuffix::Byte, "b|B"),
                    suffix_alt(Some(Signed), TypeSuffix::Short, "s|S"),
                    suffix_alt(Some(Signed), TypeSuffix::Int, "i|I"),
                    suffix_alt(Some(Signed), TypeSuffix::Long, "l|L"),
                ]),
            ]),
            suffix_alt(None, TypeSuffix::Byte, "b|B"),
            suffix_alt(None, TypeSuffix::Short, "s|S"),
            suffix_alt(None, TypeSuffix::Int, "i|I"),
            suffix_alt(None, TypeSuffix::Long, "l|L"),
        ]),
        |ps| a_scope_atom(ps, INTEGER_SUFFIX),
    );

    rules[BINARY_NUMERAL] = Some(Rule::NumberRun {
        accept: is_binary,
        no_value: msg("snbt.parser.expected_binary_numeral"),
        underscore: msg("snbt.parser.underscore_not_allowed"),
    });
    rules[DECIMAL_NUMERAL] = Some(Rule::NumberRun {
        accept: is_decimal,
        no_value: msg("snbt.parser.expected_decimal_numeral"),
        underscore: msg("snbt.parser.underscore_not_allowed"),
    });
    rules[HEX_NUMERAL] = Some(Rule::NumberRun {
        accept: is_hex_run,
        no_value: msg("snbt.parser.expected_hex_numeral"),
        underscore: msg("snbt.parser.underscore_not_allowed"),
    });


    t(&mut rules, 
        INTEGER_LITERAL,
        seq(vec![
            opt(named(SIGN)),
            alt(vec![
                seq(vec![
                    ch("0"),
                    Term::Cut,
                    alt(vec![
                        seq(vec![chs("x|X"), Term::Cut, named(HEX_NUMERAL)]),
                        seq(vec![chs("b|B"), named(BINARY_NUMERAL)]),
                        seq(vec![named(DECIMAL_NUMERAL), Term::Cut, Term::Fail(msg("snbt.parser.leading_zero_not_allowed"))]),
                        marker(DECIMAL_NUMERAL, Val::Str(vec![0x30])),
                    ]),
                ]),
                named(DECIMAL_NUMERAL),
            ]),
            opt(named(INTEGER_SUFFIX)),
        ]),
        a_integer_literal,
    );

    t(&mut rules, 
        FLOAT_TYPE_SUFFIX,
        alt(vec![
            seq(vec![chs("f|F"), marker(FLOAT_TYPE_SUFFIX, Val::TypeSuffix(TypeSuffix::Float))]),
            seq(vec![chs("d|D"), marker(FLOAT_TYPE_SUFFIX, Val::TypeSuffix(TypeSuffix::Double))]),
        ]),
        |ps| a_scope_atom(ps, FLOAT_TYPE_SUFFIX),
    );

    t(&mut rules, FLOAT_EXPONENT_PART, seq(vec![chs("e|E"), opt(named(SIGN)), named(DECIMAL_NUMERAL)]), a_float_exponent_part);

    t(&mut rules, 
        FLOAT_LITERAL,
        seq(vec![
            opt(named(SIGN)),
            alt(vec![
                seq(vec![
                    alias(DECIMAL_NUMERAL, FLOAT_WHOLE_PART),
                    ch("."),
                    Term::Cut,
                    opt(alias(DECIMAL_NUMERAL, FLOAT_FRACTION_PART)),
                    opt(named(FLOAT_EXPONENT_PART)),
                    opt(named(FLOAT_TYPE_SUFFIX)),
                ]),
                seq(vec![
                    ch("."),
                    Term::Cut,
                    alias(DECIMAL_NUMERAL, FLOAT_FRACTION_PART),
                    opt(named(FLOAT_EXPONENT_PART)),
                    opt(named(FLOAT_TYPE_SUFFIX)),
                ]),
                seq(vec![alias(DECIMAL_NUMERAL, FLOAT_WHOLE_PART), named(FLOAT_EXPONENT_PART), Term::Cut, opt(named(FLOAT_TYPE_SUFFIX))]),
                seq(vec![alias(DECIMAL_NUMERAL, FLOAT_WHOLE_PART), opt(named(FLOAT_EXPONENT_PART)), named(FLOAT_TYPE_SUFFIX)]),
            ]),
        ]),
        create_float,
    );

    for (atom, size, text) in [(STRING_HEX_2, 2usize, "2"), (STRING_HEX_4, 4, "4"), (STRING_HEX_8, 8, "8")] {
        rules[atom] = Some(Rule::Greedy {
            accept: is_hex_digit,
            min: size,
            max: size,
            error: Reason::Msg("snbt.parser.expected_hex_escape", Some(text.to_string())),
        });
    }
    rules[STRING_UNICODE_NAME] =
        Some(Rule::Greedy { accept: is_unicode_name, min: 1, max: usize::MAX, error: msg("snbt.parser.invalid_character_name") });


    t(&mut rules, 
        STRING_ESCAPE_SEQUENCE,
        alt(vec![
            seq(vec![ch("b"), mstr(STRING_ESCAPE_SEQUENCE, "\u{8}")]),
            seq(vec![ch("s"), mstr(STRING_ESCAPE_SEQUENCE, " ")]),
            seq(vec![ch("t"), mstr(STRING_ESCAPE_SEQUENCE, "\t")]),
            seq(vec![ch("n"), mstr(STRING_ESCAPE_SEQUENCE, "\n")]),
            seq(vec![ch("f"), mstr(STRING_ESCAPE_SEQUENCE, "\u{c}")]),
            seq(vec![ch("r"), mstr(STRING_ESCAPE_SEQUENCE, "\r")]),
            seq(vec![ch("\\"), mstr(STRING_ESCAPE_SEQUENCE, "\\")]),
            seq(vec![ch("'"), mstr(STRING_ESCAPE_SEQUENCE, "'")]),
            seq(vec![ch("\""), mstr(STRING_ESCAPE_SEQUENCE, "\"")]),
            seq(vec![ch("x"), named(STRING_HEX_2)]),
            seq(vec![ch("u"), named(STRING_HEX_4)]),
            seq(vec![ch("U"), named(STRING_HEX_8)]),
            seq(vec![ch("N"), ch("{"), named(STRING_UNICODE_NAME), ch("}")]),
        ]),
        a_string_escape_sequence,
    );

    rules[STRING_PLAIN_CONTENTS] =
        Some(Rule::Greedy { accept: is_plain_string_chunk, min: 1, max: usize::MAX, error: msg("snbt.parser.invalid_string_contents") });


    t(&mut rules, 
        SINGLE_QUOTED_STRING_CHUNK,
        alt(vec![
            alias(STRING_PLAIN_CONTENTS, STRING_CONTENTS),
            seq(vec![ch("\\"), alias(STRING_ESCAPE_SEQUENCE, STRING_CONTENTS)]),
            seq(vec![ch("\""), mstr(STRING_CONTENTS, "\"")]),
        ]),
        a_string_contents,
    );
    t(&mut rules, 
        SINGLE_QUOTED_STRING_CONTENTS,
        Term::Repeated { element: SINGLE_QUOTED_STRING_CHUNK, list: STRING_CHUNKS, min: 0 },
        |ps| join_list(ps),
    );
    t(&mut rules, 
        DOUBLE_QUOTED_STRING_CHUNK,
        alt(vec![
            alias(STRING_PLAIN_CONTENTS, STRING_CONTENTS),
            seq(vec![ch("\\"), alias(STRING_ESCAPE_SEQUENCE, STRING_CONTENTS)]),
            seq(vec![ch("'"), mstr(STRING_CONTENTS, "'")]),
        ]),
        a_string_contents,
    );
    t(&mut rules, 
        DOUBLE_QUOTED_STRING_CONTENTS,
        Term::Repeated { element: DOUBLE_QUOTED_STRING_CHUNK, list: STRING_CHUNKS, min: 0 },
        |ps| join_list(ps),
    );
    t(&mut rules, 
        QUOTED_STRING_LITERAL,
        alt(vec![
            seq(vec![ch("\""), Term::Cut, opt(alias(DOUBLE_QUOTED_STRING_CONTENTS, STRING_CONTENTS)), ch("\"")]),
            seq(vec![ch("'"), opt(alias(SINGLE_QUOTED_STRING_CONTENTS, STRING_CONTENTS)), ch("'")]),
        ]),
        a_string_contents,
    );

    rules[UNQUOTED_STRING] = Some(Rule::Unquoted { error: msg("snbt.parser.expected_unquoted_string") });


    t(&mut rules, ARGUMENTS, rep_sep(LITERAL, ARGUMENTS), |ps| a_arguments_or_entries(ps, ARGUMENTS));
    t(&mut rules, 
        UNQUOTED_STRING_OR_BUILTIN,
        seq(vec![named(UNQUOTED_STRING), opt(seq(vec![ch("("), named(ARGUMENTS), ch(")")]))]),
        a_unquoted_string_or_builtin,
    );
    t(&mut rules, MAP_KEY, alt(vec![named(QUOTED_STRING_LITERAL), named(UNQUOTED_STRING)]), a_map_key);
    t(&mut rules, MAP_ENTRY, seq(vec![named(MAP_KEY), ch(":"), named(LITERAL)]), a_map_entry);
    t(&mut rules, MAP_ENTRIES, rep_sep(MAP_ENTRY, MAP_ENTRIES), |ps| a_arguments_or_entries(ps, MAP_ENTRIES));
    t(&mut rules, MAP_LITERAL, seq(vec![ch("{"), named(MAP_ENTRIES), ch("}")]), a_map_literal);
    t(&mut rules, LIST_ENTRIES, rep_sep(LITERAL, LIST_ENTRIES), |ps| a_arguments_or_entries(ps, LIST_ENTRIES));
    t(&mut rules, 
        ARRAY_PREFIX,
        alt(vec![
            seq(vec![ch("B"), marker(ARRAY_PREFIX, Val::ArrayPrefix(ArrayPrefix::Byte))]),
            seq(vec![ch("L"), marker(ARRAY_PREFIX, Val::ArrayPrefix(ArrayPrefix::Long))]),
            seq(vec![ch("I"), marker(ARRAY_PREFIX, Val::ArrayPrefix(ArrayPrefix::Int))]),
        ]),
        |ps| a_scope_atom(ps, ARRAY_PREFIX),
    );
    t(&mut rules, INT_ARRAY_ENTRIES, rep_sep(INTEGER_LITERAL, INT_ARRAY_ENTRIES), |ps| a_arguments_or_entries(ps, INT_ARRAY_ENTRIES));
    t(&mut rules, 
        LIST_LITERAL,
        seq(vec![ch("["), alt(vec![seq(vec![named(ARRAY_PREFIX), ch(";"), named(INT_ARRAY_ENTRIES)]), named(LIST_ENTRIES)]), ch("]")]),
        a_list_literal,
    );
    t(&mut rules, 
        LITERAL,
        alt(vec![
            seq(vec![
                Term::LookAhead(Box::new(Term::Chars(CharSet::NumberStart, "")), true),
                alt(vec![alias(FLOAT_LITERAL, LITERAL), named(INTEGER_LITERAL)]),
            ]),
            seq(vec![Term::LookAhead(Box::new(chs("\"|'")), true), Term::Cut, named(QUOTED_STRING_LITERAL)]),
            seq(vec![Term::LookAhead(Box::new(ch("{")), true), Term::Cut, alias(MAP_LITERAL, LITERAL)]),
            seq(vec![Term::LookAhead(Box::new(ch("[")), true), Term::Cut, alias(LIST_LITERAL, LITERAL)]),
            alias(UNQUOTED_STRING_OR_BUILTIN, LITERAL),
        ]),
        a_literal,
    );

    Grammar { rules, top: LITERAL }
}

pub(crate) static GRAMMAR: LazyLock<Grammar> = LazyLock::new(build);
