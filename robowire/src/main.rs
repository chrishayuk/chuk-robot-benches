//! robowire — CLI over the E-check engine.
//!   robowire check <netlist.json> [--parts DIR] [--out report.json]

use robowire::catalogue::{sha256_hex, ElecCatalogue};
use robowire::{run_checks, Netlist};
use std::path::PathBuf;

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2)
}

fn cmd_render(args: &[String]) {
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let path = PathBuf::from(&args[0]);
    let netlist: Netlist = serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| die(&format!("{path:?}: {e}"))),
    )
    .unwrap_or_else(|e| die(&format!("parse: {e}")));
    let parts_dir = PathBuf::from(flag("--parts").unwrap_or_else(|| "parts".into()));
    let cat = ElecCatalogue::load(&parts_dir).unwrap_or_else(|e| die(&e));
    let svg = robowire::render::render_svg(&netlist, &cat).unwrap_or_else(|e| die(&e));
    let out = flag("--out").unwrap_or_else(|| format!("{}.svg", netlist.name));
    std::fs::write(&out, svg).unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
    println!("wrote {out}");
}

fn cmd_view(args: &[String]) {
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let path = PathBuf::from(&args[0]);
    let netlist: Netlist = serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| die(&format!("{path:?}: {e}"))),
    )
    .unwrap_or_else(|e| die(&format!("parse: {e}")));
    let robot_path = flag("--robot").unwrap_or_else(|| die("--robot <robot.json> required"));
    let robot: robotspec::RobotSpec = serde_json::from_slice(
        &std::fs::read(&robot_path).unwrap_or_else(|e| die(&format!("{robot_path}: {e}"))),
    )
    .unwrap_or_else(|e| die(&format!("parse {robot_path}: {e}")));
    let parts_dir = PathBuf::from(flag("--parts").unwrap_or_else(|| "parts".into()));
    let cat = ElecCatalogue::load(&parts_dir).unwrap_or_else(|e| die(&e));
    let html = robowire::view::build_scene(&netlist, &robot, &cat).unwrap_or_else(|e| die(&e));
    let out = flag("--out").unwrap_or_else(|| format!("{}-view.html", netlist.name));
    std::fs::write(&out, html).unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
    println!("wrote {out}");
    if !args.iter().any(|a| a == "--no-open") {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        if let Err(e) = std::process::Command::new(opener).arg(&out).spawn() {
            eprintln!("could not launch browser ({e}); open {out} manually");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("render") && args.len() >= 2 {
        return cmd_render(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) == Some("view") && args.len() >= 2 {
        return cmd_view(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) != Some("check") || args.len() < 2 {
        eprintln!(
            "usage: robowire <check|render|view> <netlist.json> [--robot robot.json] [--parts DIR] [--out FILE]"
        );
        std::process::exit(2);
    }
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let path = PathBuf::from(&args[1]);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| die(&format!("{path:?}: {e}")));
    let netlist: Netlist =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| die(&format!("parse: {e}")));
    let netlist_hash = sha256_hex(&bytes);
    let parts_dir = PathBuf::from(flag("--parts").unwrap_or_else(|| "parts".into()));
    let cat = ElecCatalogue::load(&parts_dir).unwrap_or_else(|e| die(&e));

    let checks = run_checks(&netlist, &cat).unwrap_or_else(|e| die(&e));
    println!("netlist  {}   {}", netlist.name, &netlist_hash[..16]);
    println!();
    for c in &checks {
        println!(
            "{} {}  {} — {}",
            if c.pass { "PASS" } else { "FAIL" },
            c.code,
            c.description,
            c.detail
        );
    }
    let all_pass = checks.iter().all(|c| c.pass);
    println!(
        "\nverdict: {}",
        if all_pass { "HARNESS LEGAL" } else { "E-CHECK FAILURES — do not solder" }
    );

    if let Some(out) = flag("--out") {
        let report = serde_json::json!({
            "netlist": netlist.name,
            "netlist_hash": netlist_hash,
            "robowire_version": robowire::ROBOWIRE_VERSION,
            "checks": checks,
            "pass": all_pass,
        });
        std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap())
            .unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
        println!("wrote {out}");
    }
    if !all_pass {
        std::process::exit(1);
    }
}
