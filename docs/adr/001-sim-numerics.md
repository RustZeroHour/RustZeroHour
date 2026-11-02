# ADR-001: Simulation numerics — f32, msun-exact entry points, no libm crate for parity code

- **Status:** Accepted
- **Date:** 2026-11-02
- **Supersedes:** none
- **Superseded by:** none

## Context

The simulation must produce bit-identical `f32` results on every client
(Windows/Linux/macOS) and, until the cut-over milestone M5, also match the
C++ oracle (plan §3.2 parity levels P0–P2).

The C++ side runs with `USE_DETERMINISTIC_MATH = 1` (the retail-compatible
build disables it; ADR-000 dropped retail compatibility, so the port follows
the deterministic path). Under that switch, `WWMath`'s float entry points
call the **GameMath** library — the FreeBSD **msun** float routines, vendored
at pin `59f7ccd4` via `cmake/gamemath.cmake` — instead of the platform libm.

The migration plan §3.3 prescribed "transcendental functions from the
pure-Rust `libm` crate, not the platform libm", on the assumption that the
`libm` crate (itself an msun descendant) matches the game's GameMath pin.
**That assumption is false.** An empirical comparison (40,275 inputs:
random bit patterns, game-range floats, and values near multiples of π/2,
run against the pinned GameMath sources compiled from
`TheSuperHackers/GameMath` at the exact pin used by the build) shows:

| Function | `libm 0.2` vs GameMath pin | Note |
|----------|---------------------------|------|
| `sqrtf`, `fabsf` | **0 mismatches** | both correctly rounded / bit-clearing |
| `floorf`, `ceilf` | 0 semantic mismatches | NaN-payload differences only |
| `sinf` | 814 mismatches | newer `libm` uses rewritten algorithms |
| `cosf` | 835 mismatches | idem |
| `tanf` | 10,282 mismatches | incl. sign of `tanf(-0.0)` |
| `asinf` | 126 mismatches | |
| `acosf` | 7,959 mismatches | incl. `acosf(0)` ≠ correctly rounded π/2 |
| `atanf` | 7,940 mismatches | |
| `expf`, `logf` | ~40 each | NaN-payload differences only |

So `libm` is a fine *deterministic* choice (same bits on every platform),
but it is **not** bit-compatible with the C++ oracle for trigonometry.

## Options

| Option | Summary | Trade-offs |
|--------|---------|------------|
| A | Use the `libm` crate for everything transcendental, as plan §3.3 prescribed | One dependency, deterministic across platforms; 1-ULP-class divergence from the C++ oracle in trig — poison for P0 trace comparison and for replaying the recorded scenario library |
| B | Vendor GameMath as C and bind via FFI | Bit-exact with the oracle; reintroduces a C toolchain into the Rust build, contradicts "FFI is a test tool, not an architecture" (plan §3.1) |
| C | Port the needed GameMath (msun) float functions to Rust as a module of `rzh-math`, bit-exactly, as their consumers are ported; use std `f32` methods where IEEE-754 exactness makes them provably identical | Pure Rust, `#![forbid(unsafe_code)]` holds, bit-exact with the oracle; a bounded porting effort (~70 small functions exist, ~15 are actually used by the game) |
| D | Fixed-point (Q16.16) everywhere | Removes float portability questions entirely; a rewrite of every algorithm and a permanent divergence from the original's behaviour — the plan kept this as a fallback only |

## Decision

**Option C, with the following split:**

1. **IEEE-754 exact operations map to Rust std** and are used freely:
   `+ - * /`, `sqrt` (`f32::sqrt`), `fabs` (`f32::abs`), `floor`, `ceil`,
   and the bit-level conversions. All are correctly rounded (or sign/bit
   operations) by specification, so Rust std, msun and the system libm
   agree bit-for-bit — verified empirically against the pinned GameMath
   (0 mismatches over 40K+ inputs; reproduced by
   `tools/differential/gen_golden.cpp` golden vectors). `rzh-math`'s first
   slice (constants, scalar helpers, Vector2/3/4) is built on these and is
   **P0-verified** by committed golden vectors.

2. **The transcendental entry points (`sin`, `cos`, `tan`, `asin`, `acos`,
   `atan`, `atan2`, `pow`, `exp`, `log`, the `Fast_*` tables, and
   `Float_To_Long` which needs msun `lrintf`) are not ported yet and must
   not be backed by the `libm` crate.** They will be ported as a
   `rzh-math::gmath` module — a direct transcription of the pinned GameMath
   float sources — before the first code that needs them lands
   (matrix rotations, locomotor turning, `Normalize_Angle`-style code that
   consumes trig). Until then, angle-taking APIs (e.g. `Vector2::rotate`,
   `Vector3::rotate_x/y/z`) only exist in their precomputed
   `(s, c)` forms.

3. `f32` everywhere in sim-bound code, original operation order, no
   fast-math, no FMA contraction (the differential harness compiles with
   `-ffp-contract=off` to match), explicit wrapping-vs-checked semantics
   per integer site — as plan §3.3 already requires.

The `libm` crate remains allowed in **non-parity** contexts (tools,
format-probing, client-side estimation code) where determinism across Rust
platforms is the only requirement.

## Consequences

- Golden-vector P0 verification now covers everything in `rzh-util` and the
  first `rzh-math` slice; the harness regenerates them from a verbatim C++
  transcription (`tools/differential/gen_golden.cpp`).
- Porting order gains a constraint: the `gmath` module must land before
  matrices, quaternions, and any sim code that consumes angles.
- The plan's §3.3 wording "transcendental functions from the pure-Rust
  `libm` crate" is amended by this ADR to "from the `rzh-math::gmath` port
  of the pinned GameMath (msun) sources".
- Cross-platform float determinism is unchanged: every machine runs the
  same Rust code, and `sqrt`/`fabs`/`floor`/`ceil` are exact by
  specification, so no platform can diverge.
- If a future msun update is pulled into the C++ oracle, the `gmath` port
  and its golden vectors must be regenerated together with the pin bump.

## Addendum (2026-11-02): the `gmath` port has landed

`rzh-math::gmath` now exists: a line-for-line Rust transcription of the
pinned msun float sources (the 17 entry points the game reaches, plus the
kernels: `k_sinf/cosf/tanf`, `e_rem_pio2f`+`k_rem_pio2f`, `k_log1pf`,
`k_expf`'s `__frexp_expf`/`__ldexp_expf`, `s_expm1f`, `s_scalbnf`,
`s_rintf`/`s_lrintf`). Verification is stronger than the original plan:
the golden vectors (`crates/rzh-math/tests/golden/gmath.txt`, 78,187
records) are generated by **compiling the pinned C sources themselves**
(`tools/differential/gen_gmath_golden.c`, wired into
`tools/differential/regenerate.sh`), not by a re-transcription — so the
comparison is port-vs-original with no second error surface. All vectors
match bit-for-bit in debug and release builds.

Three findings from the verification are now recorded as decisions:

1. **NaN payloads are exempt from P0 bit-exactness.** The pin's gcc build
   reassociates `(x+0.0f)+(y+0.0f)` in `powf`'s NaN return into an operand
   order that propagates the *second* operand's NaN; MSVC and arm64 make
   different, equally legal choices (C leaves NaN payloads unspecified).
   The golden test therefore compares NaN results by "both are NaN" and
   everything else bit-for-bit. The game never inspects payloads
   (`Is_Valid_Float` is an isnan test), so this cannot cause divergence in
   practice; the trace format (§3.2) should normalize NaNs the same way.
2. **`gm_lrintf`'s out-of-range result is the "integer indefinite"
   value.** The pin's intrinsic path (`cvtss2si`/`fcvt`) returns
   `INT_MIN`/`i64::MIN` for NaN and out-of-range inputs where the generic
   C path is undefined. The port replicates the intrinsic semantics
   explicitly (Rust's saturating `as` casts differ, e.g. NaN casts to 0).
   `scalar::float_to_long` returns `i32` matching the game's MSVC `long`;
   `gmath::lrintf` returns `i64` for LP64 platforms. In-range values agree
   on every platform (pinned by goldens).
3. **The msun hex-float and long-decimal literals must be copied exactly.**
   Abbreviating them (e.g. `3.3333334327e-01` to `0.33333343`) silently
   rounds some coefficients to neighbouring floats; the audit that found
   this is folded into the transcription notes of each module. Two of the
   pin's own hex comments are stale (`s_atanf.c`'s `aT[0]`, `aT[3]`); the
   compiled library follows the decimal literals, and so does the port.

Still deferred from WWMath alongside this module: the `Fast_*` lookup
tables (needed by `Quaternion::slerp`; they arrive with the quaternion
port) and the double-precision `gm_*` entry points (tools-only surface).
