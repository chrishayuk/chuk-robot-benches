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
/// can deliberately demonstrate (stage 5's broken variant does exactly
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
fn stage_02_regulator_legal_and_broken() {
    assert_legal("02-regulator");
    // A 3S battery on a regulator only rated for up to 9V — the classic
    // "wrong cell count for this part" mismatch, caught on the regulator's
    // own input pin regardless of what it's feeding downstream.
    assert_fails_exactly("02-regulator-broken", &["E02"]);
}

#[test]
fn stage_03_motor_driver_legal_and_broken() {
    assert_legal("03-motor-driver");
    assert_fails_exactly("03-motor-driver-broken", &["E40"]);
}

#[test]
fn stage_04_brain_and_radio_legal_and_broken() {
    assert_legal("04-brain-and-radio");
    assert_fails_exactly("04-brain-and-radio-broken", &["E41"]);
}

#[test]
fn stage_05_shared_5v_rail_legal_and_broken() {
    assert_legal("05-shared-5v-rail");
    // One mistake (skipping the BEC hop), two independent consequences: the
    // MCU is both over its rated voltage (E02) AND now shares an unbuffered
    // rail with the motor-driving ESC (E32, warn-tier) — a deliberately
    // instructive double failure, not a test bug.
    assert_fails_exactly("05-shared-5v-rail-broken", &["E02", "E32"]);
}

#[test]
fn stage_06_sensor_bus_legal_and_broken() {
    assert_legal("06-sensor-bus");
    assert_fails_exactly("06-sensor-bus-broken", &["E20"]);
}

#[test]
fn stage_07_two_wheel_drive_legal_and_broken() {
    assert_legal("07-two-wheel-drive");
    // Adding the second drive motor's channel is exactly where the classic
    // "both motors wired to the same channel" mistake shows up.
    assert_fails_exactly("07-two-wheel-drive-broken", &["E01"]);
}

#[test]
fn stage_08_battery_protection_legal_and_broken() {
    // A standalone vignette (like 1-2), revisiting stage 3's motor-driver
    // shape with a new lens: is the battery itself protected? One root
    // mistake — swap the protected pack for a bare one AND delete the
    // inline fuse — trips two warn-tier codes at once, the same "one
    // mistake, two consequences" pattern stage 5 uses for E02+E32.
    assert_legal("08-battery-protection");
    assert_fails_exactly("08-battery-protection-broken", &["E44", "E45"]);
}

#[test]
fn batt_and_sw_persist_across_every_stage() {
    // battery/switch are the one constant every stage keeps, even across the
    // foundational vignettes (1: bare basics, 2: regulator, 8: battery
    // protection) that don't literally accumulate onto each other
    // instance-for-instance yet.
    let stages = [
        "01-basics", "02-regulator", "03-motor-driver", "04-brain-and-radio", "05-shared-5v-rail",
        "06-sensor-bus", "07-two-wheel-drive", "08-battery-protection",
    ];
    for name in stages {
        let (nl, _) = load(name);
        assert!(nl.instances.contains_key("batt"), "{name}: missing 'batt'");
        assert!(nl.instances.contains_key("sw"), "{name}: missing 'sw'");
    }
}

#[test]
fn motor_stages_strictly_accumulate() {
    // The real "one accumulating build" chain starts once the motor+ESC
    // vignette (stage 3) is established: stages 1-2 are standalone
    // foundational vignettes (bare basics, then a regulator — neither needs
    // a motor at all), but from stage 3 onward, every stage is a strict
    // superset of the last (esc/m1 persist from 3, mcu/rx from 4, lifter
    // from 5, tof1/tof2 from 6) — proving this half of the curriculum is
    // one accumulating build, not disconnected snapshots.
    let stages = ["03-motor-driver", "04-brain-and-radio", "05-shared-5v-rail", "06-sensor-bus", "07-two-wheel-drive"];
    let mut prev: Option<Netlist> = None;
    for name in stages {
        let (nl, _) = load(name);
        if let Some(p) = &prev {
            for inst in p.instances.keys() {
                assert!(nl.instances.contains_key(inst), "{name}: lost '{inst}' from an earlier stage");
            }
            assert!(
                nl.instances.len() > p.instances.len(),
                "{name}: has no more instances ({}) than the previous stage ({})",
                nl.instances.len(),
                p.instances.len()
            );
        }
        prev = Some(nl);
    }
}
