//! Bench verification procedure generator (specs/robowire.md §4 item 4): a
//! generated, ordered checklist binding a design to its physical build —
//! continuity checks before power, a polarity list, a staged power-up
//! order, and expected I2C bus scan results. Reuses the exact same
//! rule/derivation logic `checks.rs` already encodes (`bus_final_addresses`,
//! `pin_decl`) rather than re-deriving any of it, so the bench procedure can
//! never disagree with what the checker itself already knows. Completing
//! this checklist against a real, physical build *is* the as-built
//! electrical record (RobotSpec's own ritual for the electrical domain).

use crate::catalogue::ElecCatalogue;
use crate::checks::{bus_final_addresses, pin_decl};
use crate::schema::{split_pin, Netlist};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContinuityCheck {
    pub probe_a: String,
    pub probe_b: String,
    pub expect: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerStage {
    pub name: String,
    pub instructions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BusScan {
    pub bus_id: String,
    /// (instance, expected final hex address), sorted by address.
    pub expected: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchProcedure {
    pub netlist_name: String,
    pub continuity: Vec<ContinuityCheck>,
    pub polarity: Vec<String>,
    pub power_stages: Vec<PowerStage>,
    pub bus_scans: Vec<BusScan>,
}

/// Generates the full procedure. Every number/expectation here traces back
/// to something already declared in the netlist or catalogue — nothing is
/// invented, matching the "missing catalogue field -> None, never guessed"
/// convention the rest of robowire already follows.
pub fn generate(nl: &Netlist, cat: &ElecCatalogue) -> Result<BenchProcedure, String> {
    Ok(BenchProcedure {
        netlist_name: nl.name.clone(),
        continuity: continuity_checks(nl, cat)?,
        polarity: polarity_list(nl, cat)?,
        power_stages: power_stages(nl, cat)?,
        bus_scans: bus_scans(nl),
    })
}

/// Before any power is ever applied: a dead-short check on every battery,
/// an open/closed continuity pair for every switch/removable-link directly
/// in a battery's own path (the same net E40 already checks), and a
/// continuity check from every battery's own ground pin to every other
/// instance's ground pin — the classic "isolated ground" mistake, caught
/// with a meter before it's caught by a part quietly not working.
fn continuity_checks(nl: &Netlist, cat: &ElecCatalogue) -> Result<Vec<ContinuityCheck>, String> {
    let mut out = Vec::new();

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        if elec.source.is_none() {
            continue;
        }
        let Some((pos_pin, _)) = elec.pins.iter().find(|(_, d)| d.role == "pos") else { continue };
        let Some((gnd_pin, _)) = elec.pins.iter().find(|(_, d)| d.role == "gnd") else { continue };
        let pos = format!("{inst}.{pos_pin}");
        let gnd = format!("{inst}.{gnd_pin}");

        out.push(ContinuityCheck {
            probe_a: pos.clone(),
            probe_b: gnd.clone(),
            expect: "open — a dead short here is a wiring fault, check before inserting a cell".into(),
        });

        // The switch/removable-link directly on this battery's own net —
        // same single-hop lookup E40 already performs.
        if let Some(net) = nl.nets.iter().find(|n| n.pins.contains(&pos)) {
            for p in &net.pins {
                let d = pin_decl(nl, cat, p)?;
                if d.role != "switch_in" {
                    continue;
                }
                let (sw_inst, _) = split_pin(p)?;
                let sw_part_id = nl.instances.get(sw_inst).ok_or_else(|| format!("unknown instance '{sw_inst}'"))?;
                let (sw_part, _) = cat.get(sw_part_id)?;
                let Some(sw_elec) = &sw_part.elec else { continue };
                let Some((out_pin, _)) = sw_elec.pins.iter().find(|(_, d)| d.role == "switch_out") else { continue };
                let sw_out = format!("{sw_inst}.{out_pin}");
                out.push(ContinuityCheck {
                    probe_a: pos.clone(),
                    probe_b: sw_out.clone(),
                    expect: "open (switch/link off)".into(),
                });
                out.push(ContinuityCheck {
                    probe_a: pos.clone(),
                    probe_b: sw_out,
                    expect: "continuity, near 0Ω (switch/link on)".into(),
                });
            }
        }

        // Ground unification: every other instance's own gnd pin should
        // show continuity back to this battery's ground pin.
        for (other_inst, other_part_id) in &nl.instances {
            if other_inst == inst {
                continue;
            }
            let (other_part, _) = cat.get(other_part_id)?;
            let Some(other_elec) = &other_part.elec else { continue };
            for (pin, d) in &other_elec.pins {
                if d.role != "gnd" {
                    continue;
                }
                out.push(ContinuityCheck {
                    probe_a: gnd.clone(),
                    probe_b: format!("{other_inst}.{pin}"),
                    expect: "continuity, near 0Ω (one shared ground plane)".into(),
                });
            }
        }
    }

    Ok(out)
}

/// One line per power-carrying pin, naming the net and its declared voltage
/// — a physical build's polarity has to match the design's, and this is
/// the design's own claim about what should land where, not a re-check of
/// E03 (which already proved the AUTHORED topology doesn't mix +/-).
fn polarity_list(nl: &Netlist, cat: &ElecCatalogue) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for net in &nl.nets {
        for p in &net.pins {
            let d = pin_decl(nl, cat, p)?;
            if !matches!(d.role.as_str(), "pos" | "power_in" | "power_out") {
                continue;
            }
            let volts = net.volts.map(|v| format!("{v}V")).unwrap_or_else(|| "an undeclared rail".to_string());
            out.push(format!("{p} ({}) should land on net '{}' at {volts} — verify before power-up", d.role, net.id));
        }
    }
    Ok(out)
}

/// Staged smoke-test order (specs/robowire.md §4 item 4): a checklist
/// ORDER, not a literal hardware reconfiguration (a static netlist can't
/// simulate unplugging a sub-board) — what to look at first with a meter,
/// least-risky/most-fundamental first, each stage naming its own expected
/// voltage from the netlist's own declared rails.
fn power_stages(nl: &Netlist, cat: &ElecCatalogue) -> Result<Vec<PowerStage>, String> {
    let mut stages = Vec::new();

    // Stage 1: switch open, nothing downstream should be live.
    let mut unloaded = Vec::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        let Some(source) = &elec.source else { continue };
        unloaded.push(format!("battery '{inst}': expect its own terminal voltage, {}V", source.volts));
    }
    unloaded.push("switch open: every other rail should read 0V".to_string());
    stages.push(PowerStage { name: "rails unloaded (switch open)".into(), instructions: unloaded });

    // Stage 2: switch closed — check every regulator/BEC/charge-controller's
    // OWN output first, before trusting anything downstream of it. Putting
    // the wrong voltage out is the single most damaging thing this class of
    // part can do, so it's checked before the brain/sensors/drive that
    // depend on it.
    let mut distribution = Vec::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if !matches!(part.kind.as_str(), "regulator" | "esc" | "charge-controller") {
            continue;
        }
        let Some(elec) = &part.elec else { continue };
        for (pin, d) in &elec.pins {
            if d.role != "power_out" {
                continue;
            }
            let endpoint = format!("{inst}.{pin}");
            let volts = d.volts.map(|v| format!("{v}V")).unwrap_or_else(|| "its declared output voltage".to_string());
            distribution.push(format!("{endpoint}: expect {volts}"));
        }
    }
    if !distribution.is_empty() {
        stages.push(PowerStage { name: "power distribution (switch closed)".into(), instructions: distribution });
    }

    // Remaining stages: switch closed, checked in this order.
    for (name, kinds) in [
        ("brain (MCU) only", &["mcu"][..]),
        ("sensors", &["tof", "imu", "light", "env"][..]),
        ("drive electronics, no motors commanded yet", &["esc"][..]),
    ] {
        let mut instructions = Vec::new();
        for (inst, part_id) in &nl.instances {
            let (part, _) = cat.get(part_id)?;
            if !kinds.contains(&part.kind.as_str()) {
                continue;
            }
            let Some(elec) = &part.elec else { continue };
            let Some((pin, _)) = elec.pins.iter().find(|(_, d)| d.role == "power_in") else { continue };
            let endpoint = format!("{inst}.{pin}");
            let Some(net) = nl.nets.iter().find(|n| n.pins.contains(&endpoint)) else { continue };
            let volts = net.volts.map(|v| format!("{v}V")).unwrap_or_else(|| "its declared rail voltage".to_string());
            instructions.push(format!("{endpoint}: expect {volts}"));
        }
        if !instructions.is_empty() {
            stages.push(PowerStage { name: name.into(), instructions });
        }
    }

    stages.push(PowerStage {
        name: "full".into(),
        instructions: vec!["everything above still holding steady — safe to command motors/servos".to_string()],
    });

    Ok(stages)
}

/// Expected I2C scan result per bus, reusing the exact same reassignment
/// resolution E20 checks against — a bench scanner and the static checker
/// can never disagree about what "correct" looks like.
fn bus_scans(nl: &Netlist) -> Vec<BusScan> {
    nl.buses
        .iter()
        .map(|bus| {
            let mut expected: Vec<(String, String)> =
                bus_final_addresses(bus).into_iter().collect();
            expected.sort_by(|a, b| a.1.cmp(&b.1));
            BusScan { bus_id: bus.id.clone(), expected }
        })
        .collect()
}

/// Plain, printable Markdown — matching the spec's own "printable bench
/// procedure" wording.
pub fn render_markdown(p: &BenchProcedure) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Bench procedure — {}\n\n", p.netlist_name));

    s.push_str("## Before power: continuity checks\n\n");
    for c in &p.continuity {
        s.push_str(&format!("- [ ] probe **{}** ↔ **{}**: expect {}\n", c.probe_a, c.probe_b, c.expect));
    }

    s.push_str("\n## Before power: polarity\n\n");
    for line in &p.polarity {
        s.push_str(&format!("- [ ] {line}\n"));
    }

    s.push_str("\n## Staged power-up\n\n");
    for stage in &p.power_stages {
        s.push_str(&format!("### {}\n\n", stage.name));
        for i in &stage.instructions {
            s.push_str(&format!("- [ ] {i}\n"));
        }
        s.push('\n');
    }

    if !p.bus_scans.is_empty() {
        s.push_str("## Expected bus scan\n\n");
        for scan in &p.bus_scans {
            s.push_str(&format!("Bus `{}`:\n\n", scan.bus_id));
            for (inst, addr) in &scan.expected {
                s.push_str(&format!("- [ ] {inst} at `{addr}`\n"));
            }
            s.push('\n');
        }
    }

    s
}
