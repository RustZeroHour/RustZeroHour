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

// ---------------------------------------------------------------------------
// TGA (TARGA.cpp transcription) — plan §6 property layer, continued.
// ---------------------------------------------------------------------------

use rzh_formats::tga::{
    SaveFlags, TGA_CMAPPED, TGA_CMAPPED_ENCODED, TGA_MONO, TGA_MONO_ENCODED, TGA_TRUECOLOR,
    TGA_TRUECOLOR_ENCODED, TGA2_FOOTER_SIZE, TGAIDF_XORIGIN, TGAIDF_YORIGIN, Tga2Footer, TgaError,
    TgaFile, TgaHeader,
};

/// Image types the original's `Load` switch accepts, weighted toward the
/// shapes the game's textures actually use.
fn tga_image_type() -> impl Strategy<Value = u8> {
    prop::strategy::Union::new_weighted(vec![
        (4, Just(TGA_TRUECOLOR).boxed()),
        (3, Just(TGA_TRUECOLOR_ENCODED).boxed()),
        (2, Just(TGA_CMAPPED).boxed()),
        (2, Just(TGA_CMAPPED_ENCODED).boxed()),
        (1, Just(TGA_MONO).boxed()),
    ])
}

/// Pixel depths the game ships: 8 (palettes/mono), 16, 24, 32.
fn tga_pixel_depth() -> impl Strategy<Value = u8> {
    prop::strategy::Union::new_weighted(vec![
        (4, Just(24u8).boxed()),
        (4, Just(32u8).boxed()),
        (1, Just(16u8).boxed()),
        (1, Just(8u8).boxed()),
    ])
}

/// A well-formed Targa structure in the state the original's callers hold
/// it: the post-`Load` in-memory form — the origin bits of the descriptor
/// are cleared and the image is flipped accordingly (the strategy applies
/// the same normalization `Load` does), so the structure round-trips
/// through its own save shape.
fn tga_file() -> impl Strategy<Value = TgaFile> {
    (
        tga_image_type(),
        tga_pixel_depth(),
        prop::collection::vec(prop::num::u8::ANY, 0..64), // dimensions live below
        prop::option::of(prop::collection::vec(prop::num::u8::ANY, 0..16)), // palette
        prop::bool::ANY,                                  // footer
        prop::bool::ANY,                                  // extension
        prop::bool::ANY,                                  // origin flags
    )
        .prop_map(
            |(image_type, pixel_depth, dim_bytes, palette, footer, extension, origins)| {
                let width = 1 + (dim_bytes.first().copied().unwrap_or(0) as u16 % 6);
                let height = 1 + (dim_bytes.get(1).copied().unwrap_or(0) as u16 % 6);
                let depth = ((pixel_depth as usize) + 7) >> 3;
                let mut header = TgaHeader {
                    color_map_type: if image_type == TGA_CMAPPED
                        || image_type == TGA_CMAPPED_ENCODED
                    {
                        1
                    } else {
                        0
                    },
                    cmap_start: 0,
                    cmap_length: 0,
                    cmap_depth: 0,
                    image_type,
                    width,
                    height,
                    pixel_depth,
                    image_descriptor: 0,
                    ..TgaHeader::default()
                };
                if header.color_map_type == 1 {
                    // Palette entry count and depth from the generated chunk.
                    let chunk = palette.clone().unwrap_or_default();
                    header.cmap_depth = 24;
                    header.cmap_length = (chunk.len() / 3) as u16;
                }
                if origins {
                    header.image_descriptor |= TGAIDF_YORIGIN;
                }
                // The X-origin bit mirrors within rows; keep both shapes.
                if dim_bytes.len() > 2 && dim_bytes[2] & 1 == 1 {
                    header.image_descriptor |= TGAIDF_XORIGIN;
                }

                let image_size = width as usize * height as usize * depth;
                // Pixel bytes: formula-driven so runs appear (the encoder's run
                // branches) but the content stays deterministic per size.
                let mut image = vec![0u8; image_size];
                for (i, b) in image.iter_mut().enumerate() {
                    *b = ((i / depth.max(1)) as u8)
                        .wrapping_mul(31)
                        .wrapping_add((i as u8) % 7);
                }
                // Colormapped/mono images hold 8-bit indices: keep them in
                // palette range so the structure stays semantically sane (the
                // format itself does not care).
                let image = if header.color_map_type == 1 || image_type == TGA_MONO {
                    image.iter().map(|b| b % 4).collect()
                } else {
                    image
                };

                let palette = if header.color_map_type == 1 {
                    // Exactly the chunk the header declares (the save writes
                    // cmap_length × (cmap_depth >> 3) bytes of it).
                    let size = header.cmap_length as usize * (header.cmap_depth >> 3) as usize;
                    let mut chunk = palette.unwrap_or_default();
                    chunk.truncate(size);
                    chunk
                } else {
                    Vec::new()
                };

                let mut file = TgaFile {
                    header,
                    footer: footer.then_some(Tga2Footer {
                        extension: 0,
                        developer: 0,
                    }),
                    extension: extension.then(Default::default),
                    palette,
                    image,
                };
                // Model the load's normalization (the callers' in-memory state):
                // mirror per the origin bits, then clear them — exactly what
                // `Targa::Load` leaves behind.
                if file.header.image_descriptor & TGAIDF_XORIGIN != 0 {
                    file.xflip();
                    file.header.image_descriptor &= !TGAIDF_XORIGIN;
                }
                if file.header.image_descriptor & TGAIDF_YORIGIN != 0 {
                    file.yflip();
                    file.header.image_descriptor &= !TGAIDF_YORIGIN;
                }
                file
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A well-formed structure survives its own save shape unchanged:
    /// `to_bytes` → `parse` → the same structure, image included (the
    /// save's truecolor inversion is undone by the load's).
    #[test]
    fn tga_round_trips_through_bytes(file in tga_file()) {
        let flags = SaveFlags {
            image: true,
            palette: true,
            compress: file.header.is_compressed(),
        };
        let bytes = file.to_bytes(flags, false).unwrap();
        let parsed = TgaFile::parse(&bytes).unwrap();
        prop_assert_eq!(parsed.header, file.header);
        prop_assert_eq!(parsed.palette, file.palette);
        prop_assert_eq!(parsed.image, file.image);
        // The save always appends the footer; a file that had none gains
        // the default one (the original's behaviour too).
        prop_assert_eq!(parsed.footer.or(Some(Tga2Footer::default())), file.footer.or(Some(Tga2Footer::default())));
    }

    /// The canonical form is a fixed point of `parse ∘ to_bytes`.
    #[test]
    fn tga_canonical_form_is_stable(file in tga_file()) {
        let flags = SaveFlags {
            image: true,
            palette: true,
            compress: file.header.is_compressed(),
        };
        let once = file.to_bytes(flags, false).unwrap();
        let twice = TgaFile::parse(&once).unwrap().to_bytes(flags, false).unwrap();
        prop_assert_eq!(twice, once);
    }

    /// The RLE encoder and decoder compose to the identity on arbitrary
    /// pixel buffers (with the count-byte grammar exercised on both ends).
    #[test]
    fn tga_rle_composes_to_identity(
        depth in 1usize..5,
        pixels in prop::collection::vec(
            prop::strategy::Union::new_weighted(vec![
                (3, (0u8..16).boxed()),
                (2, Just(0u8).boxed()),
                (2, Just(255u8).boxed()),
                (1, prop::num::u8::ANY.boxed()),
            ]),
            0..600,
        ),
    ) {
        let image: Vec<u8> = pixels.iter().flat_map(|p| vec![*p; depth]).collect();
        let encoded = rzh_formats::tga::encode_rle(&image, depth);
        let mut decoded = vec![0u8; image.len()];
        let mut cursor = 0usize;
        rzh_formats::tga::decode_rle(&encoded, &mut cursor, &mut decoded, depth).unwrap();
        prop_assert_eq!(decoded, image);
        // The stream is fully consumed (no trailing packets).
        prop_assert_eq!(cursor, encoded.len());
    }

    /// Parsing arbitrary bytes never panics (the loader's bounds checks
    /// cover every offset field: ID length, footer offsets, chunk sizes).
    #[test]
    fn tga_arbitrary_bytes_never_panic(bytes in prop::collection::vec(prop::num::u8::ANY, 0..600)) {
        // Both caller shapes: the library default and the texture loader's.
        let _ = TgaFile::parse(&bytes);
        let _ = TgaFile::parse_ex(&bytes, true, true, false, TGAIDF_YORIGIN);
    }

    /// The texture-loader and library-default shapes agree on everything
    /// except the two things that differ by design: the truecolor invert
    /// (parameterized) and the Y-origin XOR (pre-load — the XOR sets the
    /// bit on a normalized descriptor, so the loader additionally Y-flips).
    #[test]
    fn tga_parse_shapes_differ_exactly_as_documented(file in tga_file()) {
        let bytes = file
            .to_bytes(SaveFlags { image: true, palette: true, compress: file.header.is_compressed() }, false)
            .unwrap();
        let default = TgaFile::parse(&bytes).unwrap();
        let loader = TgaFile::parse_ex(&bytes, false, true, false, TGAIDF_YORIGIN).unwrap();
        // The loader wants no palette; it still consumes the chunk.
        prop_assert_eq!(loader.palette.is_empty(), true);
        // The loader's descriptor: the saved (normalized) one with the
        // Y-origin bit XORed back in — set, so Load y-flipped.
        prop_assert_eq!(loader.header.image_descriptor, file.header.image_descriptor);

        // Expectation: un-invert the default's pixels (the loader reads the
        // file's raw bytes) — truecolor only, the invert never touches
        // colormapped/mono data — then apply the loader's extra Y flip (the
        // per-pixel byte reversal and the row exchange commute).
        let depth = ((file.header.pixel_depth as usize) + 7) >> 3;
        let mut expect = default.image.clone();
        let truecolor = file.header.image_type == TGA_TRUECOLOR
            || file.header.image_type == TGA_TRUECOLOR_ENCODED;
        if truecolor && depth > 2 {
            for pixel in expect.chunks_mut(depth) {
                pixel.reverse();
            }
        }
        let stride = file.header.width as usize * depth;
        let height = file.header.height as usize;
        for y in 0..height / 2 {
            let top = y * stride;
            let bottom = (height - 1 - y) * stride;
            // top < bottom and the rows do not overlap.
            let (head, tail) = expect.split_at_mut(bottom);
            head[top..top + stride].swap_with_slice(&mut tail[..stride]);
        }
        prop_assert_eq!(loader.image, expect);
    }
}

/// Error-path unit tests: the malformed shapes whose C++ behaviour is
/// undefined (buffer overruns on over-supplied RLE streams, palette writes
/// past the allocation) pin the port's error contract.
#[cfg(test)]
mod tga_error_paths {
    use super::*;
    use rzh_formats::tga::{TGA2_EXTENSION_SIZE, tga_bytes_per_pixel};

    fn file_bytes(header: TgaHeader, body: &[u8]) -> Vec<u8> {
        let mut bytes = header.to_bytes().to_vec();
        bytes.extend_from_slice(body);
        bytes.resize(bytes.len() + TGA2_FOOTER_SIZE, 0);
        bytes
    }

    fn raw_header(image_type: u8, width: u16, height: u16, pixel_depth: u8) -> TgaHeader {
        TgaHeader {
            image_type,
            width,
            height,
            pixel_depth,
            ..TgaHeader::default()
        }
    }

    #[test]
    fn rle_overrun_by_run() {
        // A 2-pixel image; the stream answers with a 5-pixel run: the C++
        // writes past its buffer and underflows its bookkeeping.
        let bytes = file_bytes(
            raw_header(TGA_TRUECOLOR_ENCODED, 2, 1, 24),
            &[0x84, 1, 2, 3],
        );
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::ImageOverrun));
    }

    #[test]
    fn rle_overrun_by_raw() {
        let bytes = file_bytes(raw_header(TGA_TRUECOLOR_ENCODED, 1, 1, 8), &[0x02, 1, 2, 3]);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::ImageOverrun));
    }

    #[test]
    fn cmap_origin_refused() {
        // cmap_start = 1: the original writes the chunk at palette + depth
        // — past its own allocation.
        let mut header = raw_header(TGA_CMAPPED, 2, 1, 8);
        header.color_map_type = 1;
        header.cmap_start = 1;
        header.cmap_length = 2;
        header.cmap_depth = 24;
        let bytes = file_bytes(header, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::CMapOrigin));
        // Not storing the palette is fine (the original merely seeks).
        assert!(TgaFile::parse_ex(&bytes, false, true, true, 0).is_ok());
    }

    #[test]
    fn extension_offset_out_of_range() {
        let mut bytes = file_bytes(raw_header(TGA_MONO, 1, 1, 8), &[7]);
        // Stamp a signature footer with an extension offset past the data.
        let len = bytes.len();
        let footer = Tga2Footer {
            extension: 3, // inside the file but short of 495 bytes
            developer: 0,
        };
        bytes[len - TGA2_FOOTER_SIZE..].copy_from_slice(&footer.to_bytes());
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::Truncated));
    }

    #[test]
    fn short_footer_never_opens() {
        // Every length below the footer fails the seek — 18-byte
        // header-only files included.
        for len in 0..TGA2_FOOTER_SIZE {
            let bytes = vec![0u8; len];
            assert_eq!(TgaFile::parse(&bytes), Err(TgaError::Truncated));
        }
    }

    #[test]
    fn depth_matches_bytes_per_pixel() {
        for (bits, expected) in [(1u8, 1usize), (8, 1), (15, 2), (16, 2), (24, 3), (32, 4)] {
            assert_eq!(tga_bytes_per_pixel(bits), expected);
        }
    }

    #[test]
    fn zero_size_image_leaves_descriptor() {
        // Zero width with an unsupported type: no image block runs, so the
        // unsupported-type rejection never fires (the original skips the
        // switch when its allocation is empty) and the descriptor survives.
        let mut header = raw_header(TGA_MONO_ENCODED, 0, 3, 8);
        header.image_descriptor = TGAIDF_XORIGIN;
        let bytes = file_bytes(header, &[]);
        let file = TgaFile::parse(&bytes).unwrap();
        assert!(file.image.is_empty());
        assert_eq!(file.header.image_descriptor, TGAIDF_XORIGIN);
    }

    #[test]
    fn extension_layout_is_495_bytes() {
        assert_eq!(TGA2_EXTENSION_SIZE, 495);
    }
}

#[cfg(test)]
mod tga_size_guard {
    use rzh_formats::tga::{TGA2_FOOTER_SIZE, TgaError, TgaFile, TgaHeader};

    #[test]
    fn absurd_dimensions_refused_before_allocating() {
        // 65535 × 65535 × 32-bit = 17 GB: the original's malloc fails
        // (TGAERR_NOMEM); the port refuses with TooLarge instead of
        // attempting the allocation — before the image-type switch runs.
        let header = TgaHeader {
            image_type: rzh_formats::tga::TGA_MONO_ENCODED, // would be "unsupported" too
            width: 65535,
            height: 65535,
            pixel_depth: 32,
            ..TgaHeader::default()
        };
        let mut bytes = header.to_bytes().to_vec();
        bytes.resize(bytes.len() + TGA2_FOOTER_SIZE, 0);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::TooLarge));
    }
}

// ---------------------------------------------------------------------------
// chunk — the W3D chunk container (chunkio.cpp port)
// ---------------------------------------------------------------------------

mod chunk_props {
    use proptest::prelude::*;
    use rzh_formats::chunk::{ChunkError, ChunkLoad, ChunkSave, MAX_CHUNK_DEPTH, MAX_SAVE_DEPTH};

    /// A well-formed chunk tree: containers hold children, data chunks hold
    /// micro chunks (payloads within the writer's `u8` size field) plus raw
    /// trailing bytes.
    #[derive(Debug, Clone, PartialEq)]
    enum Node {
        Data {
            id: u32,
            micros: Vec<(u8, Vec<u8>)>,
            payload: Vec<u8>,
        },
        Container {
            id: u32,
            children: Vec<Node>,
        },
    }

    fn node(depth: u8) -> impl Strategy<Value = Node> {
        let id = prop::sample::select(vec![
            0u32, 1, 2, 0x5E, 0x1A, 0xABCDEF, 0x7FFFFFFF, 0xFFFFFFFF, 0x80000000,
        ]);
        if depth == 0 {
            (
                id,
                prop::collection::vec(
                    (
                        prop::num::u8::ANY,
                        prop::collection::vec(prop::num::u8::ANY, 0..64),
                    ),
                    0..3,
                ),
                prop::collection::vec(prop::num::u8::ANY, 0..96),
            )
                .prop_map(|(id, micros, payload)| Node::Data {
                    id,
                    micros,
                    payload,
                })
                .boxed()
        } else {
            (id, prop::collection::vec(node(depth - 1), 0..3))
                .prop_map(move |(id, children)| {
                    if children.is_empty() {
                        Node::Data {
                            id,
                            micros: Vec::new(),
                            payload: Vec::new(),
                        }
                    } else {
                        Node::Container { id, children }
                    }
                })
                .boxed()
        }
    }

    fn tree() -> impl Strategy<Value = Node> {
        node(3)
    }

    fn write_node(save: &mut ChunkSave, node: &Node) -> Result<(), ChunkError> {
        match node {
            Node::Data {
                id,
                micros,
                payload,
            } => {
                save.begin_chunk(*id)?;
                for (mid, mpayload) in micros {
                    save.begin_micro_chunk(*mid)?;
                    save.write(mpayload)?;
                    save.end_micro_chunk()?;
                }
                save.write(payload)?;
                save.end_chunk()?;
            }
            Node::Container { id, children } => {
                save.begin_chunk(*id)?;
                for child in children {
                    write_node(save, child)?;
                }
                save.end_chunk()?;
            }
        }
        Ok(())
    }

    /// Reads a tree back with the same lens the writer used (data chunks:
    /// the whole payload raw; containers: recurse). The micro bytes come
    /// back as part of the payload — the framing cannot tell them apart,
    /// so the comparison uses the concatenated form.
    fn read_node(load: &mut ChunkLoad) -> Result<Option<Node>, ChunkError> {
        if !load.open_chunk()? {
            return Ok(None);
        }
        let id = load.cur_chunk_id();
        let node = if load.contains_chunks() {
            let mut children = Vec::new();
            while let Some(child) = read_node(load)? {
                children.push(child);
            }
            Node::Container { id, children }
        } else {
            let mut payload = Vec::new();
            let mut remaining = load.cur_chunk_length();
            let mut buf = [0u8; 64];
            while remaining > 0 {
                let piece = remaining.min(64) as usize;
                let n = load.read(&mut buf[..piece])?;
                if n == 0 {
                    break;
                }
                payload.extend_from_slice(&buf[..n as usize]);
                remaining -= n;
            }
            Node::Data {
                id,
                micros: Vec::new(),
                payload,
            }
        };
        load.close_chunk()?;
        Ok(Some(node))
    }

    /// The concatenated form: micro chunks serialize into the payload.
    fn flatten(node: &Node) -> Node {
        match node {
            Node::Data {
                id,
                micros,
                payload,
            } => {
                let mut flat = Vec::new();
                for (mid, mpayload) in micros {
                    flat.push(*mid);
                    flat.push(mpayload.len() as u8);
                    flat.extend_from_slice(mpayload);
                }
                flat.extend_from_slice(payload);
                Node::Data {
                    id: *id,
                    micros: Vec::new(),
                    payload: flat,
                }
            }
            Node::Container { id, children } => Node::Container {
                id: *id,
                children: children.iter().map(flatten).collect(),
            },
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Trees survive write → read unchanged (in the concatenated form
        /// the framing can express).
        #[test]
        fn round_trips_through_bytes(tree in tree()) {
            let mut save = ChunkSave::new();
            write_node(&mut save, &tree).unwrap();
            let bytes = save.into_bytes().unwrap();
            let mut load = ChunkLoad::new(&bytes);
            let mut read_back = Vec::new();
            while let Some(node) = read_node(&mut load).unwrap() {
                read_back.push(node);
            }
            prop_assert_eq!(read_back, vec![flatten(&tree)]);
        }

        /// The byte form is a fixed point: read back and rewritten, the
        /// bytes are identical.
        #[test]
        fn canonical_form_is_stable(tree in tree()) {
            let mut save = ChunkSave::new();
            write_node(&mut save, &tree).unwrap();
            let once = save.into_bytes().unwrap();

            let mut load = ChunkLoad::new(&once);
            let mut rewritten = ChunkSave::new();
            rewrite_tree(&mut load, &mut rewritten).unwrap();
            let twice = rewritten.into_bytes().unwrap();
            prop_assert_eq!(twice, once);
        }

        /// Walking arbitrary bytes never panics (errors are fine — the
        /// depth cap and the state contracts are port guarantees).
        #[test]
        fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(prop::num::u8::ANY, 0..1024)) {
            let mut load = ChunkLoad::new(&bytes);
            tolerant_walk(&mut load, 0);
        }
    }

    /// Rewrites whatever the reader accepts through the writer (the
    /// fixed-point property's second half).
    fn rewrite_tree(load: &mut ChunkLoad, save: &mut ChunkSave) -> Result<(), ChunkError> {
        while load.open_chunk()? {
            let id = load.cur_chunk_id();
            save.begin_chunk(id)?;
            if load.contains_chunks() {
                rewrite_tree(load, save)?;
            } else {
                let mut remaining = load.cur_chunk_length();
                let mut buf = [0u8; 64];
                while remaining > 0 {
                    let piece = remaining.min(64) as usize;
                    let n = load.read(&mut buf[..piece])?;
                    if n == 0 {
                        break;
                    }
                    save.write(&buf[..n as usize])?;
                    remaining -= n;
                }
            }
            load.close_chunk()?;
            save.end_chunk()?;
        }
        Ok(())
    }

    /// The never-panic walker: both lenses, errors tolerated, iteration
    /// caps so a pathological loop fails the property instead of hanging.
    fn tolerant_walk(load: &mut ChunkLoad, depth: usize) {
        let mut cap = 100_000usize;
        while cap > 0 {
            cap -= 1;
            let Ok(true) = load.open_chunk() else { return };
            if load.contains_chunks() && depth < 300 {
                tolerant_walk(load, depth + 1);
            } else {
                // payload lens
                let mut remaining = load.cur_chunk_length();
                let mut buf = [0u8; 64];
                let mut reads = 0usize;
                while remaining > 0 && reads < 100_000 {
                    reads += 1;
                    let piece = remaining.min(64) as usize;
                    let Ok(n) = load.read(&mut buf[..piece]) else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    remaining -= n;
                }
                // micro lens
                let mut micros = 0usize;
                while micros < 100_000 {
                    micros += 1;
                    let Ok(true) = load.open_micro_chunk() else {
                        break;
                    };
                    let mut mremaining = load.cur_micro_chunk_length() as u32;
                    let mut mreads = 0usize;
                    while mremaining > 0 && mreads < 300 {
                        mreads += 1;
                        let piece = mremaining.min(64) as usize;
                        let Ok(n) = load.read(&mut buf[..piece]) else {
                            break;
                        };
                        if n == 0 {
                            break;
                        }
                        mremaining -= n;
                    }
                    let _ = load.close_micro_chunk();
                }
            }
            let _ = load.close_chunk();
        }
        panic!("tolerant walk exceeded its iteration cap");
    }

    // --- writer state contracts (the original's asserts, as errors) ---------

    #[test]
    fn write_requires_an_open_chunk() {
        let mut save = ChunkSave::new();
        assert_eq!(save.write(b"x"), Err(ChunkError::NoOpenChunk));
        assert_eq!(save.end_chunk(), Err(ChunkError::NoOpenChunk));
        assert_eq!(save.end_micro_chunk(), Err(ChunkError::NoOpenChunk));
    }

    #[test]
    fn data_cannot_follow_sub_chunks() {
        let mut save = ChunkSave::new();
        save.begin_chunk(1).unwrap();
        save.begin_chunk(2).unwrap();
        save.end_chunk().unwrap();
        assert_eq!(save.write(b"x"), Err(ChunkError::DataAfterSubChunks));
    }

    #[test]
    fn micro_chunks_cannot_nest_or_outlive_their_chunk() {
        let mut save = ChunkSave::new();
        save.begin_chunk(1).unwrap();
        save.begin_micro_chunk(7).unwrap();
        assert_eq!(save.begin_micro_chunk(8), Err(ChunkError::NestedMicroChunk));
        // chunk-level operations while the micro chunk is open
        assert_eq!(save.end_chunk(), Err(ChunkError::MicroChunkOpen));
        // a chunk whose payload begins with a real micro header (id 7,
        // size 2) so the micro chunk actually opens
        let bytes: Vec<u8> = [1u32, 4u32]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .chain([7u8, 2, 0xAA, 0xBB].iter().copied())
            .collect();
        let mut load = ChunkLoad::new(&bytes);
        load.open_chunk().unwrap();
        assert!(load.open_micro_chunk().unwrap());
        assert_eq!(load.open_micro_chunk(), Err(ChunkError::NestedMicroChunk));
        assert_eq!(load.open_chunk(), Err(ChunkError::MicroChunkOpen));
        assert_eq!(load.close_chunk(), Err(ChunkError::MicroChunkOpen));
    }

    #[test]
    fn micro_chunk_size_wraps_are_refused() {
        // 255 total is the release-build limit (the debug assert is an
        // off-by-one that rejects it — see the module docs); 256 would
        // wrap the u8 size field.
        let mut save = ChunkSave::new();
        save.begin_chunk(1).unwrap();
        save.begin_micro_chunk(7).unwrap();
        assert_eq!(save.write(&[0xA5; 255]).unwrap(), 255);
        assert_eq!(save.write(&[0x5A; 1]), Err(ChunkError::MicroChunkTooLarge));
    }

    #[test]
    fn writer_depth_caps_at_the_stack_bound() {
        let mut save = ChunkSave::new();
        for _ in 0..MAX_SAVE_DEPTH {
            save.begin_chunk(1).unwrap();
        }
        assert_eq!(save.cur_chunk_depth(), MAX_SAVE_DEPTH);
        assert_eq!(save.begin_chunk(1), Err(ChunkError::TooDeep));
    }

    #[test]
    fn reader_depth_caps_at_the_assert_boundary() {
        // A tree exactly MAX_CHUNK_DEPTH deep reads back fine (the
        // original's assert allows exactly this); one level more refuses.
        let build = |depth: usize| {
            let mut save = ChunkSave::new();
            for _ in 0..depth {
                save.begin_chunk(1).unwrap();
            }
            for _ in 0..depth {
                save.end_chunk().unwrap();
            }
            save.into_bytes().unwrap()
        };
        fn deepest(load: &mut ChunkLoad) -> Result<usize, ChunkError> {
            let mut max = 0;
            while load.open_chunk()? {
                max = max.max(load.cur_chunk_depth());
                if load.contains_chunks() {
                    max = max.max(deepest(load)?);
                }
                load.close_chunk()?;
            }
            Ok(max)
        }

        let legal = build(MAX_CHUNK_DEPTH);
        let mut load = ChunkLoad::new(&legal);
        assert_eq!(deepest(&mut load).unwrap(), MAX_CHUNK_DEPTH);

        let over = build(MAX_SAVE_DEPTH); // the writer can build deeper…
        let mut load = ChunkLoad::new(&over);
        assert_eq!(deepest(&mut load), Err(ChunkError::TooDeep));
    }

    #[test]
    fn unclosed_chunks_refuse_the_bytes() {
        let mut save = ChunkSave::new();
        save.begin_chunk(1).unwrap();
        assert_eq!(save.into_bytes(), Err(ChunkError::UnclosedChunk));

        let mut save = ChunkSave::new();
        save.begin_chunk(1).unwrap();
        save.begin_micro_chunk(2).unwrap();
        save.end_micro_chunk().unwrap();
        assert_eq!(save.into_bytes(), Err(ChunkError::UnclosedChunk));
    }

    #[test]
    fn read_and_seek_refuse_without_an_open_chunk() {
        let mut load = ChunkLoad::new(&[0; 32]);
        let mut buf = [0u8; 4];
        assert_eq!(load.read(&mut buf), Err(ChunkError::NoOpenChunk));
        assert_eq!(load.seek(4), Err(ChunkError::NoOpenChunk));
    }
}

// ===========================================================================
// dds — the DDS texture port's invariants
// ===========================================================================
//
// The golden script pins the C++-observable behaviour on fixed corpora;
// these properties pin the port's invariants on arbitrary input: the two
// pixel decoders agree on every well-formed DXT1/DXT5 block, the level
// layout is the prefix-sum walk the constructor builds, the packed-colour
// helpers round-trip, the zero-shift recolour stays within one quantum of
// its input, and parsing arbitrary bytes never panics.

mod dds {
    use proptest::prelude::*;

    use rzh_formats::dds::{
        DdsFile, HsvShift, Ww3dFormat, argb8888_to_rgb565, calculate_dxtc_surface_size,
        munged_name, recolor, rgb565_to_argb8888,
    };

    /// The formula bytes the differential corpus uses (identical on both
    /// sides of the harness).
    fn formula_at(offset: usize, n: usize) -> Vec<u8> {
        (0..n).map(|i| ((offset + i) * 11 + 7) as u8).collect()
    }

    /// A well-formed corpus: a 124-byte description plus a mip chain
    /// sized by the constructor's own walk (divide by four above one
    /// block), filled with the supplied bytes.
    fn build_corpus(
        width: u32,
        height: u32,
        format: Ww3dFormat,
        mips: u32,
        fill: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        let fourcc: u32 = match format {
            Ww3dFormat::Dxt1 => 0x3154_5844,
            Ww3dFormat::Dxt5 => 0x3554_5844,
            _ => 0x3154_5844,
        };
        bytes.extend_from_slice(b"DDS ");
        let mut put = |x: u32| bytes.extend_from_slice(&x.to_le_bytes());
        put(124); // Size
        put(0x1007); // Flags
        put(height);
        put(width);
        put(0); // Pitch
        put(0); // Depth
        put(mips);
        put(0); // AlphaBitDepth
        put(0); // Reserved
        put(0); // Surface
        for _ in 0..8 {
            put(0); // the four color keys
        }
        put(32); // PixelFormat.Size
        put(4); // PixelFormat.Flags
        put(fourcc);
        for _ in 0..5 {
            put(0); // the pixel-format union tail
        }
        put(0x1000); // Caps
        put(0); // Caps2
        put(0); // Caps3
        put(0); // Caps4
        put(0); // TextureStage
        debug_assert_eq!(bytes.len(), 128);

        // The constructor's size walk, mirrored so the chain and the
        // layout agree.
        let mut size = calculate_dxtc_surface_size(width, height, format);
        let mut at = 0usize;
        for _ in 0..mips {
            let chunk = formula_at(128 + at, size as usize);
            bytes.extend_from_slice(&chunk);
            at += size as usize;
            if size > 16 {
                size /= 4;
            }
        }
        // splice in the arbitrary fill over the first level (the pixel
        // queries only read what the fill covers)
        let level0 = calculate_dxtc_surface_size(width, height, format) as usize;
        let n = fill.len().min(level0).min(bytes.len() - 128);
        for (i, b) in fill.iter().take(n).enumerate() {
            bytes[128 + i] = *b;
        }
        bytes
    }

    fn dxt_format() -> impl Strategy<Value = Ww3dFormat> {
        prop::sample::select(vec![Ww3dFormat::Dxt1, Ww3dFormat::Dxt5])
    }

    /// Dimensions of sixteen texels and up: the constructor's
    /// level-size walk divides the area by four per level while the
    /// query grid floors each halved dimension to blocks (clamping
    /// sub-four dimensions up) — the two stay reconcilable only while
    /// the walk has not out-run the grid. On the small or odd shapes
    /// (an eight-texel side, a twelve) the original's queries read
    /// past the level storage into the heap; the port reports
    /// DataTooShort (the module docs' contract), so the agreement
    /// property sticks to the chains whose queries stay in-bounds.
    fn dims() -> impl Strategy<Value = (u32, u32)> {
        (
            prop::sample::select(vec![16u32, 32, 64]),
            prop::sample::select(vec![16u32, 32, 64]),
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// The two pixel decoders must agree: the block decompressor's
        /// A8R8G8B8 word at (x, y) is `get_pixel` at (x, y) — on every
        /// well-formed DXT1/DXT5 corpus, on every addressable level.
        #[test]
        fn pixel_queries_agree_with_block_decode(
            (width, height) in dims(),
            format in dxt_format(),
            mips in 1u32..7,
            fill in prop::collection::vec(prop::num::u8::ANY, 0..256),
        ) {
            let bytes = build_corpus(width, height, format, mips, &fill);
            let mut file = DdsFile::open("corpus.dds", 0, &bytes).expect("well-formed open");
            file.load(&bytes).expect("well-formed load");

            for level in 0..file.mip_level_count() {
                let w = file.width(level);
                let h = file.height(level);
                for y in (0..h).step_by(4) {
                    for x in (0..w).step_by(4) {
                        let bpp = Ww3dFormat::A8R8G8B8.bytes_per_pixel() as usize;
                        let mut block = vec![0xccu8; 4 * 4 * bpp + 4];
                        let alpha = file
                            .get_4x4_block(&mut block, 4 * bpp, Ww3dFormat::A8R8G8B8, level, x, y, HsvShift::none())
                            .expect("in-range block");
                        // The DXT1 4-colour mode's code 3 is transparent
                        // black — its blocks always "contain alpha".
                        if format == Ww3dFormat::Dxt1 {
                            let contains = block
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .any(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]) == 0);
                            prop_assert_eq!(alpha, contains);
                        }
                        for dy in 0..4u32.min(h - y) {
                            for dx in 0..4u32.min(w - x) {
                                let px = file.get_pixel(level, x + dx, y + dy).expect("pixel");
                                let at = (dy as usize * 4 + dx as usize) * 4;
                                let word = u32::from_le_bytes([
                                    block[at],
                                    block[at + 1],
                                    block[at + 2],
                                    block[at + 3],
                                ]);
                                prop_assert_eq!(px, word, "pixel ({},{} of level {}", x + dx, y + dy, level);
                            }
                        }
                    }
                }
            }
        }

        /// The level layout is the constructor's prefix-sum walk, and a
        /// well-formed chain never overruns the read window.
        #[test]
        fn level_layout_is_the_prefix_sum_walk(
            (width, height) in dims(),
            format in dxt_format(),
            mips in 1u32..7,
            fill in prop::collection::vec(prop::num::u8::ANY, 0..64),
        ) {
            let bytes = build_corpus(width, height, format, mips, &fill);
            let mut file = DdsFile::open("corpus.dds", 0, &bytes).expect("open");
            file.load(&bytes).expect("load");

            let mut expect_offset = 0u32;
            let mut expect_size = calculate_dxtc_surface_size(width, height, format);
            for level in 0..file.mip_level_count() {
                prop_assert_eq!(file.level_size(level), Ok(expect_size));
                prop_assert_eq!(file.level_offset(level), Some(expect_offset));
                expect_offset += expect_size;
                if expect_size > 16 {
                    expect_size /= 4;
                }
            }
            prop_assert!((expect_offset as usize) <= file.data().len());
        }

        /// Parsing arbitrary bytes never panics (the transcription's
        /// release no-op asserts become errors, not crashes).
        #[test]
        fn arbitrary_bytes_never_panic(
            bytes in prop::collection::vec(prop::num::u8::ANY, 0..600),
            reduction in 0u32..8,
        ) {
            let mut opened = DdsFile::open("corpus.dds", reduction, &bytes);
            if let Ok(file) = opened.as_mut() {
                let _ = file.load(&bytes);
                if file.data().len() >= 16 {
                    let _ = file.get_pixel(0, 0, 0);
                    let mut block = vec![0u8; 72];
                    let _ = file.get_4x4_block(&mut block, 16, Ww3dFormat::A8R8G8B8, 0, 0, 0, HsvShift::none());
                    let _ = file.copy_level_to_surface(0, Ww3dFormat::A8R8G8B8, 8, 8, &mut vec![0u8; 260], 32, HsvShift::none());
                }
            }
        }

        /// The RGB565 conversion round-trips exactly (the 8-bit lanes
        /// carry the 5/6/5 fields in their high bits).
        #[test]
        fn rgb565_round_trips(rgb in prop::num::u16::ANY) {
            let argb = rgb565_to_argb8888(rgb);
            prop_assert_eq!(argb8888_to_rgb565(argb), rgb);
            // the lanes are the truncated fields
            prop_assert_eq!(argb & 0x0000_0007, 0);
            prop_assert_eq!(argb & 0x0000_0300, 0);
            prop_assert_eq!(argb & 0x00f8_0000, u32::from(rgb & 0xf800) << 8);
        }

        /// A zero recolour shift is a near-identity: the packed round
        /// trip moves no channel by more than one quantum (the Foley
        /// HSV round trip's float rounding).
        #[test]
        fn recolor_zero_shift_stays_within_one(color in prop::num::u32::ANY) {
            let out = recolor(color, HsvShift::none());
            for shift in [0, 8, 16, 24] {
                let a = (color >> shift) & 0xff;
                let b = (out >> shift) & 0xff;
                prop_assert!((a as i32 - b as i32).abs() <= 1, "channel {shift}: {a:x} -> {b:x}");
            }
        }

        /// The name munging overwrites the last three characters with
        /// "dds" (after the 255-character truncation), and refuses the
        /// sub-three-character shapes the original writes out of
        /// bounds.
        #[test]
        fn name_munging_replaces_the_tail(
            name in prop::collection::vec(b'a'..=b'z', 3..300),
        ) {
            let name = String::from_utf8(name).expect("ascii");
            let munged = munged_name(&name).expect("munge");
            let effective: Vec<u8> = name.bytes().take(255).collect();
            let expect: Vec<u8> = effective[..effective.len() - 3]
                .iter()
                .copied()
                .chain(*b"dds")
                .collect();
            prop_assert_eq!(munged.as_bytes(), &expect[..]);
        }
    }

    #[test]
    fn short_names_are_refused() {
        assert!(munged_name("ab").is_err());
        assert!(munged_name("").is_err());
        assert!(munged_name("a\0bc").is_err()); // the NUL makes it length 1
        assert_eq!(munged_name("abc").unwrap(), "dds");
        assert_eq!(munged_name("tank").unwrap(), "tdds");
    }
}
