//! arena — chuk-arena CLI. M0 commands:
//!   arena run --seed N [--no-kernel] [--duration S]   run one episode
//!   arena fuzz [--seeds N]                            in-process determinism fuzz
//!   arena ablate [--n N] [--seed S] [--duration S]    failsafe ablation report

use arena_cells::EdgeFailsafeParams;
use arena_tourney::{experiments, m0_config, run_episode, EpisodeMachine};

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn flag_present(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn parse_u64(args: &[String], name: &str, default: u64) -> u64 {
    flag_value(args, name)
        .map(|v| v.parse().unwrap_or_else(|_| die(&format!("bad {name}: {v}"))))
        .unwrap_or(default)
}

fn parse_f64(args: &[String], name: &str, default: f64) -> f64 {
    flag_value(args, name)
        .map(|v| v.parse().unwrap_or_else(|_| die(&format!("bad {name}: {v}"))))
        .unwrap_or(default)
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2)
}

fn cmd_run(args: &[String]) {
    let seed = parse_u64(args, "--seed", 0);
    let duration = parse_f64(args, "--duration", 60.0);
    let kernel = if flag_present(args, "--no-kernel") {
        EdgeFailsafeParams::disabled()
    } else {
        EdgeFailsafeParams::enabled_default()
    };
    let log = run_episode(m0_config(seed, kernel, duration));
    if let Some(path) = flag_value(args, "--out") {
        std::fs::write(&path, serde_json::to_vec(&log).unwrap())
            .unwrap_or_else(|e| die(&format!("writing {path}: {e}")));
    }
    let out = serde_json::json!({
        "identity": log.identity,
        "log_hash": log.log_hash(),
        "result": log.result,
        "events": log.events.len(),
        "samples": log.samples.len(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn cmd_fuzz(args: &[String]) {
    let n = parse_u64(args, "--seeds", 16);
    let duration = parse_f64(args, "--duration", 20.0);
    let mut failures = 0u64;
    for seed in 0..n {
        let cfg = m0_config(seed, EdgeFailsafeParams::enabled_default(), duration);

        // Leg 1: rerun.
        let a = run_episode(cfg.clone());
        let b = run_episode(cfg.clone());
        let rerun_ok = a == b;

        // Leg 2: serialize-roundtrip mid-episode.
        let mut m = EpisodeMachine::new(cfg);
        for _ in 0..20_000 {
            if m.done() {
                break;
            }
            m.step();
        }
        let json = serde_json::to_string(&m).unwrap();
        let mut resumed: EpisodeMachine = serde_json::from_str(&json).unwrap();
        while !resumed.done() {
            resumed.step();
        }
        let roundtrip_ok = resumed.finish().log_hash() == a.log_hash();

        let ok = rerun_ok && roundtrip_ok;
        if !ok {
            failures += 1;
        }
        println!(
            "seed {seed}: rerun {} roundtrip {}",
            if rerun_ok { "PASS" } else { "FAIL" },
            if roundtrip_ok { "PASS" } else { "FAIL" }
        );
    }
    if failures > 0 {
        eprintln!("{failures}/{n} seeds FAILED determinism fuzz");
        std::process::exit(1);
    }
    println!("determinism fuzz green: {n}/{n} seeds");
}

fn cmd_ablate(args: &[String]) {
    let n = parse_u64(args, "--n", 500);
    let seed = parse_u64(args, "--seed", 1);
    let duration = parse_f64(args, "--duration", 60.0);
    let report = experiments::failsafe_ablation(n, seed, duration);
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[1..]),
        Some("fuzz") => cmd_fuzz(&args[1..]),
        Some("ablate") => cmd_ablate(&args[1..]),
        _ => {
            eprintln!("usage: arena <run|fuzz|ablate> [flags]");
            std::process::exit(2);
        }
    }
}
