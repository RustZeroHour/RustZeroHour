// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the W3D mesh chunk tree port (P0).
//!
//! `tests/golden/mesh.txt` is a script produced by
//! `tools/differential/gen_mesh_golden.cpp` — a verbatim transcription of
//! the Zero Hour mesh loader's data path (meshmdlio.cpp `Load_W3D` and
//! every sub-reader, the load-path half of meshmatdesc.cpp, the geometry
//! and AABTree readers, the vertex-material and texture data paths, the
//! ShaderClass bit packing and the WWLib CRC-32). Each corpus is one mesh
//! payload; the script records the corpus bytes, the load verdict, and a
//! full dump of the state the load leaves behind.
//!
//! The test replays every corpus through [`rzh_formats::w3d_mesh::W3dMesh`]
//! and re-dumps the state with the same grammar, asserting line-by-line
//! equality. The pinned quirks include: the discarded sub-reader error
//! (a failing reader aborts the walk, the load still succeeds), the
//! prelit selection fall-through, the alternate-material routing and
//! merge (including shader-only alternates counting as empty), the
//! CRC-based UV dedup, the DCG/DIG colour-array ownership and multiply,
//! the pre-3.0 bone-link fixup, the u16 TriIndex truncation, the
//! lowercased texture identity with duplicate list entries, the legacy
//! material dedup by CRC, and the pass/stage bounds standing for the
//! original's out-of-bounds reads.

use std::path::PathBuf;

use rzh_formats::w3d_mesh::{
    AabNode, MAX_COLOR_ARRAYS, MAX_PASSES, MAX_TEX_STAGES, MAX_UV_ARRAYS, MatDesc, W3dMesh,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mesh.txt")
}

fn unhex(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let nib = |b: u8| match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => panic!("bad hex digit in golden script"),
    };
    for pair in bytes.as_chunks::<2>().0 {
        out.push((nib(pair[0]) << 4) | nib(pair[1]));
    }
    out
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

fn hex_u32s(values: &[u32]) -> String {
    if values.is_empty() {
        return "-".to_string();
    }
    let mut s = String::new();
    for v in values {
        s.push_str(&hex(&v.to_le_bytes()));
    }
    s
}

fn hex_u16s(values: &[u16]) -> String {
    if values.is_empty() {
        return "-".to_string();
    }
    let mut s = String::new();
    for v in values {
        s.push_str(&hex(&v.to_le_bytes()));
    }
    s
}

fn hex_f32s(values: &[f32]) -> String {
    if values.is_empty() {
        return "-".to_string();
    }
    let mut s = String::new();
    for v in values {
        s.push_str(&hex(&v.to_bits().to_le_bytes()));
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
fn q(name: &str) -> String {
    if name.is_empty() {
        return "-".to_string();
    }
    if name.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        name.to_string()
    } else {
        format!("<{}>", hex(name.as_bytes()))
    }
}

/// One expected line of the dump, produced from the loaded mesh.
fn dump_mesh(mesh: &W3dMesh, id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let h = &mesh.header;

    let floats =
        |f: &[f32]| -> String { f.iter().map(|x| hex_f32(*x)).collect::<Vec<_>>().join(" ") };

    out.push(format!(
        "hdr {id} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        hex_u32(h.version),
        hex_u32(h.attributes),
        q(&String::from_utf8_lossy(h.mesh_name_str())),
        q(&String::from_utf8_lossy(h.container_name_str())),
        h.num_tris,
        h.num_vertices,
        h.num_materials,
        h.num_damage_stages,
        h.sort_level,
        h.prelit_version,
        h.future_counts[0],
        hex_u32(h.vertex_channels),
        hex_u32(h.face_channels),
        floats(&h.min),
        floats(&h.max),
        floats(&h.sph_center),
        hex_f32(h.sph_radius),
    ));
    out.push(format!("name {id} {}", q(&mesh.name)));
    out.push(format!("flags {id} {:04x}", mesh.flags));
    out.push(format!("prelitid {id} {}", hex_u32(mesh.prelit_chunk_id)));

    // geometry
    let dump_array3 = |opt: &Option<Vec<[f32; 3]>>| -> (usize, String) {
        match opt {
            Some(v) => {
                let flat: Vec<f32> = v.iter().flat_map(|p| p.iter().copied()).collect();
                (v.len(), hex_f32s(&flat))
            }
            None => (0, "-".to_string()),
        }
    };
    let (n, v) = dump_array3(&mesh.geometry.vertices);
    out.push(format!("geov {id} {n} {v}"));
    let (n, v) = dump_array3(&mesh.geometry.vertex_normals);
    out.push(format!("geon {id} {n} {v}"));
    match &mesh.geometry.polys {
        Some(polys) => {
            let flat: Vec<u16> = polys.iter().flat_map(|p| p.iter().copied()).collect();
            out.push(format!("geop {id} {} {}", polys.len(), hex_u16s(&flat)));
        }
        None => out.push(format!("geop {id} 0 -")),
    }
    match &mesh.geometry.surface_types {
        Some(s) => out.push(format!("geos {id} {} {}", s.len(), hex(s))),
        None => out.push(format!("geos {id} 0 -")),
    }
    match mesh.geometry.plane_equations.is_empty() {
        true => out.push(format!("geoe {id} 0 -")),
        false => {
            let flat: Vec<f32> = mesh
                .geometry
                .plane_equations
                .iter()
                .flat_map(|p| p.iter().copied())
                .collect();
            out.push(format!(
                "geoe {id} {} {}",
                mesh.geometry.plane_equations.len(),
                hex_f32s(&flat)
            ));
        }
    }
    out.push(format!(
        "geot {id} {}",
        match &mesh.geometry.user_text {
            Some(t) => hex(t),
            None => "-".to_string(),
        }
    ));
    match &mesh.geometry.bone_links {
        Some(b) => out.push(format!("geob {id} {} {}", b.len(), hex_u16s(b))),
        None => out.push(format!("geob {id} 0 -")),
    }
    match &mesh.geometry.shade_indices {
        Some(s) => out.push(format!("geox {id} {} {}", s.len(), hex_u32s(s))),
        None => out.push(format!("geox {id} 0 -")),
    }

    // the load context's material info
    let mi = &mesh.material_info;
    out.push(format!(
        "matinfo {id} {} {} {} {}",
        mi.pass_count, mi.vertex_material_count, mi.shader_count, mi.texture_count
    ));

    // shaders
    out.push(format!(
        "shaders {id} {} {}",
        mesh.shaders.len(),
        hex_u32s(&mesh.shaders)
    ));

    // vertex materials (the colour triples print as three numeric hex
    // words each — the generator's f32hex, unlike the array dumps' byte hex)
    let f32hex3 =
        |v: &[f32; 3]| -> String { v.iter().map(|f| hex_f32(*f)).collect::<Vec<_>>().join("") };
    for (i, vm) in mesh.vertex_materials.iter().enumerate() {
        out.push(format!(
            "vmat {id} {i} {} {} {} {} {} {} {} {} {} {}",
            q(&vm.name),
            hex_u32(vm.flags),
            f32hex3(&vm.ambient),
            f32hex3(&vm.diffuse),
            f32hex3(&vm.specular),
            f32hex3(&vm.emissive),
            hex_f32(vm.shininess),
            hex_f32(vm.opacity),
            if vm.mapper_args0.is_empty() {
                "-".to_string()
            } else {
                hex(&vm.mapper_args0)
            },
            if vm.mapper_args1.is_empty() {
                "-".to_string()
            } else {
                hex(&vm.mapper_args1)
            },
        ));
    }

    // textures
    for (i, t) in mesh.textures.iter().enumerate() {
        out.push(format!(
            "tex {id} {i} {} {}",
            q(&t.name),
            match &t.info {
                Some(info) => hex(&info.to_bytes()),
                None => "-".to_string(),
            }
        ));
    }

    // legacy materials
    out.push(format!("legacy {id} {}", mesh.legacy_materials.len()));
    for (i, m) in mesh.legacy_materials.iter().enumerate() {
        out.push(format!(
            "legacymat {id} {i} {} {} {}",
            m.shader_idx, m.vertex_material_idx, m.texture_idx
        ));
    }

    // material descriptions
    dump_matdesc(&mut out, id, "def", &mesh.materials);
    match &mesh.alternate_materials {
        Some(alt) => dump_matdesc(&mut out, id, "alt", alt),
        None => out.push(format!("matdesc {id} alt -")),
    }

    // the aabtree
    match &mesh.aabtree {
        Some(tree) => {
            let mut nodes = String::new();
            for AabNode {
                min,
                max,
                front_or_poly0,
                back_or_poly_count,
            } in &tree.nodes
            {
                for f in min.iter().chain(max.iter()) {
                    nodes.push_str(&hex(&f.to_bits().to_le_bytes()));
                }
                nodes.push_str(&hex(&front_or_poly0.to_le_bytes()));
                nodes.push_str(&hex(&back_or_poly_count.to_le_bytes()));
            }
            if nodes.is_empty() {
                nodes = "-".to_string();
            }
            out.push(format!(
                "aabtree {id} {} {} {} {}",
                tree.node_count,
                tree.poly_count,
                nodes,
                hex_u32s(&tree.poly_indices)
            ));
        }
        None => out.push(format!("aabtree {id} -")),
    }

    out
}

fn dump_matdesc(out: &mut Vec<String>, id: &str, which: &str, desc: &MatDesc) {
    out.push(format!("matdesc {id} {which} {}", desc.pass_count));
    for pass in 0..MAX_PASSES {
        // material
        let arr = match &desc.material_arrays[pass] {
            Some(v) => hex_u32s(&v.iter().map(|&r| r as u32).collect::<Vec<_>>()),
            None => "-".to_string(),
        };
        out.push(format!(
            "matpass {id} {which} {pass} mat {} {arr}",
            desc.material_single[pass]
        ));
        // shader
        let arr = match &desc.shader_arrays[pass] {
            Some(v) => hex_u32s(v),
            None => "-".to_string(),
        };
        out.push(format!(
            "matpass {id} {which} {pass} shd {} {arr}",
            hex_u32(desc.shader_single[pass])
        ));
        out.push(format!(
            "matpass {id} {which} {pass} dcg {} dig {}",
            desc.dcg_source[pass], desc.dig_source[pass]
        ));
        for stage in 0..MAX_TEX_STAGES {
            let arr = match &desc.texture_arrays[pass][stage] {
                Some(v) => hex_u32s(&v.iter().map(|&r| r as u32).collect::<Vec<_>>()),
                None => "-".to_string(),
            };
            out.push(format!(
                "matpass {id} {which} {pass} stg {stage} tex {} {arr}",
                desc.texture_single[pass][stage]
            ));
            out.push(format!(
                "matpass {id} {which} {pass} stg {stage} uv {}",
                desc.uv_source[pass][stage]
            ));
        }
    }
    for a in 0..MAX_COLOR_ARRAYS {
        match &desc.color_arrays[a] {
            Some(v) => out.push(format!(
                "matcolor {id} {which} {a} {} {}",
                v.len(),
                hex_u32s(v)
            )),
            None => out.push(format!("matcolor {id} {which} {a} 0 -")),
        }
    }
    for k in 0..MAX_UV_ARRAYS {
        match &desc.uv_arrays[k] {
            Some(uv) => {
                let flat: Vec<f32> = uv.xy.iter().flat_map(|p| p.iter().copied()).collect();
                out.push(format!(
                    "matuv {id} {which} {k} {} {}",
                    uv.xy.len(),
                    hex_f32s(&flat)
                ));
            }
            None => out.push(format!("matuv {id} {which} {k} 0 -")),
        }
    }
}

#[test]
fn golden_mesh() {
    let script = std::fs::read_to_string(golden_path())
        .expect("the mesh golden script exists (run tools/differential/regenerate.sh)");
    let mut lines = script.lines().peekable();
    // the header comment
    assert_eq!(
        lines.next(),
        Some("# gen_mesh_golden output — the W3D mesh chunk tree corpus (P0)")
    );
    let _ = lines.next();

    let mut corpora = 0;
    while let Some(line) = lines.next() {
        let rest = line
            .strip_prefix("file ")
            .unwrap_or_else(|| panic!("expected a file line, got: {line}"));
        let (id, hexbytes) = rest.split_once(' ').expect("file <id> <hex>");
        let bytes = if hexbytes == "-" {
            Vec::new()
        } else {
            unhex(hexbytes)
        };
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

        match W3dMesh::parse(&bytes) {
            Ok(mesh) => {
                assert_eq!(verdict, format!("ok {id}"), "corpus {id} should load");
                let actual = dump_mesh(&mesh, id);
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
    assert!(corpora >= 30, "expected the full corpus set, got {corpora}");
}
