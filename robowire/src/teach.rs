//! `robowire explain-error <CODE>` — a teaching layer over the checker
//! (specs/codes.md), for someone learning electronics by making mistakes
//! safely in JSON rather than someone who already knows what "I2C address
//! collision" means. Content lives here as data, deliberately separate
//! from `checks.rs`'s `detail` strings: those are terse and specific to one
//! netlist ("bus 'i2c0': [\"tof_l\", \"tof_r\"] collide at 0x29"); this is
//! general and tutorial (what the rule means, why it matters, how to fix
//! it), the same three questions a bench technician would actually ask.

pub struct ErrorExplanation {
    pub code: &'static str,
    pub what: &'static str,
    pub why: &'static str,
    pub fix: &'static str,
}

const EXPLANATIONS: &[ErrorExplanation] = &[
    ErrorExplanation {
        code: "E01",
        what: "A motor has two terminals, and both must land on the SAME driver channel of \
               the same ESC — one on M1+, the other on M1-, never split across two channels \
               or two different ESCs.",
        why: "If a motor's two wires end up on different channels (or one lands on nothing), \
              the ESC can't actually drive it — you'd get no motion, a motor that only \
              twitches, or two channels fighting each other through the motor.",
        fix: "Trace both of the motor's wires back to the same ESC output pair (e.g. M1+ and \
              M1- on the same channel), and check nothing else is also wired to those pins.",
    },
    ErrorExplanation {
        code: "E05",
        what: "A motor is either brushed or brushless, and an ESC's driver circuit is built \
               for one or the other — never both. This checks that a motor's declared winding \
               type matches the winding type its driving ESC actually supports.",
        why: "A brushed and a brushless ESC are genuinely different circuits (one just \
              switches DC, the other commutates three phases) — plugging a brushless motor \
              into a brushed-only ESC (or the reverse) isn't a \"runs worse\" problem, it \
              simply won't spin at all, and that's a confusing thing to debug on the bench \
              when everything else checks out fine.",
        fix: "Match the ESC to the motor's actual winding type — swap in a brushless ESC for a \
              brushless motor, or a brushed one for a brushed motor.",
    },
    ErrorExplanation {
        code: "E02",
        what: "Every part that takes power declares a safe voltage window (e.g. a 3.3V sensor \
               might accept 2.6-5.5V). This check walks every power pin and confirms the net \
               it's actually wired to falls inside that window.",
        why: "Feed a part more voltage than it's rated for and it can overheat or die \
              instantly; feed it too little and it may brown out or behave erratically. This \
              is one of the most common ways to destroy a part on first power-up.",
        fix: "Either move the part to a rail that's actually in range (usually via a \
              regulator), or swap in a part rated for the rail you already have.",
    },
    ErrorExplanation {
        code: "E03",
        what: "Checks that no single wire (net) ever mixes a supply/positive role with a \
               ground role — nothing wires + to - by mistake.",
        why: "Reversed polarity is one of the fastest ways to release the magic smoke: LEDs, \
              ICs, and many sensors have no protection against being powered backwards.",
        fix: "Re-check the two wires you most recently changed — a plus/minus swap almost \
              always traces back to the last thing you touched.",
    },
    ErrorExplanation {
        code: "E04",
        what: "Some pins are marked \"required\" in the parts catalogue — the part \
               fundamentally cannot work without them connected (e.g. an ESC's signal \
               input). This check confirms every required pin actually appears on some wire.",
        why: "A required pin left unconnected doesn't fail loudly — it just leaves that part \
              dead or its behaviour undefined, which is a confusing thing to debug on a real \
              board.",
        fix: "Find the floating pin named in the failure and wire it to whatever it's \
              supposed to reach (ground, a signal line, or a rail).",
    },
    ErrorExplanation {
        code: "E10",
        what: "A microcontroller's pins aren't interchangeable — only some can do PWM, only \
               some can do I2C, only some can receive UART. This checks that whatever a wire \
               asks a pin to do is something that pin actually supports.",
        why: "Wiring a PWM signal to a plain digital pin doesn't produce an error on the \
              bench — it just silently doesn't work, a much harder bug to spot than a meter \
              reading wrong.",
        fix: "Check the MCU's datasheet/pinout for which pins support the capability you \
              need, and move the wire there.",
    },
    ErrorExplanation {
        code: "E11",
        what: "Confirms no single MCU pin is asked to do two different jobs at once (e.g. \
               driving a PWM signal AND being the I2C clock line simultaneously).",
        why: "A pin can only do one job at a time — double-booking it means one of the two \
              functions is silently broken, and which one depends on wiring order, making it \
              a nasty intermittent bug.",
        fix: "Free up a pin — most MCUs have several general-purpose pins to spare — and move \
              one of the two conflicting jobs there.",
    },
    ErrorExplanation {
        code: "E20",
        what: "I2C is a shared bus — every device on it answers to an address, and the master \
               finds the right device by calling its address. Two devices with the same \
               address means both would try to answer the same call.",
        why: "This is the classic mistake with two identical sensors (e.g. two of the same \
              ToF sensor) fresh out of the box — they both boot up at their factory-default \
              address.",
        fix: "Reassign one device to a different address at boot (usually via an XSHUT pin \
              held low until the first device's address is changed), or put the second \
              device on a separate bus.",
    },
    ErrorExplanation {
        code: "E21",
        what: "All devices sharing an I2C bus need to agree on the same logic-level voltage \
               (typically 3.3V or 5V) for their signalling to be readable.",
        why: "A 5V device's \"high\" signal can look ambiguous or even damaging to a \
              3.3V-only device, and a 3.3V \"high\" may not register as a high at all to a \
              5V device.",
        fix: "Either pick devices that share a voltage domain, or add a level shifter between \
              the mismatched device and the rest of the bus.",
    },
    ErrorExplanation {
        code: "E30",
        what: "Adds up the worst-case current every part on a rail could draw at once \
               (motors at full stall, sensors at rated draw) and compares it against what the \
               battery or regulator feeding that rail can actually supply.",
        why: "A source asked for more current than it can deliver will sag, brown out, or in \
              the worst case overheat — and \"it worked on the bench with one motor\" is \
              exactly how people get caught out, since stall current only shows up under load \
              (a jam, a wall, a fight).",
        fix: "Either upgrade the source (a higher C-rating battery, a beefier regulator) or \
              reduce the load on that rail.",
    },
    ErrorExplanation {
        code: "E31",
        what: "A thinner wire (higher AWG number) can only safely carry so much current \
               before it heats up. This compares the worst-case current through each \
               gauge-declared wire (and each rated connector/fuse) against its ampacity.",
        why: "An undersized wire under sustained high current gets hot enough to melt its \
              insulation — a real fire risk, not a theoretical one, especially on a combat \
              robot's main power feed.",
        fix: "Use a thicker wire (lower AWG number) for that segment, or a higher-rated \
              connector/fuse.",
    },
    ErrorExplanation {
        code: "E32",
        what: "Warns when a microcontroller's own power rail is wired directly off the same, \
               unbuffered feed as a motor — no regulator/BEC between them soaking up the \
               motor's current swings. (This is a warning, not a hard failure.)",
        why: "A motor under load (a stall, a hit) pulls a current spike that sags the \
              battery's voltage momentarily — if the brain shares that same unbuffered rail, \
              it can brown out and reset at exactly the worst moment, mid-match.",
        fix: "Feed the brain from its own regulated rail (a BEC/regulator output) rather than \
              straight off the battery/motor rail.",
    },
    ErrorExplanation {
        code: "E33",
        what: "An LED has almost no internal resistance of its own — it needs a resistor (or \
               equivalent) in series to limit how much current flows through it.",
        why: "An LED wired directly across a rail with nothing else in the way will draw far \
              more current than it's rated for and burn out, often within seconds of \
              power-up.",
        fix: "Add a resistor in series between the LED and its supply (a few hundred ohms is \
              typical for a 3.3-5V rail).",
    },
    ErrorExplanation {
        code: "E40",
        what: "Confirms there's a switch (or removable link) sitting directly in the \
               battery's positive path, reachable from outside the robot without \
               disassembly.",
        why: "Combat/kit-class robot competitions require this as a tech-check item — you \
              need to be able to make the robot provably dead by flipping one switch, without \
              needing tools.",
        fix: "Wire a switch directly between the battery's positive terminal and everything \
              downstream of it.",
    },
    ErrorExplanation {
        code: "E41",
        what: "If the robot has a radio receiver, checks that a failsafe path is declared and \
               reaches the brain/ESC — a documented plan for what happens when the radio \
               signal is lost.",
        why: "A robot that keeps doing whatever it was last told when the radio drops out is \
              dangerous to everyone nearby — the whole point of a failsafe is that \"signal \
              lost\" always means \"stop\", not \"keep going\".",
        fix: "Declare the failsafe's stop pins and make sure they reach an MCU pin that can \
              actually act on a loss-of-signal condition.",
    },
    ErrorExplanation {
        code: "E22",
        what: "(Not yet implemented as a running check — reserved in specs/codes.md.) Would \
               compare each I2C device's declared read rate against the bus's total bandwidth \
               budget.",
        why: "Too many devices, or devices polled too fast, can saturate an I2C bus so no \
              device gets read often enough to be useful.",
        fix: "Reduce polling rate, move some devices to a second bus, or use faster bus \
              timing if the devices support it.",
    },
    ErrorExplanation {
        code: "E42",
        what: "(Not yet implemented as a running check — reserved in specs/codes.md.) Would \
               warn about exposed conductors on high-current nets with no declared \
               insulation class.",
        why: "A bare high-current wire that can be touched or shorted against the chassis is \
              both a shock and a fire risk.",
        fix: "Add insulation (heat-shrink, sleeving) and declare it, or reroute the wire away \
              from exposed contact.",
    },
    ErrorExplanation {
        code: "E43",
        what: "A charge controller declares which battery chemistry and cell count it's \
               actually built to charge (its `charge_profile`). This checks that the battery \
               wired to its output matches — same chemistry, same series cell count.",
        why: "A charger tuned for the wrong chemistry or cell count either never fully charges \
              the pack, or worse, keeps pushing current past the pack's real termination \
              voltage — the classic way an unattended overnight charge turns into a swollen or \
              vented cell.",
        fix: "Use a charge controller whose declared charge_profile (chemistry, cell_count) \
              matches the battery it's wired to, or swap in a battery that matches the charger \
              you already have.",
    },
    ErrorExplanation {
        code: "E44",
        what: "Warns when a multi-cell battery pack (2S, 3S, ...) has no declared `has_bms` — \
               no onboard protection or cell-balancing circuit.",
        why: "Series-wired lithium cells drift apart in charge over time; without balancing, \
              one cell can be pushed into overcharge while another is left under-charged, \
              invisible from the pack's bare +/- terminals alone.",
        fix: "Confirm this pack is charged through a balance-lead-aware charger, or fit an \
              inline protection (BMS) board — then declare the part's has_bms true once it's \
              genuinely covered.",
    },
    ErrorExplanation {
        code: "E45",
        what: "Warns when no fuse or resettable PTC fuse is reachable anywhere on the \
               battery's positive path.",
        why: "A crushed cable, a solder bridge, or a failed downstream part can pull far more \
              current than any wire is rated for — a fuse or PTC is the one part whose entire \
              job is surviving that moment so nothing else has to.",
        fix: "Add a fuse or PTC (e.g. fuse-ptc-5a) in series with the battery's positive \
              terminal, sized to the rail's normal worst-case current.",
    },
];

pub fn explain_error(code: &str) -> Option<&'static ErrorExplanation> {
    let code = code.to_uppercase();
    EXPLANATIONS.iter().find(|e| e.code == code)
}
