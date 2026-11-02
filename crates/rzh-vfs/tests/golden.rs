// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the rzh-vfs port (P0).
//!
//! `tests/golden/vfs.txt` is a *script*: every line is one command with
//! its expected output, produced by
//! `tools/differential/gen_vfs_golden.cpp` — a verbatim C++ transcription
//! of the engine's archive/overlay/routing logic, linked against the
//! REAL EAC RefPack codec (`refencode.cpp`/`refdecode.cpp`) for every
//! compressed payload. This test replays the script against the Rust
//! port and asserts line-by-line equality:
//!
//! - path keys, orderings and the wildcard truth table,
//! - BIG archive parsing, listings and duplicate-name resolution,
//! - the local-first instance routing (loose files shadow archives),
//! - mount order and `overwrite` (mods stacking in front),
//! - the INIZH.big duplicate skip,
//! - the `EAR\0` compression dispatch and the decompress-on-read
//!   consumer flow (`CachedFileInputStream`).
//!
//! Local files named by `localdef`/`bigdef` lines are created under a
//! temp directory, exactly like the generator's scratch root.

use std::collections::HashMap;
use std::path::PathBuf;

use rzh_vfs::{FilenameList, Vfs, compression, path};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("vfs.txt")
}

fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    assert_eq!(text.len() % 2, 0, "odd hex length");
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        out.push((hi * 16 + lo) as u8);
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// The '-' placeholder for empty path/pattern arguments.
fn arg(field: &str) -> &str {
    match field {
        "-" => "",
        other => other,
    }
}

/// `CachedFileInputStream::open` (DataChunk.cpp): what the consumer is
/// served after the decompress-on-read flow. Returns `None` for the
/// empty file (the C++ open returns FALSE there).
fn cached_input_stream_read(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.is_empty() {
        return None; // if (m_size) { ... } — empty files never open
    }
    if !compression::is_data_compressed(raw) {
        return Some(raw.to_vec());
    }
    let ulen = compression::get_uncompressed_size(raw);
    let mut dest = vec![0u8; ulen];
    match compression::decompress_data(raw, &mut dest) {
        Ok(actual) if actual == ulen => {
            dest.truncate(actual);
            Some(dest)
        }
        // "Decompression failed - using compressed data"
        _ => Some(raw.to_vec()),
    }
}

fn write_game_path(root: &std::path::Path, game_path: &str, data: &[u8]) {
    let native = game_path.replace('\\', "/");
    let full = root.join(native);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("create local file parent");
    }
    std::fs::write(&full, data).expect("write local file");
}

fn scratch_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rzh-vfs-golden-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create golden scratch root");
    dir
}

#[test]
fn vfs_matches_cpp_golden_script() {
    let text = std::fs::read_to_string(golden_path())
        .unwrap_or_else(|e| panic!("cannot read golden file: {e}"));

    let root = scratch_root("run");
    let mut vfs = Vfs::new(&root);
    let mut big_defs: HashMap<String, Vec<u8>> = HashMap::new();
    let mut checked = 0u32;

    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let cmd = fields[0];
        match cmd {
            "pathkey" => {
                let input = arg(fields[1]);
                let expected = fields[2];
                let actual = path::make_path_key(input);
                match (expected, &actual) {
                    ("FAIL", None) => {}
                    (_, Some(key)) => assert_eq!(key, arg(expected), "pathkey {input:?}"),
                    (_, None) => panic!("pathkey {input:?} unexpectedly failed"),
                }
            }
            "cmp" => {
                let a = arg(fields[1]);
                let b = arg(fields[2]);
                let expected: i32 = fields[3].parse().unwrap();
                assert_eq!(path::compare_path(a, b), expected, "cmp {a:?} {b:?}");
            }
            "wild" => {
                let subject = arg(fields[1]);
                let pattern = arg(fields[2]);
                let expected: u8 = fields[3].parse().unwrap();
                let matches = rzh_vfs::wildcard::search_string_matches(subject, pattern);
                assert_eq!(u8::from(matches), expected, "wild {subject:?} {pattern:?}");
            }
            "comptype" => {
                let data = unhex(fields[1]);
                let expected = fields[2];
                let actual = match compression::get_compression_type(&data) {
                    compression::CompressionType::None => "none",
                    compression::CompressionType::RefPack => "refpack",
                    compression::CompressionType::NoxLzh => "noxlzh",
                    compression::CompressionType::Zlib(n) => match n {
                        1 => "zlib1",
                        2 => "zlib2",
                        3 => "zlib3",
                        4 => "zlib4",
                        5 => "zlib5",
                        6 => "zlib6",
                        7 => "zlib7",
                        8 => "zlib8",
                        _ => "zlib9",
                    },
                    compression::CompressionType::BTree => "btree",
                    compression::CompressionType::Huff => "huff",
                };
                assert_eq!(actual, expected, "comptype {}", fields[1]);
            }
            "uncsize" => {
                let data = unhex(fields[1]);
                let expected: usize = fields[2].parse().unwrap();
                assert_eq!(
                    compression::get_uncompressed_size(&data),
                    expected,
                    "uncsize"
                );
            }
            "decomp" => {
                let data = unhex(fields[1]);
                let expected = unhex(fields[2]);
                let ulen = compression::get_uncompressed_size(&data);
                let mut dest = vec![0u8; ulen];
                match compression::decompress_data(&data, &mut dest) {
                    // the C++ decompressData returns 0 on failure; the
                    // golden never expects that for EAR samples
                    Ok(actual) => {
                        dest.truncate(actual);
                        assert_eq!(dest, expected, "decomp");
                    }
                    Err(e) => panic!("decomp failed: {e:?}"),
                }
            }
            "decompofbytes" => {
                let data = unhex(fields[1]);
                let expected = fields[2];
                match cached_input_stream_read(&data) {
                    Some(served) => assert_eq!(hex(&served), expected, "decompofbytes"),
                    None => assert_eq!(expected, "-", "decompofbytes empty"),
                }
            }
            "bigdef" => {
                let name = arg(fields[1]);
                let image = unhex(fields[2]);
                write_game_path(&root, name, &image);
                big_defs.insert(name.to_string(), image);
            }
            "localdef" => {
                let name = arg(fields[1]);
                let content = unhex(fields[2]);
                write_game_path(&root, name, &content);
            }
            "mountdir" => {
                let dir = arg(fields[1]);
                let mask = arg(fields[2]);
                let overwrite = fields[3] == "1";
                let expected = fields[4] == "1";
                assert_eq!(
                    vfs.load_archives_from_directory(dir, mask, overwrite),
                    expected,
                    "mountdir {dir:?} {mask:?} {overwrite}"
                );
            }
            "exists" => {
                let name = arg(fields[1]);
                let instance: u8 = fields[2].parse().unwrap();
                let expected = fields[3] == "1";
                assert_eq!(
                    vfs.does_file_exist(name, instance),
                    expected,
                    "exists {name:?} {instance}"
                );
            }
            "open" => {
                let name = arg(fields[1]);
                let instance: u8 = fields[2].parse().unwrap();
                let expected = fields[3];
                match vfs.read_instance(name, instance) {
                    Some(data) => assert_eq!(hex(&data), expected, "open {name:?} {instance}"),
                    None => assert_eq!(expected, "-", "open {name:?} {instance}"),
                }
            }
            "decompof" => {
                let name = arg(fields[1]);
                let instance: u8 = fields[2].parse().unwrap();
                let expected = fields[3];
                let served = vfs
                    .read_instance(name, instance)
                    .and_then(|raw| cached_input_stream_read(&raw));
                match served {
                    Some(data) => assert_eq!(hex(&data), expected, "decompof {name:?} {instance}"),
                    None => assert_eq!(expected, "-", "decompof {name:?} {instance}"),
                }
            }
            "info" => {
                let name = arg(fields[1]);
                let instance: u8 = fields[2].parse().unwrap();
                let expected = fields[3];
                match vfs.file_info(name, instance) {
                    Some(info) => {
                        assert_eq!(info.size.to_string(), expected, "info {name:?} {instance}")
                    }
                    None => assert_eq!(expected, "-", "info {name:?} {instance}"),
                }
            }
            "list" => {
                let dir = arg(fields[1]);
                let pattern = arg(fields[2]);
                let recursive = fields[3] == "1";
                let count: usize = fields[4].parse().unwrap();
                let expected: Vec<&str> = fields[5..].to_vec();
                let list: FilenameList = vfs.get_file_list_in_directory(dir, pattern, recursive);
                let actual: Vec<String> = list.iter().map(|p| p.to_string()).collect();
                assert_eq!(
                    actual.len(),
                    count,
                    "list {dir:?} {pattern:?} count\n  got: {actual:?}"
                );
                let expected: Vec<String> =
                    expected.into_iter().map(|p| arg(p).to_string()).collect();
                assert_eq!(actual, expected, "list {dir:?} {pattern:?} {recursive}");
            }
            other => panic!("unknown golden command {other:?} in {line:?}"),
        }
        checked += 1;
    }

    // the script must exercise every command family
    assert!(checked > 180, "suspiciously few records: {checked}");
    println!("vfs golden script: {checked} records checked");

    let _ = std::fs::remove_dir_all(&root);
}

/// Cross-check not covered by the C++ side: the stored bytes of every
/// golden archive re-encode to the very same RefPack streams through the
/// Rust encoder (the differential already proves decode equality; this
/// proves the whole BIG → RefPack → payload pipeline is invertible in
/// Rust alone).
#[test]
fn golden_archive_payloads_round_trip_through_rust_refpack() {
    let text = std::fs::read_to_string(golden_path())
        .unwrap_or_else(|e| panic!("cannot read golden file: {e}"));
    let mut checked = 0u32;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.first() != Some(&"decompofbytes") {
            continue;
        }
        let data = unhex(fields[1]);
        let expected = unhex(fields[2]);
        if data.is_empty() || expected.is_empty() {
            continue;
        }
        // only the EAR family round-trips through the Rust encoder
        if compression::get_compression_type(&data) != compression::CompressionType::RefPack {
            continue;
        }
        assert_eq!(
            compression::compress_refpack(&expected),
            data,
            "EAR re-encode diverged on {} byte payload",
            expected.len()
        );
        checked += 1;
    }

    assert!(checked > 3, "suspiciously few payloads: {checked}");
    println!("vfs EAR payloads re-encoded through Rust: {checked}");
}
