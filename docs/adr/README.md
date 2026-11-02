# Architecture Decision Records

This project records its significant design decisions as **ADRs** (Architecture
Decision Records), following the "lightweight ADR" style: one numbered markdown
file per decision, never rewritten after acceptance (superseded instead).

Each ADR answers four questions:

1. **Context** — what forces were in play when the decision was taken.
2. **Options** — what alternatives existed.
3. **Decision** — what was chosen and why.
4. **Consequences** — what becomes easier, harder, or mandatory as a result.

Rules:

- ADRs are numbered sequentially starting at `000`.
- An accepted ADR is immutable; if a decision changes, write a new ADR that
  lists the old one under *Supersedes*.
- A proposed ADR may be edited while its pull request is open.
- Anything that changes the structure, the porting rules, the parity
  definition, or the dependency surface of the project deserves an ADR.
  Routine implementation choices do not.

New ADRs are created from [`TEMPLATE.md`](TEMPLATE.md). The index of records:

| ADR | Title | Status |
|-----|-------|--------|
| [000](000-migrate-to-rust.md) | Migrate the code base to Rust and drop retail compatibility | Accepted |
| [001](001-sim-numerics.md) | Simulation numerics — f32, msun-exact entry points, no libm crate for parity code | Accepted |
| [002](002-logging-diagnostics.md) | Logging and diagnostics — `tracing`, log-file rotation, panic hook | Accepted |
| [003](003-benchmark-budget-exceptions.md) | Accepted §6 benchmark budget exceptions (Phase 1) | Accepted |
