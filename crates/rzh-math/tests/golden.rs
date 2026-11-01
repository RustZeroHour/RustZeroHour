// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Differential parity tests against the committed golden vectors (P0).
//!
//! The vectors in `tests/golden/` are produced by `tools/differential/
//! gen_golden.cpp`, a verbatim transcription of the WWMath code at the
//! baseline commit `571a7c5` (deterministic branches). If a test here
//! fails, either the port or the transcription has drifted from the
//! original — do not "fix" the golden files by hand; regenerate them with
//! `tools/differential/regenerate.sh` and review the diff.

use std::fs;
use std::path::PathBuf;

use rzh_math::scalar;
use rzh_math::{Vector2, Vector2i, Vector3, Vector3i, Vector4};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn read_golden(name: &str) -> String {
    let path = golden_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn hex_f32(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> f32 {
    f32::from_bits(
        u32::from_str_radix(
            fields
                .next()
                .unwrap_or_else(|| panic!("missing field in {line}")),
            16,
        )
        .unwrap(),
    )
}

fn hex_u32(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> u32 {
    u32::from_str_radix(
        fields
            .next()
            .unwrap_or_else(|| panic!("missing field in {line}")),
        16,
    )
    .unwrap()
}

fn dec_i32(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> i32 {
    fields
        .next()
        .unwrap_or_else(|| panic!("missing field in {line}"))
        .parse()
        .unwrap()
}

fn dec_u32(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> u32 {
    fields
        .next()
        .unwrap_or_else(|| panic!("missing field in {line}"))
        .parse()
        .unwrap()
}

fn bits(value: f32) -> u32 {
    value.to_bits()
}

/// Drives every scalar record through the Rust port.
#[test]
fn scalar_matches_cpp_golden_vectors() {
    let text = read_golden("scalar.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        assert_eq!(fields.next(), Some("scalar"), "unexpected line: {line}");
        let op = fields.next().expect("missing op");
        match op {
            "sqrt" => assert_eq!(
                bits(scalar::sqrt(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "sqrt in {line}"
            ),
            "inv_sqrt" => assert_eq!(
                bits(scalar::inv_sqrt(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "inv_sqrt in {line}"
            ),
            "fabs" => assert_eq!(
                bits(scalar::fabs(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "fabs in {line}"
            ),
            "floor" => assert_eq!(
                bits(scalar::floor(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "floor in {line}"
            ),
            "ceil" => assert_eq!(
                bits(scalar::ceil(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "ceil in {line}"
            ),
            "round" => assert_eq!(
                bits(scalar::round(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "round in {line}"
            ),
            "clamp" => {
                let a = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::clamp(a, 0.0, 1.0)),
                    hex_u32(&mut fields, line),
                    "clamp in {line}"
                );
            }
            "clamp2" => {
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::clamp(a, -2.5, 7.5)),
                    hex_u32(&mut fields, line),
                    "clamp2 in {line}"
                );
                let _ = b;
            }
            "wrap" => {
                let a = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::wrap(a, 0.0, 1.0)),
                    hex_u32(&mut fields, line),
                    "wrap in {line}"
                );
            }
            "min" => {
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::min(a, b)),
                    hex_u32(&mut fields, line),
                    "min in {line}"
                );
            }
            "max" => {
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::max(a, b)),
                    hex_u32(&mut fields, line),
                    "max in {line}"
                );
            }
            "lerp" => {
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                let t = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::lerp(a, b, t)),
                    hex_u32(&mut fields, line),
                    "lerp in {line}"
                );
            }
            "inverse_lerp" => {
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                let v = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::inverse_lerp(a, b, v)),
                    hex_u32(&mut fields, line),
                    "inverse_lerp in {line}"
                );
            }
            "normalize_angle" => assert_eq!(
                bits(scalar::normalize_angle(hex_f32(&mut fields, line))),
                hex_u32(&mut fields, line),
                "normalize_angle in {line}"
            ),
            "div_safe" => {
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::div_safe(a, b, scalar::EPSILON)),
                    hex_u32(&mut fields, line),
                    "div_safe in {line}"
                );
            }
            "byte_to_unit" => {
                let a = hex_f32(&mut fields, line);
                assert_eq!(
                    bits(scalar::byte_to_unit_float(a as u8)),
                    hex_u32(&mut fields, line),
                    "byte_to_unit in {line}"
                );
            }
            "float_to_int_chop" => assert_eq!(
                scalar::float_to_int_chop(hex_f32(&mut fields, line)),
                dec_i32(&mut fields, line),
                "float_to_int_chop in {line}"
            ),
            "float_to_int_floor" => assert_eq!(
                scalar::float_to_int_floor(hex_f32(&mut fields, line)),
                dec_i32(&mut fields, line),
                "float_to_int_floor in {line}"
            ),
            "unit_float_to_byte" => assert_eq!(
                u32::from(scalar::unit_float_to_byte(hex_f32(&mut fields, line))),
                dec_u32(&mut fields, line),
                "unit_float_to_byte in {line}"
            ),
            "is_valid_float" => assert_eq!(
                u32::from(scalar::is_valid_float(hex_f32(&mut fields, line))),
                dec_u32(&mut fields, line),
                "is_valid_float in {line}"
            ),
            "is_power_of_2" => assert_eq!(
                u32::from(scalar::is_power_of_2(dec_u32(&mut fields, line))),
                dec_u32(&mut fields, line),
                "is_power_of_2 in {line}"
            ),
            "fast_is_float_positive" => assert_eq!(
                u32::from(scalar::fast_is_float_positive(hex_f32(&mut fields, line))),
                dec_u32(&mut fields, line),
                "fast_is_float_positive in {line}"
            ),
            "clamp_int" => {
                let v = dec_i32(&mut fields, line);
                let lo = dec_i32(&mut fields, line);
                let hi = dec_i32(&mut fields, line);
                assert_eq!(
                    scalar::clamp_int(v, lo, hi),
                    dec_i32(&mut fields, line),
                    "clamp_int in {line}"
                );
            }
            other => panic!("unexpected scalar op: {other}"),
        }
        assert!(fields.next().is_none(), "trailing fields in {line}");
        checked += 1;
    }

    assert!(checked > 5000, "suspiciously few scalar vectors: {checked}");
}

/// Drives every vector record through the Rust port.
#[test]
fn vectors_match_cpp_golden_vectors() {
    let text = read_golden("vectors.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields.next().expect("missing kind");
        let op = fields.next().expect("missing op");
        match kind {
            "v2" => {
                let f = |fs: &mut std::str::SplitWhitespace<'_>| hex_f32(fs, line);
                match op {
                    "add" | "sub" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        let r = if op == "add" { a + b } else { a - b };
                        check_v2(&mut fields, r, line);
                    }
                    "mul" | "rmul" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let k = f(&mut fields);
                        let r = if op == "mul" { a * k } else { k * a };
                        check_v2(&mut fields, r, line);
                    }
                    "div" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let k = f(&mut fields);
                        check_v2(&mut fields, a / k, line);
                    }
                    "neg" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        check_v2(&mut fields, -a, line);
                    }
                    "dot" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        check_f(&mut fields, Vector2::dot(a, b), line);
                    }
                    "perp_dot" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        check_f(&mut fields, Vector2::perp_dot(a, b), line);
                    }
                    "length" | "length2" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let r = if op == "length" {
                            a.length()
                        } else {
                            a.length2()
                        };
                        check_f(&mut fields, r, line);
                    }
                    "normalize" => {
                        let mut a = Vector2::new(f(&mut fields), f(&mut fields));
                        a.normalize();
                        check_v2(&mut fields, a, line);
                    }
                    "normalize_free" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        check_v2(&mut fields, Vector2::normalize_free(a), line);
                    }
                    "distance" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        check_f(&mut fields, Vector2::distance(a, b), line);
                    }
                    "quick_distance" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        check_f(&mut fields, Vector2::quick_distance(a, b), line);
                    }
                    "lerp" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        let t = f(&mut fields);
                        check_v2(&mut fields, Vector2::lerp(a, b, t), line);
                    }
                    "update_min" | "update_max" => {
                        let mut a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        if op == "update_min" {
                            a.update_min(b);
                        } else {
                            a.update_max(b);
                        }
                        check_v2(&mut fields, a, line);
                    }
                    "scale" => {
                        let mut a = Vector2::new(f(&mut fields), f(&mut fields));
                        let sx = f(&mut fields);
                        let sy = f(&mut fields);
                        a.scale(sx, sy);
                        check_v2(&mut fields, a, line);
                    }
                    "scale_v" => {
                        let mut a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        a.scale_v(b);
                        check_v2(&mut fields, a, line);
                    }
                    "rotate_sc" => {
                        let mut a = Vector2::new(f(&mut fields), f(&mut fields));
                        let s = f(&mut fields);
                        let c = f(&mut fields);
                        a.rotate(s, c);
                        check_v2(&mut fields, a, line);
                    }
                    "equal_eps" => {
                        let a = Vector2::new(f(&mut fields), f(&mut fields));
                        let b = Vector2::new(f(&mut fields), f(&mut fields));
                        let expected = dec_i32(&mut fields, line);
                        assert_eq!(
                            i32::from(rzh_math::vector2::equal_within_epsilon(
                                a,
                                b,
                                scalar::EPSILON
                            )),
                            expected,
                            "equal_eps in {line}"
                        );
                    }
                    other => panic!("unexpected v2 op: {other}"),
                }
            }
            "v3" => {
                let f = |fs: &mut std::str::SplitWhitespace<'_>| hex_f32(fs, line);
                match op {
                    "add" | "sub" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let r = if op == "add" { a + b } else { a - b };
                        check_v3(&mut fields, r, line);
                    }
                    "mul" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let k = f(&mut fields);
                        check_v3(&mut fields, a * k, line);
                    }
                    "div" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let k = f(&mut fields);
                        check_v3(&mut fields, a / k, line);
                    }
                    "neg" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        check_v3(&mut fields, -a, line);
                    }
                    "dot" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        check_f(&mut fields, Vector3::dot(a, b), line);
                    }
                    "cross" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        check_v3(&mut fields, Vector3::cross(a, b), line);
                    }
                    "normalized_cross" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        check_v3(&mut fields, Vector3::normalized_cross(a, b), line);
                    }
                    "cross_x" | "cross_y" | "cross_z" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let r = match op {
                            "cross_x" => Vector3::cross_x(a, b),
                            "cross_y" => Vector3::cross_y(a, b),
                            _ => Vector3::cross_z(a, b),
                        };
                        check_f(&mut fields, r, line);
                    }
                    "length" | "length2" | "quick_length" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let r = match op {
                            "length" => a.length(),
                            "length2" => a.length2(),
                            _ => a.quick_length(),
                        };
                        check_f(&mut fields, r, line);
                    }
                    "normalize" => {
                        let mut a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        a.normalize();
                        check_v3(&mut fields, a, line);
                    }
                    "distance" | "quick_distance" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let r = if op == "distance" {
                            Vector3::distance(a, b)
                        } else {
                            Vector3::quick_distance(a, b)
                        };
                        check_f(&mut fields, r, line);
                    }
                    "lerp" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let t = f(&mut fields);
                        check_v3(&mut fields, Vector3::lerp(a, b, t), line);
                    }
                    "update_min" | "update_max" | "cap_absolute_to" | "scale_v" => {
                        let mut a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        match op {
                            "update_min" => a.update_min(b),
                            "update_max" => a.update_max(b),
                            "cap_absolute_to" => a.cap_absolute_to(b),
                            _ => a.scale_v(b),
                        }
                        check_v3(&mut fields, a, line);
                    }
                    "rotate_x" | "rotate_y" | "rotate_z" => {
                        let mut a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let s = f(&mut fields);
                        let c = f(&mut fields);
                        match op {
                            "rotate_x" => a.rotate_x(s, c),
                            "rotate_y" => a.rotate_y(s, c),
                            _ => a.rotate_z(s, c),
                        }
                        check_v3(&mut fields, a, line);
                    }
                    "find_x_at_y" | "find_y_at_x" | "find_z_at_x" => {
                        let p1 = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let p2 = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let r = match op {
                            "find_x_at_y" => Vector3::find_x_at_y(p1.y, p1, p2),
                            "find_y_at_x" => Vector3::find_y_at_x(p1.x, p1, p2),
                            _ => Vector3::find_z_at_x(p1.x, p1, p2),
                        };
                        check_f(&mut fields, r, line);
                    }
                    "equal_eps" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let b = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        let expected = dec_i32(&mut fields, line);
                        assert_eq!(
                            i32::from(rzh_math::vector3::equal_within_epsilon(
                                a,
                                b,
                                scalar::EPSILON
                            )),
                            expected,
                            "equal_eps in {line}"
                        );
                    }
                    "convert_abgr" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        assert_eq!(
                            a.convert_to_abgr(),
                            hex_u32(&mut fields, line),
                            "convert_abgr in {line}"
                        );
                    }
                    "convert_argb" => {
                        let a = Vector3::new(f(&mut fields), f(&mut fields), f(&mut fields));
                        assert_eq!(
                            a.convert_to_argb(),
                            hex_u32(&mut fields, line),
                            "convert_argb in {line}"
                        );
                    }
                    other => panic!("unexpected v3 op: {other}"),
                }
            }
            "v4" => {
                let f = |fs: &mut std::str::SplitWhitespace<'_>| hex_f32(fs, line);
                match op {
                    "add" | "sub" | "sub_assign" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let b = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let mut r = a;
                        match op {
                            "add" => r = a + b,
                            "sub" => r = a - b,
                            _ => r -= b, // the W += quirk
                        }
                        check_v4(&mut fields, r, line);
                    }
                    "mul" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let k = f(&mut fields);
                        check_v4(&mut fields, a * k, line);
                    }
                    "div" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let k = f(&mut fields);
                        check_v4(&mut fields, a / k, line);
                    }
                    "neg" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        check_v4(&mut fields, -a, line);
                    }
                    "dot" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let b = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        check_f(&mut fields, Vector4::dot(a, b), line);
                    }
                    "length" | "length2" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let r = if op == "length" {
                            a.length()
                        } else {
                            a.length2()
                        };
                        check_f(&mut fields, r, line);
                    }
                    "normalize" => {
                        let mut a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        a.normalize();
                        check_v4(&mut fields, a, line);
                    }
                    "normalize_free" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        check_v4(&mut fields, Vector4::normalize_free(a), line);
                    }
                    "lerp_free" | "lerp_static" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let b = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let t = f(&mut fields);
                        check_v4(&mut fields, Vector4::lerp(a, b, t), line);
                    }
                    "lerp_out" => {
                        let a = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let b = Vector4::new(
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                            f(&mut fields),
                        );
                        let t = f(&mut fields);
                        // The generator's out-parameter carried the previous
                        // lerp_static result; the quirk leaves W untouched,
                        // so the same prior state is reproduced here.
                        let mut out = Vector4::lerp(a, b, t);
                        Vector4::lerp_out(a, b, t, &mut out);
                        check_v4(&mut fields, out, line);
                    }
                    other => panic!("unexpected v4 op: {other}"),
                }
            }
            "vi2" => {
                assert_eq!(op, "eq");
                let a = Vector2i::new(dec_i32(&mut fields, line), dec_i32(&mut fields, line));
                let b = Vector2i::new(dec_i32(&mut fields, line), dec_i32(&mut fields, line));
                assert_eq!(
                    i32::from(a == b),
                    dec_i32(&mut fields, line),
                    "vi2 eq in {line}"
                );
            }
            "vi3" => {
                assert_eq!(op, "eq");
                let a = Vector3i::new(
                    dec_i32(&mut fields, line),
                    dec_i32(&mut fields, line),
                    dec_i32(&mut fields, line),
                );
                let b = Vector3i::new(
                    dec_i32(&mut fields, line),
                    dec_i32(&mut fields, line),
                    dec_i32(&mut fields, line),
                );
                assert_eq!(
                    i32::from(a == b),
                    dec_i32(&mut fields, line),
                    "vi3 eq in {line}"
                );
            }
            other => panic!("unexpected record kind: {other}"),
        }
        assert!(fields.next().is_none(), "trailing fields in {line}");
        checked += 1;
    }

    assert!(checked > 5000, "suspiciously few vector vectors: {checked}");
}

fn check_f(fields: &mut std::str::SplitWhitespace<'_>, actual: f32, line: &str) {
    let expected = hex_u32(fields, line);
    assert_eq!(
        actual.to_bits(),
        expected,
        "float mismatch in {line}: {} vs {}",
        actual,
        f32::from_bits(expected)
    );
}

fn check_v2(fields: &mut std::str::SplitWhitespace<'_>, actual: Vector2, line: &str) {
    assert_eq!(
        bits(actual.x),
        hex_u32(fields, line),
        "x mismatch in {line}"
    );
    assert_eq!(
        bits(actual.y),
        hex_u32(fields, line),
        "y mismatch in {line}"
    );
}

fn check_v3(fields: &mut std::str::SplitWhitespace<'_>, actual: Vector3, line: &str) {
    assert_eq!(
        bits(actual.x),
        hex_u32(fields, line),
        "x mismatch in {line}"
    );
    assert_eq!(
        bits(actual.y),
        hex_u32(fields, line),
        "y mismatch in {line}"
    );
    assert_eq!(
        bits(actual.z),
        hex_u32(fields, line),
        "z mismatch in {line}"
    );
}

fn check_v4(fields: &mut std::str::SplitWhitespace<'_>, actual: Vector4, line: &str) {
    assert_eq!(
        bits(actual.x),
        hex_u32(fields, line),
        "x mismatch in {line}"
    );
    assert_eq!(
        bits(actual.y),
        hex_u32(fields, line),
        "y mismatch in {line}"
    );
    assert_eq!(
        bits(actual.z),
        hex_u32(fields, line),
        "z mismatch in {line}"
    );
    assert_eq!(
        bits(actual.w),
        hex_u32(fields, line),
        "w mismatch in {line}"
    );
}
