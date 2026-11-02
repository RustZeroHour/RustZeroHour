// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Differential parity test against the committed golden dump (P0).
//!
//! The dump in `tests/golden/ini.txt` is produced by
//! `tools/differential/gen_ini_golden.cpp`, a verbatim transcription of the
//! C++ INI engine and the three task-10 block types at baseline commit
//! `571a7c5`. This test replays the identical load scenario over the same
//! fixtures and must produce the **byte-identical** dump. If it fails,
//! either the port or the transcription drifted from the original — do not
//! "fix" the golden file by hand; regenerate it with
//! `tools/differential/regenerate.sh` and review the diff.

use std::fs;
use std::path::PathBuf;

use rzh_ini::blocks::armor::ArmorStore;
use rzh_ini::blocks::locomotor::LocomotorStore;
use rzh_ini::blocks::weapon::WeaponStore;
use rzh_ini::{BlockParser, Ini, LoadType};
use rzh_util::{NameKey, NameKeyGenerator};

/// The three stores plus the shared name-key generator (the C++
/// `TheNameKeyGenerator`/`TheXxxStore` singletons as one explicit context).
struct GameData {
    names: NameKeyGenerator,
    armor: ArmorStore,
    loco: LocomotorStore,
    weapon: WeaponStore,
}

impl BlockParser for GameData {
    fn parse_block(&mut self, ini: &mut Ini, block: &[u8]) -> Result<bool, rzh_ini::IniError> {
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

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = fixtures_dir().join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
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

fn load_type_name(load_type: LoadType) -> &'static str {
    match load_type {
        LoadType::Overwrite => "OVERWRITE",
        LoadType::CreateOverrides => "CREATE_OVERRIDES",
        LoadType::Multifile => "MULTIFILE",
    }
}

/// One load, recorded exactly like the generator's `run`.
fn run(
    out: &mut Vec<u8>,
    game: &mut GameData,
    ini: &mut Ini,
    name: &str,
    load_type: LoadType,
    crlf: bool,
) {
    let data = read_fixture(name);
    let data = if crlf { to_crlf(&data) } else { data };
    match ini.load(name, data, load_type, game) {
        Ok(files) => {
            out.extend_from_slice(
                format!("file {name} {} ok {files}\n", load_type_name(load_type)).as_bytes(),
            );
        }
        Err(error) => {
            out.extend_from_slice(
                format!(
                    "file {name} {} error {} line {}\n",
                    load_type_name(load_type),
                    error.kind.constant_name(),
                    error.line
                )
                .as_bytes(),
            );
        }
    }
}

fn dump_string(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(format!("{}:", s.len()).as_bytes());
    out.extend_from_slice(s);
}

fn dump_hex_float(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(format!("0x{:08x}", v.to_bits()).as_bytes());
}

fn line(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
    out.push(b'\n');
}

/// The full dump, mirroring the generator's format line for line.
fn build_dump(game: &mut GameData) -> Vec<u8> {
    let mut out = Vec::new();
    line(&mut out, "# rzh-ini golden dump v1");

    let mut ini = Ini::new();
    run(
        &mut out,
        game,
        &mut ini,
        "armor.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "locomotor.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "weapon.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "override.ini",
        LoadType::CreateOverrides,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "edge.ini",
        LoadType::Overwrite,
        true,
    );

    run(
        &mut out,
        game,
        &mut ini,
        "bad_unknown_block.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_missing_end.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_value.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_bool.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_index.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_bitstring_mix.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_loco_thrust.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "bad_no_value.ini",
        LoadType::Overwrite,
        false,
    );
    run(
        &mut out,
        game,
        &mut ini,
        "weapon_dup.ini",
        LoadType::Overwrite,
        false,
    );

    // dump armor
    line(&mut out, &format!("dump armor {}", game.armor.len()));
    for (key, template) in game.armor.iter() {
        out.extend_from_slice(b"armor ");
        let name = game
            .names
            .key_to_name(NameKey::from_ordinal(key))
            .expect("armor key must resolve")
            .to_vec();
        dump_string(&mut out, &name);
        out.push(b'\n');
        for (d, coefficient) in template.damage_coefficient.iter().enumerate() {
            out.extend_from_slice(format!("  damage_coefficient[{d}] ").as_bytes());
            dump_hex_float(&mut out, *coefficient);
            out.push(b'\n');
        }
    }

    // dump locomotor (the final override of every base template)
    line(&mut out, &format!("dump locomotor {}", game.loco.len()));
    for (_, base) in game.loco.iter() {
        let final_t = base.final_override();
        let mut depth = 0usize;
        let mut node = Some(base);
        while let Some(current) = node {
            depth += 1;
            node = current.next_override.as_deref();
        }
        out.extend_from_slice(b"locomotor ");
        dump_string(&mut out, &final_t.name);
        out.extend_from_slice(
            format!(
                " depth {depth} override {}\n",
                u8::from(final_t.is_override)
            )
            .as_bytes(),
        );
        let f = |out: &mut Vec<u8>, label: &str, v: f32| {
            out.extend_from_slice(format!("  {label} ").as_bytes());
            dump_hex_float(out, v);
            out.push(b'\n');
        };
        line(&mut out, &format!("  surfaces 0x{:x}", final_t.surfaces));
        f(&mut out, "max_speed", final_t.max_speed);
        f(&mut out, "max_speed_damaged", final_t.max_speed_damaged);
        f(&mut out, "max_turn_rate", final_t.max_turn_rate);
        f(
            &mut out,
            "max_turn_rate_damaged",
            final_t.max_turn_rate_damaged,
        );
        f(&mut out, "acceleration", final_t.acceleration);
        f(
            &mut out,
            "acceleration_damaged",
            final_t.acceleration_damaged,
        );
        f(&mut out, "lift", final_t.lift);
        f(&mut out, "lift_damaged", final_t.lift_damaged);
        f(&mut out, "braking", final_t.braking);
        f(&mut out, "min_speed", final_t.min_speed);
        f(&mut out, "min_turn_speed", final_t.min_turn_speed);
        f(&mut out, "preferred_height", final_t.preferred_height);
        f(
            &mut out,
            "preferred_height_damping",
            final_t.preferred_height_damping,
        );
        f(&mut out, "circling_radius", final_t.circling_radius);
        f(&mut out, "extra_2d_friction", final_t.extra_2d_friction);
        f(&mut out, "speed_limit_z", final_t.speed_limit_z);
        f(&mut out, "max_thrust_angle", final_t.max_thrust_angle);
        line(&mut out, &format!("  behavior_z {}", final_t.behavior_z));
        line(&mut out, &format!("  appearance {}", final_t.appearance));
        line(
            &mut out,
            &format!("  move_priority {}", final_t.move_priority),
        );
        f(&mut out, "accel_pitch_limit", final_t.accel_pitch_limit);
        f(&mut out, "decel_pitch_limit", final_t.decel_pitch_limit);
        f(&mut out, "bounce_kick", final_t.bounce_kick);
        f(&mut out, "pitch_stiffness", final_t.pitch_stiffness);
        f(&mut out, "roll_stiffness", final_t.roll_stiffness);
        f(&mut out, "pitch_damping", final_t.pitch_damping);
        f(&mut out, "roll_damping", final_t.roll_damping);
        f(&mut out, "thrust_roll", final_t.thrust_roll);
        f(&mut out, "wobble_rate", final_t.wobble_rate);
        f(&mut out, "min_wobble", final_t.min_wobble);
        f(&mut out, "max_wobble", final_t.max_wobble);
        f(&mut out, "pitch_by_z_vel_coef", final_t.pitch_by_z_vel_coef);
        f(&mut out, "forward_vel_coef", final_t.forward_vel_coef);
        f(&mut out, "lateral_vel_coef", final_t.lateral_vel_coef);
        f(&mut out, "forward_accel_coef", final_t.forward_accel_coef);
        f(&mut out, "lateral_accel_coef", final_t.lateral_accel_coef);
        f(
            &mut out,
            "uniform_axial_damping",
            final_t.uniform_axial_damping,
        );
        f(&mut out, "turn_pivot_offset", final_t.turn_pivot_offset);
        line(
            &mut out,
            &format!(
                "  apply_2d_friction_when_airborne {}",
                u8::from(final_t.apply_2d_friction_when_airborne)
            ),
        );
        line(
            &mut out,
            &format!("  downhill_only {}", u8::from(final_t.downhill_only)),
        );
        line(
            &mut out,
            &format!(
                "  allow_motive_force_while_airborne {}",
                u8::from(final_t.allow_motive_force_while_airborne)
            ),
        );
        line(
            &mut out,
            &format!(
                "  locomotor_works_when_dead {}",
                u8::from(final_t.locomotor_works_when_dead)
            ),
        );
        line(
            &mut out,
            &format!(
                "  airborne_targeting_height {}",
                final_t.airborne_targeting_height
            ),
        );
        line(
            &mut out,
            &format!("  stick_to_ground {}", u8::from(final_t.stick_to_ground)),
        );
        line(
            &mut out,
            &format!(
                "  can_move_backward {}",
                u8::from(final_t.can_move_backward)
            ),
        );
        line(
            &mut out,
            &format!("  has_suspension {}", u8::from(final_t.has_suspension)),
        );
        f(&mut out, "wheel_turn_angle", final_t.wheel_turn_angle);
        f(
            &mut out,
            "maximum_wheel_extension",
            final_t.maximum_wheel_extension,
        );
        f(
            &mut out,
            "maximum_wheel_compression",
            final_t.maximum_wheel_compression,
        );
        f(&mut out, "close_enough_dist", final_t.close_enough_dist);
        line(
            &mut out,
            &format!(
                "  is_close_enough_dist_3d {}",
                u8::from(final_t.is_close_enough_dist_3d)
            ),
        );
        f(
            &mut out,
            "ultra_accurate_slide_into_place_factor",
            final_t.ultra_accurate_slide_into_place_factor,
        );
        f(&mut out, "wander_width_factor", final_t.wander_width_factor);
        f(
            &mut out,
            "wander_length_factor",
            final_t.wander_length_factor,
        );
        f(
            &mut out,
            "wander_about_point_radius",
            final_t.wander_about_point_radius,
        );
        f(
            &mut out,
            "rudder_correction_degree",
            final_t.rudder_correction_degree,
        );
        f(
            &mut out,
            "rudder_correction_rate",
            final_t.rudder_correction_rate,
        );
        f(
            &mut out,
            "elevator_correction_degree",
            final_t.elevator_correction_degree,
        );
        f(
            &mut out,
            "elevator_correction_rate",
            final_t.elevator_correction_rate,
        );
    }

    // dump weapon
    line(&mut out, &format!("dump weapon {}", game.weapon.len()));
    for (_, t) in game.weapon.iter() {
        out.extend_from_slice(b"weapon ");
        dump_string(&mut out, &t.name);
        out.push(b'\n');
        let f = |out: &mut Vec<u8>, label: &str, v: f32| {
            out.extend_from_slice(format!("  {label} ").as_bytes());
            dump_hex_float(out, v);
            out.push(b'\n');
        };
        f(&mut out, "primary_damage", t.primary_damage);
        f(&mut out, "primary_damage_radius", t.primary_damage_radius);
        f(&mut out, "secondary_damage", t.secondary_damage);
        f(
            &mut out,
            "secondary_damage_radius",
            t.secondary_damage_radius,
        );
        f(&mut out, "attack_range", t.attack_range);
        f(&mut out, "minimum_attack_range", t.minimum_attack_range);
        f(&mut out, "aim_delta", t.aim_delta);
        line(&mut out, &format!("  damage_type {}", t.damage_type));
        line(&mut out, &format!("  death_type {}", t.death_type));
        f(&mut out, "weapon_speed", t.weapon_speed);
        f(&mut out, "min_weapon_speed", t.min_weapon_speed);
        line(
            &mut out,
            &format!(
                "  is_scale_weapon_speed {}",
                u8::from(t.is_scale_weapon_speed)
            ),
        );
        f(&mut out, "weapon_recoil", t.weapon_recoil);
        out.extend_from_slice(b"  projectile_name ");
        dump_string(&mut out, &t.projectile_name);
        out.push(b'\n');
        line(&mut out, &format!("  clip_size {}", t.clip_size));
        line(
            &mut out,
            &format!("  clip_reload_time {}", t.clip_reload_time),
        );
        line(
            &mut out,
            &format!("  min_delay_between_shots {}", t.min_delay_between_shots),
        );
        line(
            &mut out,
            &format!("  max_delay_between_shots {}", t.max_delay_between_shots),
        );
        line(
            &mut out,
            &format!("  shots_per_barrel {}", t.shots_per_barrel),
        );
        line(
            &mut out,
            &format!(
                "  damage_dealt_at_self_position {}",
                u8::from(t.damage_dealt_at_self_position)
            ),
        );
        line(&mut out, &format!("  affects_mask 0x{:x}", t.affects_mask));
        line(&mut out, &format!("  anti_mask 0x{:x}", t.anti_mask));
        line(&mut out, &format!("  reload_type {}", t.reload_type));
        line(
            &mut out,
            &format!("  pre_attack_delay {}", t.pre_attack_delay),
        );
        line(&mut out, &format!("  prefire_type {}", t.prefire_type));
        f(&mut out, "historic_bonus_radius", t.historic_bonus_radius);
        line(
            &mut out,
            &format!("  leech_range_weapon {}", u8::from(t.leech_range_weapon)),
        );
        line(
            &mut out,
            &format!("  is_shows_ammo_pips {}", u8::from(t.is_shows_ammo_pips)),
        );
        out.extend_from_slice(b"  laser_name ");
        dump_string(&mut out, &t.laser_name);
        out.push(b'\n');
    }

    line(&mut out, "end");
    out
}

/// The differential parity test: the Rust replay must be byte-identical
/// to the C++ transcription's dump.
#[test]
fn ini_pipeline_matches_cpp_golden_dump() {
    let mut game = GameData {
        names: NameKeyGenerator::new(),
        armor: ArmorStore::new(),
        loco: LocomotorStore::new(),
        weapon: WeaponStore::new(),
    };
    let actual = build_dump(&mut game);

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("ini.txt");
    let expected = fs::read(&golden_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", golden_path.display()));

    if actual != expected {
        // Print the first divergence to make failures reviewable.
        let actual_text = String::from_utf8_lossy(&actual);
        let expected_text = String::from_utf8_lossy(&expected);
        let mut actual_lines = actual_text.lines();
        let mut expected_lines = expected_text.lines();
        let mut line_no = 0usize;
        let mut shown = false;
        while let (Some(a), Some(e)) = (actual_lines.next(), expected_lines.next()) {
            line_no += 1;
            if a != e {
                println!("first divergence at line {line_no}:\n  golden: {e}\n  rust:   {a}");
                shown = true;
                break;
            }
        }
        if !shown {
            println!(
                "dumps diverge in length: golden {} lines, rust {} lines",
                expected_text.lines().count(),
                actual_text.lines().count()
            );
        }
        panic!(
            "INI golden dump mismatch ({} vs {} bytes); regenerate with \
             tools/differential/regenerate.sh and review the diff",
            expected.len(),
            actual.len()
        );
    }

    // Sanity anchors so a regenerated-but-empty golden can never pass.
    let text = String::from_utf8_lossy(&expected);
    assert!(text.contains("file armor.ini OVERWRITE ok 1"));
    assert!(text.contains("file weapon_dup.ini OVERWRITE error INI_UNKNOWN_TOKEN line 2"));
    assert!(text.contains("dump armor 11"));
    assert!(text.contains("dump locomotor 7"));
    assert!(text.contains("dump weapon 3"));
}
