# robowire — Wiring Definition & Verification Tool — Spec v0.1 (draft for review)

**Codename:** robowire (the electrical truth)
**Status:** Draft — pre-registration pending
**Position:** authoring and verification tool for RobotSpec's electrical source (§2:
`elec {ref+hash}`). robowire's netlist *is* that source; its derived power graph feeds
arena-plant's brownout scenarios and the inspector's rail overlay.
**Companion specs:** RobotSpec (consumer of the netlist), chuk-arena (consumer of the
power graph), robotspec-viewer (renders rails), lab plan Station 3/5 (physical
verification procedures are generated here).

---

## 0. Thesis

Wiring is the leading killer of kit-class robots — reversed polarity, wrong pins, I²C
address collisions, undersized wires, missing failsafe paths — and every one of those
failures is *statically detectable* from a netlist plus a parts catalogue. robowire makes
the wiring a checked artifact: authored as data, verified by electrical rule checks
before a joint is soldered, and compiled into both the simulator's power model and a
printable bench procedure for verifying the physical harness matches the design.

**Success statement:** "motors connect to blah blah" is typed once as a netlist; the tool
answers — is every connection electrically legal, does every rail carry its worst-case
load, does the failsafe path exist, do the buses address cleanly — and then prints the
multimeter checklist that proves the built harness is the designed harness.

## 1. Non-goals (v0.1)

- **Not PCB ECAD.** No board layout, no traces, no Gerbers. robowire covers the *harness*
  — off-board wiring between modules (battery, switch, ESC, motors, MCU board, sensors,
  receiver). When the custom reflex-organ PCB arrives, its ECAD is a *part* in the
  catalogue with declared pins; robowire wires between parts.
- **Not a circuit simulator.** Static rule checks and worst-case budget arithmetic, not
  SPICE. Time-domain dynamic electrical behaviour (a transient sag under an impact
  spike, recovery curves) is arena-plant's job, consuming robowire's derived power
  graph. A battery's steady-state terminal-voltage sag AT ITS CURRENT operating point
  (`current_a * r_internal_ohm`, one-shot, not iterative — `battery_sag_v` in
  `checks.rs`) is a deliberate, called-out exception: single-sourced in robowire so
  both robosim's live run-mode display (§3a) and any future arena-plant/M1 power-graph
  consumer read the identical number, never two divergent formulas.
- **Not schematic-capture UI in v0.1.** Netlist is authored as structured text
  (reviewable, diffable, hashable — house style); the tool renders diagrams *from* it.
  Graphical editing is a v2 question, not a v1 promise.

## 2. Inputs

### 2.1 Parts catalogue (shared, answers RobotSpec Q2)

A versioned `parts/` catalogue owned jointly with RobotSpec/arena-sense — one entry per
part number, declaring the electrical personality:

```
part: vl53l0x-breakout
  pins: [VIN(2.6-5.5V), GND, SCL(i2c), SDA(i2c), XSHUT(gpio-in), GPIO1(int-out)]
  bus: i2c { default_addr: 0x29, addr_reassignable: true, requires: XSHUT-per-device }
  draw_mA: { idle: 6, active: 19, peak: 40 }
part: n20-motor-6v
  pins: [M+, M-]
  stall_A: 1.6, nominal_V: 6, notes: brushed
part: rp2350-zero
  pins: [3V3(out,300mA), 5V(in), GND×N, GP0..GP28(gpio: pwm|i2c|spi|adc capable map)]
  ...
```

Catalogue entries are content-hashed; a netlist cites parts by `part@hash`.

### 2.2 Netlist (the authored artifact)

Instances + nets + rails + buses, structured text:

```
instance: batt   = lipo-2s-260   ; esc = bbb-dual-esc ; mcu = rp2350-zero
instance: m_l    = n20-motor-6v  ; m_r = n20-motor-6v
instance: tof_l  = vl53l0x-breakout ; tof_r = vl53l0x-breakout
instance: sw     = power-switch-slide

rail VBAT: batt.+ -> sw.in ; sw.out -> esc.VIN, mcu.5V     ; wire 26awg
rail 3V3:  mcu.3V3 -> tof_l.VIN, tof_r.VIN, imu.VIN
net:  esc.M1± -> m_l.M±  ;  esc.M2± -> m_r.M±
bus i2c0 (mcu.GP4/GP5): imu@0x68, tof_l@0x29->0x2A via xshut mcu.GP6,
                        tof_r@0x29 via xshut mcu.GP7
net:  rx.CRSF -> mcu.GP1(uart-rx)
```

The netlist hash is RobotSpec's `elec.ref+hash`. Every wire may carry gauge and length
(for loss/mass derivation); defaults per rail class.

## 3. Electrical rule checks (the heart)

Each check is named, versioned, and yields pass / warn / fail with the offending element.
v0.1 set, drawn from how ant-class bots actually die:

**Connectivity & polarity**
- E01 every motor terminal pair reaches exactly one driver channel
- E02 every part's power pins reach a rail of legal voltage (per catalogue range)
- E03 polarity continuity: no +/− swap reachable through the net graph
- E04 no floating required pins (catalogue marks required vs optional)

**Pin legality (MCU)**
- E10 every MCU net uses a pin with the required capability (PWM-capable for ESC signal,
  ADC for current sense, UART for CRSF, I²C pins on an I²C-capable pair)
- E11 no pin double-booked across nets; no capability conflicts between muxed functions

**Buses**
- E20 I²C address collisions after reassignment plan (two VL53L0X at default 0x29 without
  per-device XSHUT = fail — the classic)
- E21 bus voltage-domain consistency (no 5V device on a 3.3V bus without level shift)
- E22 declared device rates vs bus bandwidth budget (warn-level)

**Power budget**
- E30 per-rail worst-case draw (Σ peak, motors at stall) vs source capability: battery
  C-rating × capacity vs total stall; regulator output (mcu.3V3 @300mA) vs sensor sum
- E31 wire gauge vs worst-case current per segment (ampacity table); connector ratings
- E32 brownout topology: MCU rail's exposure to motor-stall sag — warn if MCU supply
  shares the unbuffered motor rail (recommend decoupling per catalogue rules)

**Safety & compliance (rules-derived)**
- E40 a switch or removable link interrupts the main power path (AWS tech-check
  requirement) and is reachable without disassembly (declared property)
- E41 failsafe path exists: receiver loss-of-signal behaviour → ESC/MCU stop chain is
  declared and complete
- E42 exposed-conductor warnings for high-current nets without declared insulation class

Check set is extensible; every new physical failure in the lab that was statically
detectable becomes a new E-code (the compose-linker discipline: bugs become rules).

**Teaching layer.** The checker doubles as a tutor: `robowire explain <netlist.json>`
prints the same plain-English per-net prose the designer shows (`robowire::prose`,
single-sourced — no separate CLI-only copy), and `robowire explain-error <CODE>` prints a
what/why/fix explanation for a check code (`robowire::teach`), independent of any one
netlist. `harness/examples/lesson-*.json` pairs a legal harness with a deliberately-broken
variant per code (`lesson-<code>-<name>.json`), auto-verified by
`examples_are_legal_and_lessons_fail_their_named_code` — predict whether it should pass,
run the checker, then read `explain-error` for the code it names.

`harness/lessons/NN-slug[.json]/NN-slug-broken.json` is a separate, ordered curriculum —
"start from the real basics and work up." Stages 1-2 are standalone foundational
vignettes (neither needs a motor at all); the real accumulating build
(`motor_stages_strictly_accumulate`, `robowire/tests/lessons.rs`) runs from stage 3
onward, each a strict superset of the last: `01-basics` (battery/switch/resistor/LED,
E33) → `02-regulator` (+standalone 5V regulator, no ESC/BEC involved — a 3S battery on a
part only rated to 9V, E02, the classic wrong-cell-count mismatch) → `03-motor-driver`
(+ESC+motor, E40) → `04-brain-and-radio` (+MCU+receiver, BEC/PWM/UART/failsafe, E41) →
`05-shared-5v-rail` (+servo — its broken variant deliberately fails two codes, E02
overvoltage and E32 brownout, from one mistake: wiring the brain straight to the battery
instead of through the BEC) → `06-sensor-bus` (+2×ToF, the dual-0x29 classic, E20) →
`07-two-wheel-drive` (+second drive motor on `esc.M2`/`mcu.GP7` — a capstone matching
`mvp-wedge-harness.json`'s real 2WD topology; its broken variant plants the classic "both
motors wired to the same channel" mistake, E01). `rp2350-zero`'s `GP6`/`GP7` gained
`analog`/`pwm` capabilities respectively to make room for this and the sensor catalogue
below, widening the catalogue rather than working around it.

The sensor catalogue also grew a `light` kind (`line-sensor-analog` — an analog
reflectance/photoresistor line-and-edge sensor, not on any bus, sharing `tof`/`imu`'s
exact fake-reading/current-draw component shape) and an `env` kind (`env-bme280` — an
I2C temp/humidity/pressure breakout; this model's single scalar `value` field can only
carry ONE representative reading, documented on the part rather than silently implied),
plus a longer-range `tof` part (`tof-longrange`, needs 5V rather than 3.3V, a real
power-budget tradeoff). `env` is general robotics-teaching breadth, not something an
antweight combat robot would ever wire in — documented as such on the part itself.

The designer has a **teaching mode** (mirrors run mode's toggle) that puts this loop
directly in the UI: the sidebar renders the numbered curriculum in order (not the flat
`lesson-*.json` drills, which stay in the normal sidebar), editing stays live (unlike run
mode — a repair exercise means fixing the broken netlist, not just watching it), and a
teach panel shows the what/why/fix for whichever check is currently failing (computed
live, not parsed from a filename) or any check row clicked — fed by a new wasm export
(`explain_error_json`) over the same `robowire::teach` content the CLI uses.

## 3a. Interactive run mode (designer)

The netlist plus catalogue support one more read: not just "is this legal" (§3) but
"what does this DO right now" — click the switch, the LED lights; set a throttle, the
motor spins. This is the designer's client-side stand-in for the bench technician who
manually drives test points with a bench supply and probes, standing in for the
not-yet-written reflex-kernel firmware. It is the interactive virtual twin of the
generated bench verification procedure (§4 item 4) — the same rehearsal, live in the
browser instead of printed on paper, before a single joint is soldered. The premise is
literal: a design is built and proven *virtually* here before it is ever wired
physically, so the numbers have to be right, not just plausible.

Owned by the standalone **`robosim`** crate, not robowire itself — netlist + catalogue
in, live per-net/per-instance state out. Kept separate so the simulator can gain
consumers beyond the designer (arena-plant; a future real firmware emulator standing in
for the human) without coupling them to netlist authoring or the E-check toolchain. It
re-uses the E-check rule logic directly from `robowire::checks`
(`led_current_limited`, `bus_final_addresses`, `motor_output_pin`) rather than
re-deriving it, so a check and a run-mode projection can never disagree.

Explicitly **not** a retreat from the non-goal wall (§1): no SPICE, no continuous
dynamics, no timeline/trace, no firmware execution. Every net's hot/grounded state is
event-driven boolean propagation over a kind/role-gated reachability graph:
switch/button open/closed (user input), regulator/ESC-BEC/MCU-3V3-out passthrough
(gated on the instance's own ground actually being connected), resistor/wiring
always-conducting, seeded from the battery. arena-plant's *time-domain* dynamic
sag/brownout behaviour (a transient dip and recovery curve under an impact spike)
remains strictly downstream and out of scope here; run mode doesn't depend on the power
graph (§4 item 1, not yet built). The one deliberate exception (§1): a battery's
steady-state terminal-voltage sag at its *current* operating point is shown live
(`InstanceRunState.sag_v`), computed by the same single-sourced `battery_sag_v` helper
E30's static budget arithmetic could also use — not a re-derivation, and not the
transient behaviour the paragraph above still excludes. Likewise, a net's declared wire
gauge/length (`Net.gauge_awg`/`length_mm`) yields a live, one-shot IR-drop annotation
(`NetRunState.wire_drop_v`) alongside E31's static ampacity check — both display-only,
neither feeds back into any other net's already-computed current or voltage (that would
require re-solving the whole graph, the iterative step this model exists to avoid).

**Every electrical value is real component math, never a fixed lookup.** Each net
carries a live **voltage**: its own schema-declared `Net.volts` when directly authored,
or — for the many intermediate nets nobody bothers to hand-annotate (the wire between a
closed switch and the next component) — inherited from whatever it's *ideally*
(losslessly) connected to, by propagating declared voltages across switch/button/wiring
bridges only. A resistor or a regulated passthrough output is a real voltage boundary
and never inherits this way; without that distinction, an unannotated net downstream of
a closed switch would wrongly read 0V. Current is Ohm's law against that live voltage,
using catalogue-declared REAL component properties: a resistor's `ohms` and an LED's
`forward_v` solve I = (V − Vf) / R for a lit, current-limited LED (the standard
LED+resistor hand-calculation — a fixed-Vf diode approximation, not an iterative
nonlinear SPICE solve); a motor's winding resistance (`nominal_v / stall_current_a`)
scales its current with both throttle and the actual supply voltage; a fixed-power
device's equivalent resistance (`nominal_v / current_ma`) does the same for anything
else with a declared rated operating point (sensors, MCU, ESC/regulator quiescent draw,
radio, buzzer). Every net's current is the Σ of every such load reachable downstream of
it over the same graph as `hot` — worst-case-style summation (still not a
current-divider/Kirchhoff solve), but now built from real per-component resistances
instead of a flat "fixed mA" figure, so **if the voltage changes, the current changes
with it**. A part missing the catalogue fields a calculation needs contributes 0A
rather than a guess.

- **Inputs:** switch/button state (user-toggled/held), per-motor throttle, per-sensor
  fake reading (there is no firmware yet to generate a real one — see `robosim`'s
  module docs for the seam a future emulator would plug into instead).
- **Outputs:** per-net energized state (hot/grounded) + live volts/amps; per-instance
  projection — `powered` (regulator/ESC/MCU/sensor/radio/buzzer), `closed`
  (switch/button), `lit` + `current_limited` (LED, with a `reason` when dark or
  unprotected), `spin` + `current_a` (motor), `current_a` (battery, its own net's amps;
  regulator/ESC/MCU/sensor/radio/buzzer, their own equivalent-resistance draw),
  `value` + `bus_conflict` (bus sensors).

## 4. Derived outputs

1. **Power graph** → RobotSpec `power:` section and arena-plant brownout model (rails,
   chains, sense points, per-segment resistance from gauge+length). **Done** —
   `robowire::power_graph::derive_power_graph` (one `PowerRail` per battery/regulator
   `power_out` pin with a declared capacity, the same sources E30 checks; one
   `WireSegment` per gauge-declared net, the same nets E31 checks; one `PowerChain` per
   motor, source→ESC→motor). `sense_points` is honestly left empty — no current-sense
   part exists in the catalogue yet, so nothing is fabricated there. `PowerGraph` is a
   plain-data shape owned by `robotspec` (so it can live on `DerivedRecord`) that
   `robowire` — which already depends on `robotspec` — constructs directly.
2. **Wiring mass estimate** → RobotSpec mass roll-up (gauge × length × density + connector
   masses) — wiring is 3–5g of a 150g budget; it should be derived, not guessed. **Done**
   — `robowire::power_graph::wiring_mass_g` sums bare-copper conductor mass (derived from
   copper resistivity + density against the same resistance table E31's ampacity check
   uses, not a second independent reference) over every gauge+length-declared net, plus
   the catalogue `mass_g` of every connector/fuse/PTC instance in the netlist. This
   retires the old flat `harness-allowance` placeholder part entirely (removed from the
   catalogue and from `robots/mvp-wedge.json`'s `components[]`); `attach_power_graph`
   folds the derived figure into `mass_total_g`/`budget_margin_g` and re-runs D02 against
   the corrected total, via the new `robowire power <netlist> --robot <robot.json>` CLI
   command (`robotspec::derive()` itself is untouched — `mass_wiring_g: 0.0`, `power:
   None` for any bare, non-merged call).
3. **Diagram render** — logical schematic view (rails left-to-right, buses grouped),
   generated from the netlist; SVG output suitable for the inspector and the build sheet.
4. **Bench verification procedure (the physical test)** — a generated, ordered checklist
   binding design to build:
   - continuity list: "probe batt.+ ↔ esc.VIN: expect <0.5Ω; probe batt.+ ↔ batt.−:
     expect open (switch off)"
   - polarity list before first power
   - staged smoke-test order: rails unloaded → MCU only → sensors → ESC, no motors →
     full, with expected voltages/currents at each stage
   - bus enumeration: expected I²C scan result (the address map, post-reassignment)
   Completing the checklist *is* the as-built electrical record (RobotSpec §5 ritual for
   the electrical domain; scale and tilt table's sibling).
5. **Netlist hash** → RobotSpec identity chain.

## 5. Consumers

| Consumer | Takes |
|---|---|
| RobotSpec | netlist hash (source), power graph + wiring mass (derived) |
| arena-plant | power graph for sag/brownout scenarios |
| Inspector | rail topology for click-to-highlight overlay (viewer M3) |
| Station 3 (build) | diagram + build sheet |
| Station 5 (HIL) | bench procedure + expected bus enumeration |
| Claims registry | E-check report hash accompanying any electrical claim |

## 6. Milestones

- **M0 (wk 1–2):** netlist format + parts catalogue seeded with the MVP BOM (battery,
  switch, BBB ESC, N20s, RP2350, IMU, 2×VL53L0X, ELRS rx); checks E01–E04, E10–E11,
  E20–E21, E40–E41. *Acceptance: the MVP wedge's harness passes, and deliberately
  broken variants (swapped polarity, dual-0x29, missing switch) fail with correct
  E-codes.*
- **M0.5:** interactive run mode (§3a), in the standalone `robosim` crate — click-to-
  toggle switch/button, throttle + fake-sensor + dial controls, event-driven net
  energization, real Ohm's-law voltage/current per net and component (resistor
  `ohms`, potentiometer live `ohms` from dial position, LED `forward_v`, motor winding
  resistance, fixed-power equivalent resistance — never a fixed figure), animated
  wire-flow visualization, user-draggable wire bend points (2D, 3D, and while running),
  a weighted auto-arrange pass, no firmware/timeline. Depends only on M0 (schema +
  checks), not on M1's power graph. *Acceptance: a green `run_state` test suite against
  the MVP wedge harness and dedicated demo harnesses exercising
  switch+LED+motor+sensor+button and a potentiometer dimmer,
  including tests proving current changes when voltage does.*
- **M1 — done:** power budget checks (E30–E32): worst-case per-rail draw vs battery
  C-rating/regulator `max_a`, wire gauge vs an AWG ampacity table (`Net.gauge_awg`/
  `length_mm`), MCU/motor-rail brownout topology warning — plus a live, one-shot wire
  IR-drop and battery terminal-voltage sag shown in run mode (§3a), single-sourced from
  the same helpers. Power graph derivation into RobotSpec's `power:` section and wiring
  mass roll-up (§4 items 1–2): also done — `robowire power <netlist> --robot <robot>`
  produces the merged, wiring-inclusive `DerivedRecord`; the old flat `harness-allowance`
  mass placeholder is retired.
- **M2:** diagram render (SVG) + generated bench verification procedure; first physical
  harness verified against its checklist (the electrical as-built ritual goes live).
- **M3:** arena-plant consumes the power graph — first brownout scenario runs against a
  derived, not hand-modelled, electrical topology.

## 7. Risks & responses

1. **Catalogue burden** (every part needs an entry). Response: the MVP BOM is ~10 parts;
   entries are small; and an unmodelled part is exactly the situation where wiring errors
   happen — the burden is the point. Datasheet-provisional entries flagged as such.
2. **False confidence** (ERC passes ≠ harness works — cold joints, chafe, crimp quality
   are physical). Response: the tool's claim is scoped — *statically detectable* faults;
   the generated bench procedure exists precisely because the rest is physical.
3. **Netlist/text UX friction vs "just wire it".** Response: the MVP netlist is ~20
   lines; the E20 dual-ToF collision alone repays the typing on day one.
4. **Scope creep toward ECAD.** Response: non-goal wall; the PCB, when it comes, enters
   as a part.

## 8. Open questions

- Q1. Format: bespoke minimal DSL (as sketched) vs structured data (TOML/JSON) with the
  DSL as sugar? (Lean: structured data canonical, DSL front-end optional — hashing and
  tooling want the data form.)
- Q2. Should E-checks run in CI on every netlist change, scoreboard-style, like the cell
  corpus? (Lean: yes — it's the same overnight loop, near-zero cost.)
- Q3. Does the bench procedure generator eventually drive Station 5 semi-automatically
  (expected I²C scan checked by the HIL rig itself, not by eye)? (Lean: yes at M3+ —
  the rig already talks to the bus.)
- Q4. Connector taxonomy: model connectors as parts (with mating rules E-checkable) or as
  wire attributes? (Lean: parts — mis-mating and rating are real failure modes.)
