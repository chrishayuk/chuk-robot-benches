//! RobotSpec M0 acceptance, exercised through the public API only —
//! composability is part of what's under test.

use robotspec::schema::MechSource;
use robotspec::{derive, Catalogue, RobotSpec, WEIGHT_LIMIT_G};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn load_mvp() -> (RobotSpec, Catalogue) {
    let root = repo_root();
    let spec: RobotSpec = serde_json::from_slice(
        &std::fs::read(root.join("robots/mvp-wedge.json")).unwrap(),
    )
    .unwrap();
    let cat = Catalogue::load(&root.join("parts")).unwrap();
    (spec, cat)
}

#[test]
fn mvp_wedge_derives_and_passes_checks() {
    let (spec, cat) = load_mvp();
    let d = derive(&spec, &cat).unwrap();
    assert!(
        d.checks.iter().all(|c| c.pass),
        "failed checks: {:?}",
        d.checks.iter().filter(|c| !c.pass).collect::<Vec<_>>()
    );
    assert!(d.mass_total_g < WEIGHT_LIMIT_G);
    assert!(
        d.budget_margin_g >= 30.0,
        "wanted >=30g margin for a future lifter, got {:.1}",
        d.budget_margin_g
    );
    assert!(d.cube_fit);
    assert!(d.worst_tip_distance_mm > 0.0, "CoG outside support polygon");
    assert!(d.cog_mm[2] < 20.0, "CoG too high: {:.1}mm", d.cog_mm[2]);
    assert!((d.mass_chassis_g + d.mass_parts_g - d.mass_total_g).abs() < 1e-9);
}

#[test]
fn planted_faults_fail_with_correct_codes() {
    // D01: over-cube chassis.
    let (mut spec, cat) = load_mvp();
    {
        let MechSource::Parametric { chassis } = &mut spec.sources.mech;
        chassis.rear_height_mm = 120.0;
    }
    let d = derive(&spec, &cat).unwrap();
    assert!(!d.checks.iter().find(|c| c.code == "D01").unwrap().pass);

    // D02: absurd density.
    let (mut spec, cat) = load_mvp();
    {
        let MechSource::Parametric { chassis } = &mut spec.sources.mech;
        chassis.density_map.insert(chassis.material.clone(), 8.0);
    }
    let d = derive(&spec, &cat).unwrap();
    assert!(!d.checks.iter().find(|c| c.code == "D02").unwrap().pass);

    // D05: wheel hanging outside the footprint.
    let (mut spec, cat) = load_mvp();
    spec.drive.wheels[0].pos_mm[1] = 60.0;
    let d = derive(&spec, &cat).unwrap();
    assert!(!d.checks.iter().find(|c| c.code == "D05").unwrap().pass);
}

#[test]
fn hash_nesting_body_vs_robot() {
    let (spec, cat) = load_mvp();
    let d1 = derive(&spec, &cat).unwrap();
    let d2 = derive(&spec, &cat).unwrap();
    assert_eq!(d1.body_hash, d2.body_hash, "derivation not deterministic");
    assert_eq!(d1.robot_hash, d2.robot_hash);

    // Kernel-only change: same body, different robot.
    let mut brain = spec.clone();
    brain.sources.kernel.family_hash = "some-other-kernel@deadbeef".into();
    let db = derive(&brain, &cat).unwrap();
    assert_eq!(db.body_hash, d1.body_hash);
    assert_ne!(db.robot_hash, d1.robot_hash);

    // Geometry change: different body AND robot.
    let mut wider = spec.clone();
    {
        let MechSource::Parametric { chassis } = &mut wider.sources.mech;
        chassis.width_mm += 1.0;
    }
    let dw = derive(&wider, &cat).unwrap();
    assert_ne!(dw.body_hash, d1.body_hash);
    assert_ne!(dw.robot_hash, d1.robot_hash);
}
