//! robowire — the electrical truth, at the command line.
//!   robowire <check|render|view|design> … (see cli.rs per command)

mod cli;

use cli::*;
use robowire::catalogue::{sha256_hex, ElecCatalogue};
use robowire::{run_checks, Netlist};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("design") {
        return cmd_design(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) == Some("render") && args.len() >= 2 {
        return cmd_render(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) == Some("view") && args.len() >= 2 {
        return cmd_view(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) != Some("check") || args.len() < 2 {
        eprintln!(
            "usage: robowire <check|render|view|design> [netlist.json] [--robot robot.json] [--netlist F] [--parts DIR] [--out FILE]"
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
