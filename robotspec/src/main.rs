//! robotspec — CLI over the derivation pipeline.
//!   robotspec show <robot.json> [--parts DIR] [--out derived.json]

use robotspec::{derive, Catalogue, RobotSpec};
use std::path::PathBuf;

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2)
}

fn cmd_view(args: &[String]) {
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let robot_path = PathBuf::from(&args[0]);
    let parts_dir = PathBuf::from(flag("--parts").unwrap_or_else(|| "parts".into()));
    let spec: RobotSpec = serde_json::from_slice(
        &std::fs::read(&robot_path).unwrap_or_else(|e| die(&format!("{robot_path:?}: {e}"))),
    )
    .unwrap_or_else(|e| die(&format!("parse {robot_path:?}: {e}")));
    let cat = Catalogue::load(&parts_dir).unwrap_or_else(|e| die(&e));
    let d = derive(&spec, &cat).unwrap_or_else(|e| die(&e));
    let html = robotspec::view::build_inspector(&spec, &cat, &d).unwrap_or_else(|e| die(&e));
    let out = flag("--out").unwrap_or_else(|| format!("{}-inspector.html", spec.identity.name));
    std::fs::write(&out, html).unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
    println!("wrote {out}   robot {}", &d.robot_hash[..16]);
    if !args.iter().any(|a| a == "--no-open") {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        if let Err(e) = std::process::Command::new(opener).arg(&out).spawn() {
            eprintln!("could not launch browser ({e}); open {out} manually");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("view") && args.len() >= 2 {
        return cmd_view(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) != Some("show") || args.len() < 2 {
        eprintln!("usage: robotspec <show|view> <robot.json> [--parts DIR] [--out FILE]");
        std::process::exit(2);
    }
    let robot_path = PathBuf::from(&args[1]);
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let parts_dir = PathBuf::from(flag("--parts").unwrap_or_else(|| "parts".into()));

    let spec: RobotSpec = serde_json::from_slice(
        &std::fs::read(&robot_path).unwrap_or_else(|e| die(&format!("{robot_path:?}: {e}"))),
    )
    .unwrap_or_else(|e| die(&format!("parse {robot_path:?}: {e}")));
    let cat = Catalogue::load(&parts_dir).unwrap_or_else(|e| die(&e));
    let d = derive(&spec, &cat).unwrap_or_else(|e| die(&e));

    println!("robot    {} {}", spec.identity.name, spec.identity.revision);
    println!("body     {}", &d.body_hash[..16]);
    println!("robot#   {}", &d.robot_hash[..16]);
    println!("pipeline {}", d.pipeline_version);
    println!();
    println!(
        "mass     {:.1} g  (chassis {:.1} + parts {:.1})   margin {:.1} g",
        d.mass_total_g, d.mass_chassis_g, d.mass_parts_g, d.budget_margin_g
    );
    println!(
        "CoG      ({:.1}, {:.1}, {:.1}) mm    yaw inertia {:.0} g·mm²",
        d.cog_mm[0], d.cog_mm[1], d.cog_mm[2], d.yaw_inertia_gmm2
    );
    println!(
        "bbox     {:.1} x {:.1} x {:.1} mm   cube: {}",
        d.bbox_mm[0],
        d.bbox_mm[1],
        d.bbox_mm[2],
        if d.cube_fit { "FITS" } else { "VIOLATION" }
    );
    println!(
        "tip      worst edge {} at {:.1} mm -> {:.1}°, {:.2} mJ to tip",
        d.worst_tip_edge, d.worst_tip_distance_mm, d.worst_tip_angle_deg, d.static_tip_energy_mj
    );
    println!("brake    pitch-over limit {:.1} m/s²", d.brake_pitch_limit_ms2);
    println!();
    for c in &d.checks {
        println!(
            "{} {}  {} — {}",
            if c.pass { "PASS" } else { "FAIL" },
            c.code,
            c.description,
            c.detail
        );
    }

    if let Some(out) = flag("--out") {
        std::fs::write(&out, serde_json::to_string_pretty(&d).unwrap())
            .unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
        println!("\nwrote {out}");
    }
    if d.checks.iter().any(|c| !c.pass) {
        std::process::exit(1);
    }
}
