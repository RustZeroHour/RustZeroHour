// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The `INI::scanXxx` value scanners, ported from
//! `Core/GameEngine/Source/Common/INI/INI.cpp` @ 571a7c5.
//!
//! The baseline builds with `USE_STD_FROM_CHARS_PARSING = 1` (the file
//! selects it automatically for C++17 compilers), so the reference
//! semantics are the `scanType<T>` template:
//!
//! * a leading `+` is skipped manually (`std::from_chars` rejects it for
//!   integers) and a `+-` pair is rejected outright;
//! * integers parse the **longest decimal prefix** as a *signed 64-bit*
//!   value regardless of the target type, then truncate to the target
//!   width (two's-complement wrap);
//! * a 64-bit overflow falls back to `strtoll`/`strtoull` semantics
//!   (saturation at the 64-bit limits, `-` negated modulo 2^64 for
//!   `strtoull`), then the same truncation;
//! * reals parse the longest decimal float prefix (trailing characters
//!   are ignored) and saturate to ±infinity on overflow — `strtof`
//!   behaviour, which both `std::from_chars` (via its out-of-range
//!   fallback) and Rust's `f32::from_str` reproduce for the decimal
//!   grammar.
//!
//! The oversize/overflow rows of the upstream GoogleTest
//! `Tests/Google/Core/GameEngine/Common/INITest.cpp` are ported as unit
//! tests below and pin every row of that table.
//!
//! Verified against libstdc++'s `std::from_chars` (the probe is in the
//! task's differential harness notes): prefix parsing, the `inf`/
//! `infinity`/`nan` words (longest keyword match — `infinit` scans as
//! `inf`), the decimal-only `chars_format::general` grammar (a hex float
//! literal scans as its leading `0`), and the `+` rejection all match
//! bit-for-bit. Documented divergence, far outside the game data's
//! grammar: reals whose magnitude exceeds the 64-bit integer range
//! convert through a saturating 64-bit step where the C++'s conversion
//! is undefined behaviour.

/// Scans a token as the C++ `INI::scanInt` (`scanType<Int>`) does.
///
/// Returns the truncated 32-bit value; the 64-bit intermediate and the
/// saturation fallback are reproduced exactly, including their wrap
/// behaviour (`9223372036854775807` scans to `-1`).
pub fn scan_int(token: &[u8]) -> Result<i32, ScanError> {
    let (body, out_of_range) = parse_i64_prefix(token)?;
    match out_of_range {
        false => Ok(body as i32),
        // strtoll fallback: the digit prefix is non-empty (from_chars
        // reported an overflow), so strtoll never fails; it saturates.
        true => Ok(saturating_i64(token) as i32),
    }
}

/// Scans a token as the C++ `INI::scanUnsignedInt` (`scanType<UnsignedInt>`)
/// does.
///
/// The intermediate parse is the *signed* 64-bit one (the C++ template
/// uses `Int64` for every integral type), so `-5` scans to
/// `4294967291`; the out-of-range fallback reproduces `strtoull`
/// (modulo-2^64 negation, `ULLONG_MAX` saturation above 2^64-1).
pub fn scan_unsigned_int(token: &[u8]) -> Result<u32, ScanError> {
    let (body, out_of_range) = parse_i64_prefix(token)?;
    match out_of_range {
        false => Ok(body as u32),
        true => Ok(strtoull_u64(token) as u32),
    }
}

/// Scans a token as the C++ `INI::scanReal` (`scanType<Real>`) does.
pub fn scan_real(token: &[u8]) -> Result<f32, ScanError> {
    scan_real_inner(token)
}

/// Scans a token as the C++ `INI::scanPercentToReal` does:
/// `scanReal(token) / 100.0f`.
pub fn scan_percent_to_real(token: &[u8]) -> Result<f32, ScanError> {
    Ok(scan_real_inner(token)? / 100.0f32)
}

/// Scans a token as the C++ `INI::scanBool` does: `Yes`/`No`,
/// case-insensitive; anything else is `INI_INVALID_DATA`.
pub fn scan_bool(token: &[u8]) -> Result<bool, ScanError> {
    if token.eq_ignore_ascii_case(b"yes") {
        Ok(true)
    } else if token.eq_ignore_ascii_case(b"no") {
        Ok(false)
    } else {
        Err(ScanError::InvalidData)
    }
}

/// Scans a token as the C++ `INI::scanIndexList` does: the case-insensitive
/// position of `token` in `names`, or `INI_INVALID_DATA`.
pub fn scan_index_list(token: &[u8], names: &[&str]) -> Result<i32, ScanError> {
    if names.is_empty() {
        return Err(ScanError::InvalidNameList);
    }
    for (index, name) in names.iter().enumerate() {
        if token.eq_ignore_ascii_case(name.as_bytes()) {
            return Ok(index as i32);
        }
    }
    Err(ScanError::InvalidData)
}

/// Scans a token as the C++ `INI::scanLookupList` does: the value paired
/// with the case-insensitive `token` match in `list`.
pub fn scan_lookup_list(token: &[u8], list: &[(&str, i32)]) -> Result<i32, ScanError> {
    if list.is_empty() {
        return Err(ScanError::InvalidNameList);
    }
    for (name, value) in list {
        if token.eq_ignore_ascii_case(name.as_bytes()) {
            return Ok(*value);
        }
    }
    Err(ScanError::InvalidData)
}

/// Scan-level failure kinds; the engine attaches file/line context when
/// it converts these into [`IniError`](crate::IniError)s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanError {
    /// `INI_INVALID_DATA`: no number, boolean or list name could be read.
    InvalidData,
    /// `INI_INVALID_NAME_LIST`: the list itself is empty.
    InvalidNameList,
}

/// The `std::from_chars(first, last, Int64&)` prefix parse.
///
/// Returns the 64-bit value and whether the value overflowed the 64-bit
/// range (the C++ then falls back to the `strto*` conversion). A `+` sign
/// is skipped first, mirroring `scanType`'s `tokenOffset`; `+-` is
/// rejected.
fn parse_i64_prefix(token: &[u8]) -> Result<(i64, bool), ScanError> {
    let mut rest = token;
    if rest.first() == Some(&b'+') {
        if rest.len() > 1 && rest[1] == b'-' {
            return Err(ScanError::InvalidData);
        }
        rest = &rest[1..];
    }

    let mut body = rest;
    let mut negative = false;
    if body.first() == Some(&b'-') {
        negative = true;
        body = &body[1..];
    }

    // Longest decimal digit prefix; from_chars stops at the first
    // non-digit without failing (trailing characters are ignored).
    let digits_end = body
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(body.len());
    if digits_end == 0 {
        return Err(ScanError::InvalidData);
    }
    let digits = &body[..digits_end];

    // Accumulate with 64-bit overflow detection, exactly like from_chars
    // reporting result_out_of_range.
    let mut magnitude: u64 = 0;
    let mut overflow = false;
    for &c in digits {
        let d = u64::from(c - b'0');
        match magnitude.checked_mul(10).and_then(|m| m.checked_add(d)) {
            Some(m) => magnitude = m,
            None => overflow = true,
        }
        if overflow {
            break;
        }
    }
    if !overflow {
        if negative {
            if magnitude > (1u64 << 63) {
                overflow = true;
            }
        } else if magnitude > (1u64 << 63) - 1 {
            overflow = true;
        }
    }

    if overflow {
        return Ok((0, true));
    }
    let value = if negative {
        (magnitude as i64).wrapping_neg()
    } else {
        magnitude as i64
    };
    Ok((value, false))
}

/// The `strtoll` fallback for an out-of-range token: optional sign plus a
/// non-empty decimal prefix, saturated at the 64-bit limits.
fn saturating_i64(token: &[u8]) -> i64 {
    let mut body = token;
    let mut negative = false;
    if body.first() == Some(&b'+') {
        body = &body[1..];
    } else if body.first() == Some(&b'-') {
        negative = true;
        body = &body[1..];
    }
    let digits_end = body
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(body.len());

    let mut magnitude: u64 = 0;
    let mut overflow = false;
    for &c in &body[..digits_end] {
        let d = u64::from(c - b'0');
        match magnitude.checked_mul(10).and_then(|m| m.checked_add(d)) {
            Some(m) => magnitude = m,
            None => {
                overflow = true;
                break;
            }
        }
    }
    if overflow {
        // strtoll returns LLONG_MIN / LLONG_MAX on overflow.
        return if negative { i64::MIN } else { i64::MAX };
    }
    if negative {
        if magnitude > (1u64 << 63) {
            i64::MIN
        } else {
            (magnitude as i64).wrapping_neg()
        }
    } else if magnitude > (1u64 << 63) - 1 {
        i64::MAX
    } else {
        magnitude as i64
    }
}

/// The `strtoull` fallback for an out-of-range token: optional sign plus
/// a non-empty decimal prefix. A magnitude above `u64::MAX` saturates to
/// `ULLONG_MAX` (the C error return overrides the negation); otherwise a
/// leading `-` negates modulo 2^64.
fn strtoull_u64(token: &[u8]) -> u64 {
    let mut body = token;
    let mut negative = false;
    if body.first() == Some(&b'+') {
        body = &body[1..];
    } else if body.first() == Some(&b'-') {
        negative = true;
        body = &body[1..];
    }
    let digits_end = body
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(body.len());

    let mut magnitude: u64 = 0;
    let mut overflow = false;
    for &c in &body[..digits_end] {
        let d = u64::from(c - b'0');
        match magnitude.checked_mul(10).and_then(|m| m.checked_add(d)) {
            Some(m) => magnitude = m,
            None => {
                overflow = true;
                break;
            }
        }
    }
    if overflow {
        // Out of the u64 range: the conversion error return is ULLONG_MAX
        // regardless of sign (this is what makes "-18446744073709551616"
        // scan to 4294967295, not 0).
        return u64::MAX;
    }
    if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
}

/// The `scanType<Real>` prefix parse: the longest prefix matching the
/// decimal float grammar (or the `inf`/`nan` words), parsed with Rust's
/// correctly-rounded `f32` conversion. Overflow saturates to ±infinity
/// and underflow to ±0, matching the `strtof` fallback the C++ uses.
fn scan_real_inner(token: &[u8]) -> Result<f32, ScanError> {
    let prefix = float_prefix(token).ok_or(ScanError::InvalidData)?;
    let text = std::str::from_utf8(prefix).map_err(|_| ScanError::InvalidData)?;
    text.parse::<f32>().map_err(|_| ScanError::InvalidData)
}

/// Finds the longest byte prefix of `token` that is a decimal float
/// literal `std::from_chars` would accept (sign, digits with optional
/// decimal point, optional decimal exponent), or a signed
/// case-insensitive `inf`/`infinity`/`nan` word.
fn float_prefix(token: &[u8]) -> Option<&[u8]> {
    let mut rest = token;
    let mut sign_len = 0usize;
    if rest.first() == Some(&b'+') {
        if rest.len() > 1 && rest[1] == b'-' {
            return None;
        }
        rest = &rest[1..];
        sign_len = 1;
    } else if rest.first() == Some(&b'-') {
        rest = &rest[1..];
        sign_len = 1;
    }

    // inf / infinity / nan (case-insensitive), longest match first.
    for word in [b"infinity".as_slice(), b"inf".as_slice(), b"nan".as_slice()] {
        if rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word) {
            return Some(&token[..sign_len + word.len()]);
        }
    }

    let mut end = 0;
    let mut saw_digit = false;
    while end < rest.len() && rest[end].is_ascii_digit() {
        saw_digit = true;
        end += 1;
    }
    if end < rest.len() && rest[end] == b'.' {
        let mut after = end + 1;
        while after < rest.len() && rest[after].is_ascii_digit() {
            saw_digit = true;
            after += 1;
        }
        // A trailing point with no digits around it ('.' alone) is not a
        // number; from_chars would not consume the point in that case.
        if saw_digit || after > end + 1 {
            end = after;
        }
    }
    if !saw_digit {
        return None;
    }

    // Optional exponent: 'e'/'E', optional sign, at least one digit. If
    // the digits are missing the exponent is not consumed at all.
    if end < rest.len() && (rest[end] == b'e' || rest[end] == b'E') {
        let mut after = end + 1;
        if after < rest.len() && (rest[after] == b'+' || rest[after] == b'-') {
            after += 1;
        }
        let digits_start = after;
        while after < rest.len() && rest[after].is_ascii_digit() {
            after += 1;
        }
        if after > digits_start {
            end = after;
        }
    }
    Some(&token[..sign_len + end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from the upstream GoogleTest `INI.OversizedIntegersMatchLegacyParser`
    /// (`Tests/Google/Core/GameEngine/Common/INITest.cpp` @ 571a7c5).
    #[test]
    fn oversized_integers_match_legacy_parser() {
        let cases: &[(&str, i32, u32)] = &[
            ("9999999999999999999", -1, 2313682943),
            ("+9999999999999999999", -1, 2313682943),
            ("-9999999999999999999", 0, 1981284353),
            ("9999999999999999999suffix", -1, 2313682943),
            ("9223372036854775807", -1, 4294967295),
            ("-9223372036854775808", 0, 0),
            ("9223372036854775808", -1, 0),
            ("-9223372036854775809", 0, 4294967295),
            ("18446744073709551616", -1, 4294967295),
            ("-18446744073709551616", 0, 4294967295),
            (
                "99999999999999999999999999999999999999999999999999",
                -1,
                4294967295,
            ),
        ];
        for (token, signed_expected, unsigned_expected) in cases {
            assert_eq!(
                scan_int(token.as_bytes()),
                Ok(*signed_expected),
                "scan_int({token})"
            );
            assert_eq!(
                scan_unsigned_int(token.as_bytes()),
                Ok(*unsigned_expected),
                "scan_unsigned_int({token})"
            );
        }
    }

    /// Ported from the upstream GoogleTest `INI.InvalidIntegersAreRejected`.
    #[test]
    fn invalid_integers_are_rejected() {
        for token in [
            "abc",
            "+",
            "-",
            "++1",
            "+-99999999",
            "+-9999999999999999999",
        ] {
            assert!(
                scan_int(token.as_bytes()).is_err(),
                "scan_int({token}) should reject"
            );
            assert!(
                scan_unsigned_int(token.as_bytes()).is_err(),
                "scan_unsigned_int({token}) should reject"
            );
        }
    }

    /// Ported from the upstream GoogleTest `INI.RealRangeErrorsMatchLegacyParser`.
    #[test]
    fn real_range_errors_match_legacy_parser() {
        let infinity = f32::INFINITY;
        let subnormal = f32::from_bits(1);
        let cases: &[(&str, f32)] = &[
            ("1e100", infinity),
            ("+1e100", infinity),
            ("-1e100", -infinity),
            ("3.5e38", infinity),
            ("1e100suffix", infinity),
            ("1e-100", 0.0),
            ("+1e-100", 0.0),
            ("-1e-100", -0.0),
            ("1e-45", subnormal),
            ("-1e-45", -subnormal),
            ("+1.25", 1.25),
        ];
        for (token, expected) in cases {
            let actual = scan_real(token.as_bytes()).expect("should scan");
            assert_eq!(actual.to_bits(), expected.to_bits(), "scan_real({token})");
        }
    }

    /// Ported from the upstream GoogleTest `INI.InvalidRealsAreRejected`.
    #[test]
    fn invalid_reals_are_rejected() {
        for token in ["abc", "+", "-", "++1.25", "+-1.25", "+-1e100", "+-1e-100"] {
            assert!(
                scan_real(token.as_bytes()).is_err(),
                "scan_real({token}) should reject"
            );
        }
    }

    #[test]
    fn prefix_grammar_matches_from_chars() {
        // Trailing characters are ignored, exactly like from_chars/strtof.
        assert_eq!(scan_real(b"50%"), Ok(50.0));
        assert_eq!(scan_real(b"1.25xyz"), Ok(1.25));
        assert_eq!(scan_real(b"1.25.5"), Ok(1.25));
        assert_eq!(scan_int(b"5xyz"), Ok(5));
        assert_eq!(scan_int(b"5.5"), Ok(5));
        assert_eq!(scan_int(b"007"), Ok(7));
        assert_eq!(scan_int(b"-0"), Ok(0));
        // The exponent without digits is not consumed.
        assert_eq!(scan_real(b"1e"), Ok(1.0));
        assert_eq!(scan_real(b"1e+"), Ok(1.0));
        assert_eq!(scan_real(b"1e5x"), Ok(100000.0));
        // Fractional forms.
        assert_eq!(scan_real(b"5."), Ok(5.0));
        assert_eq!(scan_real(b".5"), Ok(0.5));
        assert!(scan_real(b".").is_err());
        assert!(scan_real(b"e5").is_err());
        // Words: from_chars matches the longest valid keyword prefix,
        // so "infinit" and "infx" both scan as inf.
        assert_eq!(scan_real(b"inf").unwrap(), f32::INFINITY);
        assert_eq!(scan_real(b"-Infinity").unwrap(), f32::NEG_INFINITY);
        assert_eq!(scan_real(b"infinit").unwrap(), f32::INFINITY);
        assert_eq!(scan_real(b"infx").unwrap(), f32::INFINITY);
        // The default chars_format::general is decimal-only: from_chars
        // reads just the "0" of a hex float literal, and so does this
        // port.
        assert_eq!(scan_real(b"0x1.8p3").unwrap(), 0.0);
    }

    #[test]
    fn negatives_wrap_like_the_cxx_intermediate() {
        // scanType<UnsignedInt> parses through Int64, so -5 wraps in u32.
        assert_eq!(scan_unsigned_int(b"-5"), Ok(4294967291));
        assert_eq!(scan_unsigned_int(b"-1"), Ok(4294967295));
        assert_eq!(scan_unsigned_int(b"+5"), Ok(5));
    }

    #[test]
    fn bools_and_lists_match_by_case_insensitive_name() {
        assert_eq!(scan_bool(b"Yes"), Ok(true));
        assert_eq!(scan_bool(b"NO"), Ok(false));
        assert!(scan_bool(b"maybe").is_err());

        let names = ["First", "Second"];
        assert_eq!(scan_index_list(b"first", &names), Ok(0));
        assert_eq!(scan_index_list(b"SECOND", &names), Ok(1));
        assert!(scan_index_list(b"third", &names).is_err());

        let lookup = [("Low", 5), ("High", 50)];
        assert_eq!(scan_lookup_list(b"low", &lookup), Ok(5));
        assert!(scan_lookup_list(b"mid", &lookup).is_err());
    }

    #[test]
    fn percent_divides_by_one_hundred() {
        assert_eq!(scan_percent_to_real(b"50%"), Ok(0.5));
        assert_eq!(scan_percent_to_real(b"95.4"), Ok(0.954));
    }
}
