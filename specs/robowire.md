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
  SPICE. Dynamic electrical behaviour (sag under hit-spike) is arena-plant's job,
  consuming robowire's derived graph.
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

## 4. Derived outputs

1. **Power graph** → RobotSpec `power:` section and arena-plant brownout model (rails,
   chains, sense points, per-segment resistance from gauge+length).
2. **Wiring mass estimate** → RobotSpec mass roll-up (gauge × length × density + connector
   masses) — wiring is 3–5g of a 150g budget; it should be derived, not guessed.
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
- **M1:** power budget checks (E30–E32) + power graph derivation into RobotSpec; wiring
  mass derivation.
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
