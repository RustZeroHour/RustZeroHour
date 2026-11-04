// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the W3D hierarchy tree port (P0).
//!
//! `tests/golden/htree.txt` is a script produced by
//! `tools/differential/gen_htree_golden.cpp` — a verbatim transcription
//! of the Zero Hour `HTreeClass::Load_W3D` data path
//! (`Core/Libraries/Source/WWVegas/WW3D2/htree.cpp` at the baseline
//! commit, plus the WWMath quaternion/translation/multiply formulas its
//! `read_pivots` drives and the PivotClass state the allocation leaves).
//!
//! The corpus bytes are the payload of the `W3D_CHUNK_HIERARCHY`
//! container the asset manager opens before calling the loader — the
//! same entry the Rust `W3dHtree::parse` models. The script pins:
//!
//! - the load verdicts (`ok`/`err`) — including the *stated-contract*
//!   deviations both sides share (the out-of-range parent index, the
//!   pivot-count conversions) and the contrast with the mesh loader
//!   (a short pivots chunk fails the whole load here);
//! - the decoded header — version, name, the declared count vs the
//!   pre-3.0-bumped member count, the centre;
//! - every loaded pivot — name (C-string view), resolved parent
//!   (index or `-1` root), the `Index` field, visibility, and the 12
//!   base-transform words with the original's mixed-precision
//!   `Build_Matrix3D`/`Translate`/`postMul` bit-for-bit;
//! - the query surface the skinning path drives — every
//!   `Get_Bone_Index` probe (each pivot's own name, `RootTransform`,
//!   the lowercased pivot-1 name, the `nomatch` fallback) and every
//!   `Get_Parent_Index`.
//!
//! The test replays the script against the Rust port and asserts
//! line-by-line equality.

use std::path::PathBuf;

use rzh_formats::w3d_htree::{
    HIERARCHY_HEADER_SIZE, NO_PARENT, PIVOT_SIZE, ROOT_TRANSFORM_NAME, W3dHtree,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("htree.txt")
}

fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    assert_eq!(text.len() % 2, 0, "odd hex length in golden: {text:?}");
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks(2) {
        let hi = hex_val(pair[0]);
        let lo = hex_val(pair[1]);
        out.push(hi << 4 | lo);
    }
    out
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("non-hex digit in golden: {:?}", c as char),
    }
}

fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn hex_f32s(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{:08x}", v.to_bits()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The base transform dumps as one contiguous hex word (the generator's
/// per-pivot `base` string).
fn hex_base(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{:08x}", v.to_bits()))
        .collect::<Vec<_>>()
        .join("")
}

fn hex_u32(value: u32) -> String {
    format!("{value:08x}")
}

/// Names: `-` when empty, the raw bytes when printable non-space ASCII,
/// `<hex>` otherwise (the generator's `q`).
fn q(name: &[u8]) -> String {
    if name.is_empty() {
        return "-".to_string();
    }
    if name.iter().all(|&b| (0x21..=0x7e).contains(&b)) {
        String::from_utf8_lossy(name).into_owned()
    } else {
        format!("<{}>", hex(name))
    }
}

/// One expected line of the dump, produced from the loaded tree.
fn dump_tree(tree: &W3dHtree, id: &str) -> Vec<String> {
    let mut out = Vec::new();

    out.push(format!(
        "hdr {id} {} {} {} {} {}",
        hex_u32(tree.header.version),
        q(tree.header.name_str()),
        tree.header.num_pivots,
        tree.pivots.len(),
        hex_f32s(&tree.header.center),
    ));

    for (i, pivot) in tree.pivots.iter().enumerate() {
        out.push(format!(
            "piv {id} {i} {} {} {} {} {}",
            q(pivot.name_str()),
            match pivot.parent {
                None => -1,
                Some(p) => p as i32,
            },
            pivot.index,
            if pivot.is_visible { 1 } else { 0 },
            hex_base(&pivot.base_transform),
        ));
    }

    // Get_Bone_Index probes: every pivot's own name, "RootTransform",
    // the ASCII lowercase of pivot 1's name when the tree has one, and
    // the fixed miss — the generator's deterministic probe list.
    for pivot in &tree.pivots {
        let probe = pivot.name_str().to_vec();
        out.push(format!(
            "getbone {id} {} {}",
            q(&probe),
            tree.get_bone_index(&probe)
        ));
    }
    out.push(format!(
        "getbone {id} {} {}",
        q(b"RootTransform"),
        tree.get_bone_index(b"RootTransform")
    ));
    // The lowercase probe is skipped when pivot 1's name fills its whole
    // field: the original's stricmp would compare past the 16 bytes into
    // the Parent pointer — a heap-layout result that is not reproducible
    // (both sides skip it; the generator documents the same rule).
    if tree.pivots.len() > 1 && tree.pivots[1].name_str().len() < 16 {
        let probe: Vec<u8> = tree.pivots[1]
            .name_str()
            .iter()
            .map(|&b| b.to_ascii_lowercase())
            .collect();
        out.push(format!(
            "getbone {id} {} {}",
            q(&probe),
            tree.get_bone_index(&probe)
        ));
    }
    out.push(format!(
        "getbone {id} {} {}",
        q(b"nomatch"),
        tree.get_bone_index(b"nomatch")
    ));

    for i in 0..tree.pivots.len() {
        out.push(format!("getparent {id} {i} {}", tree.get_parent_index(i)));
    }

    out
}

#[test]
fn golden_htree() {
    let script = std::fs::read_to_string(golden_path())
        .expect("the htree golden script exists (run tools/differential/regenerate.sh)");
    let mut lines = script.lines().peekable();
    // the header comment
    assert_eq!(
        lines.next(),
        Some("# gen_htree_golden output — the W3D hierarchy tree corpus (P0)")
    );
    let _ = lines.next();

    let mut corpora = 0;
    while let Some(line) = lines.next() {
        let rest = line
            .strip_prefix("file ")
            .unwrap_or_else(|| panic!("expected a file line, got: {line}"));
        let (id, hexbytes) = rest.split_once(' ').expect("file <id> <hex>");
        let bytes = unhex(hexbytes);
        corpora += 1;

        let verdict = lines
            .next()
            .unwrap_or_else(|| panic!("missing verdict for {id}"));
        // the dump lines run until the next file line
        let mut expected: Vec<String> = Vec::new();
        while let Some(peek) = lines.peek() {
            if peek.starts_with("file ") {
                break;
            }
            expected.push(lines.next().unwrap().to_string());
        }

        match W3dHtree::parse(&bytes) {
            Ok(tree) => {
                assert_eq!(verdict, format!("ok {id}"), "corpus {id} should load");
                let actual = dump_tree(&tree, id);
                assert_eq!(
                    expected.len(),
                    actual.len(),
                    "corpus {id}: dump line count differs (the golden has {}, the port produced {})",
                    expected.len(),
                    actual.len()
                );
                for (want, got) in expected.iter().zip(&actual) {
                    assert_eq!(want, got, "corpus {id}: dump mismatch");
                }
            }
            Err(e) => {
                assert_eq!(
                    verdict,
                    format!("err {id}"),
                    "corpus {id} should fail (got {e:?})"
                );
                assert!(expected.is_empty(), "corpus {id}: no dump after an error");
            }
        }
    }
    assert!(corpora >= 36, "expected the full corpus set, got {corpora}");
}

/// The corpus set is self-describing enough to cross-check the framing
/// invariants the loader depends on: every `ok` corpus starts with the
/// 36-byte header chunk, and every pivot dump line's parent resolves
/// inside the tree (the stated-contract rule the `err` corpora pin from
/// the other side).
#[test]
fn corpus_framing_invariants_hold() {
    let script = std::fs::read_to_string(golden_path()).expect("the htree golden script exists");
    let mut lines = script.lines().peekable();
    let _ = lines.next();
    let _ = lines.next();

    let mut checked = 0;
    while let Some(line) = lines.next() {
        let rest = line.strip_prefix("file ").expect("a file line");
        let (id, hexbytes) = rest.split_once(' ').expect("file <id> <hex>");
        let bytes = unhex(hexbytes);

        let verdict = lines.next().expect("a verdict line");
        let mut dump: Vec<String> = Vec::new();
        while lines.peek().is_some_and(|peek| !peek.starts_with("file ")) {
            dump.push(lines.next().unwrap().to_string());
        }

        if verdict == format!("ok {id}") {
            // the header chunk: id 0x101, size at least 36 (h14 pads its
            // header chunk with 4 trailing junk bytes — Close_Chunk seeks
            // past them)
            assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0x101);
            assert!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) >= HIERARCHY_HEADER_SIZE,
                "corpus {id}: header chunk smaller than the 36-byte struct"
            );
            // every piv line's parent resolves, or is the root sentinel
            let count = dump.iter().filter(|l| l.starts_with("piv ")).count();
            for line in dump.iter().filter(|l| l.starts_with("piv ")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let parent: i64 = parts[4].parse().unwrap();
                assert!(
                    parent == -1 || (0..count as i64).contains(&parent),
                    "corpus {id}: pivot parent {parent} outside the tree"
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 25,
        "expected a meaningful ok corpus, got {checked}"
    );
    // the constants the loader states match the format
    assert_eq!(PIVOT_SIZE, 60);
    assert_eq!(NO_PARENT, 0xFFFF_FFFF);
    assert_eq!(ROOT_TRANSFORM_NAME, b"RootTransform");
}
