# AI Contribution Policy

RustZeroHour is intentionally developed **with** AI assistance: the migration
plan itself (docs/RUST_MIGRATION_PLAN.md) anticipates that large parts of the
C++-to-Rust port will be produced with AI tools, and this fork exists to
execute that plan. This policy describes how AI-assisted contributions are
made trustworthy here. It replaces the upstream project's policy, which was
written for a different development model.

## 1. Accountability

**A human is responsible for every merged change.**

- AI tools may draft code, tests and documentation, but a human reviewer
  must understand and approve the result before it lands on `main`.
- Every ported unit must name its C++ origin (`Ported from <path> @ 571a7c5`)
  and carry a test that states its parity level, so reviewers can compare
  the contribution against the original rather than take the code on faith.
- "The AI generated it and it compiles" is not acceptance. The reviewer
  checks the algorithm, the data semantics and the order of operations
  against the C++ source.

## 2. Disclosure

- If a contribution contains a significant amount of AI-generated content,
  say so in the pull request description. This fork does not reject such
  contributions — it audits them.
- Automated agents must not merge their own pull requests. A human presses
  the merge button.

## 3. Quality gates

AI-assisted or not, every change passes the same gates (see
[CONTRIBUTING.md](CONTRIBUTING.md)):

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`
  on Linux, Windows and macOS;
- golden/differential tests prove parity where the plan requires it (P0–P2);
- `cargo-deny` validates licenses and advisories;
- provenance and copyright notices are preserved.

## 4. Prohibited uses

- **Hallucinated origins:** never claim a change ports a specific C++ file
  unless it actually does; never invent C++ behaviour that is not in the
  baseline. When uncertain about the original's semantics, stop and ask in
  the pull request rather than guessing.
- **Bulk rewrites of the C++ tree:** the C++ baseline is frozen; do not run
  an AI over it to "modernize" it.
- **Asset generation:** EA-owned content must not be produced, converted or
  committed to this repository (see plan §7).
- **Unreviewed landings on `main`:** direct pushes of AI-generated code
  without a green CI run and a human review are not acceptable.

## 5. Review capacity

Reviewers may close AI-assisted pull requests that cannot be audited within
a reasonable time — thousands of unexplained lines, no tests, or no
provenance. Split large ports into reviewable slices; the plan's vertical
slices (§4, Phase 3) exist partly for this reason.

### Acknowledgement

This policy adapts the structure of the [Mastodon AI
Policy](https://github.com/mastodon/.github/blob/main/AI_POLICY.md) and the
upstream [TheSuperHackers AI
Policy](https://github.com/TheSuperHackers/GeneralsGameCode/blob/main/AI_POLICY.md)
to this fork's migration methodology.
