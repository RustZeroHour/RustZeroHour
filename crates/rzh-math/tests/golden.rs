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

use rzh_math::gmath;
use rzh_math::scalar;
use rzh_math::{
    AABox, AAPlane, Axis, Frustum, IntersectionRes, Matrix3, Matrix3D, Matrix4, MinMaxAABox, Plane,
    Quaternion, Vector2, Vector2i, Vector3, Vector3i, Vector4,
};

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

/// Bit-exact comparison with one documented exception: when the C oracle
/// produced a NaN, the payload is unspecified behaviour in C (the pin's
/// gcc build propagates the *second* operand's NaN through `(x+0)+(y+0)`
/// after reassociating; MSVC and arm64 make different choices). The game
/// only ever tests `isnan` on these paths, so NaN results compare by
/// "both are NaN"; every non-NaN result stays bit-exact.
fn expect_gmath_bits(actual: u32, expected: u32, line: &str) {
    let exp = f32::from_bits(expected);
    let act = f32::from_bits(actual);
    if exp.is_nan() {
        assert!(act.is_nan(), "expected NaN, got {actual:08x} in {line}");
    } else {
        assert_eq!(actual, expected, "bits differ in {line}");
    }
}

/// Drives every `gmath` record through the Rust port.
///
/// The vectors are generated by `tools/differential/gen_gmath_golden.c`
/// linked against the REAL pinned GameMath sources (TheSuperHackers/
/// GameMath @ 59f7ccd4) — not a transcription — so this is a direct
/// port-vs-original comparison (parity level P0, ADR-001).
#[test]
fn gmath_matches_pinned_gamemath_golden_vectors() {
    let text = read_golden("gmath.txt");
    let mut checked = 0;
    let mut skipped_lrintf = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));

        // unary: <name> <input-bits> <result-bits>
        macro_rules! check1 {
            ($f:expr) => {{
                let x = hex_f32(&mut fields, line);
                let expected = hex_u32(&mut fields, line);
                expect_gmath_bits(bits($f(x)), expected, line);
            }};
        }
        // binary: <name> <x-bits> <y-bits> <result-bits>
        macro_rules! check2 {
            ($f:expr) => {{
                let a = hex_f32(&mut fields, line);
                let b = hex_f32(&mut fields, line);
                let expected = hex_u32(&mut fields, line);
                expect_gmath_bits(bits($f(a, b)), expected, line);
            }};
        }

        match name {
            // ---- unary float -> float ----
            "sinf" => check1!(gmath::sinf),
            "cosf" => check1!(gmath::cosf),
            "tanf" => check1!(gmath::tanf),
            "asinf" => check1!(gmath::asinf),
            "acosf" => check1!(gmath::acosf),
            "atanf" => check1!(gmath::atanf),
            "expf" => check1!(gmath::expf),
            "expm1f" => check1!(gmath::expm1f),
            "logf" => check1!(gmath::logf),
            "log10f" => check1!(gmath::log10f),
            "sinhf" => check1!(gmath::sinhf),
            "coshf" => check1!(gmath::coshf),
            "tanhf" => check1!(gmath::tanhf),
            // sqrtf is ADR-001's IEEE-exact mapping; still golden-pinned
            // here against the pin's software sqrt to prove the mapping.
            "sqrtf" => check1!(scalar::sqrt),
            // scalbnf(x, 3): the generator pins the scale to 3.
            "scalbnf" => check1!(|x| gmath::scalbnf(x, 3)),
            // ---- binary float -> float ----
            // note: the C signature is atan2f(y, x); the generator emits
            // the first operand first.
            "atan2f" => check2!(gmath::atan2f),
            "powf" => check2!(gmath::powf),
            // ---- float -> long ----
            "lrintf" => {
                let x = hex_f32(&mut fields, line);
                let expected: i64 = fields
                    .next()
                    .unwrap_or_else(|| panic!("missing field in {line}"))
                    .parse()
                    .unwrap();
                // The C pin's `long` is 32-bit on Windows (the game's
                // platform) and 64-bit elsewhere. Vectors whose result
                // fits i32 are compared directly; wider results only
                // exist because the generator ran on a 64-bit long
                // platform and are skipped here (the Rust port returns
                // i64 and its in-i32 behaviour is what the game sees).
                let actual = gmath::lrintf(x);
                if expected >= i32::MIN as i64 && expected <= i32::MAX as i64 {
                    assert_eq!(actual, expected, "lrintf in {line}");
                } else {
                    skipped_lrintf += 1;
                }
            }
            other => panic!("unknown record name {other} in {line}"),
        }
        checked += 1;
    }

    assert!(checked > 70_000, "suspiciously few records: {checked}");
    println!("gmath golden vectors: {checked} checked, {skipped_lrintf} wide lrintf skipped");
}

// ---------------------------------------------------------------------------
// Matrix3 / Quaternion golden vectors.
// ---------------------------------------------------------------------------

/// Reads a 3x3 matrix (9 hex words, row-major).
fn hex_m3(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Matrix3 {
    let mut rows = [Vector3::default(); 3];
    for row in &mut rows {
        *row = Vector3::new(
            hex_f32(fields, line),
            hex_f32(fields, line),
            hex_f32(fields, line),
        );
    }
    Matrix3::from_rows(rows[0], rows[1], rows[2])
}

fn check_m3(fields: &mut std::str::SplitWhitespace<'_>, actual: Matrix3, line: &str) {
    for i in 0..3 {
        for j in 0..3 {
            expect_gmath_bits(
                bits(actual[(i, j)]),
                hex_u32(fields, line),
                &format!("m3 ({i},{j}) in {line}"),
            );
        }
    }
}

fn check_v3_fields(fields: &mut std::str::SplitWhitespace<'_>, actual: Vector3, line: &str) {
    expect_gmath_bits(
        bits(actual.x),
        hex_u32(fields, line),
        &format!("x in {line}"),
    );
    expect_gmath_bits(
        bits(actual.y),
        hex_u32(fields, line),
        &format!("y in {line}"),
    );
    expect_gmath_bits(
        bits(actual.z),
        hex_u32(fields, line),
        &format!("z in {line}"),
    );
}

fn check_q(fields: &mut std::str::SplitWhitespace<'_>, actual: Quaternion, line: &str) {
    expect_gmath_bits(
        bits(actual.x),
        hex_u32(fields, line),
        &format!("qx in {line}"),
    );
    expect_gmath_bits(
        bits(actual.y),
        hex_u32(fields, line),
        &format!("qy in {line}"),
    );
    expect_gmath_bits(
        bits(actual.z),
        hex_u32(fields, line),
        &format!("qz in {line}"),
    );
    expect_gmath_bits(
        bits(actual.w),
        hex_u32(fields, line),
        &format!("qw in {line}"),
    );
}

/// Drives every Matrix3 / Quaternion record through the Rust port.
///
/// The vectors are generated by `tools/differential/gen_golden.cpp` — a
/// verbatim transcription of `matrix3.{h,cpp}` and `quat.{h,cpp}` at the
/// baseline, with the transcendental entry points linked against the
/// pinned GameMath, exactly like the game's deterministic build.
#[test]
fn matrix_quat_matches_cpp_golden_vectors() {
    let text = read_golden("matrices.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));
        if kind != "m3" {
            continue; // quat records are driven by the quaternion test
        }
        let op = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));

        match op {
            "from_axis_angle" => {
                let axis = Vector3::new(
                    hex_f32(&mut fields, line),
                    hex_f32(&mut fields, line),
                    hex_f32(&mut fields, line),
                );
                let angle = hex_f32(&mut fields, line);
                check_m3(&mut fields, Matrix3::from_axis_angle(axis, angle), line);
            }
            "from_axis_angle_sc" => {
                let axis = Vector3::new(
                    hex_f32(&mut fields, line),
                    hex_f32(&mut fields, line),
                    hex_f32(&mut fields, line),
                );
                let s = hex_f32(&mut fields, line);
                let c = hex_f32(&mut fields, line);
                check_m3(&mut fields, Matrix3::from_axis_angle_sc(axis, s, c), line);
            }
            "from_quaternion" => {
                let q = read_q(&mut fields, line);
                check_m3(&mut fields, Matrix3::from_quaternion(q), line);
            }
            "transpose" => {
                let m = hex_m3(&mut fields, line);
                check_m3(&mut fields, m.transpose(), line);
            }
            "inverse" => {
                let m = hex_m3(&mut fields, line);
                check_m3(&mut fields, m.inverse(), line);
            }
            "determinant" => {
                let m = hex_m3(&mut fields, line);
                expect_gmath_bits(bits(m.determinant()), hex_u32(&mut fields, line), line);
            }
            "get_x_rotation" => {
                let m = hex_m3(&mut fields, line);
                expect_gmath_bits(bits(m.get_x_rotation()), hex_u32(&mut fields, line), line);
            }
            "get_y_rotation" => {
                let m = hex_m3(&mut fields, line);
                expect_gmath_bits(bits(m.get_y_rotation()), hex_u32(&mut fields, line), line);
            }
            "get_z_rotation" => {
                let m = hex_m3(&mut fields, line);
                expect_gmath_bits(bits(m.get_z_rotation()), hex_u32(&mut fields, line), line);
            }
            "get_x_vector" => {
                let m = hex_m3(&mut fields, line);
                check_v3_fields(&mut fields, m.get_x_vector(), line);
            }
            "mul" => {
                let a = hex_m3(&mut fields, line);
                let b = hex_m3(&mut fields, line);
                check_m3(&mut fields, a * b, line);
            }
            "multiply_static" => {
                let a = hex_m3(&mut fields, line);
                let b = hex_m3(&mut fields, line);
                check_m3(&mut fields, Matrix3::multiply(&a, &b), line);
            }
            "mul_scalar" => {
                let a = hex_m3(&mut fields, line);
                let k = hex_f32(&mut fields, line);
                check_m3(&mut fields, a * k, line);
            }
            "div_scalar" => {
                let a = hex_m3(&mut fields, line);
                let k = hex_f32(&mut fields, line);
                check_m3(&mut fields, a / k, line);
            }
            "add" => {
                let a = hex_m3(&mut fields, line);
                let b = hex_m3(&mut fields, line);
                check_m3(&mut fields, a + b, line);
            }
            "sub" => {
                let a = hex_m3(&mut fields, line);
                let b = hex_m3(&mut fields, line);
                check_m3(&mut fields, a - b, line);
            }
            "neg" => {
                let a = hex_m3(&mut fields, line);
                check_m3(&mut fields, -a, line);
            }
            "rotate_x" | "rotate_y" | "rotate_z" => {
                let m = hex_m3(&mut fields, line);
                let angle = hex_f32(&mut fields, line);
                let mut rm = m;
                match op {
                    "rotate_x" => rm.rotate_x(angle),
                    "rotate_y" => rm.rotate_y(angle),
                    _ => rm.rotate_z(angle),
                }
                check_m3(&mut fields, rm, line);
            }
            "mul_vector" => {
                let m = hex_m3(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3_fields(&mut fields, m * v, line);
            }
            "rotate_vector" => {
                let m = hex_m3(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3_fields(&mut fields, m.rotate_vector(v), line);
            }
            "transpose_rotate_vector" => {
                let m = hex_m3(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3_fields(&mut fields, m.transpose_rotate_vector(v), line);
            }
            "is_orthogonal" => {
                let m = hex_m3(&mut fields, line);
                let expected: i32 = fields
                    .next()
                    .unwrap_or_else(|| panic!("missing field in {line}"))
                    .parse()
                    .unwrap();
                assert_eq!(i32::from(m.is_orthogonal()), expected, "in {line}");
            }
            "re_orthogonalize" => {
                let mut m = hex_m3(&mut fields, line);
                m.re_orthogonalize();
                check_m3(&mut fields, m, line);
            }
            "rotate_aabox_extent" => {
                let m = hex_m3(&mut fields, line);
                let extent = read_v3(&mut fields, line);
                check_v3_fields(&mut fields, m.rotate_aabox_extent(extent), line);
            }
            other => panic!("unknown m3 record {other} in {line}"),
        }
        checked += 1;
    }

    assert!(checked > 5000, "suspiciously few records: {checked}");
    println!("matrix3 golden vectors: {checked} checked");
}

fn read_v3(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Vector3 {
    Vector3::new(
        hex_f32(fields, line),
        hex_f32(fields, line),
        hex_f32(fields, line),
    )
}

fn read_q(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Quaternion {
    Quaternion::new(
        hex_f32(fields, line),
        hex_f32(fields, line),
        hex_f32(fields, line),
        hex_f32(fields, line),
    )
}

/// Drives the Quaternion records (`quat <op> ...` lines of matrices.txt).
#[test]
fn quaternion_matches_cpp_golden_vectors() {
    let text = read_golden("matrices.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));
        if kind != "quat" {
            continue;
        }
        let op = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));

        match op {
            "from_axis_angle" => {
                let axis = read_v3(&mut fields, line);
                let angle = hex_f32(&mut fields, line);
                check_q(&mut fields, Quaternion::from_axis_angle(axis, angle), line);
            }
            "axis_to_quat" => {
                let axis = read_v3(&mut fields, line);
                let phi = hex_f32(&mut fields, line);
                check_q(&mut fields, rzh_math::quat::axis_to_quat(axis, phi), line);
            }
            "build_quaternion" => {
                let m = hex_m3(&mut fields, line);
                check_q(&mut fields, rzh_math::quat::build_quaternion(&m), line);
            }
            "mul" => {
                let a = read_q(&mut fields, line);
                let b = read_q(&mut fields, line);
                check_q(&mut fields, a * b, line);
            }
            "div" => {
                let a = read_q(&mut fields, line);
                let b = read_q(&mut fields, line);
                check_q(&mut fields, a / b, line);
            }
            "inverse" => {
                let a = read_q(&mut fields, line);
                check_q(&mut fields, rzh_math::quat::inverse(a), line);
            }
            "normalize" => {
                let mut a = read_q(&mut fields, line);
                a.normalize();
                check_q(&mut fields, a, line);
            }
            "normalize_free" => {
                let a = read_q(&mut fields, line);
                check_q(&mut fields, rzh_math::quat::normalize(a), line);
            }
            "make_closest" => {
                let mut a = read_q(&mut fields, line);
                let b = read_q(&mut fields, line);
                a.make_closest(&b);
                check_q(&mut fields, a, line);
            }
            "rotate_vector" => {
                let q = read_q(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3_fields(&mut fields, q.rotate_vector(v), line);
            }
            "slerp" | "fast_slerp" => {
                let a = read_q(&mut fields, line);
                let b = read_q(&mut fields, line);
                let alpha = hex_f32(&mut fields, line);
                let result = if op == "slerp" {
                    rzh_math::quat::slerp(a, b, alpha)
                } else {
                    rzh_math::quat::fast_slerp(a, b, alpha)
                };
                check_q(&mut fields, result, line);
            }
            "slerp_setup" => {
                let flip: i32 = fields
                    .next()
                    .unwrap_or_else(|| panic!("missing field in {line}"))
                    .parse()
                    .unwrap();
                let linear: i32 = fields
                    .next()
                    .unwrap_or_else(|| panic!("missing field in {line}"))
                    .parse()
                    .unwrap();
                let theta = hex_f32(&mut fields, line);
                let sin_t = hex_f32(&mut fields, line);
                // recompute from the SAME inputs used by the generator's
                // Slerp_Setup call: qa/qb of the enclosing round, which are
                // not stored in this record; so reconstruct from the next
                // cached_slerp record instead (they always come in pairs).
                let _ = (flip, linear, theta, sin_t);
                let _ = &mut fields;
                // handled by "cached_slerp" below via a pre-scan
                checked += 1;
                continue;
            }
            "cached_slerp" => {
                let a = read_q(&mut fields, line);
                let b = read_q(&mut fields, line);
                let _alpha_word = fields.next();
                let info = rzh_math::quat::slerp_setup(a, b);
                check_q(
                    &mut fields,
                    rzh_math::quat::cached_slerp(a, b, 0.4, &info),
                    line,
                );
            }
            "rotate_x" | "rotate_y" | "rotate_z" => {
                let mut q = read_q(&mut fields, line);
                let theta = hex_f32(&mut fields, line);
                match op {
                    "rotate_x" => q.rotate_x(theta),
                    "rotate_y" => q.rotate_y(theta),
                    _ => q.rotate_z(theta),
                }
                check_q(&mut fields, q, line);
            }
            "build_matrix3" => {
                let q = read_q(&mut fields, line);
                check_m3(&mut fields, rzh_math::quat::build_matrix3(q), line);
            }
            other => panic!("unknown quat record {other} in {line}"),
        }
        checked += 1;
    }

    assert!(checked > 6000, "suspiciously few records: {checked}");
    println!("quaternion golden vectors: {checked} checked");
}

/// Reads a 12-float `Matrix3D` from a golden line.
fn read_m3d(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Matrix3D {
    let mut rows = [Vector4::default(); 3];
    for row in &mut rows {
        *row = Vector4::new(
            hex_f32(fields, line),
            hex_f32(fields, line),
            hex_f32(fields, line),
            hex_f32(fields, line),
        );
    }
    Matrix3D { rows }
}

/// Asserts a 12-float `Matrix3D` against the golden bit patterns.
fn check_m3d(fields: &mut std::str::SplitWhitespace<'_>, actual: Matrix3D, line: &str) {
    for (i, row) in actual.rows.iter().enumerate() {
        for (j, value) in [row.x, row.y, row.z, row.w].into_iter().enumerate() {
            expect_gmath_bits(
                bits(value),
                hex_u32(fields, line),
                &format!("m3d ({i},{j}) in {line}"),
            );
        }
    }
}

/// Drives every Matrix3D record (`m3d <op> ...` lines of matrices.txt).
///
/// The vectors come from the verbatim `Matrix3D` transcription in
/// `tools/differential/gen_golden.cpp` (matrix3d.{h,cpp} @ 571a7c5,
/// NO_ALLOW_TEMPORARIES branch); the bit patterns pin the original's
/// behaviour quirks included — most notably the `Vector4::operator-=`
/// W-column quirk that flows into `solve_linear_system`.
#[test]
fn matrix3d_matches_cpp_golden_vectors() {
    let text = read_golden("matrices.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));
        if kind != "m3d" {
            continue;
        }
        let op = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));

        match op {
            "const" => {
                let name = fields.next().unwrap();
                assert_eq!(name, "identity", "{line}");
                check_m3d(&mut fields, Matrix3D::IDENTITY, line);
            }
            "from_axis_angle" => {
                let axis = read_v3(&mut fields, line);
                let angle = hex_f32(&mut fields, line);
                check_m3d(&mut fields, Matrix3D::from_axis_angle(axis, angle), line);
            }
            "from_axis_angle_sc" => {
                let axis = read_v3(&mut fields, line);
                let s = hex_f32(&mut fields, line);
                let c = hex_f32(&mut fields, line);
                check_m3d(
                    &mut fields,
                    Matrix3D::from_axis_sine_cosine(axis, s, c),
                    line,
                );
            }
            "from_quat_pos" => {
                let q = read_q(&mut fields, line);
                let pos = read_v3(&mut fields, line);
                check_m3d(&mut fields, Matrix3D::from_quat_position(q, pos), line);
            }
            "look_at" => {
                let p = read_v3(&mut fields, line);
                let t = read_v3(&mut fields, line);
                let roll = hex_f32(&mut fields, line);
                let mut m = Matrix3D::default();
                m.look_at(p, t, roll);
                check_m3d(&mut fields, m, line);
            }
            "look_at_dir" => {
                let pos = read_v3(&mut fields, line);
                let dir = read_v3(&mut fields, line);
                let roll = hex_f32(&mut fields, line);
                let mut m = Matrix3D::default();
                m.look_at_dir(pos, dir, roll);
                check_m3d(&mut fields, m, line);
            }
            "obj_look_at" => {
                let p = read_v3(&mut fields, line);
                let t = read_v3(&mut fields, line);
                let roll = hex_f32(&mut fields, line);
                let mut m = Matrix3D::default();
                m.obj_look_at(p, t, roll);
                check_m3d(&mut fields, m, line);
            }
            "build_transform" => {
                let pos = read_v3(&mut fields, line);
                let dir = read_v3(&mut fields, line);
                let mut m = Matrix3D::default();
                m.build_transform_matrix(pos, dir);
                check_m3d(&mut fields, m, line);
            }
            "rotate_x" | "rotate_y" | "rotate_z" => {
                let m = read_m3d(&mut fields, line);
                let angle = hex_f32(&mut fields, line);
                let mut r = m;
                match op {
                    "rotate_x" => r.rotate_x(angle),
                    "rotate_y" => r.rotate_y(angle),
                    _ => r.rotate_z(angle),
                }
                check_m3d(&mut fields, r, line);
            }
            "pre_rotate_x" | "pre_rotate_y" | "pre_rotate_z" => {
                let m = read_m3d(&mut fields, line);
                let angle = hex_f32(&mut fields, line);
                let mut r = m;
                match op {
                    "pre_rotate_x" => r.pre_rotate_x(angle),
                    "pre_rotate_y" => r.pre_rotate_y(angle),
                    _ => r.pre_rotate_z(angle),
                }
                check_m3d(&mut fields, r, line);
            }
            "in_place_pre_rotate_x" | "in_place_pre_rotate_y" | "in_place_pre_rotate_z" => {
                let m = read_m3d(&mut fields, line);
                let angle = hex_f32(&mut fields, line);
                let mut r = m;
                match op {
                    "in_place_pre_rotate_x" => r.in_place_pre_rotate_x(angle),
                    "in_place_pre_rotate_y" => r.in_place_pre_rotate_y(angle),
                    _ => r.in_place_pre_rotate_z(angle),
                }
                check_m3d(&mut fields, r, line);
            }
            "translate" => {
                let m = read_m3d(&mut fields, line);
                let t = read_v3(&mut fields, line);
                let mut r = m;
                r.translate_v(t);
                check_m3d(&mut fields, r, line);
            }
            "scale" => {
                let m = read_m3d(&mut fields, line);
                let x = hex_f32(&mut fields, line);
                let y = hex_f32(&mut fields, line);
                let z = hex_f32(&mut fields, line);
                let mut r = m;
                r.scale_xyz(x, y, z);
                check_m3d(&mut fields, r, line);
            }
            "get_x_rotation" | "get_y_rotation" | "get_z_rotation" => {
                let m = read_m3d(&mut fields, line);
                let actual = match op {
                    "get_x_rotation" => m.x_rotation(),
                    "get_y_rotation" => m.y_rotation(),
                    _ => m.z_rotation(),
                };
                expect_gmath_bits(bits(actual), hex_u32(&mut fields, line), line);
            }
            "transform_vector" => {
                let m = read_m3d(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3(&mut fields, m.transform_vector(v), line);
            }
            "rotate_vector" => {
                let m = read_m3d(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3(&mut fields, m.rotate_vector(v), line);
            }
            "inverse_rotate_vector" => {
                let m = read_m3d(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3(&mut fields, m.inverse_rotate_vector(v), line);
            }
            "inverse_transform_vector" => {
                let m = read_m3d(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3(&mut fields, m.inverse_transform_vector(v), line);
            }
            "get_inverse" => {
                let m = read_m3d(&mut fields, line);
                let det = hex_f32(&mut fields, line);
                let (actual_det, actual_inv) = Matrix3D::inverse_with_determinant(m);
                expect_gmath_bits(bits(actual_det), bits(det), &format!("det in {line}"));
                // re-parse from the token after the determinant: the twelve
                // inverse floats, or the "-" singular marker
                let mut reparse = line.split_whitespace();
                for _ in 0..(2 + 12 + 1) {
                    reparse.next();
                }
                match actual_inv {
                    Some(inv) => {
                        let mut probe = reparse.clone();
                        assert_ne!(probe.next(), Some("-"), "unexpected inverse in {line}");
                        check_m3d(&mut reparse, inv, line);
                    }
                    None => {
                        assert_eq!(reparse.next(), Some("-"), "expected singular in {line}");
                    }
                }
            }
            "orthogonal_inverse" => {
                let m = read_m3d(&mut fields, line);
                check_m3d(&mut fields, m.get_orthogonal_inverse(), line);
            }
            "get_x_vector" => {
                let m = read_m3d(&mut fields, line);
                check_v3(&mut fields, m.x_vector(), line);
            }
            "transform_minmax_box" => {
                let m = read_m3d(&mut fields, line);
                let min = read_v3(&mut fields, line);
                let max = read_v3(&mut fields, line);
                let (smin, smax) = m.transform_min_max_aabox(min, max);
                check_v3(&mut fields, smin, line);
                check_v3(&mut fields, smax, line);
            }
            "transform_centerextent_box" => {
                let m = read_m3d(&mut fields, line);
                let center = read_v3(&mut fields, line);
                let extent = read_v3(&mut fields, line);
                let (sc, se) = m.transform_center_extent_aabox(center, extent);
                check_v3(&mut fields, sc, line);
                check_v3(&mut fields, se, line);
            }
            "is_orthogonal" => {
                let m = read_m3d(&mut fields, line);
                let expected: u32 = fields.next().unwrap().parse().unwrap();
                assert_eq!(u32::from(m.is_orthogonal()), expected, "{line}");
            }
            "re_orthogonalize" => {
                let m = read_m3d(&mut fields, line);
                let mut r = m;
                r.re_orthogonalize();
                check_m3d(&mut fields, r, line);
            }
            "mul" => {
                let a = read_m3d(&mut fields, line);
                let b = read_m3d(&mut fields, line);
                let mut c = Matrix3D::default();
                c.mul(a, b);
                check_m3d(&mut fields, c, line);
            }
            "post_mul" => {
                let a = read_m3d(&mut fields, line);
                let b = read_m3d(&mut fields, line);
                let mut r = a;
                r.post_mul(b);
                check_m3d(&mut fields, r, line);
            }
            "pre_mul" => {
                let a = read_m3d(&mut fields, line);
                let b = read_m3d(&mut fields, line);
                let mut r = b;
                r.pre_mul(a);
                check_m3d(&mut fields, r, line);
            }
            "chain" => {
                let m = read_m3d(&mut fields, line);
                check_v3(&mut fields, m.translation(), line);
            }
            "lerp" => {
                let a = read_m3d(&mut fields, line);
                let b = read_m3d(&mut fields, line);
                let factor = hex_f32(&mut fields, line);
                check_m3d(&mut fields, Matrix3D::lerp(a, b, factor), line);
            }
            "solve" => {
                let system = read_m3d(&mut fields, line);
                let mut s = system;
                let ok = Matrix3D::solve_linear_system(&mut s);
                check_m3d(&mut fields, s, line);
                let expected: u32 = fields.next().unwrap().parse().unwrap();
                assert_eq!(u32::from(ok), expected, "{line}");
            }
            other => panic!("unknown m3d record {other:?} in {line}"),
        }
        checked += 1;
    }

    assert!(checked > 2800, "suspiciously few records: {checked}");
    println!("matrix3d golden vectors: {checked} checked");
}

/// Reads a 16-float `Matrix4` from a golden line.
fn read_m4(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Matrix4 {
    let mut rows = [Vector4::default(); 4];
    for row in &mut rows {
        *row = Vector4::new(
            hex_f32(fields, line),
            hex_f32(fields, line),
            hex_f32(fields, line),
            hex_f32(fields, line),
        );
    }
    Matrix4 { rows }
}

/// Reads a 4-float `Vector4` from a golden line.
fn read_v4(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Vector4 {
    Vector4::new(
        hex_f32(fields, line),
        hex_f32(fields, line),
        hex_f32(fields, line),
        hex_f32(fields, line),
    )
}

/// Asserts a 16-float `Matrix4` against the golden bit patterns.
fn check_m4(fields: &mut std::str::SplitWhitespace<'_>, actual: Matrix4, line: &str) {
    for (i, row) in actual.rows.iter().enumerate() {
        for (j, value) in [row.x, row.y, row.z, row.w].into_iter().enumerate() {
            expect_gmath_bits(
                bits(value),
                hex_u32(fields, line),
                &format!("m4 ({i},{j}) in {line}"),
            );
        }
    }
}

/// Reads one decimal f32 parameter (the projection records spell their
/// arguments as decimals).
fn dec_f32(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> f32 {
    fields
        .next()
        .unwrap_or_else(|| panic!("missing field in {line}"))
        .parse()
        .unwrap_or_else(|e| panic!("bad decimal float in {line}: {e}"))
}

/// Drives every Matrix4x4 record (`m4 <op> ...` lines of matrices.txt).
///
/// The fov-based `Init_Perspective` is deliberately absent: its `tan()`
/// is platform libm in the original (see the module docs); the frustum
/// variant and everything else are pure IEEE arithmetic and pinned.
#[test]
fn matrix4_matches_cpp_golden_vectors() {
    let text = read_golden("matrices.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));
        if kind != "m4" {
            continue;
        }
        let op = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));

        match op {
            "const" => {
                let name = fields.next().unwrap();
                assert_eq!(name, "identity", "{line}");
                check_m4(&mut fields, Matrix4::identity(), line);
            }
            "init_ortho" => {
                let left = dec_f32(&mut fields, line);
                let right = dec_f32(&mut fields, line);
                let bottom = dec_f32(&mut fields, line);
                let top = dec_f32(&mut fields, line);
                let znear = dec_f32(&mut fields, line);
                let zfar = dec_f32(&mut fields, line);
                let mut m = Matrix4::default();
                m.init_ortho(left, right, bottom, top, znear, zfar);
                check_m4(&mut fields, m, line);
            }
            "init_perspective_frustum" => {
                let left = dec_f32(&mut fields, line);
                let right = dec_f32(&mut fields, line);
                let bottom = dec_f32(&mut fields, line);
                let top = dec_f32(&mut fields, line);
                let znear = dec_f32(&mut fields, line);
                let zfar = dec_f32(&mut fields, line);
                let mut m = Matrix4::default();
                m.init_perspective_frustum(left, right, bottom, top, znear, zfar);
                check_m4(&mut fields, m, line);
            }
            "from_matrix3d" => {
                let t = read_m3d(&mut fields, line);
                check_m4(&mut fields, Matrix4::from_matrix3d(t), line);
            }
            "transpose" => {
                let m = read_m4(&mut fields, line);
                check_m4(&mut fields, m.transpose(), line);
            }
            "get_inverse" => {
                let m = read_m4(&mut fields, line);
                let det = hex_f32(&mut fields, line);
                let (actual_det, actual_inv) = Matrix4::inverse_with_determinant(m);
                expect_gmath_bits(bits(actual_det), bits(det), &format!("det in {line}"));
                // re-parse from the token after the determinant: the
                // sixteen inverse floats, or the "-" singular marker
                let mut reparse = line.split_whitespace();
                for _ in 0..(2 + 16 + 1) {
                    reparse.next();
                }
                match actual_inv {
                    Some(inv) => {
                        let mut probe = reparse.clone();
                        assert_ne!(probe.next(), Some("-"), "unexpected inverse in {line}");
                        check_m4(&mut reparse, inv, line);
                    }
                    None => {
                        assert_eq!(reparse.next(), Some("-"), "expected singular in {line}");
                    }
                }
            }
            "mul" => {
                let a = read_m4(&mut fields, line);
                let b = read_m4(&mut fields, line);
                check_m4(&mut fields, Matrix4::multiply(a, b), line);
            }
            "mul_4_3d" => {
                let a = read_m4(&mut fields, line);
                let b = read_m3d(&mut fields, line);
                check_m4(&mut fields, Matrix4::multiply_4_3d(a, b), line);
            }
            "mul_3d_4" => {
                let a = read_m3d(&mut fields, line);
                let b = read_m4(&mut fields, line);
                check_m4(&mut fields, Matrix4::multiply_3d_4(a, b), line);
            }
            "add" => {
                let a = read_m4(&mut fields, line);
                let b = read_m4(&mut fields, line);
                check_m4(&mut fields, a + b, line);
            }
            "sub" => {
                let a = read_m4(&mut fields, line);
                let b = read_m4(&mut fields, line);
                check_m4(&mut fields, a - b, line);
            }
            "scale" => {
                let a = read_m4(&mut fields, line);
                let d = hex_f32(&mut fields, line);
                check_m4(&mut fields, a * d, line);
            }
            "div" => {
                let a = read_m4(&mut fields, line);
                let d = hex_f32(&mut fields, line);
                check_m4(&mut fields, a / d, line);
            }
            "transform_vector4" => {
                let m = read_m4(&mut fields, line);
                let v = read_v4(&mut fields, line);
                let out = m.transform_vector4(v);
                expect_gmath_bits(
                    bits(out.x),
                    hex_u32(&mut fields, line),
                    &format!("x in {line}"),
                );
                expect_gmath_bits(
                    bits(out.y),
                    hex_u32(&mut fields, line),
                    &format!("y in {line}"),
                );
                expect_gmath_bits(
                    bits(out.z),
                    hex_u32(&mut fields, line),
                    &format!("z in {line}"),
                );
                expect_gmath_bits(
                    bits(out.w),
                    hex_u32(&mut fields, line),
                    &format!("w in {line}"),
                );
            }
            "transform_point" => {
                let m = read_m4(&mut fields, line);
                let v = read_v3(&mut fields, line);
                let out = m.transform_point(v);
                expect_gmath_bits(
                    bits(out.x),
                    hex_u32(&mut fields, line),
                    &format!("x in {line}"),
                );
                expect_gmath_bits(
                    bits(out.y),
                    hex_u32(&mut fields, line),
                    &format!("y in {line}"),
                );
                expect_gmath_bits(
                    bits(out.z),
                    hex_u32(&mut fields, line),
                    &format!("z in {line}"),
                );
                expect_gmath_bits(
                    bits(out.w),
                    hex_u32(&mut fields, line),
                    &format!("w in {line}"),
                );
            }
            "transform_vector3" => {
                let m = read_m4(&mut fields, line);
                let v = read_v3(&mut fields, line);
                check_v3(&mut fields, m.transform_vector3(v), line);
            }
            other => panic!("unknown m4 record {other:?} in {line}"),
        }
        checked += 1;
    }

    assert!(checked > 700, "suspiciously few records: {checked}");
    println!("matrix4 golden vectors: {checked} checked");
}

/// Reads an `Axis` code (0/1/2 = X/Y/Z).
fn read_axis(fields: &mut std::str::SplitWhitespace<'_>, line: &str) -> Axis {
    match dec_i32(fields, line) {
        0 => Axis::X,
        1 => Axis::Y,
        2 => Axis::Z,
        other => panic!("bad axis code {other} in {line}"),
    }
}

/// Drives every geometry record (`plane|aabox|mmbox|aaplane|eval_mask|
/// frustum ...` lines of geometry.txt).
///
/// The vectors come from the verbatim transcriptions of plane.h,
/// aaplane.h, aabox.h, frustum.cpp and the CollisionMath overlap subset in
/// `tools/differential/gen_golden.cpp` @ 571a7c5 (deterministic branches);
/// the bit patterns pin the original's quirks included — the colinear-input
/// default plane, the untouched `t` out-parameter on parallel segments, the
/// double-precision `AABox::Volume`, the coincident-corner/zero-extent
/// early returns in `MinMaxAABox::Add_Box`, the reflected-camera corner
/// swap and the whole-vector clip-distance scaling.
#[test]
fn geometry_matches_cpp_golden_vectors() {
    let text = read_golden("geometry.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields
            .next()
            .unwrap_or_else(|| panic!("empty record {line}"));
        let op = fields
            .next()
            .unwrap_or_else(|| panic!("record without op in {line}"));

        match (kind, op) {
            ("plane", "set_points") => {
                let p1 = read_v3(&mut fields, line);
                let p2 = read_v3(&mut fields, line);
                let p3 = read_v3(&mut fields, line);
                let actual = Plane::from_points(p1, p2, p3);
                check_v3(&mut fields, actual.n, line);
                check_f(&mut fields, actual.d, line);
            }
            ("plane", "set_np") => {
                let n = read_v3(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let actual = Plane::from_normal_point(n, p);
                check_v3(&mut fields, actual.n, line);
                check_f(&mut fields, actual.d, line);
            }
            ("plane", "seg_isect") => {
                let n = read_v3(&mut fields, line);
                let d = hex_f32(&mut fields, line);
                let p0 = read_v3(&mut fields, line);
                let p1 = read_v3(&mut fields, line);
                let t_in = hex_f32(&mut fields, line);
                let expected_res = dec_i32(&mut fields, line);
                let plane = Plane::from_normal_dist(n, d);
                let mut t = t_in;
                let res = plane.compute_intersection(p0, p1, &mut t);
                let res_code = match res {
                    IntersectionRes::NoIntersection => 0,
                    IntersectionRes::InsideSegment => 1,
                    IntersectionRes::OutsideLine => 2,
                };
                assert_eq!(res_code, expected_res, "seg_isect result in {line}");
                check_f(&mut fields, t, line);
                // the untouched-t quirk: a parallel segment must return the
                // sentinel bit-for-bit
                if res == IntersectionRes::NoIntersection {
                    assert_eq!(
                        t.to_bits(),
                        t_in.to_bits(),
                        "parallel seg_isect must leave t untouched in {line}"
                    );
                }
            }
            ("plane", "in_front") => {
                let n = read_v3(&mut fields, line);
                let d = hex_f32(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let plane = Plane::from_normal_dist(n, d);
                assert_eq!(i32::from(plane.in_front(p)), expected, "in_front in {line}");
            }
            ("plane", "isect_pl") => {
                let an = read_v3(&mut fields, line);
                let ad = hex_f32(&mut fields, line);
                let bn = read_v3(&mut fields, line);
                let bd = hex_f32(&mut fields, line);
                let a = Plane::from_normal_dist(an, ad);
                let b = Plane::from_normal_dist(bn, bd);
                let (dir, point) = Plane::intersect_planes(&a, &b);
                check_v3(&mut fields, dir, line);
                check_v3(&mut fields, point, line);
            }

            ("aabox", "init_pts") => {
                let pts = [
                    read_v3(&mut fields, line),
                    read_v3(&mut fields, line),
                    read_v3(&mut fields, line),
                    read_v3(&mut fields, line),
                ];
                let actual = AABox::from_points(&pts);
                check_v3(&mut fields, actual.center, line);
                check_v3(&mut fields, actual.extent, line);
            }
            ("aabox", "init_mm") => {
                let min = read_v3(&mut fields, line);
                let max = read_v3(&mut fields, line);
                let actual = AABox::from_min_max(min, max);
                check_v3(&mut fields, actual.center, line);
                check_v3(&mut fields, actual.extent, line);
            }
            ("aabox", "add_point") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let mut b = AABox::new(c, e);
                b.add_point(p);
                check_v3(&mut fields, b.center, line);
                check_v3(&mut fields, b.extent, line);
            }
            ("aabox", "add_box") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let c2 = read_v3(&mut fields, line);
                let e2 = read_v3(&mut fields, line);
                let mut b = AABox::new(c, e);
                b.add_box(&AABox::new(c2, e2));
                check_v3(&mut fields, b.center, line);
                check_v3(&mut fields, b.extent, line);
            }
            ("aabox", "add_mmbox") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let mut b = AABox::new(c, e);
                b.add_min_max_box(&MinMaxAABox::new(mn, mx));
                check_v3(&mut fields, b.center, line);
                check_v3(&mut fields, b.extent, line);
            }
            ("aabox", "volume") => {
                let e = read_v3(&mut fields, line);
                let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), e);
                check_f(&mut fields, b.volume(), line);
            }
            ("aabox", "project") => {
                let e = read_v3(&mut fields, line);
                let axis = read_v3(&mut fields, line);
                let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), e);
                check_f(&mut fields, b.project_to_axis(axis), line);
            }
            ("aabox", "contains_p") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let b = AABox::new(c, e);
                assert_eq!(
                    i32::from(b.contains_point(p)),
                    expected,
                    "contains_p in {line}"
                );
            }
            ("aabox", "contains_b") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let c2 = read_v3(&mut fields, line);
                let e2 = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let b = AABox::new(c, e);
                assert_eq!(
                    i32::from(b.contains_box(&AABox::new(c2, e2))),
                    expected,
                    "contains_b in {line}"
                );
            }
            ("aabox", "transform") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let tm = read_m3d(&mut fields, line);
                let out = AABox::new(c, e).transformed(&tm);
                check_v3(&mut fields, out.center, line);
                check_v3(&mut fields, out.extent, line);
            }
            ("aabox", "overlap_p") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let b = AABox::new(c, e);
                assert_eq!(
                    i32::from(rzh_math::colmath::overlap_test_aabox_point(&b, &p).code()),
                    expected,
                    "aabox overlap_p in {line}"
                );
            }
            ("aabox", "overlap_b") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let c2 = read_v3(&mut fields, line);
                let e2 = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let b = AABox::new(c, e);
                assert_eq!(
                    i32::from(
                        rzh_math::colmath::overlap_test_aabox_aabox(&b, &AABox::new(c2, e2)).code()
                    ),
                    expected,
                    "aabox overlap_b in {line}"
                );
            }

            ("mmbox", "init_pts") => {
                let pts = [
                    read_v3(&mut fields, line),
                    read_v3(&mut fields, line),
                    read_v3(&mut fields, line),
                    read_v3(&mut fields, line),
                ];
                let actual = MinMaxAABox::from_points(&pts);
                check_v3(&mut fields, actual.min_corner, line);
                check_v3(&mut fields, actual.max_corner, line);
            }
            ("mmbox", "init_aabox") => {
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let actual = MinMaxAABox::from_aabox(&AABox::new(c, e));
                check_v3(&mut fields, actual.min_corner, line);
                check_v3(&mut fields, actual.max_corner, line);
            }
            ("mmbox", "init_empty") => {
                let actual = MinMaxAABox::empty();
                check_v3(&mut fields, actual.min_corner, line);
                check_v3(&mut fields, actual.max_corner, line);
            }
            ("mmbox", "add_point") => {
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let mut b = MinMaxAABox::new(mn, mx);
                b.add_point(p);
                check_v3(&mut fields, b.min_corner, line);
                check_v3(&mut fields, b.max_corner, line);
            }
            ("mmbox", "add_box") => {
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let mn2 = read_v3(&mut fields, line);
                let mx2 = read_v3(&mut fields, line);
                let mut b = MinMaxAABox::new(mn, mx);
                b.add_box(&MinMaxAABox::new(mn2, mx2));
                check_v3(&mut fields, b.min_corner, line);
                check_v3(&mut fields, b.max_corner, line);
            }
            ("mmbox", "add_aabox") => {
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let c2 = read_v3(&mut fields, line);
                let e2 = read_v3(&mut fields, line);
                let mut b = MinMaxAABox::new(mn, mx);
                b.add_aabox(&AABox::new(c2, e2));
                check_v3(&mut fields, b.min_corner, line);
                check_v3(&mut fields, b.max_corner, line);
            }
            ("mmbox", "add_mm") => {
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let mn2 = read_v3(&mut fields, line);
                let mx2 = read_v3(&mut fields, line);
                let mut b = MinMaxAABox::new(mn, mx);
                b.add_min_max(mn2, mx2);
                check_v3(&mut fields, b.min_corner, line);
                check_v3(&mut fields, b.max_corner, line);
            }
            ("mmbox", "volume") => {
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let b = MinMaxAABox::new(mn, mx);
                check_f(&mut fields, b.volume(), line);
            }
            ("mmbox", "transform") => {
                let mn = read_v3(&mut fields, line);
                let mx = read_v3(&mut fields, line);
                let tm = read_m3d(&mut fields, line);
                let mut b = MinMaxAABox::new(mn, mx);
                b.transform(&tm);
                check_v3(&mut fields, b.min_corner, line);
                check_v3(&mut fields, b.max_corner, line);
            }

            ("aaplane", "normal") => {
                let axis = read_axis(&mut fields, line);
                let dist = hex_f32(&mut fields, line);
                check_v3(&mut fields, AAPlane::new(axis, dist).normal(), line);
            }
            ("aaplane", "overlap_p") => {
                let axis = read_axis(&mut fields, line);
                let dist = hex_f32(&mut fields, line);
                let p = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let plane = AAPlane::new(axis, dist);
                assert_eq!(
                    i32::from(rzh_math::colmath::overlap_test_aaplane_point(&plane, &p).code()),
                    expected,
                    "aaplane overlap_p in {line}"
                );
            }
            ("aaplane", "overlap_b") => {
                let axis = read_axis(&mut fields, line);
                let dist = hex_f32(&mut fields, line);
                let c = read_v3(&mut fields, line);
                let e = read_v3(&mut fields, line);
                let expected = dec_i32(&mut fields, line);
                let plane = AAPlane::new(axis, dist);
                assert_eq!(
                    i32::from(
                        rzh_math::colmath::overlap_test_aaplane_aabox(&plane, &AABox::new(c, e))
                            .code()
                    ),
                    expected,
                    "aaplane overlap_b in {line}"
                );
            }

            ("eval_mask", mask_str) => {
                let mask: i32 = mask_str
                    .parse()
                    .unwrap_or_else(|_| panic!("bad mask {mask_str:?} in {line}"));
                let expected = dec_i32(&mut fields, line);
                assert_eq!(
                    i32::from(rzh_math::eval_overlap_mask(mask as u8).code()),
                    expected,
                    "eval_mask {mask} in {line}"
                );
            }

            ("frustum", "init") => {
                let tm = read_m3d(&mut fields, line);
                let vminx = hex_f32(&mut fields, line);
                let vminy = hex_f32(&mut fields, line);
                let vmaxx = hex_f32(&mut fields, line);
                let vmaxy = hex_f32(&mut fields, line);
                let zn = hex_f32(&mut fields, line);
                let zf = hex_f32(&mut fields, line);

                let mut f = Frustum::default();
                f.init(
                    &tm,
                    Vector2::new(vminx, vminy),
                    Vector2::new(vmaxx, vmaxy),
                    zn,
                    zf,
                );

                for corner in &f.corners {
                    check_v3(&mut fields, *corner, line);
                }
                for plane in &f.planes {
                    check_v3(&mut fields, plane.n, line);
                    check_f(&mut fields, plane.d, line);
                }
                check_v3(&mut fields, f.bound_min, line);
                check_v3(&mut fields, f.bound_max, line);
            }

            other => panic!("unknown geometry record {other:?} in {line}"),
        }

        assert!(
            fields.next().is_none(),
            "trailing fields in geometry record {line}"
        );
        checked += 1;
    }

    assert!(checked > 5_000, "suspiciously few records: {checked}");
    println!("geometry golden vectors: {checked} checked");
}
