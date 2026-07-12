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
| `specs/robowire.md` | the electrical truth: netlist + parts catalogue → E-checks, power graph, teaching curriculum, generated bench procedure |
| `specs/energy-sim.md` | duty-cycle/energy-budget simulator (solar/battery mission survival) — deliberately separate from robosim (stateless) and arena-plant (combat-scoped, no state-of-charge); catalogue layer (M0) done, simulator crate not yet built |
| `specs/design-servers.md` | chuk-mcp-robocad + chuk-mcp-robowire: the design toolchain as MCP servers — AI-runnable propose-verify loop over the same libraries |
| `specs/codes.md` | E/D/X check-code registry (cross-spec index; bugs-become-rules intake) |
| `chuk-arena/` | Rust workspace implementing the chuk-arena spec (M0 done, M1 near-done) |
| `robotspec/` | standalone crate: schema v0.1, derivation pipeline v0, D/X checks, body⊂robot hashing, 3D inspector (`robotspec show|view robots/<name>.json`) |
| `parts/` | the shared content-hashed parts catalogue (robotspec + robowire cite `part@hash`) |
| `robots/` | authored RobotSpecs — the robots, as data |
| `robowire/` | standalone crate: netlist schema, E-check engine, SVG render, 3D harness view, interactive designer, bench procedure generator (`robowire check|render|view|design|power|explain|explain-error|bench`) |
| `robowire-wasm/` | the E-check engine compiled for the browser — the designer's live verifier (same code as the CLI) |
| `robosim/` | standalone crate: the live component simulator behind the designer's run mode — real Ohm's-law math (resistors, LED forward voltage, motor winding resistance, multi-reading sensors) over a netlist + catalogue, decoupled from robowire so it can gain other consumers later |
| `harness/` | authored netlists — the wiring, as data |
| `harness/lessons/` | the numbered teaching curriculum (`01-basics` → `07-two-wheel-drive`) — each stage's `-broken` sibling fails exactly one named E-code |
| `derived/` | committed derived artifacts (pipeline outputs, per derivation-pipeline v0 discipline) |

Module directories appear when their first code lands (robotspec derivation library,
robotspec-viewer prototype); specs precede code, per house discipline.

## Dev rituals

- **Designer changes:** edit `robowire/templates/designer/*.js` modules (never an
  assembled artifact), then `cargo build --release` in `robowire/` (templates are
  compiled in), regenerate, and run `node tools/verify-designer.mjs` against the
  output. The page's corner stamp identifies the build in any screenshot.
- **robosim/robowire Rust changes that affect run mode:** the designer embeds a
  compiled wasm binary, not source — after any change to `robosim`/`robowire` library
  code, rebuild `robowire-wasm` (`cargo build --release --target wasm32-unknown-unknown`
  in `robowire-wasm/`) *before* regenerating the designer, or the artifact silently
  ships stale simulation behavior with a misleadingly fresh build stamp.

## Boundaries

- **chuk-arena** consumes RobotSpecs; it never defines robot identity.
- **robotspec** owns identity, hashing, derivation; it knows nothing about arenas or
  procedures.
- **robotspec-viewer** binds Robot only — no environment, no protocol, no physics.
- Shared derivation code (mass roll-up, CoG, hulls) becomes one library with multiple
  consumers (robotspec-viewer M1 = robotspec M1); until then each copy is flagged
  provisional.
