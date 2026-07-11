# Check-code registry — E / D / X codes (cross-spec index)

**Status:** seeded per design-servers Q4 lean. This file is the single index; the owning
spec holds each code's full definition. **Intake rule (bugs-become-rules):** every
physical or agent-discovered failure that was statically detectable becomes a new code
here first, then a check in the owning tool. Codes are never renumbered or reused.

## E-codes — electrical (owner: robowire §3)

| Code | Check | Tier |
|---|---|---|
| E01 | every motor terminal pair reaches exactly one driver channel | fail |
| E02 | power pins reach a rail of legal voltage (catalogue range) | fail |
| E03 | polarity continuity — no +/− swap reachable through the net graph | fail |
| E04 | no floating required pins | fail |
| E10 | MCU nets use pins with required capability (PWM/ADC/UART/I²C) | fail |
| E11 | no pin double-booked; no muxed-function capability conflicts | fail |
| E20 | I²C address collisions after reassignment plan (dual-0x29 classic) | fail |
| E21 | bus voltage-domain consistency | fail |
| E22 | device rates vs bus bandwidth budget | warn |
| E30 | per-rail worst-case draw vs source capability (C-rating, regulator) | fail |
| E31 | wire gauge / connector rating vs worst-case segment current | fail |
| E32 | brownout topology — MCU rail exposure to motor-stall sag | warn |
| E33 | LED without series current limiting (neither adjacent net contains a resistor) | fail |
| E40 | switch/removable link in main power path, reachable (tech-check) | fail |
| E41 | failsafe stop chain declared and complete | fail |
| E42 | exposed-conductor on high-current nets without insulation class | warn |

## D-codes — design/geometry (owner: robocad, design-servers §2; numbering provisional until robocad M0 freezes it)

| Code | Check | Tier |
|---|---|---|
| D01 | cube violation (bbox vs 101.6mm) | fail |
| D02 | mass over class limit | fail |
| D03 | component collision / overlap | fail |
| D04 | sensor cone occluded by chassis | fail |
| D05 | wheel–chassis interference | fail |
| D06 | unreachable CoG target | fail |

## X-codes — assembly cross-checks (owner: design-servers §4; numbering provisional)

| Code | Check | Tier |
|---|---|---|
| X01 | robocad placement without robowire instance (or vice versa) | fail |
| X02 | sensor bus rates vs kernel manifest expectations | warn |
| X03 | hand-entered derivable quantity (derivation-rule violation, RobotSpec §2) | fail |
