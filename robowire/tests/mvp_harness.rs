//! robowire M0 acceptance (specs/robowire.md §6): the MVP harness passes,
//! and deliberately broken variants fail with the CORRECT E-codes. The
//! checker is verified by its ability to catch planted faults, not by
//! agreeing with a correct design.

use robowire::catalogue::ElecCatalogue;
use robowire::{run_checks, Netlist};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn load() -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist = serde_json::from_slice(
        &std::fs::read(root.join("harness/mvp-wedge-harness.json")).unwrap(),
    )
    .unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

fn code_result(nl: &Netlist, cat: &ElecCatalogue, code: &str) -> bool {
    run_checks(nl, cat)
        .unwrap()
        .into_iter()
        .find(|c| c.code == code)
        .unwrap_or_else(|| panic!("no check {code}"))
        .pass
}

#[test]
fn mvp_harness_passes_all_checks() {
    let (nl, cat) = load();
    let checks = run_checks(&nl, &cat).unwrap();
    let failed: Vec<_> = checks.iter().filter(|c| !c.pass).collect();
    assert!(failed.is_empty(), "failed: {failed:?}");
}

#[test]
fn planted_swapped_polarity_fails_e03() {
    let (mut nl, cat) = load();
    // Battery output wired to the ESC's ground and vice versa.
    for net in nl.nets.iter_mut() {
        if net.id == "vbat" {
            for p in net.pins.iter_mut() {
                if p == "esc.VIN" {
                    *p = "esc.GND".into();
                }
            }
        }
        if net.id == "gnd" {
            for p in net.pins.iter_mut() {
                if p == "esc.GND" {
                    *p = "esc.VIN".into();
                }
            }
        }
    }
    assert!(!code_result(&nl, &cat, "E03"), "E03 must catch the polarity swap");
}

#[test]
fn planted_dual_0x29_fails_e20() {
    let (mut nl, cat) = load();
    // Drop the reassignment plan: both ToFs boot at 0x29.
    nl.buses[0].devices.iter_mut().for_each(|d| d.reassign_to = None);
    assert!(!code_result(&nl, &cat, "E20"), "E20 must catch the dual-0x29 classic");
}

#[test]
fn planted_missing_switch_fails_e40() {
    let (mut nl, cat) = load();
    // Battery wired straight to the ESC, switch deleted from the path.
    nl.nets.retain(|n| n.id != "vbat_sw");
    for net in nl.nets.iter_mut() {
        if net.id == "vbat" {
            net.pins = vec!["batt.+".into(), "esc.VIN".into()];
        }
    }
    nl.instances.remove("sw");
    assert!(!code_result(&nl, &cat, "E40"), "E40 must catch the missing switch");
}

#[test]
fn planted_wrong_pin_fails_e10() {
    let (mut nl, cat) = load();
    // ESC signal moved to a pin with no PWM capability.
    for net in nl.nets.iter_mut() {
        if net.id == "pwm_l" {
            for p in net.pins.iter_mut() {
                if p == "mcu.GP2" {
                    *p = "mcu.GP6".into();
                }
            }
        }
    }
    // Keep GP6 single-booked so this isolates E10, not E11.
    nl.buses[0].devices[1].xshut = Some("mcu.GP2".into());
    assert!(!code_result(&nl, &cat, "E10"), "E10 must catch the capability mismatch");
}

#[test]
fn planted_unwired_xshut_fails_e20() {
    let (mut nl, cat) = load();
    // Reassignment declared but the XSHUT line never wired.
    nl.buses[0].devices[1].xshut = None;
    assert!(
        !code_result(&nl, &cat, "E20"),
        "E20 must require a wired XSHUT for the reassignment recipe"
    );
}

#[test]
fn planted_bare_led_fails_e33() {
    let (mut nl, cat) = load();
    // An LED wired straight across the 3V3 rail and ground — no resistor.
    nl.instances.insert("led1".into(), "led-red-5mm".into());
    for net in nl.nets.iter_mut() {
        if net.id == "v33" {
            net.pins.push("led1.A".into());
        }
        if net.id == "gnd" {
            net.pins.push("led1.K".into());
        }
    }
    assert!(!code_result(&nl, &cat, "E33"), "E33 must catch the bare LED");

    // Same LED with a series resistor: rail -> R -> LED -> gnd. Legal.
    let (mut nl, cat) = load();
    nl.instances.insert("led1".into(), "led-red-5mm".into());
    nl.instances.insert("r1".into(), "resistor-330r".into());
    for net in nl.nets.iter_mut() {
        if net.id == "v33" {
            net.pins.push("r1.P1".into());
        }
        if net.id == "gnd" {
            net.pins.push("led1.K".into());
        }
    }
    nl.nets.push(robowire::schema::Net {
        id: "led_feed".into(),
        pins: vec!["r1.P2".into(), "led1.A".into()],
        volts: None,
        signal: None,
        gauge_awg: None,
        length_mm: None,
    });
    assert!(code_result(&nl, &cat, "E33"), "resistor in series must satisfy E33");
}

#[test]
fn examples_are_legal_and_lessons_fail_their_named_code() {
    let root = repo_root();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    let dir = root.join("harness/examples");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(true, |x| x != "json") {
            continue;
        }
        count += 1;
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let nl: Netlist =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let checks = run_checks(&nl, &cat).unwrap();
        if let Some(code) = name
            .strip_prefix("lesson-")
            .and_then(|r| r.split('-').next())
        {
            let code = code.to_uppercase();
            let c = checks.iter().find(|c| c.code == code).unwrap();
            assert!(!c.pass, "{name}: expected {code} to FAIL (it's the lesson)");
            assert!(
                checks.iter().filter(|c| !c.pass).all(|c| c.code == code),
                "{name}: only {code} may fail, got {:?}",
                checks.iter().filter(|c| !c.pass).map(|c| &c.code).collect::<Vec<_>>()
            );
        } else {
            let fails: Vec<_> = checks.iter().filter(|c| !c.pass).collect();
            assert!(fails.is_empty(), "{name}: {fails:?}");
        }
    }
    assert!(count >= 3, "expected at least 3 examples, found {count}");
}
