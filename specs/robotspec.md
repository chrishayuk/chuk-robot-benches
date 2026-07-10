# RobotSpec — Robot Definition Artifact — Spec v0.1 (draft for review)

**Codename:** RobotSpec (the robot, as data)
**Status:** Draft — pre-registration pending
**Position:** the independent artifact standard. chuk-arena consumes it (§3), the
inspector displays it, the design search sweeps it, the claims registry cites it, the
physical build realises it. None of them own it; this spec does.
**Companion specs:** cell80 multi-target (kernel artifacts), chuk-arena (Environment ×
Protocol binding), robotspec-viewer (inspection).

---

## 0. Thesis

A robot is a content-addressed artifact: everything the simulator needs, everything the
build needs, and everything a claim needs to cite, derivable from declared sources of
truth with no hand-duplicated numbers. Two robots are the same robot iff their hashes are
equal; every episode, bench result, inspection report, and fight log cites the hash of the
robot that produced it — including the physical one.

**Success statement:** given a RobotSpec hash, any consumer can reconstruct without
ambiguity: the geometry, the mass properties, the drivetrain, the power topology, the
sensor fit, and the exact kernel — and the physical bot on the bench can be audited
against it field by field.

## 1. Non-goals (v0.1)

- **Not a CAD format.** CAD (STEP/STL) is a *source*; RobotSpec stores the derivation
  outputs plus a hash reference to the source files, never the modelling history.
- **Not a build manual.** Assembly instructions, print settings, and wiring routing live
  with the build; RobotSpec captures what the built artifact *is*, not how to make it.
- **Not environment- or protocol-aware.** Nothing about arenas, floors, opponents, or
  procedures (§3a factoring). A RobotSpec is meaningful with zero episodes run.
- **Not a runtime state format.** Live state is the cell-ABI schema (chuk-arena §9.1);
  RobotSpec is the static identity that state is *about*.

## 2. Sources of truth and the derivation rule

Four declared sources; everything else is derived. **The rule: any quantity obtainable
from a source may not be hand-entered.** Hand-entered duplicates are schema violations,
not conveniences.

| Source | Format | Yields (derived) |
|---|---|---|
| **Mechanical** | CAD export (STEP/STL) + material/density map; or parametric geometry (the design-search representation) | footprint polygon, wedge profile, mass, CoG, yaw inertia, bounding box, cube fit, mounting frames for components/sensors |
| **Electrical** | ECAD/netlist (structured; hand-authored netlist acceptable at ant scale) | power graph: rails, source→ESC→motor chains, wiring loss estimates, current-sense points, brownout topology |
| **Model refs** | versioned bench artifacts (chuk-arena §9 calibration outputs) | motor curves, tyre μ ranges, battery sag, chassis coupling fraction, sensor noise/latency models |
| **Kernel** | cell80 family hash + parameter set | control behaviour, WCET manifests, escalation contract |

Parametric geometry is a first-class mechanical source (not a fallback): it is the
design-search's native representation; CAD binds at validation fidelity. A RobotSpec
declares which mode its mechanical source is in.

## 3. Schema (v0.1 sections)

```
robotspec:
  identity:      name, revision, robot_hash (computed), created, notes
  sources:       mech {mode: cad|parametric, ref+hash, density_map},
                 elec {ref+hash}, models {refs+versions}, kernel {family_hash, params}
  geometry:      footprint, wedge/armour profile, bbox, cube_fit          # derived
  mass:          total_g, cog_xyz, yaw_inertia, per_component roll-up     # derived
  drive:         wheels [{pos, radius, width, driven, motor_ref, gearing, tyre_ref}]
  power:         rails, chains, sense_points, budget                       # derived
  sensors:       [{id, type_ref, mounting_frame, dir, fov, range, bus, rate}]
  radio:         rx type, protocol, failsafe behaviour ref
  compliance:    class {weight_limit, cube, cluster: n_parts}, checks      # derived
  as_built:      see §5 (empty until a physical unit exists)
```

Cluster bots are N `parts[]` each with the full section set, plus shared compliance
(combined weight/cube) and an inter-part link declaration — the schema is
one-robot-with-N-bodies, matching the one-nervous-system architecture.

## 4. Identity and hashing

- `robot_hash = hash(canonicalised sources section + derived sections + schema version)`.
  Sources are hashed by content (the STEP file's hash, the netlist's hash, the kernel
  family hash), so a geometry change anywhere upstream changes the robot.
- **Revision vs identity:** `name/revision` is for humans; the hash is the identity.
  Changing a ToF mounting angle is a new robot. This is deliberate — it is what makes
  A/B results and gap-ledger entries attributable.
- **Kernel-only changes produce a new robot hash** (the kernel ref is part of identity)
  but consumers may group by `mech+elec hash` to ask "same body, different brain" —
  both queries must be cheap; the hash structure nests to allow it
  (`body_hash` ⊂ `robot_hash`).
- The derivation pipeline is itself versioned; derived sections record the pipeline
  version that produced them. Pipeline upgrades re-derive and re-hash — old records
  remain citable under their original hashes.

## 5. As-built binding (the honesty layer)

The physical bot diverges from the design: print variance, solder mass, glue, worn tyres.
The twin stays honest through an **as-built record** attached to (not replacing) the
as-designed spec:

- **Required rituals per physical unit:** measured total mass (0.1g scale), measured CoG
  (tilt table, chuk-arena Station 2), per-wheel free-run check. Each yields an
  `as_built` entry: measured value, delta vs derived, date, instrument.
- **Divergence thresholds:** deltas beyond declared bounds (e.g. mass >2%, CoG >2mm)
  flag the unit — either the build is corrected or the spec is revised to match reality;
  an over-threshold unit may not fight under the design's certificates.
- **Unit identity:** a physical unit is `robot_hash + unit_serial + as_built record`;
  fight logs cite the unit, not just the design. Repairs and rebuilds append as-built
  entries (the unit has a maintenance history the gap ledger can condition on).
- Hardware revision tags (build bench discipline, lab plan Station 3) are as-built
  entries, unifying the two systems.

## 6. Consumers and their obligations

| Consumer | Reads | Must cite |
|---|---|---|
| arena-plant (sim) | full spec + model refs | robot_hash in every episode |
| Inspector | full spec | hash in HUD and inspection reports |
| Design search | parametric mech source + kernel params | hashes of every candidate on the Pareto front |
| Claims registry | hashes only | robot (and unit, if physical) per claim |
| Physical build | sources + drive/power/sensors | unit_serial bound to robot_hash |
| Fight/telemetry logs | identity | unit identity per session |

## 7. Milestones

- **M0:** schema v0.1 frozen for the MVP wedge; hand-schema from the viewer prototype
  migrated; parametric mechanical mode only. *Acceptance: the MVP bot exists as a
  hashed RobotSpec cited by both the inspector and the first arena episodes.*
- **M1:** derivation pipeline v1 (parametric → geometry/mass sections) extracted as the
  shared library (viewer M1 = this M1); body_hash/robot_hash nesting implemented.
- **M2:** CAD mode (STEP/STL + density map → mesh-integrated mass properties); electrical
  source ingested to power graph.
- **M3:** as-built layer live — tilt-table and scale rituals producing unit records;
  divergence thresholds enforced in the claims registry.
- **M4:** cluster schema exercised (even if only in sim) — N-part robot round-trips
  through arena-tourney's N-vs-1 episodes.

## 8. Risks & responses

1. **Schema churn under early discovery.** Response: schema version in the hash;
   migrations are explicit tools; v0.x freezes only per-milestone, stability promised
   from v1.
2. **Derivation rule friction** (hand-entering is faster in week one). Response: the
   parametric mode *is* the fast path — it's authored data, not duplicated data; the rule
   forbids duplication, not authoring.
3. **As-built rituals skipped under event pressure.** Response: no certificate citation
   without a current as-built record; the rule is mechanical, not aspirational.
4. **Hash granularity too fine** (every tweak = new robot = scoreboard fragmentation).
   Response: body_hash grouping + revision lineage links (`derived_from`) make families
   queryable; fragmentation is a query problem, not an identity problem.

## 9. Open questions

- Q1. Density map fidelity: per-part densities vs per-material — is per-material enough
  for ant-scale CoG accuracy (±1mm target)? (Lean: per-material, validated by the M3
  tilt-table deltas.)
- Q2. ~~Sensor type registry~~ — **resolved by robowire §2.1**: a shared, versioned,
  content-hashed `parts/` catalogue cited as `part@hash`; same answer for
  motors/tyres/batteries.
- Q3. Does the transmitter/controller configuration (intent mapping, rates) belong in
  RobotSpec, or is it a Protocol concern? (Lean: the *capability* — intent format,
  link protocol — is RobotSpec; the human's stick preferences are Protocol.)
- Q4. Storage: files-in-repo (specs as versioned text, sources in LFS/content store) vs
  the Lazarus experiment store — or repo-canonical with store-indexed? (Lean:
  repo-canonical, store-indexed; matches the cell corpus pattern.)
