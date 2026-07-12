//! `robowire::bench` — the generated physical verification procedure
//! (specs/robowire.md §4 item 4). Every assertion here checks the
//! generator's output against something independently knowable from the
//! netlist/catalogue, the same discipline as every other check in this
//! crate: never just "did it not crash".

use robowire::bench::generate;
use robowire::catalogue::ElecCatalogue;
use robowire::checks::bus_final_addresses;
use robowire::Netlist;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn load(harness_file: &str) -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist = serde_json::from_slice(&std::fs::read(root.join(harness_file)).unwrap()).unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

#[test]
fn mvp_wedge_continuity_covers_dead_short_switch_and_every_ground() {
    let (nl, cat) = load("harness/mvp-wedge-harness.json");
    let p = generate(&nl, &cat).unwrap();

    assert!(
        p.continuity.iter().any(|c| c.probe_a == "batt.+" && c.probe_b == "batt.-" && c.expect.contains("open")),
        "missing the battery dead-short check"
    );
    assert!(
        p.continuity.iter().any(|c| c.probe_b == "sw.out" && c.expect.contains("open")),
        "missing the switch-open continuity check"
    );
    assert!(
        p.continuity.iter().any(|c| c.probe_b == "sw.out" && c.expect.contains("continuity")),
        "missing the switch-closed continuity check"
    );

    // Every OTHER instance's gnd pin gets its own continuity check back to
    // the battery — esc, mcu, rx, tof_l, tof_r, imu (6 devices).
    let ground_checks = p.continuity.iter().filter(|c| c.probe_a == "batt.-").count();
    assert_eq!(ground_checks, 6, "expected one ground-continuity check per other instance, got {ground_checks}");
}

#[test]
fn solar_charging_demo_checks_the_charge_controllers_own_output_first() {
    // The most damaging thing a regulator/charge-controller can do is put
    // out the wrong voltage — that must be checked before anything
    // downstream of it, in its own "power distribution" stage.
    let (nl, cat) = load("harness/examples/example-solar-charging-demo.json");
    let p = generate(&nl, &cat).unwrap();

    let dist = p.power_stages.iter().find(|s| s.name == "power distribution (switch closed)").unwrap();
    assert!(dist.instructions.iter().any(|i| i.starts_with("cc.OUT") && i.contains("7.4V")));

    // The solar panel itself has no elec.source (it never seeds robosim's
    // run-mode graph, energy-sim.md §2.1) — it must NOT get a dead-short
    // continuity check the way a real battery does.
    assert!(!p.continuity.iter().any(|c| c.probe_a.starts_with("panel.") && c.expect.contains("dead")));
}

#[test]
fn stage1_basics_has_no_brain_sensor_or_drive_stages() {
    // 01-basics is just battery/switch/resistor/LED — the generator must
    // not fabricate empty checklist sections for kinds that aren't present.
    let (nl, cat) = load("harness/lessons/01-basics.json");
    let p = generate(&nl, &cat).unwrap();

    let stage_names: Vec<&str> = p.power_stages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(stage_names, vec!["rails unloaded (switch open)", "full"]);
    assert!(p.bus_scans.is_empty(), "no buses in this lesson, so no bus scan section");
}

#[test]
fn bus_scan_matches_bus_final_addresses_exactly() {
    // The bench procedure's bus scan must be the SAME reassignment
    // resolution E20 already checks against — generated from the identical
    // helper, not a second derivation that could quietly drift.
    let (nl, cat) = load("harness/mvp-wedge-harness.json");
    let p = generate(&nl, &cat).unwrap();

    let scan = &p.bus_scans[0];
    assert_eq!(scan.bus_id, "i2c0");
    let expected_map: std::collections::BTreeMap<_, _> =
        bus_final_addresses(&nl.buses[0]).into_iter().collect();
    let scan_map: std::collections::BTreeMap<_, _> = scan.expected.iter().cloned().collect();
    assert_eq!(scan_map, expected_map);
}

#[test]
fn markdown_render_is_stable_and_nonempty() {
    let (nl, cat) = load("harness/mvp-wedge-harness.json");
    let p = generate(&nl, &cat).unwrap();
    let md = robowire::bench::render_markdown(&p);
    assert!(md.starts_with("# Bench procedure — mvp-wedge-harness"));
    assert!(md.contains("## Before power: continuity checks"));
    assert!(md.contains("## Expected bus scan"));
}
