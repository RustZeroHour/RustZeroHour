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
