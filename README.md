# chuk-robot-benches

The robot programme's estate: specifications, the simulation proving ground, and (as
they land) the artifact standard and its tools. Modules are decoupled along the
Robot × Environment × Protocol factoring (specs/chuk-arena.md §3a): each owns one
concern, communicates through content-addressed artifacts, and never reaches into
another's internals.

**Where we are and what's next: [ROADMAP.md](ROADMAP.md).**

## Layout

| path | what it is |
|---|---|
| `specs/` | all specifications — the source of truth for every module boundary |
| `specs/chuk-arena.md` | the proving ground: deterministic sim, benches, tournament, gap ledger |
| `specs/robotspec.md` | the robot as a content-addressed artifact (schema, hashing, derivation rule, as-built layer) |
| `specs/robotspec-viewer.md` | the inspector: derived quantities rendered on the geometry that produced them |
| `specs/robowire.md` | the electrical truth: netlist + parts catalogue → E-checks, power graph, generated bench procedure |
| `specs/design-servers.md` | chuk-mcp-robocad + chuk-mcp-robowire: the design toolchain as MCP servers — AI-runnable propose-verify loop over the same libraries |
| `specs/codes.md` | E/D/X check-code registry (cross-spec index; bugs-become-rules intake) |
| `chuk-arena/` | Rust workspace implementing the chuk-arena spec (M0 done, M1 near-done) |
| `robotspec/` | standalone crate: schema v0.1, derivation pipeline v0, D/X checks, body⊂robot hashing, 3D inspector (`robotspec show|view robots/<name>.json`) |
| `parts/` | the shared content-hashed parts catalogue (robotspec + robowire cite `part@hash`) |
| `robots/` | authored RobotSpecs — the robots, as data |
| `robowire/` | standalone crate: netlist schema, E-check engine, schematic SVG render (`robowire check|render harness/<name>.json`) |
| `harness/` | authored netlists — the wiring, as data |
| `derived/` | committed derived artifacts (pipeline outputs, per derivation-pipeline v0 discipline) |

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
