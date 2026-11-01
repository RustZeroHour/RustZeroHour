# How to contribute as a developer

Thank you for contributing to RustZeroHour. This fork is executing the
[Rust migration plan](docs/RUST_MIGRATION_PLAN.md); contributions that move
the plan forward — ports, tests, tooling, documentation — are all welcome.

## Where to start

The plan's §9 lists the first tasks in order, and `docs/DEVELOPMENT.md`
tracks the current status of the groundwork. Good first contributions:

- Port a small, well-bounded unit from the C++ baseline **with** its
  differential or golden test (see the porting rules below).
- Improve the inventory tooling or the differential harness.
- Write documentation that future porters will need.

Larger ideas (new subsystems, format changes, dependency additions) go
through the [ADR process](docs/adr/README.md) first, exactly as the plan
prescribes.

## Contribution guidelines

Prefer fewer high-quality pull requests over many low-quality ones. Keep
pull requests focused on one goal; split refactors from behaviour changes
into separate commits. We generally rebase and merge.

Every pull request must leave the repository green:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs these on Linux, Windows and macOS, plus a license/advisory check
(`cargo-deny`). **Never commit EA-owned assets** — game data is read from a
user-owned install via `RZH_GAME_DIR` (see
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)).

## AI contribution policy

Read the [AI Contribution Policy](AI_POLICY.md) before submitting. In short:
AI-assisted development is this project's stated methodology, but every
contribution must name its C++ origin, prove its behaviour with tests, and be
understood and reviewed by a human before it merges.

## Code guidelines (Rust)

- **Follow the plan's porting rules** (§3.3 of the
  [migration plan](docs/RUST_MIGRATION_PLAN.md)). The essentials:
  - Port the Zero Hour branch and the non-retail branch of each
    `RETAIL_COMPATIBLE_*` switch; drop dead `#if` code.
  - Record provenance in every ported file
    (`Ported from <path> @ 571a7c5`) and preserve the original copyright
    notices.
  - Preserve algorithms, data semantics and **order of operations** first;
    improve structure second.
  - Sim-bound crates use `#![forbid(unsafe_code)]`; `unsafe` is only for
    platform/FFI crates, each block carrying a `// SAFETY:` comment.
  - No global mutable state; state lives in explicit contexts.
  - Determinism rules apply from day one: no wall-clock or OS randomness in
    sim code, no `HashMap`/`HashSet` iteration in the sim, `f32` with the
    original operation order, fixed-width integers wherever values are
    hashed or serialized, objects referenced by handle/ID, never pointers.
- **Every ported unit ships tests**: Rust unit tests plus a differential or
  golden test that states its parity level (P0–P3, plan §3.2). Golden files
  are generated, never hand-edited.
- **Document the public API.** `missing_docs` is enabled workspace-wide;
  crate-level docs should explain provenance and parity level.
- **Dependencies** are deliberate: discuss additions in an ADR; the workspace
  starts dependency-free by design, and `deny.toml` enforces license
  compatibility.

## Code guidelines (legacy C++)

The C++ tree is a frozen reference. Changes to it are limited to manual
cherry-picks of upstream **logic fixes** (until milestone M2) and oracle
instrumentation behind CMake options. Such changes follow the upstream
[TheSuperHackers](https://github.com/TheSuperHackers/GeneralsGameCode)
conventions: touch only the lines the change needs, keep the local style,
document user-facing changes in present tense.

## Change documentation

User-facing changes are documented in the pull request and, where
applicable, in code comments at the changed site. Commit messages use a
lowercase prefix followed by an imperative summary, for example:

```
feat(rzh-util): port CRC and game RNG from the C++ baseline
docs(plan): record ADR-000 and relocate the migration plan
fix(rzh-math): preserve WWMath operation order in Vector3::normalize
```

When a change ports or rewrites a specific C++ unit, mention the origin path
in the commit body.
