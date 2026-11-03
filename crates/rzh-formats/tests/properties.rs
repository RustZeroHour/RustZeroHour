// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property tests for the CSF port (plan §6 test layers).
//!
//! The golden vectors pin the C++-observable behaviour on fixed corpora;
//! these properties pin the port's invariants on *arbitrary* input:
//!
//! - structures round-trip through bytes without loss,
//! - the canonical byte form is a fixed point of `parse ∘ to_bytes`
//!   (so any writer output re-encodes identically),
//! - parsing arbitrary bytes never panics, and whatever it accepts
//!   re-serializes to a file that parses back to the same structure.

use proptest::prelude::*;

use rzh_formats::csf::{CsfFile, CsfLabel, CsfString};

/// Text code units: mostly plausible string-table content (with spaces,
/// newlines and tabs, which `strip_spaces` consumes in the game view),
/// sometimes arbitrary units — surrogates, NULs, `0xFFFF` (the loader's
/// truncation quirk), CJK-range units.
fn text_units() -> impl Strategy<Value = Vec<u16>> {
    prop::collection::vec(
        prop::strategy::Union::new_weighted(vec![
            (6, (0x20u16..0x7f).boxed()),
            (1, Just(b' ' as u16).boxed()),
            (1, Just(b'\n' as u16).boxed()),
            (1, Just(b'\t' as u16).boxed()),
            (1, (0x4e00u16..0x9fff).boxed()),
            (1, prop::num::u16::ANY.boxed()),
        ]),
        0..48,
    )
}

/// Label name bytes: ASCII keys in practice, but the format allows any
/// byte (including NULs and high bytes) — all must round-trip.
fn label_name() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop::strategy::Union::new_weighted(vec![
            (8, (b'A'..b'z').boxed()),
            (1, prop::num::u8::ANY.boxed()),
        ]),
        0..32,
    )
}

/// Wave chunk bytes: the two shapes real writers emit (path + NUL, path +
/// language letter), plus arbitrary junk — all preserved verbatim.
fn wave_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::strategy::Union::new_weighted(vec![
        (4, prop::collection::vec(b'A'..b'z', 0..24).boxed()),
        (1, prop::collection::vec(prop::num::u8::ANY, 0..24).boxed()),
    ])
}

fn csf_string() -> impl Strategy<Value = CsfString> {
    (text_units(), proptest::option::of(wave_bytes()))
        .prop_map(|(text, wave)| CsfString { text, wave })
}

fn csf_label() -> impl Strategy<Value = CsfLabel> {
    (label_name(), prop::collection::vec(csf_string(), 0..4))
        .prop_map(|(name, strings)| CsfLabel { name, strings })
}

fn csf_file() -> impl Strategy<Value = CsfFile> {
    (
        prop::num::i32::ANY,
        prop::num::i32::ANY,
        prop::num::i32::ANY,
        prop::collection::vec(csf_label(), 0..16),
    )
        .prop_map(|(version, skip, language, labels)| CsfFile {
            version,
            skip,
            language,
            labels,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Structures survive `to_bytes` → `parse` unchanged.
    #[test]
    fn round_trips_through_bytes(file in csf_file()) {
        let bytes = file.to_bytes().unwrap();
        let parsed = CsfFile::parse(&bytes).unwrap();
        prop_assert_eq!(parsed, file);
    }

    /// The canonical byte form is a fixed point: any writer output
    /// re-encodes byte-identically (this is what makes the golden
    /// writer-direction pin enforceable for every structure).
    #[test]
    fn canonical_form_is_stable(file in csf_file()) {
        let once = file.to_bytes().unwrap();
        let twice = CsfFile::parse(&once).unwrap().to_bytes().unwrap();
        prop_assert_eq!(twice, once);
    }

    /// Parsing arbitrary bytes never panics; whatever is accepted
    /// re-serializes to a file that parses back to the same structure.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(prop::num::u8::ANY, 0..512)) {
        if let Ok(file) = CsfFile::parse(&bytes) {
            let reencoded = file.to_bytes().unwrap();
            let reparsed = CsfFile::parse(&reencoded).unwrap();
            prop_assert_eq!(reparsed, file);
        }
    }

    /// Header-only parsing never panics and agrees with the full parse on
    /// the header fields (the `getCSFInfo` path).
    #[test]
    fn header_agrees_with_full_parse(bytes in prop::collection::vec(prop::num::u8::ANY, 0..512)) {
        let header = CsfFile::parse_header(&bytes);
        let full = CsfFile::parse(&bytes);
        prop_assert_eq!(header.is_ok(), full.is_ok());
        if let (Ok(header), Ok(file)) = (header, full) {
            prop_assert_eq!(header.version, file.version);
            prop_assert_eq!(header.skip, file.skip);
            prop_assert_eq!(header.language, file.language);
        }
    }
}

/// Error-path unit tests: the malformed shapes whose C++ behaviour is
/// undefined (out-of-bounds buffer writes, stale-buffer reuse) pin the
/// port's error contract — the golden vectors only pin *defined* C++
/// behaviour, so these live here (see the module docs).
#[cfg(test)]
mod error_paths {
    use rzh_formats::csf::*;

    fn base_file() -> CsfFile {
        CsfFile {
            version: 3,
            skip: 0,
            language: 0,
            labels: vec![CsfLabel {
                name: b"GUI:Hello".to_vec(),
                strings: vec![CsfString::new("Hi".encode_utf16().collect())],
            }],
        }
    }

    #[test]
    fn truncated_header() {
        for len in 0..HEADER_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                CsfFile::parse(&bytes),
                Err(CsfError::TruncatedHeader { got: len })
            );
            assert_eq!(
                CsfFile::parse_header(&bytes),
                Err(CsfError::TruncatedHeader { got: len })
            );
        }
    }

    #[test]
    fn bad_magic() {
        let mut bytes = base_file().to_bytes().unwrap();
        bytes[0] = b'X';
        let got = u32::from_le_bytes(*b"XFSC");
        assert_eq!(
            CsfFile::parse_header(&bytes),
            Err(CsfError::BadMagic { got })
        );
    }

    #[test]
    fn truncated_label_name() {
        let mut bytes = base_file().to_bytes().unwrap();
        // Layout: header(24) + label id(4) + count(4) + name_len(4) +
        // name(9) — cut with only 5 of the 9 name bytes present.
        bytes.truncate(HEADER_LEN + 12 + 5);
        assert!(matches!(
            CsfFile::parse(&bytes),
            Err(CsfError::Truncated {
                field: "label name",
                ..
            })
        ));
    }

    #[test]
    fn truncated_string_text() {
        let mut bytes = base_file().to_bytes().unwrap();
        bytes.truncate(bytes.len() - 1); // cut into the inverted text
        assert!(matches!(
            CsfFile::parse(&bytes),
            Err(CsfError::Truncated {
                field: "string text",
                ..
            })
        ));
    }

    #[test]
    fn truncated_wave_path() {
        let file = CsfFile {
            version: 3,
            skip: 0,
            language: 0,
            labels: vec![CsfLabel {
                name: b"GUI:Speech".to_vec(),
                strings: vec![CsfString::with_wave(Vec::new(), b"SPEECH:x.wav")],
            }],
        };
        let mut bytes = file.to_bytes().unwrap();
        bytes.truncate(bytes.len() - 1);
        assert!(matches!(
            CsfFile::parse(&bytes),
            Err(CsfError::Truncated {
                field: "wave path",
                ..
            })
        ));
    }

    #[test]
    fn negative_label_name_length() {
        let mut bytes = base_file().to_bytes().unwrap();
        // The label-name length is the third header word + 8 bytes in.
        let off = HEADER_LEN + 8;
        bytes[off..off + 4].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(
            CsfFile::parse(&bytes),
            Err(CsfError::InvalidLength {
                field: "label name",
                got: -1
            })
        ));
    }

    #[test]
    fn negative_text_length() {
        let mut bytes = base_file().to_bytes().unwrap();
        // Layout: header(24) + label id(4) + count(4) + name_len(4) +
        // name(9) + string id(4) = 49 — the text length field.
        let off = 49;
        bytes[off..off + 4].copy_from_slice(&(-2i32).to_le_bytes());
        assert!(matches!(
            CsfFile::parse(&bytes),
            Err(CsfError::InvalidLength {
                field: "string text",
                got: -2
            })
        ));
    }

    #[test]
    fn string_id_mismatch_reports_offset() {
        let mut bytes = base_file().to_bytes().unwrap();
        let off = 45; // the string chunk id, after header + label id + count + name_len + name
        bytes[off..off + 4].copy_from_slice(&0xdead_beefu32.to_le_bytes());
        assert_eq!(
            CsfFile::parse(&bytes),
            Err(CsfError::StringIdMismatch {
                got: 0xdead_beef,
                offset: off
            })
        );
    }

    #[test]
    fn label_id_mismatch_reports_offset() {
        let mut bytes = base_file().to_bytes().unwrap();
        // Append a second, corrupt label id.
        let off = bytes.len();
        bytes.extend_from_slice(&0xdead_beefu32.to_le_bytes());
        assert_eq!(
            CsfFile::parse(&bytes),
            Err(CsfError::LabelIdMismatch {
                got: 0xdead_beef,
                offset: off
            })
        );
    }

    #[test]
    fn trailing_short_bytes_are_accepted() {
        // The original's read loop ends on a 1..3-byte remnant and accepts
        // the file.
        let mut bytes = base_file().to_bytes().unwrap();
        bytes.extend_from_slice(&[0x42, 0x00]);
        let file = CsfFile::parse(&bytes).unwrap();
        assert_eq!(file.labels.len(), 1);
    }

    #[test]
    fn declared_counts_are_ignored_on_read() {
        // The original never validates num_labels/num_strings.
        let mut bytes = base_file().to_bytes().unwrap();
        bytes[8..12].copy_from_slice(&7i32.to_le_bytes()); // num_labels
        bytes[12..16].copy_from_slice(&99i32.to_le_bytes()); // num_strings
        let file = CsfFile::parse(&bytes).unwrap();
        assert_eq!(file.labels.len(), 1);
        // ... and the writer canonicalizes them (Babylon's form).
        assert_ne!(file.to_bytes().unwrap(), bytes);
        assert_eq!(file.to_bytes().unwrap()[8..12], 1i32.to_le_bytes());
        assert_eq!(file.to_bytes().unwrap()[12..16], 1i32.to_le_bytes());
    }

    #[test]
    fn negative_per_label_string_count_behaves_as_zero() {
        let mut bytes = base_file().to_bytes().unwrap();
        let off = HEADER_LEN + 4; // the label's num_strings
        bytes[off..off + 4].copy_from_slice(&(-3i32).to_le_bytes());
        // The original's copy loop never runs; parsing continues after
        // the label's name — the next dword is the string chunk id
        // (" RTS"), which is not a label id, so the whole file is
        // rejected exactly like the original's goto quit.
        assert_eq!(
            CsfFile::parse(&bytes),
            Err(CsfError::LabelIdMismatch {
                got: CSF_STRING,
                offset: 45
            })
        );
    }

    #[test]
    fn game_view_of_quirk_units() {
        // 0xFFFF (stored 0x0000) and 0x0000 (stored 0xFFFF) both end the
        // runtime-visible text — see the module docs.
        let text: Vec<u16> = vec!['a' as u16, 0xFFFF, 'b' as u16];
        let label = CsfLabel {
            name: b"L".to_vec(),
            strings: vec![CsfString::new(text.clone())],
        };
        assert_eq!(label.game_text(), vec!['a' as u16]);

        let text: Vec<u16> = vec!['a' as u16, 0x0000, 'b' as u16];
        let label = CsfLabel {
            name: b"L".to_vec(),
            strings: vec![CsfString::new(text.clone())],
        };
        assert_eq!(label.game_text(), vec!['a' as u16]);

        // Both survive the format layer untouched and round-trip.
        let file = CsfFile {
            version: 3,
            skip: 0,
            language: 0,
            labels: vec![CsfLabel {
                name: b"L".to_vec(),
                strings: vec![CsfString::new(vec![0xFFFF, 0x0000, 'x' as u16])],
            }],
        };
        let parsed = CsfFile::parse(&file.to_bytes().unwrap()).unwrap();
        assert_eq!(
            parsed.labels[0].strings[0].text,
            vec![0xFFFF, 0x0000, 'x' as u16]
        );
    }

    #[test]
    fn strip_spaces_shapes() {
        fn strip(s: &str) -> String {
            let mut units: Vec<u16> = s.encode_utf16().collect();
            strip_spaces(&mut units);
            String::from_utf16(&units).unwrap()
        }
        assert_eq!(strip("  hello  "), "hello");
        assert_eq!(strip("a  b"), "a b");
        assert_eq!(strip("a \nb"), "a\nb"); // space before newline dropped
        assert_eq!(strip("   "), "");
        assert_eq!(strip(""), "");
        assert_eq!(strip("\t\ta\t\t"), "\t\ta\t\t"); // tabs kept
        // A newline/tab re-arms the leading-space skip (skipall = TRUE),
        // so the space after it goes away.
        assert_eq!(strip("\n a"), "\na");
    }
}
