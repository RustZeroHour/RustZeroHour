// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the W3D raw animation port (P0).
//!
//! `tests/golden/hanim.txt` is a script produced by
//! `tools/differential/gen_hanim_golden.cpp` — a verbatim transcription
//! of the Zero Hour `HRawAnimClass::Load_W3D` data path
//! (`GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hrawanim.cpp` at the
//! baseline commit, plus the classic motion/bit channel readers from
//! `motchan.cpp`, the `WWMath` fast tables and the deterministic GameMath
//! entry points the frame queries drive).
//!
//! The corpus bytes are the payload of the `W3D_CHUNK_ANIMATION`
//! container the asset manager opens before calling the loader — the
//! same entry the Rust `W3dRawAnim::parse` models, with the caller-side
//! tree resolution the parse callback stands in for. The script's
//! `tree` lines are the manager's table (lowercase keys, exactly what
//! `HTreeManagerClass::Get_Tree` hashes); the test rebuilds the same
//! closure. The script pins:
//!
//! - the load verdicts (`ok`/`err`) — including the *stated-contract*
//!   deviations both sides share (the empty/negative channel spans, the
//!   zero `VectorLen`) and the strcpy quirks: the hierarchy-name run
//!   through the frame-count word (a24's 17-byte lookup key, a25's
//!   key miss against the 16 field bytes) and the `strlcat` composition
//!   (a26's unterminated animation name, a27's 31-character
//!   truncation, a28's trailing dot);
//! - the decoded header — version, the composed name, the hierarchy
//!   name (the 16-byte view the generator dumps), the frame count as
//!   the class's `int` (a23's negative wrap), the **numerically**
//!   converted frame rate (a19's raw bits of `50.0f` loading as
//!   `1112014848.0`, a20's division by zero, a21's rounding) and the
//!   total time;
//! - the channel routing — every `Has_*` probe,
//!   `Is_Node_Motion_Present`, and every stored channel's full state
//!   (type, frame range, vector length, the complete float data), the
//!   pre-3.0 pivot shift (a07/a08), the discard of out-of-range or
//!   unknown-typed channels (a09/a10), the newest-wins duplicate
//!   (a11) and the exporter-junk seek (a12);
//! - the frame queries at the deterministic probe list — translations
//!   through `Vector3::Lerp` (the identity blending below a channel's
//!   first frame, the `frame1` wrap past the end), orientations and
//!   transforms through the table-based `Fast_Slerp` (the linear
//!   near-parallel branch, the hemisphere flip, the `gm_acosf`
//!   fallback, `Build_Matrix3D`'s mixed-precision rotation), and
//!   visibility bits with their `(int)` frame truncation.
//!
//! The test replays the script against the Rust port and asserts
//! line-by-line equality.

use std::path::PathBuf;

use rzh_formats::w3d_hanim::W3dRawAnim;
use rzh_formats::w3d_htree::{W3dHierarchyHeader, W3dHtree, W3dPivot};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("hanim.txt")
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
        format!("<{}>", hex(name))
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
/// plus the count-derived frames when the count is sane.
fn probe_frames(num_frames: u32) -> Vec<f32> {
    let mut f = vec![-1.0, -0.5, 0.0, 0.5, 1.5, 2.25, 3.5];
    let nf = num_frames as i32;
    if (1..=1000).contains(&nf) {
        f.push((nf - 1) as f32);
        f.push(nf as f32);
    }
    f
}

/// The pivots the queries run for (the generator's `probe_pivots`).
fn probe_pivots(num_nodes: usize) -> Vec<usize> {
    let mut p: Vec<usize> = (0..num_nodes.min(8)).collect();
    if num_nodes > 8 {
        p.push(num_nodes - 1);
    }
    p
}

/// One expected line of the dump, produced from the loaded animation.
fn dump_anim(anim: &W3dRawAnim, id: &str) -> Vec<String> {
    let mut out = Vec::new();

    // the hname view: the generator dumps the member capped at the
    // 16-byte field (the C string continues past it for the
    // unterminated corpora; the extension is pinned by the verdicts)
    let hname = anim.hierarchy_name_str();
    let hname_view = &hname[..hname.len().min(16)];
    out.push(format!(
        "hdr {id} {} {} {} {} {} {} {}",
        hex_u32(anim.header.version),
        q(anim.name_str()),
        q(hname_view),
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

    // the stored channels (slot order: x y z xr yr zr q, then vis)
    for (i, node) in anim.node_motion.iter().enumerate() {
        for (slot, channel) in [
            ("x", node.x.as_ref()),
            ("y", node.y.as_ref()),
            ("z", node.z.as_ref()),
            ("xr", node.xr.as_ref()),
            ("yr", node.yr.as_ref()),
            ("zr", node.zr.as_ref()),
            ("q", node.q.as_ref()),
        ] {
            if let Some(channel) = channel {
                let words: Vec<String> = channel.data.iter().map(|v| hex_f32(*v)).collect();
                out.push(format!(
                    "chan {id} {i} {slot} {} {} {} {} {}",
                    channel.channel_type,
                    channel.first_frame,
                    channel.last_frame,
                    channel.vector_len,
                    words.join(" "),
                ));
            }
        }
        if let Some(channel) = node.vis.as_ref() {
            out.push(format!(
                "bchan {id} {i} {} {} {} {} {}",
                channel.channel_type,
                channel.first_frame,
                channel.last_frame,
                channel.default_val,
                hex(&channel.bits),
            ));
        }
    }

    // the frame queries
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
fn golden_hanim() {
    let script = std::fs::read_to_string(golden_path())
        .expect("the hanim golden script exists (run tools/differential/regenerate.sh)");
    let mut lines = script.lines().peekable();
    assert_eq!(
        lines.next(),
        Some("# gen_hanim_golden output — the W3D raw animation corpus (P0)")
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
    assert!(trees.len() >= 7, "the tree table loaded");

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
        let result = W3dRawAnim::parse(&bytes, |name| {
            trees
                .iter()
                .find(|(n, _)| *n == name.to_ascii_lowercase())
                .map(|(_, t)| t)
        });
        match result {
            Ok(anim) => {
                assert_eq!(verdict, format!("ok {id}"), "corpus {id} should load");
                let actual = dump_anim(&anim, id);
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
    assert!(corpora >= 38, "expected the full corpus set, got {corpora}");
}

/// The corpus set is self-describing enough to cross-check the framing
/// invariants: every `ok` corpus starts with the 44-byte header chunk,
/// and every dumped channel resolves inside the animation's pivot count
/// (the discard rule the `err` corpora pin from the other side).
#[test]
fn corpus_framing_invariants_hold() {
    let script = std::fs::read_to_string(golden_path()).expect("the hanim golden script exists");
    let mut lines = script.lines().peekable();
    let _ = lines.next();
    let _ = lines.next();
    while lines.peek().is_some_and(|peek| peek.starts_with("tree ")) {
        lines.next();
    }

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
            // the header chunk: id 0x201, size at least 44
            assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0x201);
            assert!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) >= 44,
                "corpus {id}: header chunk smaller than the 44-byte struct"
            );
            // every chan/bchan line's pivot resolves inside the tree
            let numnodes: usize = dump
                .iter()
                .find_map(|l| l.strip_prefix(&format!("hdr {id} ")))
                .and_then(|rest| rest.split_whitespace().nth(6))
                .and_then(|n| n.parse().ok())
                .expect("the hdr line's pivot count");
            for line in dump.iter().filter(|l| l.starts_with("chan ")) {
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
        checked >= 25,
        "expected a meaningful ok corpus, got {checked}"
    );
}
