# robotspec-viewer — Robot Inspector — Spec v0.1 (draft for review)

**Codename:** robotspec-viewer (the inspector)
**Status:** Draft — prototype exists (robotspec_viewer.html, single-file three.js)
**Parent:** chuk-arena spec §arena-view; consumes RobotSpec per chuk-arena §3;
respects the §3a factoring (this tool binds Robot only — no Environment, no Protocol)
**Role:** the verification instrument for the RobotSpec derivation pipeline — the place
where derived quantities are seen against the geometry that produced them, before any
episode is run.

---

## 0. Thesis

The derivation pipeline (CAD/ECAD → mass properties, sensor cones, power graph) is a chain
of silent computations; an error anywhere in it silently corrupts every downstream
simulation, certificate, and design-search result. The inspector makes every derived
quantity *visible on the geometry it was derived from*, so the pipeline is verified by
inspection the day a robot is defined — not discovered broken by a divergent episode weeks
later.

**Success statement:** a new RobotSpec is loaded; within one minute of orbiting, toggling
overlays, and reading the ledger, a builder can answer: is the mass budget met, where is
the CoG and what does it imply (tip angles, tip energy, braking pitch limit), does the bot
fit the cube, do the sensors see what the kernel assumes they see, and which component is
on which power rail — with every number derived, none declared.

## 1. Non-goals (v0.1)

- **No physics.** The inspector renders and derives statics; dynamics belong to
  arena-core. (One deliberate exception class: closed-form static derivations — tip
  energy, brake pitch limit — that are functions of geometry alone.)
- **No environment.** No floor friction, no arena, no opponents. Robot-only binding per
  §3a. An arena/episode replayer is a separate arena-view deliverable.
- **No CAD editing.** The inspector displays; the spec is edited as data (JSON now,
  derived artifact later). It is a viewer of truth, not a source of it.
- **No server.** Single-file, static, runs from disk or a file share. Zero build step in
  v0.1 (three.js via CDN); packaging/offline bundling is a v1.1 concern.
- **No photorealism.** Instrument aesthetic: geometry, overlays, numbers. Rendering
  quality serves legibility only.

## 2. Architecture

```
robotspec_viewer.html          # v0.1: single file, three.js r128, no build step
  spec ingest                  # JSON textarea now; file-drop + URL param v1.1
  derivation module            # pure functions: spec → derived record (see §4)
  scene builder                # spec + derived → meshes, overlays, layer groups
  ledger renderer              # derived record → panel rows (numbers with verdicts)
  interaction                  # custom orbit (drag), zoom (wheel/pinch), layer toggles
```

Two contracts to honour from birth:

1. **The derivation module is extractable.** Its functions (mass roll-up, CoG, hull,
   tip geometry, cube fit) are pure spec→record and must be liftable into the real
   derivation pipeline / arena-plant unchanged. The viewer is the first *consumer* of the
   derivation library, not the owner of a private copy. (v0.1 ships them inline; the
   extraction is a v1 acceptance criterion.)
2. **Displayed ≠ recomputed, eventually.** When the canonical pipeline exists, the
   inspector displays the pipeline's derived record and *recomputes independently as a
   cross-check* — any mismatch renders as a red ledger row. The viewer becomes a
   differential adversary for the pipeline, house pattern.

## 3. Spec ingest

- **v0.1:** editable JSON panel + Apply; parse errors surfaced inline; the loaded spec is
  the hand-schema (chassis, wheels, components, sensors — see prototype).
- **v1:** canonical RobotSpec artifact (chuk-arena §3): CAD-derived mesh + mass properties
  replace the parametric chassis; ECAD-derived power graph arrives; spec hash displayed in
  the HUD and stamped on any exported view.
- **Mesh path:** STL/GLTF load for the chassis (three.js loaders); parametric wedge
  retained as the zero-CAD authoring mode — it is also the design-search's native
  geometry representation, so it stays first-class, not legacy.

## 4. Derived record (the ledger contract)

Everything shown is computed from the spec. v0.1 set:

| Quantity | Derivation | Verdict rule |
|---|---|---|
| Total mass | Σ component/wheel/sensor/chassis masses | ≤ class limit → green |
| Budget margin | limit − mass; % bar | red on violation |
| CoG (x, y, z) | mass-weighted; chassis centroid from geometry | — (amber marker + drop line) |
| Support polygon | convex hull of wheel contact points | rendered on floor |
| Worst tip edge | min CoG-to-hull-edge distance, labelled front/rear/side | — |
| Worst tip angle | atan(d/h) | — |
| Static tip energy | mg(√(h²+d²)−h) | — (context vs weapon energies) |
| Brake pitch limit | g·(x_front−CoG_x)/h | cyan — feeds envelope discussion |
| Bounding box | from assembled scene | — |
| Cube fit | bbox vs 101.6mm | FITS / VIOLATION |
| Sensor fit | count; per-sensor cone rendered at true FoV/range | coverage judged visually v0.1 |
| Drive summary | driven/total wheels | — |

v1 additions: chassis centroid by mesh integration (density-weighted); per-rail power
roll-up (click component → rail highlight, rail budget row); sensor coverage as computed
quantities (e.g., "downward ToF first sees a drop-off at X mm from lip at Y mm/s closing"
— a statics-only precursor to the observability bench); yaw inertia; invertibility check
(geometry valid both ways up); AWS legality checklist row (cube at start, weight, no
entanglement geometry classes).

## 5. Overlays and interaction

Layer-toggled groups, each independently visible: cube ghost, CoG (marker + drop line),
sensor cones (true FoV half-angle, true range, end-ring), support polygon, components,
exploded view (components lift on stable per-index offsets; animated unless
reduced-motion). Orbit/zoom via pointer events (no OrbitControls dependency); touch pinch
zoom; keyboard focus on all controls. v1: component picking (click → ledger detail +
power-rail highlight), section plane, dimension callouts on hover, side/top/front ortho
snap views.

## 6. Milestones

- **M0 (done):** prototype — parametric wedge, seven-overlay scene, live ledger,
  editable spec, exploded view.
- **M1:** derivation module extracted as a library consumed by both viewer and
  arena-plant; canonical RobotSpec schema replaces hand-schema; spec hash in HUD.
  *Acceptance: one derivation codebase, two consumers, zero drift by construction.*
- **M2:** mesh ingest (STL from real CAD) + mesh-integrated centroid; ledger shows
  parametric-vs-mesh centroid delta during transition (a built-in pipeline cross-check).
- **M3:** power graph overlay + per-rail ledger; component picking.
- **M4:** computed sensor-coverage rows; independent-recompute cross-check against the
  canonical pipeline's derived record (viewer becomes the pipeline's adversary).

## 7. Risks & responses

1. **Viewer-private derivation drift** (viewer maths diverges from pipeline maths as both
   evolve). Response: M1 extraction is the fix; until then the prototype is explicitly
   provisional and its numbers are not citable in claims.
2. **Chassis centroid approximation misleads** (v0.1 parametric estimate). Response:
   flagged in ledger as approximate until M2; no claim may cite CoG-derived numbers from
   the approximate path.
3. **CDN dependency** (three.js from cdnjs; offline lab bench). Response: v1.1 vendored
   bundle; single-file constraint retained.
4. **Scope creep toward a CAD tool.** Response: the non-goal is a wall — display and
   derive only; any editing beyond the JSON panel requires its own spec.

## 8. Open questions

- Q1. Authoring split: does the parametric wedge schema remain the design-search
  representation forever (with CAD as the fidelity upgrade), or does the search
  eventually operate on mesh-parameter hybrids? (Lean: parametric stays canonical for
  search; CAD binds at validation.)
- Q2. Export: should the inspector emit a signed "inspection report" (spec hash + derived
  record + rendered views) as the artifact that accompanies a robot into the claims
  registry? (Lean: yes — cheap, and it makes "which robot" auditable in documents.)
- Q3. Where does the WASM kernel meet the viewer: is the v2 inspector also the HIL
  cockpit (live state overlaid on the twin, belief-vs-truth per §9.1), or does the
  cockpit stay a separate arena-view tool sharing the scene builder? (Lean: shared scene
  builder, separate tools — inspection and operation are different jobs.)
