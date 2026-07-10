# chuk-robot-benches

The robot programme's estate: specifications, the simulation proving ground, and (as
they land) the artifact standard and its tools. Modules are decoupled along the
Robot × Environment × Protocol factoring (specs/chuk-arena.md §3a): each owns one
concern, communicates through content-addressed artifacts, and never reaches into
another's internals.

## Layout

| path | what it is |
|---|---|
| `specs/` | all specifications — the source of truth for every module boundary |
| `specs/chuk-arena.md` | the proving ground: deterministic sim, benches, tournament, gap ledger |
| `specs/robotspec.md` | the robot as a content-addressed artifact (schema, hashing, derivation rule, as-built layer) |
| `specs/robotspec-viewer.md` | the inspector: derived quantities rendered on the geometry that produced them |
| `specs/robowire.md` | the electrical truth: netlist + parts catalogue → E-checks, power graph, generated bench procedure |
| `chuk-arena/` | Rust workspace implementing the chuk-arena spec (M0 done, M1 near-done) |

Module directories appear when their first code lands (robotspec derivation library,
robotspec-viewer prototype); specs precede code, per house discipline.

## Boundaries

- **chuk-arena** consumes RobotSpecs; it never defines robot identity.
- **robotspec** owns identity, hashing, derivation; it knows nothing about arenas or
  procedures.
- **robotspec-viewer** binds Robot only — no environment, no protocol, no physics.
- Shared derivation code (mass roll-up, CoG, hulls) becomes one library with multiple
  consumers (robotspec-viewer M1 = robotspec M1); until then each copy is flagged
  provisional.
