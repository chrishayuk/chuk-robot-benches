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
