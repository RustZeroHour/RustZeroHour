# Differential testing harness

This directory holds the temporary C++ **reference transcriptions** used to
generate the golden vectors that prove parity between the Rust port and the
original Zero Hour algorithms (migration plan §3.1 stage A and §3.2 parity
level **P0**: byte-exact output on identical input).

## Files

| File | Purpose |
|------|---------|
| `gen_golden.cpp` | Verbatim transcription of the baseline CRC (`Common/crc.h` / `crc.cpp`) and RNG (`Common/RandomValue.cpp`, non-retail branches) at commit `571a7c5`. Emits the golden vector files. |
| `regenerate.sh` | Compiles the harness and regenerates `crates/rzh-util/tests/golden/{crc,rng}.txt`. |

## Workflow

```sh
tools/differential/regenerate.sh     # regenerate the golden files
cargo test -p rzh-util               # Rust must match them byte-for-byte
git diff crates/rzh-util/tests/golden  # review before committing
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
