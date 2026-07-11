//! robowire M1 acceptance for the power graph + wiring mass (the two pieces
//! left open after E30-32 landed — specs/robowire.md §4 items 1-2). Same
//! discipline as `power_budget.rs`: assertions hand-compute the expected
//! figure from the same physics/catalogue data the engine uses, not a
//! hardcoded number.

use robowire::catalogue::ElecCatalogue;
use robowire::power_graph::{attach_power_graph, derive_power_graph, wiring_mass_g};
use robowire::Netlist;
use std::path::PathBuf;

const EPS: f64 = 1e-6;
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn load_wedge() -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist =
        serde_json::from_slice(&std::fs::read(root.join("harness/mvp-wedge-harness.json")).unwrap()).unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

#[test]
fn copper_mass_matches_first_principles_physics() {
    // A single 1-metre, 14AWG net — easy to hand-check independently:
    // R(14AWG) = 0.008284 ohm/m (the same standard reference value the
    // ampacity table already uses); A = resistivity/R; mass = A * density.
    let nl: Netlist = serde_json::from_value(serde_json::json!({
        "name": "one-meter-14awg",
        "instances": { "sw": "power-switch-slide" },
        "nets": [
            { "id": "a", "pins": ["sw.in"], "gauge_awg": 14, "length_mm": 1000.0 }
        ],
        "buses": [],
        "failsafe": null
    }))
    .unwrap();
    let root = repo_root();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();

    let resistivity_ohm_m = 1.68e-8;
    let density_kg_per_m3 = 8960.0;
    let r_per_m = 0.008284;
    let area_m2 = resistivity_ohm_m / r_per_m;
    let expected_g = area_m2 * density_kg_per_m3 * 1000.0;

    let actual = wiring_mass_g(&nl, &cat).unwrap();
    assert!(close(actual, expected_g), "wiring_mass_g = {actual}, expected {expected_g}");
    assert!(
        expected_g > 15.0 && expected_g < 25.0,
        "sanity check: 14AWG bare copper should be in the ballpark of 15-25 g/m, got {expected_g}"
    );
}

#[test]
fn wiring_mass_is_zero_when_nothing_declares_gauge_or_connector_parts() {
    let nl: Netlist = serde_json::from_value(serde_json::json!({
        "name": "no-gauge",
        "instances": { "sw": "power-switch-slide" },
        "nets": [ { "id": "a", "pins": ["sw.in"] } ],
        "buses": [],
        "failsafe": null
    }))
    .unwrap();
    let root = repo_root();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    assert!(close(wiring_mass_g(&nl, &cat).unwrap(), 0.0), "no gauge/connector data — must not fabricate a mass");
}

#[test]
fn wiring_mass_includes_connector_and_fuse_own_catalogue_mass() {
    let nl: Netlist = serde_json::from_value(serde_json::json!({
        "name": "connector-and-fuse",
        "instances": { "conn": "connector-xt30", "fuse": "fuse-ptc-5a" },
        "nets": [ { "id": "a", "pins": ["conn.P1", "fuse.P1"] }, { "id": "b", "pins": ["conn.P2"] }, { "id": "c", "pins": ["fuse.P2"] } ],
        "buses": [],
        "failsafe": null
    }))
    .unwrap();
    let root = repo_root();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    // connector-xt30 mass_g=1.2, fuse-ptc-5a mass_g=0.6 (parts/*.json).
    let actual = wiring_mass_g(&nl, &cat).unwrap();
    assert!(close(actual, 1.8), "expected connector+fuse own mass (1.2+0.6=1.8g), got {actual}");
}

#[test]
fn mvp_wedge_power_graph_matches_hand_computed_rails_segments_chains() {
    let (nl, cat) = load_wedge();
    let pg = derive_power_graph(&nl, &cat).unwrap();

    // Rails: battery (30C x 260mAh = 7.8A cap), ESC's BEC5V (max_a 1.0),
    // MCU's 3V3 (max_a 0.3) — the same three E30 already checks.
    assert_eq!(pg.rails.len(), 3, "{:?}", pg.rails.iter().map(|r| &r.source).collect::<Vec<_>>());
    let batt = pg.rails.iter().find(|r| r.source == "batt").unwrap();
    assert!(close(batt.capacity_a.unwrap(), 7.8));
    assert!(batt.worst_case_a > 3.0 && batt.worst_case_a < 3.5, "got {}", batt.worst_case_a);
    assert!(close(batt.margin_a.unwrap(), batt.capacity_a.unwrap() - batt.worst_case_a));

    // Segments: exactly the 6 gauge-declared nets in the harness file.
    assert_eq!(pg.segments.len(), 6, "{:?}", pg.segments.iter().map(|s| &s.net).collect::<Vec<_>>());
    let vbat = pg.segments.iter().find(|s| s.net == "vbat").unwrap();
    assert_eq!(vbat.gauge_awg, 14);
    assert!(close(vbat.length_mm, 60.0));
    assert!(vbat.resistance_ohms.unwrap() > 0.0);
    assert!(close(vbat.ampacity_a.unwrap(), 5.9));

    // Chains: both drive motors, batt -> esc -> motor.
    assert_eq!(pg.chains.len(), 2);
    for chain in &pg.chains {
        assert_eq!(chain.source, "batt");
        assert_eq!(chain.esc, "esc");
        assert!(chain.motor == "m_l" || chain.motor == "m_r");
    }

    // sense_points is honestly empty — no current-sense part exists yet.
    assert!(pg.sense_points.is_empty());
}

#[test]
fn attach_power_graph_folds_wiring_mass_into_total_and_reevaluates_d02() {
    let root = repo_root();
    let (nl, cat) = load_wedge();
    let robot: robotspec::RobotSpec =
        serde_json::from_slice(&std::fs::read(root.join("robots/mvp-wedge.json")).unwrap()).unwrap();

    let before = robotspec::derive(&robot, &cat).unwrap();
    assert!(close(before.mass_wiring_g, 0.0), "a bare derive() call must not know about wiring mass");
    assert!(before.power.is_none());

    let wiring_g = wiring_mass_g(&nl, &cat).unwrap();
    assert!(wiring_g > 0.0);

    let after = attach_power_graph(before.clone(), &nl, &cat).unwrap();
    assert!(close(after.mass_wiring_g, wiring_g));
    assert!(close(after.mass_total_g, before.mass_total_g + wiring_g));
    assert!(close(after.budget_margin_g, before.budget_margin_g - wiring_g));
    assert!(after.power.is_some());
    assert_eq!(after.power.as_ref().unwrap().chains.len(), 2);

    // D02 must be re-run against the wiring-inclusive total, not left stale
    // from before() — both records still pass D02 here (well within
    // budget), but the DETAIL string must reflect the new total.
    let d02_before = before.checks.iter().find(|c| c.code == "D02").unwrap();
    let d02_after = after.checks.iter().find(|c| c.code == "D02").unwrap();
    assert!(d02_before.pass && d02_after.pass);
    assert_ne!(
        d02_before.detail, d02_after.detail,
        "D02's detail should report the wiring-inclusive mass, not the stale pre-merge figure"
    );
    assert!(d02_after.detail.contains(&format!("{:.1}", after.mass_total_g)), "{:?}", d02_after);

    // Every other check is untouched by the merge.
    for code in ["D01", "D05", "X03"] {
        let b = before.checks.iter().find(|c| c.code == code).unwrap();
        let a = after.checks.iter().find(|c| c.code == code).unwrap();
        assert_eq!(b.detail, a.detail, "check {code} should be untouched by attach_power_graph");
    }
}
