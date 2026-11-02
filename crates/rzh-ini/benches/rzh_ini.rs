// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Criterion benchmarks for `rzh-ini` (migration plan §6, Phase 1 exit
//! criterion).
//!
//! Three layers, matching where time actually goes during a data load:
//!
//! 1. **scan kernels** — `scan_int`/`scan_real`/`scan_bool` over a token
//!    corpus (the `scanType<T>` conversions behind every field); these are
//!    Rust-side regression tracking (no C++ counterpart — the fixture and
//!    synthetic parses below cover the engine end to end),
//! 2. **fixture scenario** — the five committed task-10 fixtures loaded
//!    into one fresh context with the same load types as the golden dump
//!    (armor/locomotor/weapon overwrite, override create-overrides, edge
//!    CRLF-transformed). One name-key generator serves the whole scenario,
//!    exactly like a real data load — this is the number the C++ baseline
//!    (`tools/differential/bench_baseline.sh`, the INI transcription) is
//!    compared against for the §6 budget,
//! 3. **synthetic bulk parse** — 512 generated Armor blocks (~300 KiB),
//!    parser throughput at the scale of one shipped `Armor.ini` without
//!    committing game data; also mirrored in the C++ baseline.
//!
//! Compare with:
//!
//! ```text
//! cargo bench -p rzh-ini
//! tools/differential/bench_baseline.sh   # ini section
//! ```

// Bench targets have no public API; the `criterion_group!` macro generates
// `pub fn`s that the workspace `missing_docs` lint would otherwise flag.
#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rzh_ini::blocks::armor::ArmorStore;
use rzh_ini::blocks::locomotor::LocomotorStore;
use rzh_ini::blocks::weapon::WeaponStore;
use rzh_ini::scan::{scan_bool, scan_int, scan_real};
use rzh_ini::{BlockParser, Ini, IniError, LoadType};
use rzh_util::NameKeyGenerator;

/// The differential harness's LCG (`gen_compress_golden.cpp`).
fn lcg(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// The three stores plus the shared name-key generator, exactly like the
/// golden test's `GameData` (the C++ singletons as one explicit context).
struct GameData {
    names: NameKeyGenerator,
    armor: ArmorStore,
    loco: LocomotorStore,
    weapon: WeaponStore,
}

impl BlockParser for GameData {
    fn parse_block(&mut self, ini: &mut Ini, block: &[u8]) -> Result<bool, IniError> {
        match block {
            b"Armor" => {
                self.armor.parse_definition(ini, &mut self.names)?;
                Ok(true)
            }
            b"Locomotor" => {
                self.loco.parse_definition(ini, &mut self.names)?;
                Ok(true)
            }
            b"Weapon" => {
                self.weapon.parse_definition(ini, &mut self.names)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn new_game() -> GameData {
    GameData {
        names: NameKeyGenerator::new(),
        armor: ArmorStore::new(),
        loco: LocomotorStore::new(),
        weapon: WeaponStore::new(),
    }
}

/// The scan kernels: `scanType<T>`'s numeric and boolean conversions.
fn bench_scan(c: &mut Criterion) {
    // A mixed token corpus in the shapes INI data actually uses.
    let mut state = 0xc0f_fee11_u32;
    let ints: Vec<Vec<u8>> = (0..256)
        .map(|_| {
            let v = (lcg(&mut state) % 10_000) as i32 - 5_000;
            v.to_string().into_bytes()
        })
        .collect();
    let reals: Vec<Vec<u8>> = (0..256)
        .map(|i| {
            let v = (lcg(&mut state) % 100_000) as f32 / 1_000.0 + (i % 7) as f32;
            format!("{v:.3}").into_bytes()
        })
        .collect();
    let bools: Vec<Vec<u8>> = (0..256)
        .map(|i| match i % 4 {
            0 => b"yes".to_vec(),
            1 => b"No".to_vec(),
            2 => b"YES".to_vec(),
            _ => b"no".to_vec(),
        })
        .collect();

    let mut group = c.benchmark_group("scan");
    group.bench_function("scan_int/x256", |b| {
        b.iter(|| {
            let tokens = black_box(&ints);
            tokens
                .iter()
                .map(|t| i64::from(scan_int(t).unwrap()))
                .sum::<i64>()
        })
    });
    group.bench_function("scan_real/x256", |b| {
        b.iter(|| {
            let tokens = black_box(&reals);
            tokens.iter().map(|t| scan_real(t).unwrap()).sum::<f32>()
        })
    });
    group.bench_function("scan_bool/x256", |b| {
        b.iter(|| {
            let tokens = black_box(&bools);
            tokens.iter().filter(|t| scan_bool(t).unwrap()).count()
        })
    });
    group.finish();
}

/// The committed fixtures: the golden load scenario, one context per run.
fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn read_fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures_dir().join(name)).expect("fixture present")
}

/// LF -> CRLF in memory (the fixture files stay LF-only in git).
fn to_crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    for &c in data {
        if c == b'\n' {
            out.push(b'\r');
        }
        out.push(c);
    }
    out
}

fn bench_fixture_scenario(c: &mut Criterion) {
    // (name, bytes, load type) — the golden scenario, edge.ini CRLF'd.
    let armor = read_fixture("armor.ini");
    let loco = read_fixture("locomotor.ini");
    let weapon = read_fixture("weapon.ini");
    let over = read_fixture("override.ini");
    let edge = to_crlf(&read_fixture("edge.ini"));
    let total: usize = armor.len() + loco.len() + weapon.len() + over.len() + edge.len();

    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(total as u64));
    group.bench_function("fixtures_all", |b| {
        b.iter(|| {
            let mut game = new_game();
            let loads: [(&str, &[u8], LoadType); 5] = [
                ("armor.ini", &armor, LoadType::Overwrite),
                ("locomotor.ini", &loco, LoadType::Overwrite),
                ("weapon.ini", &weapon, LoadType::Overwrite),
                ("override.ini", &over, LoadType::CreateOverrides),
                ("edge.ini", &edge, LoadType::Overwrite),
            ];
            let mut files = 0;
            for (name, data, load_type) in loads {
                let mut ini = Ini::new();
                ini.load(name, data.to_vec(), load_type, &mut game).unwrap();
                files += 1;
            }
            files
        })
    });
    group.finish();
}

/// 512 synthetic Armor blocks (~300 KiB): parser throughput at the scale of
/// one shipped `Armor.ini`, without committing game data. The corpus mixes
/// the `Armor = <DAMAGE_TYPE> <percent>%` field shape with comments and
/// blank blocks, all syntax the schema accepts; the C++ baseline generates
/// the identical bytes.
fn bench_synthetic_parse(c: &mut Criterion) {
    let damage_names = [
        "EXPLOSION",
        "CRUSH",
        "ARMOR_PIERCING",
        "SMALL_ARMS",
        "GATTLING",
        "RADIATION",
        "FLAME",
        "LASER",
        "SNIPER",
        "POISON",
        "HEALING",
        "UNRESISTABLE",
        "WATER",
        "DEPLOY",
        "SURRENDER",
        "HACK",
        "KILL_PILOT",
        "PENALTY",
        "FALLING",
        "MELEE",
    ];
    let mut ini_text = String::with_capacity(300_000);
    for index in 0..512_u32 {
        ini_text.push_str(&format!("Armor TestArmor{index:05}\n"));
        if index % 16 != 0 {
            // the usual shape: a handful of damage coefficients
            for (slot, name) in damage_names.iter().enumerate() {
                let percent = (index as usize + slot * 7) % 250 + 1;
                ini_text.push_str(&format!("  Armor = {name} {percent}%\n"));
            }
            ini_text.push_str("  ; a comment line the tokenizer must skip\n");
        }
        // every 16th block stays blank (legal and common in shipped data)
        ini_text.push_str("End\n\n");
    }
    let data = ini_text.into_bytes();

    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("synthetic_armor_x512", |b| {
        b.iter(|| {
            let mut game = new_game();
            let mut ini = Ini::new();
            ini.load(
                "Synthetic.ini",
                black_box(&data).clone(),
                LoadType::Overwrite,
                &mut game,
            )
            .unwrap();
            game.armor.len() as u32
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_scan,
    bench_fixture_scenario,
    bench_synthetic_parse
);
criterion_main!(benches);
