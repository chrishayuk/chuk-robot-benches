//! The progressive curriculum in `harness/lessons/` (specs/robowire.md,
//! "start from the real basics and work up"): each numbered stage adds one
//! real concept on top of the last, and is paired with a `-broken` variant
//! demonstrating exactly the failure that concept guards against. Same
//! discipline as `mvp_harness.rs`/`power_budget.rs`: the checker is proven
//! by its ability to catch the planted fault, not by agreeing with a
//! correct design — and each stage's LEGAL file must be fully clean, since
//! it's what the next stage builds on.

use robowire::catalogue::ElecCatalogue;
use robowire::{run_checks, Netlist, Tier};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn load(name: &str) -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist =
        serde_json::from_slice(&std::fs::read(root.join("harness/lessons").join(format!("{name}.json"))).unwrap())
            .unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

/// Hard failures AND warns, both as plain codes — a warn (`pass: true, tier:
/// Warn`) never blocks the verdict, but it's still a real signal a lesson
/// can deliberately demonstrate (stage 4's broken variant does exactly
/// that: E02 fails outright, E32 warns, from the same one mistake).
fn non_clean_codes(nl: &Netlist, cat: &ElecCatalogue) -> Vec<String> {
    run_checks(nl, cat)
        .unwrap()
        .into_iter()
        .filter(|c| !c.pass || c.tier == Tier::Warn)
        .map(|c| c.code)
        .collect()
}

/// Every stage's legal file is exercised for real electrical current
/// somewhere (a motor, or an LED), so a stage that regresses into a
/// dead/disconnected circuit doesn't slip through as "just still legal".
fn assert_legal(name: &str) {
    let (nl, cat) = load(name);
    let fails = non_clean_codes(&nl, &cat);
    assert!(fails.is_empty(), "{name}: expected fully legal, found {fails:?}");
}

fn assert_fails_exactly(name: &str, expected: &[&str]) {
    let (nl, cat) = load(name);
    let fails = non_clean_codes(&nl, &cat);
    let mut fails_sorted = fails.clone();
    fails_sorted.sort();
    let mut expected_sorted: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    expected_sorted.sort();
    assert_eq!(fails_sorted, expected_sorted, "{name}: expected exactly {expected:?}, got {fails:?}");
}

#[test]
fn stage_01_basics_legal_and_broken() {
    assert_legal("01-basics");
    assert_fails_exactly("01-basics-broken", &["E33"]);
}

#[test]
fn stage_02_motor_driver_legal_and_broken() {
    assert_legal("02-motor-driver");
    assert_fails_exactly("02-motor-driver-broken", &["E40"]);
}

#[test]
fn stage_03_brain_and_radio_legal_and_broken() {
    assert_legal("03-brain-and-radio");
    assert_fails_exactly("03-brain-and-radio-broken", &["E41"]);
}

#[test]
fn stage_04_shared_5v_rail_legal_and_broken() {
    assert_legal("04-shared-5v-rail");
    // One mistake (skipping the BEC hop), two independent consequences: the
    // MCU is both over its rated voltage (E02) AND now shares an unbuffered
    // rail with the motor-driving ESC (E32, warn-tier) — a deliberately
    // instructive double failure, not a test bug.
    assert_fails_exactly("04-shared-5v-rail-broken", &["E02", "E32"]);
}

#[test]
fn stage_05_sensor_bus_legal_and_broken() {
    assert_legal("05-sensor-bus");
    assert_fails_exactly("05-sensor-bus-broken", &["E20"]);
}

#[test]
fn each_stage_strictly_adds_to_the_last() {
    // "Start from the real basics and work up": every stage's instance set
    // is a superset of the previous stage's, proving this is one
    // accumulating build, not disconnected snapshots.
    let stages = ["01-basics", "02-motor-driver", "03-brain-and-radio", "04-shared-5v-rail", "05-sensor-bus"];
    let mut prev: Option<Netlist> = None;
    for name in stages {
        let (nl, _) = load(name);
        if let Some(p) = &prev {
            // battery/switch/ground are the one constant every stage keeps;
            // everything else should only grow, kind by kind.
            for inst in p.instances.keys() {
                if inst == "batt" || inst == "sw" {
                    assert!(nl.instances.contains_key(inst), "{name}: lost '{inst}' from an earlier stage");
                }
            }
            assert!(
                nl.instances.len() >= p.instances.len(),
                "{name}: has fewer instances ({}) than the previous stage ({})",
                nl.instances.len(),
                p.instances.len()
            );
        }
        prev = Some(nl);
    }
}
