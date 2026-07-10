# chuk-arena — Virtual Physics Test Environment — Spec v0.1 (draft for review)

**Codename:** chuk-arena (the proving ground)
**Status:** Draft — pre-registration pending
**Depends on:** cell80 multi-target spec (cell-core, rustrv32, reference executors); lab plan (Stations 1–5)
**Role:** the sim half of the lab — where designs, kernels, and strategies are explored;
the physical arena and benches validate, never explore.

---

## 0. Thesis

Every competitive claim in the robot programme — "cannot be edge-KO'd", "bite denial halves
delivered energy", "gearing X beats the human meta" — must exist first as a pre-registered,
seeded, reproducible simulation result with owned error bars, produced against the *same
compiled cells* that deploy to silicon. chuk-arena is the instrument that makes such claims
cheap to produce and impossible to fake.

**Success statement:** a kernel or chassis change made in the evening produces, by morning,
a scoreboard delta across the full episode corpus, per-bench regression results, and an
updated gap-ledger position — with every number traceable to (corpus hash, cell family hash,
plant-model version, seed).

## 1. Non-goals (v0.1)

- **No 3D.** Top-down planar rigid-body world; flips, launches, and airborne states are
  analytic events over the 2D core, not simulated 3D dynamics.
- **No first-principles acoustics, RF, or vision.** Recorded/parametric channel models only
  (see §6). We simulate estimator *outputs with error models*, not wave propagation.
- **No general game engine, no rendering dependency.** Headless core; visualisation is a
  separate consumer of episode logs (SVG/canvas replayer).
- **No learning infrastructure in v0.1.** Scripted archetypes and parameterised policies.
  Self-play/learned opponents are a v2 layer on the same episode format.
- **No human-in-the-loop realtime mode in v0.1** (HIL station covers firmware-in-loop;
  interactive driving mode is v1.1 — needed for shared-autonomy experiments, deferred).

## 2. Architecture

Layered, each with its own version tag appearing in every episode record:

```
chuk-arena/
  arena-core/       # deterministic 2D rigid-body world: fixed timestep, seeded, f64
  arena-events/     # analytic event layer: impacts, flips, bites, srimech arcs
  arena-plant/      # bot plant models: drive, battery, tyres, chassis params (design vector)
  arena-sense/      # sensor models: IMU, ToF, mic-as-estimator, encoders, radio replay
  arena-cells/      # cell80 executor embedding: deployed kernels run in-loop, cycle-accounted
  arena-agents/     # opponent archetypes, scripted drivers, noise-injected intent streams
  arena-bench/      # virtual benches (§4): dyno, tilt, braking, shove, edge, bite
  arena-tourney/    # match/tournament harness, scoring, Pareto tooling, design search
  arena-store/      # episode schema, corpus versioning, scoreboard, gap ledger
  arena-view/       # offline replayer (reads episode logs; zero coupling to core)
```

### 2.1 Determinism contract

- Fixed timestep: world integrates at 8kHz internal (125µs tick); control ticks at 1kHz,
  decimated from the world clock. (Q1 resolved pre-M0: locking the higher internal rate now
  costs little and avoids invalidating the M0 banked claim under the corpus-rot rule if the
  higher-rate kernel ambition lands later.) Contact events sub-scheduled analytically,
  never by timestep refinement.
- Seeded PRNG per episode; all stochastic elements (noise, jitter, mixed-strategy sampling)
  draw from the episode seed.
- Determinism fuzzing per chuk-speccy/cell80 discipline: rerun / fresh-process /
  serialize-roundtrip must be bit-identical on the reference platform (M3 Max).
  Cross-platform bit-determinism is *not* claimed in v0.1 (f64 core); a fixed-point core is
  a pre-registered v2 option if it becomes load-bearing.
- Episode identity = hash(RobotSpec(s), EnvSpec, ProtocolSpec, plant/model versions, seed)
  per the §3a factoring. Same identity ⇒ same log, byte-for-byte.

### 2.2 Physics core decision (pre-registered)

Owned minimal 2D core (bodies, convex hulls, friction cones per wheel, impulse resolution)
rather than embedding Rapier. **Rapier's role: differential adversary** — a cross-check rig
runs matched scenarios in both and files divergences as findings. Rationale: determinism,
analytic-event integration, and WCET-style auditability of the sim itself; the core needed
is small (planar, few bodies) and the oracle pattern is house style. *Kill criterion:* if
the owned core cannot match Rapier within the §2.3 tolerances on the §2.3 scenario list by
M1, embed Rapier and demote the owned core to v2.

### 2.3 Rapier cross-check scenarios & tolerances (provisional)

The kill criterion is evaluated against this list, not against vibes. Each scenario is run
in both cores from identical initial conditions across the swept parameter band; a scenario
fails if any sample exceeds its tolerance. Tolerances are provisional until M1: tightening
is free, loosening requires a filed claim with rationale.

| # | Scenario | Sweep | Metric | Tolerance |
|---|----------|-------|--------|-----------|
| C1 | Free sliding deceleration, single body | v₀ × μ band | stopping distance | 1% |
| C2 | Wall impact at varying incidence | angle × v₀ | post-impact linear + angular velocity | 2% |
| C3 | Two-body head-on push (traction stall) | mass ratio × μ | steady-state contact force | 5% |
| C4 | Lateral shove during forward motion | shove impulse × timing | position divergence over 500ms | 5mm |
| C5 | Glancing spin contact | closing v × offset | post-hit velocities / delivered energy | 5% / 10% |

Known-divergence classes (e.g. Rapier's non-analytic contact stepping) are documented per
scenario before comparison, so filed divergences are attributable, not just counted.

## 3. Plant model (the design vector) — RobotSpec as derived digital twin

*Canonical artifact standard: [`robotspec.md`](robotspec.md) — this section states what
chuk-arena consumes; the RobotSpec spec owns the schema, hashing, derivation rule, and
as-built layer. Inspection: [`robotspec-viewer.md`](robotspec-viewer.md).*

A robot is a content-addressed artifact bundle, **derived from the same sources that build
the physical bot** — never hand-duplicated:

- **From CAD** (STL/STEP + material densities): footprint polygon, wedge profile, mass,
  CoG, yaw inertia, sensor mounting positions and view cones. Geometry changes propagate
  to the sim by derivation, not by remembering.
- **From the circuit/power definition** (ECAD/netlist): power graph — rails, wiring
  losses, battery→ESC→motor chain, current-sense points. Enables honest brownout
  scenarios (hit-spike sag on the MCU rail is a real ant failure mode).
- **Model refs:** bench-calibrated motor curves, tyre compounds (as ranges), battery sag,
  chassis coupling fraction.
- **Kernel ref:** cell family hash + parameters.

`RobotSpec = hash(CAD derivation, power graph, model refs, sensor fit, kernel hash)`.
Every episode cites the robot hash; a wedge-angle change is a new robot. Everything the
design search sweeps lives here; nothing sweepable lives in code.

*Derivation pipeline v0 (honest version):* run the CAD→mass-properties/sensor-cone tool on
export and commit the derived artifact; the ECAD/netlist becomes a first-class design
artifact alongside the CAD even for a bot simple enough to wire by hand — the brownout sim
is only as honest as the power graph it is fed. (Interim state as of M1: `BotSpec` /
`RigidBotSpec` are hand-written datasheet-provisional structs, flagged as such; they are
the model-refs component of RobotSpec until the derivation pipeline lands.)

Referenced sub-models are versioned, bench-calibrated artifacts:
- **Motor curves:** stall torque, no-load speed, thermal derating — fitted from Station 2
  dyno data (or datasheet-provisional, flagged as such).
- **Tyre compounds:** μ_static/μ_kinetic distributions from sled campaigns — carried as
  *ranges*, never points.
- **Battery:** sag curve vs current and state-of-charge, fitted.
- **Chassis coupling:** the pendulum-fitted impulse→rotation coupling fraction (§4.3);
  effective restitution per face.

## 3a. The factoring: Episode = Robot(s) × Environment × Protocol

Robots, environments, and procedures are three orthogonal, independently content-addressed
artifacts; an episode binds them with a seed.

- **EnvSpec** — the environment alone: platform geometry, μ(x,y) field, edge/wall
  configuration, temperature, acoustic background profile, camera/lighting profile.
  Contains nothing about any robot. Physical environments are characterised into EnvSpecs
  by environment instruments — **the friction sled is an EnvSpec instrument, not a robot
  one** — and *venues are EnvSpecs*: characterise an event arena on arrival
  (`bbb-event-YYYY-MM`), re-run the certification suite against it overnight, and fight
  with a per-venue certificate.
- **ProtocolSpec** — what a bench *is* once decoupled: a procedure (braking sweep, tilt,
  shove, edge-adversarial, tournament, sensor-detection) parameterised over whichever
  robot(s) and environment it binds. §4's benches are the standard protocol library, not
  world-owners. Some protocols bind robot-only (HIL streams, dyno, tilt), some
  environment-only (surface survey), most bind both.
- **Queries are cross-products:** same robot × many environments (transfer), many robots ×
  one environment (A/B), one protocol × both axes (regression). The physical lab mirrors
  the factoring: Station 2 = robot characterisation, sled/survey = environment
  characterisation, Station 1 = binding under protocol.

Episode identity is `hash(RobotSpec(s), EnvSpec, ProtocolSpec, plant/model versions,
seed)` (§2.1).

## 4. Virtual benches

Each bench is a scenario family + metric + acceptance format, mirroring a Station-2 physical
counterpart where one exists. Bench results are first-class records in arena-store and feed
the scoreboard's regression suite.

### 4.1 Dyno bench (speed / thrust)
Straight-line runs on parameterised μ: top speed, 0→v times, stopping distances (kernel on
and off), push-force vs opposing bot (shove stall curve), current draw and battery sag
trajectory. **Physical twin:** load-cell dyno + arena speed traps.

### 4.2 Braking / envelope bench
Sweeps (v, heading, μ, CoG) → measured stopping point vs the envelope cell's certified
prediction. **Primary output: envelope conservatism margin** — certified distance minus
achieved distance, distribution required ≥ 0 across the swept μ band (a single negative
sample is a filed safety finding, build-blocking). Includes pitch-over limit interaction
(a = g·x/h) and the rotate-then-brake anisotropic case. Once μ is a field (§4.7),
braking across a μ-boundary — grippy onto slippery — joins the required sweep.
**Adjudication:** a negative sample can mean a broken kernel *or* a broken sim/plant model
— before the gap ledger closes (M4) these are indistinguishable from the number alone. The
failing scenario is first re-run through the Rapier differential rig (§2.2): if the cores
disagree, it is filed as a sim finding (build still blocked, but routed to arena-core, not
the kernel); if they agree, the kernel finding stands. Either way the build reopens only
with a fix plus the scenario added to the bench's standing regression set.

### 4.3 Tilt / anti-flip bench
Static tip energy computed from BotSpec's explicit CoG position and footprint (the closed
form E = mg(√(h²+(t/2)²)−h) assumes a centred CoG and is retained only as a cross-check,
never the source of truth), cross-checked against virtual tilt table; impulse sweeps through arena-events flip model (using bench-fitted coupling
fraction) → outcome map: recovered / inverted / tumbling vs impulse magnitude and point of
application; active-recovery cell catch-window measurement (fraction of the hit
distribution converted from flip to recovery, by kernel version). **Physical twin:** tilt
table + pendulum rig.

### 4.4 Shove bench
Two-bot contact scenarios: sustained push (traction-limited stall), lateral shove
rejection, shove-detect latency (contact → classified → response torque), edge-shove
survival envelope (position/velocity/opponent-force states from which the kernel provably
escapes vs provably cannot — the boundary is the deliverable).

### 4.5 Edge bench
The MVP acceptance test, virtualised: adversarial intent streams (full-stick toward edge,
noise-injected human models, worst-case shove timing) × μ band × sensor staleness sweeps →
edge-loss rate. Target for certified kernels: zero across the swept band, by construction;
any loss is a counterexample trace exported for cell-level debugging.

### 4.6 Bite / impact bench
Analytic bite model (tooth engagement vs relative closing velocity, geometry, stored E):
delivered-energy maps vs approach profiles; bite-denial cell effectiveness (energy with vs
without velocity-matching); opponent self-load per skim (feeds weapon-fatigue state §5.2).
**Physical twin:** drop rig + high-g logging + slow-mo. This bench carries the widest error
bars in the system; its gap-ledger entry is expected to dominate and is watched accordingly.

### 4.7 Perception / observability bench
Environmental parameters become fields, not scalars: μ(x, y) over the platform (uniform
sweeps, patches, gradients, in-episode variation); braking-across-a-μ-boundary is a
required envelope scenario. For each (sensor, environmental parameter) pair the bench
asks: detectable? time-to-detect? at what confidence? and does kernel behaviour change in
response? — perception → estimation → action verified as a chain. Flagship capability:
**online friction estimation** (commanded torque vs IMU-measured acceleration → live,
spatially local μ estimate with staleness/confidence fields per §6), with the physical
friction sled demoted to calibrating the estimator — the bot then measures the floor
itself, continuously, during the fight. The envelope filter consuming live μ instead of a
session constant is a certified-safety upgrade and a standing claim in §5.3.

## 5. Agents and strategy layer

### 5.1 Opponent archetypes
Parameterised families, each with plant + driver: vert/drum spinner (spin-up curve, stored
E, gyro handling penalty), horizontal spinner, wedge/pusher, flipper (CO2 shot budget,
srimech arc distribution), plus "kit bot" baselines. Driver models: scripted policies with
skill parameters (reaction latency 150–300ms, aim noise, aggression, edge-caution) — the
human meta as a distribution, not a constant.

### 5.2 Stateful subsystems
Opponent weapon carries energy/thermal/fatigue state (spin-up, discharge on hit, stall
heating, bearing fatigue per skim) — required for smothering/stall-warfare/self-destruction-
farming experiments. Own-bot damage state (per-face armour wear, per-motor derating) for
wear-levelling policies.

### 5.3 Strategy experiments (the standing question list)
Each is a claim template over the tournament harness: edge-failsafe ablation; asymmetric
hazard tolerance (time-in-edge-zone vs win rate); bite denial ablation; ledger-gated
engagement vs naive aggression; smother/stall time-to-weapon-kill; srimech-press conversion
rate; flee-naive vs posture-aware vs stochastic (exploitability curve: best-response
opponent trained/scripted against each policy, edge measured); staleness tolerance
(win-rate vs sensor latency — where does the rate-decoupled advantage collapse);
live-μ envelope vs session-constant μ (edge-loss rate and conservatism cost on μ-field
arenas, per §4.7 — the online-friction-estimation claim).

## 6. Sensor and channel models

All sensors modelled as (truth → sampled, delayed, noisy, quantised observable) with
staleness as an explicit output field consumed by cells (per cell80 spec §WS-C3):
- **IMU:** bench-recorded noise + vibration spectra replayed; saturation and clipping modelled
  (control-range vs high-g split).
- **ToF:** update rate, cone geometry, range noise, dropout; downward edge-detection geometry
  from BotSpec mounting.
- **Acoustic ledger:** modelled at estimator level — opponent ω observable with (bias, noise,
  latency, dropout) swept; §4.6-adjacent bench establishes the achievable operating point
  from real recordings (never from first principles).
- **Encoders/back-EMF:** optional per BotSpec.
- **Radio:** recorded ELRS session traces replayed (packet timing, dropouts); synthetic
  worst-case patterns for failsafe tests.

## 7. Cells in the loop

arena-cells embeds the cell80 reference executors: the kernel under test is the *compiled
artifact* (family hash), executed instruction-level with cycle accounting, consuming the
same typed state/intent structs as on silicon. Modes:
- **Fast mode:** native-compiled cell logic (same source, host build) for large sweeps —
  permitted only while a standing differential job proves fast ≡ executor on a sampled
  episode subset (fast-vs-authentic fuzz, chuk-speccy pattern).
- **Authentic mode:** full executor, cycle-accounted, for certification runs and any
  timing-sensitive experiment.
Cycle budgets are enforced in-sim: a cell exceeding its WCET manifest in authentic mode is
an episode-invalidating finding, not a warning.

**Bootstrapping caveat:** authentic mode does not exist until M2, so the fast-mode gate
cannot be evaluated at first use. All fast-mode results produced before the differential
job is standing are flagged *provisional* in arena-store, and are re-scored by the
differential job as soon as the executor lands; any fast≢authentic divergence found then
retroactively invalidates the provisional results that depended on the divergent path.

## 8. Tournament and design search

- **Match harness:** seeded episodes, AWS rule model (edge-out, 10s immobility count, 20s
  pin limit, 3min + judge model scoring control/aggression for decisions).
- **Scoring:** win rate (by mechanism: edge-out / immobilisation / decision), edge-loss
  rate, time-in-edge-zone ratio, damage ledgers, WCET margins.
- **Design search:** Sobol screening → CMA-ES/NSGA-II over BotSpec vector × kernel
  parameters (co-design mandatory: geometry never swept under a fixed human-model
  controller). **Robustness-weighted fitness:** candidates scored across the measured
  uncertainty bands (μ range, coupling-fraction range, contact perturbations), not at
  nominal. Output: Pareto front + sensitivity report + a physical A/B shortlist (top-N
  designs with pre-registered predicted ranking for Station-1 validation).
- **Compute budget (back-of-envelope, keeps "by morning" honest):** a 3-min episode at
  8kHz world / 1kHz control with ≤4 bodies should run ≥100× realtime per core in fast mode
  (~1.8s/episode); on the reference platform (M3 Max, ~12 usable cores) that clears
  ~24k episodes/hour, so a 50k-episode corpus re-baselines in ~2h with overnight headroom
  left for authentic-mode certification subsets (expected 1–10× realtime, reserved for
  ≤1k episodes). Measured throughput below 25× realtime at M1 re-scopes the overnight
  promise explicitly rather than silently missing it.

## 9. Calibration interface & gap ledger

### 9.1 State observability contract

Full robot state is readable at all times, digital or physical, under one schema:

- **One schema:** the cell-ABI typed structs (inputs with staleness, outputs) + substrate
  fields (per-motor current, rail voltages, link stats, mcycle margin, temperature),
  versioned with the family hash. Sim and silicon serialize identically; consumers cannot
  tell the worlds apart except by connection string.
- **Transport tiers:** bench = wired full-rate (SWD/RTT/USB, Station 5 native); arena live
  = decimated summary (~25–50Hz) over ELRS telemetry + event-triggered bursts (hits,
  envelope overrides, escalations); arena complete = onboard hash-chained full-rate log
  (RAM ring → flash), dumped post-run. Live is summary-rate; reconstruction is total.
- **Belief vs truth:** physical believed state (bot) is always paired with ground truth
  (overhead camera) in the same record; **live estimation error** is a first-class
  observable and the dashboard's primary panel. Sim exposes the same pair natively.
- **Survivorship rule:** capture must outlive failure — reset-persistent RAM ring,
  last-gasp flush on brownout interrupt. Runs that end badly are the runs the record
  exists for.

- Every physical bench campaign (Station 2) produces a versioned model artifact consumed by
  arena-plant; every physical arena session (Station 1) produces episodes in the same
  schema as sim episodes.
- **Gap ledger:** per-primitive sim-vs-real divergence (braking distance, shove response,
  flip outcomes, bite energy, sensor error), versioned, trending. Ledger drift beyond
  per-primitive thresholds auto-files a claim ("recalibrate or explain") that outranks
  feature work.
- **Replay validation:** recorded physical episodes re-run through the sim from logged
  initial conditions + intent streams; divergence traces are the ledger's raw material.

## 10. Milestones

- **M0 (wk 2):** arena-core + edge geometry + kinematic plant; determinism fuzz green;
  the browser-widget experiment reproduced properly (failsafe ablation, N=500, first
  pre-registered claim banked).
- **M1 (wk 4):** full plant model + benches 4.1/4.2/4.5; Rapier differential rig running;
  first envelope-conservatism report against the real (naive) kernel via arena-cells fast
  mode (results provisional per §7 — no differential gate exists yet). *Core
  kill-criterion checkpoint (§2.2/§2.3); fast-mode throughput checkpoint (§8).*
- **M2 (wk 6):** authentic mode (RV32 executor in-loop, gated on cell80 M2); impact/flip
  event layer + benches 4.3/4.4/4.6 with provisional (datasheet/video-fitted) parameters;
  archetype population v1; first tournament scoreboard.
- **M3 (wk 8–10):** stateful weapon/damage systems; μ-field arenas + observability bench
  4.7 with online μ-estimator v0; strategy experiment suite (§5.3) run end-to-end; design
  search v1 producing first Pareto front + physical A/B shortlist; RobotSpec derivation
  pipeline v0 (CAD export → derived artifact, committed).
- **M4 (aligned with lab Stations 1–2 coming online):** calibration loop closed — bench-
  fitted models replace provisional ones; gap ledger live; replay validation running on
  real sessions.

## 11. Risks & pre-registered responses

1. **Sim-overfit designs / policies.** Mitigations: robustness-weighted fitness (mandatory),
   Rapier adversary, exploitability testing (§5.3), physical A/B as standing final gate.
2. **Contact/bite model wrong in kind, not degree.** Watched via §4.6 gap-ledger dominance;
   response: model class revision is a campaign, not a patch — pre-register the replacement.
3. **Owned-core scope creep.** Core feature freeze after M1; anything beyond planar hulls +
   analytic events needs a claim justifying it.
4. **Judge model unfounded.** Decision-scoring model is declared *low-confidence*; strategies
   whose wins depend primarily on judge-model behaviour are flagged, not banked.
5. **Corpus rot.** Corpus is versioned; scoreboard deltas always cite corpus hash; archetype
   additions bump the version and trigger full re-baselining overnight.

## 12. Open questions

- Q1. ~~World tick vs control tick~~ — **resolved into §2.1**: world 8kHz internal,
  control decimated to 1kHz. Locked pre-M0 because the corpus-rot rule (§11.5) would
  otherwise invalidate the first banked claim on a later switch.
- Q2. Judge model: encode from published AWS/BBB judging guidance, or fit from annotated
  event footage? (Lean: encode now, annotate later, keep flagged low-confidence.)
- Q3. Interactive driving mode (v1.1): gamepad-in-the-loop for shared-autonomy experiments —
  does it reuse the widget/WASM path (cells compiled to WASM, browser as cockpit) or a
  native viewer? (Lean: WASM — same-kernel-in-browser is demo gold and tests the family
  hash story.)
- Q4. Episode schema: extend the Lazarus experiment-store schema or define fresh with an
  import bridge? (Lean: extend — one query layer over research and robot lab was the point.)
- Q5. Public artifact: does arena-view + corpus subset ship as the YouTube-facing "watch
  the strategies evolve" asset, and if so what stays private (per the moat posture:
  methodology public, tuned parameters and full corpus private)?
