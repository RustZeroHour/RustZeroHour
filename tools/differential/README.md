# Differential testing harness

This directory holds the temporary C++ **reference transcriptions** used to
generate the golden vectors that prove parity between the Rust port and the
original Zero Hour algorithms (migration plan §3.1 stage A and §3.2 parity
level **P0**: byte-exact output on identical input).

## Files

| File | Purpose |
|------|---------|
| `gen_golden.cpp` | Verbatim transcription of the baseline CRC (`Common/crc.h` / `crc.cpp`), RNG (`Common/RandomValue.cpp`, non-retail branches), name-key interning (`NameKeyGenerator.{h,cpp}`) and the IEEE-754 exact WWMath subset (`wwmath.h`, `vector{2,3,4}.h`, `vector2i.h`, `Vector3i.h`, deterministic branches) at commit `571a7c5`. Emits the golden vector files. |
| `regenerate.sh` | Compiles the harness and regenerates `crates/rzh-util/tests/golden/{crc,rng,namekey}.txt` and `crates/rzh-math/tests/golden/{scalar,vectors}.txt`. |

## Numerics

The WWMath transcription covers only operations that are IEEE-754 exact on
every conforming platform (add/sub/mul/div, `sqrt`, `fabs`, `floor`,
`ceil`, and integer bit work), where GameMath (the game's pinned FreeBSD
msun float library), the system libm and Rust's `f32` methods produce
bit-identical results — verified empirically against the pinned GameMath
sources (see ADR-001). The harness is compiled with `-ffp-contract=off`
so `a*b+c` never fuses into an FMA (Rust never contracts; neither does the
game's MSVC build).

The transcendental entry points (`Sin`…`Atan2`, `Pow`, `Exp`, `Log`,
`Fast_*`) are deliberately absent: the generic Rust `libm` crate is **not**
bit-identical to the game's GameMath pin for them, and the dedicated
`rzh-math::gmath` port (ADR-001) will bring its own transcription when it
lands.

Several quirks of the original headers are transcribed verbatim and pinned
by the vectors (they are behaviour): `Vector3::Quick_Length`'s `min = mid`
sort slip, `Vector4`'s compound subtraction adding W, the out-parameter
`Vector4::Lerp` writing the W interpolation into X, `Vector2`'s free
`Normalize` dividing by the reciprocal square root, and `Quick_Distance`
discarding its absolute values.

## Workflow

```sh
tools/differential/regenerate.sh          # regenerate the golden files
cargo test --workspace                    # Rust must match byte-for-byte
git diff crates/*/tests/golden            # review before committing
```

Golden files are committed; never edit them by hand. If a test fails, either
the Rust port or the C++ transcription drifted from the original: compare
against the real sources at the baseline tag `legacy-cpp-baseline` and fix
whichever side is wrong.

## Lifetime

This harness exists only while the C++ tree does. It is the minimal seed of
the oracle idea from migration plan §3.2 — the full oracle (headless replay
simulator with trace instrumentation) supersedes it in Phase 0/1, and both
are removed at the cut-over milestone M5 when Rust-vs-Rust golden traces take
over.
