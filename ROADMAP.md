# Roadmap — the robot programme

One page, cross-spec, kept honest: phases sequence the seven specs' milestone ladders
against what is actually built. Every phase ends in an acceptance gate that is a
verifiable artifact (hash, report, or green suite) — never a vibe. Owning specs:
[chuk-arena](specs/chuk-arena.md) · [robotspec](specs/robotspec.md) ·
[robotspec-viewer](specs/robotspec-viewer.md) · [robowire](specs/robowire.md) ·
[energy-sim](specs/energy-sim.md) · [design-servers](specs/design-servers.md) ·
[codes](specs/codes.md) · cell80 (external companion spec).

## Phase 0 — done (2026-07-10 → 11)

- **chuk-arena M0** ✅ — deterministic core (8kHz/1kHz), kinematic plant, edge geometry,
  determinism fuzz green on all three legs; first pre-registered claim banked:
  failsafe ablation N=500/arm, kernel-off 500/500 edge losses, kernel-on 0/500,
  corpus `38f634eb…` (re-verified byte-identical after every subsequent change).
- **chuk-arena M1 (all but one item)** ✅ — dynamic plant (friction-circle wheels,
  motor curves, battery sag); envelope bench §4.2 (naive kernel: FINDING 179/210,
  worst −178mm; active aligned kernel: PASS 210/210); dyno bench §4.1; throughput
  checkpoint >2000× realtime single-core vs the 100× budget.
- **Contact layer pulled forward from M2** ✅ — SAT/clipping manifolds, sequential
  impulses, restitution, contact friction; **§2.2 kill criterion: PASS on the full
  §2.3 table** (C1 0.13%, C2 0.03% vs Rapier and analytic, C3 exact via force
  balance, C4 84µm, C5 0.9% energy). Two Rapier artifacts documented as
  known-divergence classes; friction/corner impacts assigned to M4 physical
  adjudication.
- **arena-view v0** ✅ — episode replayer (counterfactual ghost) + interactive WASM
  bench console (`arena replay`, `bench.html`), local-only.
- **robotspec-viewer M0** ✅ — prototype exists (user-side; **not yet in repo** — import
  is a Phase 1 chore).
- **Six specs banked** ✅ — amended, cross-referenced, E/D/X code registry seeded.

## Phase 1 — close M1, start the electrical truth ✅ (completed 2026-07-11)

| Item | Owner spec | Gate |
|---|---|---|
| ✅ Edge bench §4.5 on the dynamic plant, against the BOUND robot | chuk-arena M1 | **done 2026-07-11:** 0/200 certified losses across the μ band (unprotected 200/200 — pressure confirmed); known limitation: worst-case boundary intrusion to 13mm CoG-to-edge via veto-state scrub drift, watched for §4.7 |
| ✅ robowire M0 + schematic + 3D harness view w/ inspector | robowire M0 | **done:** HARNESS LEGAL, planted faults fail with correct E-codes |
| ✅ RobotSpec M0: schema v0.1, parametric mode, hashed robot cited by arena episode AND inspector | robotspec M0 | **done 2026-07-11:** robot_hash in the §4.5 report and in the inspector HUD |
| ✅ Inspector in repo (`robotspec view`) — built to spec, superseding the prototype import: displays the record computed by robotspec::derive itself (viewer M1 "one derivation codebase" met by construction; no private copy ever existed in-repo) | robotspec-viewer M0+M1(partial) | hash in HUD; ledger = pipeline output; prototype import now optional |

## Phase 2 — organs share one truth

| Item | Owner spec | Gate |
|---|---|---|
| Impact/flip analytic event layer over the contact core; benches 4.3 (tilt) / 4.4 (shove) / 4.6 (bite), provisional parameters | chuk-arena M2 | bench reports with version tags; corner-impact divergence class re-examined against the event layer |
| Opponent archetypes v1 + match harness + first tournament scoreboard | chuk-arena M2 | seeded scoreboard citing corpus hash |
| Authentic mode (RV32 executor in-loop) | chuk-arena M2 | **gated on cell80 M2 (external)** — fast-mode results retroactively re-scored per §7 |
| Shared derivation library (mass/CoG/hull/tip): one crate, consumed by viewer + arena-plant + robotspec | robotspec M1 = viewer M1 | one codebase, N consumers, zero drift by construction |
| ✅ robowire M1: power-budget checks E30–32 (worst-case draw vs C-rating/regulator `max_a`, AWG ampacity, MCU/motor-rail brownout warning) + live wire IR-drop/battery sag in run mode, single-sourced; power graph derivation (`robowire::power_graph`) + wiring mass into RobotSpec's `power:`/`mass_wiring_g` fields, retiring the flat `harness-allowance` placeholder | robowire M1 | **done:** `robowire power harness/mvp-wedge-harness.json --robot robots/mvp-wedge.json` derives the merged power section + wiring-inclusive mass, no hand-entered duplicates (X03), D02 re-evaluated against the corrected total |
| ✅ robowire teaching layer: numbered curriculum (`harness/lessons/`, 7 stages — 1-2 standalone foundational vignettes, 3-7 a strict accumulating build) + in-designer teaching mode (palette stays usable mid-lesson, not locked) + `explain`/`explain-error` CLI and wasm export, single-sourced prose (`robowire::teach`) | robowire (informal, between M1 and M2) | **done:** per-stage legal/broken pairs each fail exactly their named code; full headless browser regression suite passing against the real designer artifact |
| ✅ robowire component-centric run mode + catalogue breadth: the MCU is a real, drivable component — run-mode signal source resolved through the actual netlist wiring (`robowire::signal`), never pinned to a motor instance; multi-reading sensors (`roboparts::Part::readings`, e.g. `env-bme280`'s temp/humidity/pressure as three independent live values, not one collapsed number); new kinds/parts (light/env/longer-range tof, brushless drive motor+ESC, solar panel + charge controller per `energy-sim.md` M0, second brushed motor/ESC voltage class); new **E05** check (motor winding type vs its driving ESC's declared support, `roboparts::EscProps`) | robowire (informal, between M1 and M2) | **done:** full catalogue coverage (every part exercised in a lesson/example/robot harness), every new kind checked by existing E-codes with no bespoke logic where the existing role-based checks already generalized |
| robowire M0.5: interactive run mode, in the standalone `robosim` crate — click-to-toggle switch/button, throttle + fake-sensor + dial controls, event-driven net energization, real Ohm's-law voltage/current per net and component incl. a potentiometer dimmer (not fixed figures), animated wire-flow, draggable wire bends (2D/3D/while running), weighted auto-arrange | robowire M0.5 | green `run_state` test suite against the MVP wedge harness + dedicated demo harnesses (switch+LED+motor+sensor+button; potentiometer dimmer), incl. voltage-changes-current-changes tests |

## Phase 3 — fields, search, and the AI design loop

| Item | Owner spec | Gate |
|---|---|---|
| μ(x,y) fields + observability bench §4.7 + online μ-estimator v0; μ-boundary braking joins §4.2 | chuk-arena M3 | live-μ envelope vs session-constant claim runs (§5.3) |
| Stateful weapon/damage systems; strategy suite §5.3 end-to-end | chuk-arena M3 | claims with error bars, corpus-hash cited |
| Design search v1 (Sobol → CMA-ES/NSGA-II, robustness-weighted) → Pareto front + physical A/B shortlist | chuk-arena M3 | pre-registered predicted ranking for Station-1 validation |
| RobotSpec derivation pipeline v0 (CAD export → derived artifact, committed) | robotspec M2 | derived sections carry pipeline version |
| ✅ robowire M2 (bench procedure half): generated Markdown bench verification procedure (`robowire::bench`, `robowire bench <netlist> [--out FILE]`) — continuity checks, polarity list, staged power-up (rails unloaded → power distribution → brain → sensors → drive → full), expected I²C bus scan, all reusing the checker's own helpers so the procedure can never disagree with `robowire check`. SVG diagram render not yet started. | robowire M2 | **done (bench half):** `robowire/tests/bench.rs` green against real harnesses; first physical harness verified against its own checklist is a real-world step, not a code milestone — it happens whenever a harness from this repo actually gets built |
| design-servers M0–M1: robowire server, then robocad server | design-servers | agent transcript: propose → E-fail → fix → pass, no human edits |
| Viewer M2: mesh ingest + parametric-vs-mesh centroid delta row | robotspec-viewer M2 | built-in pipeline cross-check visible |

## Phase 4 — the loop closes on reality (aligned with lab Stations 1–2)

| Item | Owner spec | Gate |
|---|---|---|
| Calibration loop: bench-fitted models replace provisional; gap ledger live; replay validation on real sessions | chuk-arena M4 | ledger trending; drift auto-files claims |
| As-built layer: scale/tilt/free-run rituals + robowire electrical checklist as unit records; thresholds enforced | robotspec M3 | no certificate citation without a current as-built record |
| Brownout scenarios from the derived power graph (not hand-modelled) | robowire M3 | first honest brownout episode |
| Corner/friction impact adjudication: pendulum campaign data vs both solvers | chuk-arena §2.3/C2f | divergence class closed or model revised (as a campaign) |
| Venue-as-EnvSpec ritual: characterise event arena on arrival, overnight recertification | chuk-arena §3a | per-venue kernel certificate |
| design-servers M2–M3: robotspec_assemble + X-checks; LLM-proposer vs numeric optimiser (pre-registered) | design-servers | first end-to-end AI-authored RobotSpec hash in an arena episode |

## Critical path to the first fight-ready MVP wedge

robowire M0 (harness checked) → RobotSpec M0 (hashed identity) → §4.5 certification
against that RobotSpec → physical build + as-built rituals (Phase 4 machinery, minimal
form) → venue EnvSpec + overnight recertification. The external dependency is cell80 M2
for authentic-mode certification; everything before it runs in flagged fast mode.

## Standing invariants (every phase)

- Determinism fuzz green (rerun / roundtrip / fresh-process) on every commit that
  touches the sim.
- M0 corpus hash `38f634eb…` reproduces until a re-baseline is declared (corpus-rot
  rule §11.5) — never silently.
- A negative envelope margin anywhere is build-blocking (§4.2), adjudicated via the
  differential rig before reopening.
- Bugs become rules: physical/agent-discovered statically-detectable failures land in
  `specs/codes.md` first, then their checker.
