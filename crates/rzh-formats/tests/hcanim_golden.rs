// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the W3D compressed animation port (P0).
//!
//! `tests/golden/hcanim.txt` is a script produced by
//! `tools/differential/gen_hcanim_golden.cpp` — a verbatim transcription
//! of the Zero Hour `HCompressedAnimClass::Load_W3D` data path
//! (`GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hcanim.cpp` at the
//! baseline commit, plus the time-coded, time-coded-bit and
//! adaptive-delta channel readers from `motchan.cpp`, the adaptive-delta
//! filter table, the `WWMath` fast tables and the deterministic GameMath
//! entry points the frame queries drive).
//!
//! The corpus bytes are the payload of the
//! `W3D_CHUNK_COMPRESSED_ANIMATION` container the asset manager opens
//! before calling `HAnimManagerClass::Load_Compressed_Anim` — the same
//! entry the Rust `W3dCompressedAnim::parse` models, with the
//! caller-side tree resolution the parse callback stands in for. The
//! script's `tree` lines are the manager's table (lowercase keys,
//! exactly what `HTreeManagerClass::Get_Tree` hashes); the test rebuilds
//! the same closure. The script pins:
//!
//! - the load verdicts (`ok`/`err`) — including the *stated-contract*
//!   deviations both sides share (the zero-time-code bit channel's
//!   wrapped read, the mid-chunk file truncation, the channel chunk
//!   smaller than the adaptive-delta struct) and the strcpy quirks:
//!   the hierarchy-name run through the NumFrames word (c24's 17-byte
//!   lookup key), the unterminated animation name (c25), the
//!   31-character truncation (c26) and the empty-name trailing dot with
//!   the -inf total time of the negative-as-int frame count (c27);
//! - the decoded header — version, the composed name, the hierarchy
//!   name (the 16-byte view the generator dumps), the flavor, the frame
//!   count as the class's `int`, the **numerically** converted frame
//!   rate (c28's 0xFFFF) and the total time;
//! - the channel routing — every `Has_*` probe,
//!   `Is_Node_Motion_Present`, and every stored channel's full state
//!   (type, vector length, the declared counts, the complete word
//!   arrays), the discard of out-of-range or unroutable channels
//!   (c12's TIMECODED_X..Q type values, c14's and c39's missing-bone
//!   channels, c21's unknown flavor reading no channels at all), the
//!   newest-wins duplicate (c13) and the cross-shape flavor reads
//!   (c23's/c38's reader-follows-flavor payloads);
//! - the frame queries at the deterministic probe list — the
//!   time-coded branches (the binary-movement verbatim return, the
//!   last-packet clamp, the scan look-ahead, the binary search and its
//!   rewind, the numeric time-code rounding at 2^24 in c22, the
//!   `VectorLen`-2 X channel's multi-word writes in c07), the
//!   adaptive-delta decompression (the block walks over both
//!   filter-table halves in c33, the sign-extended nybbles in c34, the
//!   getframe clamp, the sliding window, and the far-jump
//!   cache-continue path with the original's off-by-one in c29), the
//!   orientations/transforms through the table-based `Fast_Slerp` and
//!   `Build_Matrix3D`, and the bit channel's scan with its dead
//!   `DefaultVal` and negative-frame clamp (c41).
//!
//! The motion probes are non-negative only (the `(uint32)` frame casts
//! are undefined for negatives and differ between x86-64 and ARM); the
//! visibility probes add two negative frames (the `(int)` cast is
//! well-defined there). The orientation/transform probes are omitted
//! for the unknown-flavor corpus (c21: the original leaves the
//! caller's destinations untouched — not a function of the animation;
//! the port returns the identity there).
//!
//! The test replays the script against the Rust port and asserts
//! line-by-line equality.

use std::path::PathBuf;

use rzh_formats::w3d_hcanim::{
    ANIM_FLAVOR_ADAPTIVE_DELTA, ANIM_FLAVOR_TIMECODED, W3dCompressedAnim, W3dCompressedChannel,
};
use rzh_formats::w3d_htree::{W3dHierarchyHeader, W3dHtree, W3dPivot};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("hcanim.txt")
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

fn hex_f32(value: f32) -> String {
    format!("{:08x}", value.to_bits())
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
        format!(
            "<{}>",
            name.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    }
}

/// A parse-time tree stand-in: the format layer only reads the pivot
/// count, exactly like `Load_W3D` reads `base_pose->Num_Pivots()`.
fn tree(pivots: usize) -> W3dHtree {
    W3dHtree {
        header: W3dHierarchyHeader {
            version: 0,
            name: [0; 16],
            num_pivots: pivots as u32,
            center: [0.0; 3],
        },
        pivots: vec![W3dPivot::default(); pivots],
    }
}

/// The frame probe list (the generator's `probe_frames`): a fixed set
/// plus the count-derived frames when the count is sane. The motion
/// probes are non-negative only — the `(uint32)` frame casts are
/// undefined for negatives and differ between x86-64 and ARM.
fn probe_frames(num_frames: u32) -> Vec<f32> {
    let mut f = vec![0.0, 0.25, 0.5, 1.5, 2.25, 3.5, 7.75];
    let nf = num_frames as i32;
    if (1..=1000).contains(&nf) {
        f.push((nf - 1) as f32);
        f.push(nf as f32);
    } else if (1000..(1 << 25)).contains(&nf) {
        f.push((nf - 15) as f32);
        f.push((nf - 11) as f32);
        f.push((nf - 8) as f32);
        f.push((nf - 1) as f32);
        f.push(nf as f32);
    }
    f
}

/// The visibility probes add the negative frames (Get_Bit's `(int)`
/// cast is well-defined there).
const VIS_EXTRA_FRAMES: [f32; 2] = [-1.0, -0.5];

/// The pivots the queries run for (the generator's `probe_pivots`).
fn probe_pivots(num_nodes: usize) -> Vec<usize> {
    let mut p: Vec<usize> = (0..num_nodes.min(8)).collect();
    if num_nodes > 8 {
        p.push(num_nodes - 1);
    }
    p
}

/// One expected line of the dump, produced from the loaded animation —
/// the exact mirror of the generator's `dump_anim` (same line formats,
/// same probe lists, same unknown-flavor skip rule).
fn dump_anim(anim: &mut W3dCompressedAnim, id: &str) -> Vec<String> {
    let mut out = Vec::new();

    // the hname view: the generator dumps the member capped at the
    // 16-byte field (the C string continues past it for the
    // unterminated corpora; the extension is pinned by the verdicts)
    let hname = anim.hierarchy_name_str();
    let hname_view = &hname[..hname.len().min(16)];
    out.push(format!(
        "hdr {id} {} {} {} {} {} {} {} {}",
        hex_u32(anim.header.version),
        q(anim.name_str()),
        q(hname_view),
        anim.flavor(),
        anim.num_frames() as i32,
        hex_f32(anim.frame_rate()),
        hex_f32(anim.total_time()),
        anim.num_pivots(),
    ));

    // the Has_* probes and Is_Node_Motion_Present
    for i in 0..anim.num_pivots() {
        out.push(format!(
            "has {id} {i} {} {} {} {} {} {}",
            i32::from(anim.has_x_translation(i)),
            i32::from(anim.has_y_translation(i)),
            i32::from(anim.has_z_translation(i)),
            i32::from(anim.has_rotation(i)),
            i32::from(anim.has_visibility(i)),
            i32::from(anim.is_node_motion_present(i)),
        ));
    }

    // the stored channels (slot order: x y z q, then vis)
    for (i, node) in anim.node_motion.iter().enumerate() {
        for (slot, channel) in [
            ("x", node.x.as_ref()),
            ("y", node.y.as_ref()),
            ("z", node.z.as_ref()),
            ("q", node.q.as_ref()),
        ] {
            match channel {
                Some(W3dCompressedChannel::TimeCoded(c)) => {
                    let words: Vec<String> = c.data.iter().map(|v| hex_u32(*v)).collect();
                    out.push(format!(
                        "tcchan {id} {i} {slot} {} {} {} {}",
                        c.channel_type,
                        c.vector_len,
                        c.num_time_codes,
                        words.join(" "),
                    ));
                }
                Some(W3dCompressedChannel::AdaptiveDelta(c)) => {
                    let words: Vec<String> = c.data.iter().map(|v| hex_u32(*v)).collect();
                    out.push(format!(
                        "adchan {id} {i} {slot} {} {} {} {} {}",
                        c.channel_type,
                        c.vector_len,
                        c.num_frames,
                        hex_f32(c.scale),
                        words.join(" "),
                    ));
                }
                None => {}
            }
        }
        if let Some(channel) = node.vis.as_ref() {
            let words: Vec<String> = channel.bits.iter().map(|v| hex_u32(*v)).collect();
            out.push(format!(
                "tcbchan {id} {i} {} {} {} {}",
                channel.channel_type,
                channel.default_val,
                channel.num_time_codes,
                words.join(" "),
            ));
        }
    }

    // the frame queries: the orientation/transform probes are omitted
    // for unknown flavors (the original leaves the caller's
    // destinations untouched — not a function of the animation; the
    // port returns the identity there)
    let queryable = matches!(
        anim.flavor(),
        ANIM_FLAVOR_TIMECODED | ANIM_FLAVOR_ADAPTIVE_DELTA
    );
    for piv in probe_pivots(anim.num_pivots()) {
        for frame in probe_frames(anim.num_frames()) {
            let t = anim.get_translation(piv, frame);
            out.push(format!(
                "trans {id} {piv} {} {} {} {}",
                hex_f32(frame),
                hex_f32(t[0]),
                hex_f32(t[1]),
                hex_f32(t[2]),
            ));

            if queryable {
                let quat = anim.get_orientation(piv, frame);
                out.push(format!(
                    "orient {id} {piv} {} {} {} {} {}",
                    hex_f32(frame),
                    hex_f32(quat[0]),
                    hex_f32(quat[1]),
                    hex_f32(quat[2]),
                    hex_f32(quat[3]),
                ));

                let m = anim.get_transform(piv, frame);
                let words: Vec<String> = m.iter().map(|v| hex_f32(*v)).collect();
                out.push(format!(
                    "xform {id} {piv} {} {}",
                    hex_f32(frame),
                    words.join(" "),
                ));
            }

            out.push(format!(
                "vis {id} {piv} {} {}",
                hex_f32(frame),
                i32::from(anim.get_visibility(piv, frame)),
            ));
        }
        for frame in VIS_EXTRA_FRAMES {
            out.push(format!(
                "vis {id} {piv} {} {}",
                hex_f32(frame),
                i32::from(anim.get_visibility(piv, frame)),
            ));
        }
    }

    out
}

#[test]
fn golden_hcanim() {
    let script = std::fs::read_to_string(golden_path())
        .expect("the hcanim golden script exists (run tools/differential/regenerate.sh)");
    let mut lines = script.lines().peekable();
    assert_eq!(
        lines.next(),
        Some("# gen_hcanim_golden output — the W3D compressed animation corpus (P0)")
    );
    let _ = lines.next();

    // the manager's tree table (lowercase keys)
    let mut trees: Vec<(Vec<u8>, W3dHtree)> = Vec::new();
    while let Some(peek) = lines.peek() {
        let Some(rest) = peek.strip_prefix("tree ") else {
            break;
        };
        let (name, count) = rest.rsplit_once(' ').expect("tree <name> <count>");
        let name_bytes = if name == "-" {
            Vec::new()
        } else if let Some(hexpart) = name.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
            unhex(hexpart)
        } else {
            name.as_bytes().to_vec()
        };
        trees.push((name_bytes, tree(count.parse().expect("tree count"))));
        lines.next();
    }
    assert!(trees.len() >= 6, "the tree table loaded");

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
        let mut expected: Vec<String> = Vec::new();
        while lines.peek().is_some_and(|peek| !peek.starts_with("file ")) {
            expected.push(lines.next().unwrap().to_string());
        }

        // the caller-side Get_HTree: lowercase exact match (the parse
        // callback owns it in the port; the generator models the manager)
        let result = W3dCompressedAnim::parse(&bytes, |name| {
            trees
                .iter()
                .find(|(n, _)| *n == name.to_ascii_lowercase())
                .map(|(_, t)| t)
        });
        match result {
            Ok(mut anim) => {
                assert_eq!(verdict, format!("ok {id}"), "corpus {id} should load");
                let actual = dump_anim(&mut anim, id);
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
    assert!(corpora >= 43, "expected the full corpus set, got {corpora}");
}

/// The corpus set is self-describing enough to cross-check the framing
/// invariants: every `ok` corpus starts with the 44-byte header chunk,
/// every dumped channel resolves inside the animation's pivot count
/// (the discard rule the `err` corpora pin from the other side), and
/// every flavor-bearing header declares one of the two known flavors or
/// the corpus is the unknown-flavor one whose motion probes are absent.
#[test]
fn corpus_framing_invariants_hold() {
    let script = std::fs::read_to_string(golden_path()).expect("the hcanim golden script exists");
    let mut lines = script.lines().peekable();
    let _ = lines.next();
    let _ = lines.next();
    while lines.peek().is_some_and(|peek| peek.starts_with("tree ")) {
        lines.next();
    }

    let mut checked = 0;
    let mut unknown_flavor = 0;
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
            // the header chunk: id 0x281, size at least 44
            assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0x281);
            assert!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) >= 44,
                "corpus {id}: header chunk smaller than the 44-byte struct"
            );
            // every tcchan/adchan/tcbchan line's pivot resolves inside the tree
            let hdr = dump
                .iter()
                .find_map(|l| l.strip_prefix(&format!("hdr {id} ")))
                .expect("the hdr line");
            // hdr <id> <ver> <name> <hname> <flavor> <numframes> <frerate>
            // <totaltime> <numnodes> — the name fields are <hex>-quoted or
            // printable-non-space, so whitespace splitting is safe
            let parts: Vec<&str> = hdr.split_whitespace().collect();
            let numnodes: usize = parts[7].parse().expect("the hdr line's pivot count");
            let flavor: u16 = parts[3].parse().expect("the hdr line's flavor");
            if flavor != ANIM_FLAVOR_TIMECODED && flavor != ANIM_FLAVOR_ADAPTIVE_DELTA {
                // the unknown-flavor corpus: no motion channels, no
                // orient/xform probes
                unknown_flavor += 1;
                assert!(
                    dump.iter()
                        .all(|l| !l.starts_with("orient ") && !l.starts_with("xform ")),
                    "corpus {id}: the unknown flavor must omit the orientation/transform probes"
                );
            }
            for line in dump.iter().filter(|l| {
                l.starts_with("tcchan ") || l.starts_with("adchan ") || l.starts_with("tcbchan ")
            }) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let piv: usize = parts[2].parse().unwrap();
                assert!(
                    piv < numnodes,
                    "corpus {id}: channel pivot {piv} outside the tree"
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 35,
        "expected a meaningful ok corpus, got {checked}"
    );
    assert_eq!(unknown_flavor, 1, "exactly one unknown-flavor corpus");
}
