# chuk-mcp-robocad & chuk-mcp-robowire — AI Design Interface — Spec v0.1 (draft)

**Codename:** the design servers (robolab MCP layer)
**Status:** Draft — pre-registration pending
**Position:** MCP servers exposing the design toolchain (parametric CAD + wiring) to AI
agents, joining the chukai.io fleet (dem, lidar, her, maritime, physics, solver…).
The servers wrap the *same* derivation and E-check libraries the CLI, viewer, and
arena-plant consume — one codebase, N consumers (viewer spec M1 discipline).
**Companion specs:** RobotSpec (the artifact being authored), robowire (the check
catalogue), robotspec-viewer (rendering), chuk-arena (evaluation of the results).

---

## 0. Thesis

Every design artifact in the programme is structured data with a hash and a verifier:
parametric geometry with a derivation pipeline, netlists with an E-check catalogue. That
makes robot design a propose-verify loop an AI agent can run: the agent authors, the
verifier returns precise pass/fail with actionable errors, the agent iterates, and the
output is a hashed RobotSpec indistinguishable in form from a hand-designed one —
entering the same claims registry, sim evaluation, and build pipeline on identical terms.

**Success statement:** a conversation of the form "design me a 150g 4WD wedge with dual
downward ToF, front multizone, and 30g of margin for a future lifter" produces — through
tool calls alone — a geometry that fits the cube with a stated CoG, a harness that passes
all E-checks, a generated bench procedure, and an assembled RobotSpec hash ready for
chuk-arena episodes. No step hand-edited; every step auditable.

## 1. Non-goals (v0.1)

- **No AI-certified anything.** AI-designed ≠ certified: designs enter the registry as
  candidates with check reports attached; certificates still come only from the
  verification machinery (E-checks, derivations, sim campaigns, as-built rituals).
- **No mesh-sculpting CAD.** robocad exposes the parametric representation (RobotSpec §2
  mechanical mode: parametric). STEP/mesh mode remains a human-CAD import path.
- **No direct hardware actions.** The servers author artifacts; nothing flashes, orders
  parts, or drives the HIL rig in v0.1.
- **No bespoke agent.** The servers are model-agnostic MCP; the "designer" is whatever
  agent connects (Claude in chat, mcp-cli pipelines, future design-search hybrid).

## 2. chuk-mcp-robocad — parametric geometry server

Wraps the derivation library. Artifacts are content-addressed; every mutating call
returns the new artifact hash plus the *re-derived record* so the agent always sees
consequences immediately (the tight feedback is the product).

**Tool surface (draft):**
- `robocad_create(design_class, params)` → new parametric design (wedge class first:
  chassis dims, wedge profile, lip, material)
- `robocad_set / robocad_get(design, path, value)` — parameter edits, hash-versioned
- `robocad_place_wheel / place_component / place_sensor(design, part_ref, frame, …)` —
  placements cite `parts/` catalogue entries (shared with robowire)
- `robocad_derive(design)` → the full derived record: mass roll-up, CoG, yaw inertia,
  support polygon, tip angles/energy, brake pitch limit, bbox, cube verdict, sensor
  cones (the inspector's ledger, as data)
- `robocad_check(design)` → design-rule results (D-codes: cube violation, mass over
  limit, component collision/overlap, sensor cone occluded by chassis, wheel-chassis
  interference, unreachable CoG targets)
- `robocad_render(design, view)` → SVG/PNG views (ortho + iso) for chat display;
  `robocad_export(design)` → STL + RobotSpec mechanical section
- `robocad_diff(a, b)` → parameter and derived-record deltas (the agent's A/B primitive)
- `robocad_provenance(artifact)` → full lineage (house pattern, per lidar_provenance)

## 3. chuk-mcp-robowire — wiring server

Wraps the netlist format, catalogue, and E-check engine.

**Tool surface (draft):**
- `robowire_parts_search / parts_get(query | part_ref)` — the shared catalogue, hashed
- `robowire_netlist_create / netlist_edit(ops)` — instances, nets, rails, buses;
  structured-data canonical (robowire spec Q1: data canonical, DSL as sugar)
- `robowire_check(netlist)` → full E-check report (E-codes with offending elements and,
  where mechanical, suggested remedies — e.g. E20 returns the XSHUT reassignment recipe)
- `robowire_derive(netlist)` → power graph, per-rail budgets, wiring mass estimate
- `robowire_bench_procedure(netlist)` → the generated continuity/smoke-test/bus-scan
  checklist (markdown + structured form for future Station-5 automation)
- `robowire_render(netlist)` → schematic SVG
- `robowire_diff / robowire_provenance` — as above

## 4. Composition and the assembled robot

- `robotspec_assemble(mech_ref, elec_ref, model_refs, kernel_ref)` (thin third surface,
  may live on either server or a small chuk-mcp-robotspec): validates the derivation
  rule (no hand-entered derivables), computes `body_hash`/`robot_hash`, emits the
  RobotSpec artifact.
- Cross-checks at assembly: robocad placements vs robowire instances must correspond
  (a placed ToF with no netlist instance, or vice versa, is an assembly failure —
  X-codes); sensor bus rates vs kernel manifest expectations (warn tier).
- Downstream, unchanged: the assembled hash is what chuk-arena episodes, the inspector,
  and the claims registry cite. The AI never gets a private path around the factoring.

## 5. The design loop (what this unlocks)

1. **Conversational design:** the success-statement flow — agent iterates geometry and
   harness against checkers in one session, human reviews rendered views and the ledger.
2. **Agent-in-the-search:** the design search (chuk-arena §8) gains an LLM proposer
   alongside CMA-ES/NSGA-II — the agent proposes semantically-motivated candidates
   ("lower the battery, pull ToFs outboard"), the tournament evaluates. *Pre-registered
   question: do LLM-proposed candidates reach the Pareto front faster than the numeric
   optimiser per evaluation spent?* Either answer is a finding.
3. **Design review as a service:** point the agent at an existing RobotSpec →
   check reports + derived record → natural-language review with citations to E/D-codes.
4. **Dogfood note:** an AI designing the body that carries the reflex organ is the SOMA
   programme designing its own soma — the content and research framings coincide.

## 6. Deployment

chukai.io fleet pattern: HTTP MCP endpoints (`robocad.chukai.io/mcp`,
`robowire.chukai.io/mcp`), artifact store shared with the existing servers'
content-addressed conventions, capability/status tools per house style
(`*_capabilities`, `*_status`). Libraries remain the single source: server, CLI, viewer,
and arena-plant link the same derivation/check crates.

## 7. Milestones

- **M0:** robowire server over the M0 check set; agent authors the MVP harness from the
  BOM in one session, including recovering from planted E-code failures.
  *Acceptance: transcript shows propose→E-fail→fix→pass without human edits.*
- **M1:** robocad server over the parametric wedge class + derive/check/render; agent
  reproduces the MVP geometry to stated targets (mass ≤150g, CoG ≤ X mm, cube FITS).
- **M2:** `robotspec_assemble` with cross-checks; first end-to-end AI-authored RobotSpec
  hash accepted by the inspector and cited by a chuk-arena episode.
- **M3:** design-review mode; LLM-proposer hook into the design search (the §5.2
  pre-registered comparison runs).

## 8. Risks & responses

1. **Verifier gaps become AI-scale gaps** (an unchecked failure mode will be found by an
   agent faster than by a human). Response: this is a feature wearing a risk costume —
   agent-discovered escapes become new E/D-codes (bugs-become-rules), and the M0
   planted-fault acceptance test grows with them.
2. **Plausible-but-mediocre design monoculture** (agent converges on safe templates).
   Response: the tournament, not the agent, judges; diversity pressure lives in the
   search layer (novelty terms), not in prompt hope.
3. **Tool-surface churn while the underlying specs are young.** Response: servers version
   with the library crates; v0.x surfaces marked unstable; the artifact formats (hashed)
   are the stability contract, not the tool names.
4. **Catalogue as bottleneck** (agent wants parts that lack entries). Response:
   `parts_request` queue emitting datasheet-provisional entries flagged as such; no
   silent invention of electrical personalities.

## 9. Open questions

- Q1. One combined server (robolab) vs two focused ones? (Lean: two, matching the
  library split and the fleet's one-domain-per-server convention; assemble as a thin
  third.)
- Q2. Should `robocad_render`/`robowire_render` reuse the map-server-style interactive
  artifact pattern (browser-viewable, shareable) rather than static images? (Lean: yes —
  the inspector's scene builder is the natural renderer; static SVG as fallback.)
- Q3. Write access control: design servers on the open fleet, or authenticated —
  given artifacts feed a claims registry? (Lean: authenticated writes, open reads,
  registry ingestion gated regardless.)
- Q4. Does the E/D/X-code taxonomy get a shared registry document now (three specs
  reference it) — a codes.md as the cross-spec index? (Lean: yes, cheap now, painful
  later.) → seeded as [`codes.md`](codes.md).
