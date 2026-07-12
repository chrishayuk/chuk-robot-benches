# energy-sim — Duty-Cycle & Energy-Budget Simulator — Spec v0.1 (draft for review)

**Codename:** energy-sim (working name — see Q1 for the real crate name)
**Status:** Draft — pre-registration pending, nothing built yet
**Position:** a new, separate capability — not an extension of `robosim` (deliberately
stateless/event-driven, no timestep) and not an extension of `chuk-arena/arena-plant`
(deliberately scoped to combat rigid-body/contact physics at an 8kHz tick, seconds-long
episodes). Neither existing system has any concept of energy accumulating over time; this
spec is that concept's first home.
**Companion specs:** robowire (power graph, the electrical facts this consumes),
robotspec (`DerivedRecord`, mass/power roll-up), chuk-arena (a structurally similar but
non-overlapping "virtual bench" precedent — see §1).

---

## 0. Thesis

Two different questions get asked about a robot's power system, and only one of them has
a tool today:

1. *"Is this wiring legal — right voltages, no shorts, adequate wire gauge, a failsafe
   path?"* — robowire's E-checks, static, instantaneous, answered.
2. *"Given how this robot spends its day — sun it gets, time it's active vs idle —
   does the battery survive the mission, or does it die at 3am?"* — nobody answers this.
   robosim only ever answers "what is this circuit doing RIGHT NOW"; it has no
   before/after. arena-plant integrates a rigid body's velocity/position/current over
   time, but only across a combat episode lasting seconds, and its battery model has no
   capacity or state-of-charge at all — current only ever produces instantaneous voltage
   sag, never depletes anything (confirmed by reading its actual source: `RigidState`
   carries `current_a` only, no energy quantity).

That second question is the one a solar-powered, long-duration, autonomous robot lives or
dies by — a weather station, a roaming sensor platform, anything meant to run
unattended for longer than one battery charge. This spec is a mission/duty-cycle
simulator that answers it: given a robot's real power budget (from robowire), an
authored environmental profile (sun exposure over a day or week), and an authored
activity schedule (when it's driving/sensing/sleeping), does state-of-charge stay above
a safe floor across the whole window?

**Success statement:** author a day-long sun profile and a duty cycle for a solar
weather-station design; run the simulator; get back a state-of-charge trace and a single
answer — survives the night, or doesn't, and by how much margin — traceable to the exact
power-graph hash and profile that produced it.

## 1. Non-goals (v0.1)

- **Not a circuit legality checker.** robowire already owns "is this wiring legal"
  (E01-E41); this tool assumes a netlist that already passes and asks a completely
  different question about it.
- **Not a combat/contact physics simulator.** arena-plant's rigid-body/contact/traction
  model, its 8kHz world tick, and its second-scale combat episodes are untouched by this
  spec and vice versa — these are non-overlapping timescales and non-overlapping
  concerns (an in-match impact-spike sag lasting milliseconds is not the same problem as
  a multi-day energy budget, even though both are technically "battery state changing
  over time"). If a solar-powered *combat* robot's in-match SoC ever becomes a real
  question, that is arena-plant's M3 power-graph-consumption milestone, not this one.
- **Not first-principles solar physics.** No I-V curves, no MPPT algorithm simulation, no
  spectral response, no cell temperature derating. A panel is `rated_w` at "full nominal
  sun," scaled by an authored 0.0-1.0 sun-fraction input and a fixed efficiency factor —
  an energy-balance approximation, the same "provisional, datasheet-typical, not a
  physics engine" posture the whole parts catalogue already takes.
- **Not a weather simulator.** Sun/temperature profiles are authored or parametric input
  *data* (a curve you provide or generate from a simple model), never physically
  simulated (no atmospheric modeling, no cloud physics).
- **Not real-time.** Like robowire/robosim/arena-tourney, this runs a simulated mission
  window (hours to weeks) in however long it takes to compute — an offline analysis
  tool, not a live control-loop stand-in and not a firmware emulator.
- **Not a replacement for `robosim`'s instantaneous view.** The two are complementary:
  this tool calls `robosim::run_state` once per tick as a subroutine (see §3) to get
  "what does the circuit draw right now," and owns everything about walking simulated
  time and integrating charge — `robosim` itself gains zero new responsibilities and no
  new concept of time.

## 2. Two separate layers

This spec covers two genuinely different pieces of work, staged deliberately so the
small, low-risk one doesn't wait on the large, unscoped one.

### 2.1 Catalogue layer (small, robowire-side, same shape as every other part addition)

Two new kinds, checked exactly like any other part — no new check codes needed, the
existing ones already apply:

- **`solar-panel`** — a power *source* alongside `battery`, not a passthrough like
  `regulator`. Declares a rated wattage at full nominal sun (`rated_w`) and an open-
  circuit voltage range, analogous to `battery`'s `elec.source`. Whether it's
  *currently* producing anything is a run-mode/energy-sim question (§2.2/§3), not a
  static-legality one — statically, robowire only checks that it's wired with legal
  polarity/voltage/required-pins, same as any source.
- **`charge-controller`** — a passthrough like `regulator` (`VIN` from the panel, `OUT`
  to the battery), with a declared maximum throughput (`max_a` or `max_w`) — the same
  "rated capacity vs worst-case draw" shape E30 already checks for a regulator's
  `power_out` pin, reused as-is.

Both are ordinary catalogue parts. `robowire check` already validates them the moment
they exist — this layer needs no new spec of its own beyond the two part shapes above,
and is buildable independent of, and before, §2.2.

**Open sub-question:** does `robosim`'s Phase 2 seeding (`elec.source.is_none() { continue
}`, `robosim/src/simulate.rs`) need to treat `solar-panel` as an unconditional second
seed (always "on," matching a battery), or should it stay out of run mode's hot/grounded
graph entirely and be visible only to energy-sim? Leaning toward the latter for v0.1: run
mode's boolean hot/not-hot graph has no slot for "partially powered depending on sun," and
forcing a boolean answer here would either lie (always on) or require exactly the
continuous-value graph work this spec's whole reason for existing is to avoid smuggling
into `robosim`. Revisit once §2.2 exists and it's clear whether anyone actually wants a
solar panel to visibly do anything in the click-a-switch designer.

### 2.2 The energy simulator itself (large, new, not yet scoped in code)

Everything else in this spec.

## 3. Core simulation loop

A coarse-grained, event-driven-over-time integrator — nothing here needs sub-millisecond
fidelity, so ticks are minutes, not microseconds:

```
for each tick (dt = e.g. 5-60 simulated minutes, configurable):
    sun_fraction   = env_profile.sun_at(t)              // 0.0-1.0, authored curve
    solar_w        = panel.rated_w * sun_fraction * panel.efficiency
    controller_w   = min(solar_w, charge_controller.max_w)

    active_inputs  = duty_cycle.inputs_at(t)            // a RunInputs snapshot: what's
                                                         // switched on/driving right now
    st             = robosim::run_state(netlist, catalogue, active_inputs)
    load_w         = st.instances["batt"].current_a * bus_voltage

    net_w          = controller_w - load_w
    battery.energy_wh = clamp(battery.energy_wh + net_w * (dt_minutes / 60.0),
                               0.0, battery.capacity_wh)

    record(t, battery.energy_wh / battery.capacity_wh, solar_w, load_w)
```

The key architectural move: **`robosim::run_state` is called unchanged, as a pure
subroutine, once per tick.** It still has no idea time exists — it answers exactly the
question it already answers ("given this input snapshot, what does the circuit draw
right now"), and energy-sim is entirely responsible for constructing the sequence of
snapshots and doing the accumulation robosim was never meant to own. No changes to
robosim's own state model are implied by this spec.

- **Inputs:** a netlist + catalogue (robowire's existing schema, unmodified); a solar/
  battery/charge-controller parameter set (either read off the catalogue parts in §2.1,
  or hand-specified for a scenario that predates them); an environmental sun-fraction
  profile over the simulated window; a duty-cycle schedule mapping simulated time to a
  `RunInputs` snapshot (which switches are closed, which sensors are active, when the
  drive motors run, if any).
- **Outputs:** a state-of-charge trace over the window; min/max/final SoC; whether SoC
  ever crossed a declared safe floor (and when); total energy harvested vs consumed.

## 4. Consumers

| Consumer | Takes |
|---|---|
| Robot designer | survives-the-mission verdict + SoC trace for a solar/long-duration build |
| robowire | power-budget facts (worst-case draw, rail topology) as energy-sim's load input |
| robotspec | `DerivedRecord` citation for the netlist/robot version a scenario ran against |
| (not) arena-plant | explicitly no consumer relationship in v0.1 — see §1 |

## 5. Milestones

- **M0 — done:** the two catalogue parts (§2.1) — `solar-panel-9v-2w`, `charge-controller-
  small` — checked by existing E-codes, no new crate yet. Both kinds reuse the existing
  `power_out`/`power_in`/`max_a` generic role-checking (E02/E30/E40) with zero new check
  logic; `solar-panel` deliberately has no `power_in` pin and no `elec.source`, so it
  never becomes a passthrough candidate or a hot-graph seed in robosim (proven, not just
  asserted — `solar_panel_never_seeds_the_hot_graph_by_design`, `robosim/tests/
  run_state.rs`). `charge-controller` got the same generic `powered`/current-draw
  projection every other passthrough kind (`regulator`, `esc`) already has. *Acceptance,
  met: `example-solar-charging-demo.json` (panel → charge controller → battery → LED
  load) passes robowire's checker fully; `lesson-e30-undersized-charge-controller.json`
  (same topology, downstream motor draw exceeding the charge controller's 0.5A rating)
  fails exactly E30 ("cc.OUT: worst-case downstream draw 1.61A exceeds its rated
  0.50A").*
- **M1:** first energy-sim crate — hand-specified duty cycle and sun profile (no robowire
  integration yet), the core loop in §3, a CLI reporting a SoC trace for a synthetic
  scenario. *Acceptance: a scenario with generous sun and light load shows SoC holding
  near full; the same scenario with sun set to zero for 12 simulated hours shows SoC
  monotonically declining and crossing a configurable floor at a specific, checkable
  tick.*
- **M2:** real integration — read `rated_w`/`max_w`/`capacity_wh` off actual catalogue
  parts and a real netlist via robowire, rather than hand-specified numbers; duty cycle
  authored against real instance names in that netlist.
- **M3:** richer environmental profiles (multi-day, weather-derived sun curves — still
  authored/parametric, not simulated per §1); scenario library the same way
  `harness/examples` and `arena-bench` are libraries of standing scenarios, not one-offs.

## 6. Risks & responses

1. **Scope creep toward a real solar/thermal physics engine.** Response: the non-goal
   wall in §1 is explicit and the energy-balance approximation is the whole point —
   revisit only if a specific, named prediction actually needs I-V-curve fidelity, and
   treat that as a pre-registered scope change, not a drift.
2. **Duplicating arena-plant's tick loop.** Response: different timescale (minutes vs
   125µs), different state (energy/SoC vs rigid-body pose), different purpose (mission
   survival vs combat physics) — kept as a structurally similar but deliberately separate
   "virtual bench," the same relationship arena-bench's dyno/tilt/shove benches already
   have to each other despite sharing a physics core underneath. This one has no shared
   core to begin with.
3. **robosim accreting time-awareness by the back door.** Response: the call boundary in
   §3 is the whole design — robosim's function signature doesn't change, it's called
   repeatedly by an external loop that owns every bit of the "over time" concept. If a
   future change to robosim's own API is ever proposed to "make this easier," that's a
   signal this boundary is being violated, not a convenience worth taking.

## 7. Open questions

- Q1. Real crate/codename — `energy-sim` here is a placeholder. Candidates matching this
  toolchain's `robo-` prefix convention (robowire/robosim/robotspec/roboparts):
  `robobudget`, `roboduty`. (Lean: no lean yet — pick when M1 actually starts.)
- Q2. Does `RunInputs`-per-tick get authored directly (a literal schedule of snapshots),
  or does it want its own small DSL/schema (e.g. "active 08:00-18:00, sensors-only
  otherwise")? (Lean: start literal/explicit for M1, since the whole point is staying
  small until the shape earns a DSL — the same lesson robowire's own netlist format
  already learned.)
- Q3. Where does a sun-fraction profile come from in practice — a hand-authored curve, a
  simple parametric day/night model (e.g. a clipped sinusoid), or eventually real
  recorded irradiance data? (Lean: parametric model first, matching §1's "authored, not
  simulated" stance — a recorded-data importer is a later, separable feature.)
- Q4. Does §2.1's `solar-panel` kind ever need to show *something* in robosim's run mode
  (the interactive designer), even without a full graph-model change — e.g. a purely
  cosmetic "producing/not producing" indicator driven by a run-mode sun-fraction slider,
  with zero effect on the hot/grounded boolean graph? (Lean: revisit after M1 makes it
  concrete what people actually want to see; don't build a cosmetic feature against a
  capability that doesn't exist yet.)
- Q5. Multi-battery/multi-source mission profiles (a swappable-battery field robot, not
  just a fixed-install weather station) — in scope for v0.1 or deferred? (Lean: deferred;
  the single-battery-single-panel case is the whole v0.1 acceptance bar in §5.)
