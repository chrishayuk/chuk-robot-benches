//! arena — chuk-arena CLI. M0 commands:
//!   arena run --seed N [--no-kernel] [--duration S]   run one episode
//!   arena replay [--seed N] [--duration S] [--out F]  render + open visual replay
//!   arena fuzz [--seeds N]                            in-process determinism fuzz
//!   arena ablate [--n N] [--seed S] [--duration S]    failsafe ablation report
//!   arena bench <envelope|dyno> [--out F]             virtual benches (§4.1/§4.2)
//!   arena diff [--out F]                              Rapier differential rig (§2.3)

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

fn round_to(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

/// Compact an EpisodeLog into the shape arena-view/template.html consumes
/// (replay precision, not the bit-exact record — same as arena-view/render.py).
fn compact_episode(log: &arena_store::EpisodeLog) -> serde_json::Value {
    let cfg = &log.config;
    let res = &log.result;
    let events: Vec<serde_json::Value> = log
        .events
        .iter()
        .map(|e| {
            let v = serde_json::to_value(e).unwrap();
            serde_json::json!({
                "k": v["kind"],
                "t": round_to(v["t"].as_f64().unwrap(), 3),
            })
        })
        .collect();
    let samples: Vec<serde_json::Value> = log
        .samples
        .iter()
        .map(|s| {
            serde_json::json!([
                round_to(s.t, 2),
                round_to(s.x, 4),
                round_to(s.y, 4),
                round_to(s.heading, 3),
                round_to(s.v, 3),
            ])
        })
        .collect();
    serde_json::json!({
        "seed": cfg.seed,
        "kernel": cfg.kernel.enabled,
        "arena": cfg.arena.half_extent,
        "bot": {"w": cfg.bot.footprint_half_w, "l": cfg.bot.footprint_half_l},
        "mu": round_to(res.mu, 3),
        "driver": {
            "lat": round_to(res.driver.reaction_latency_s, 3),
            "agg": round_to(res.driver.aggression, 2),
        },
        "outcome": &res.outcome,
        "interventions": res.interventions,
        "minEdge": round_to(res.min_edge_distance, 4),
        "events": events,
        "samples": samples,
    })
}

fn cmd_replay(args: &[String]) {
    let seed = parse_u64(args, "--seed", 42);
    let duration = parse_f64(args, "--duration", 45.0);
    let out = flag_value(args, "--out").unwrap_or_else(|| format!("replay-seed{seed}.html"));

    let on = run_episode(m0_config(seed, EdgeFailsafeParams::enabled_default(), duration));
    let off = run_episode(m0_config(seed, EdgeFailsafeParams::disabled(), duration));
    let data =
        serde_json::to_string(&[compact_episode(&on), compact_episode(&off)]).unwrap();

    const PLACEHOLDER: &str = "//__DATA__\n[];";
    let template = include_str!("../../arena-view/template.html");
    if !template.contains(PLACEHOLDER) {
        die("arena-view/template.html is missing the //__DATA__ placeholder");
    }
    let html = template.replace(PLACEHOLDER, &format!("{data};"));
    std::fs::write(&out, html).unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
    println!("wrote {out} (seed {seed}, failsafe on + off arms)");

    if !flag_present(args, "--no-open") {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(opener).arg(&out).spawn() {
            eprintln!("could not launch browser ({e}); open {out} manually");
        }
    }
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

fn cmd_bench(args: &[String]) {
    let json = match args.first().map(|s| s.as_str()) {
        Some("envelope") => {
            let naive = arena_bench::envelope_bench(arena_bench::BrakeKernel::NaiveCoast);
            let active =
                arena_bench::envelope_bench(arena_bench::BrakeKernel::ActiveAligned);
            eprintln!(
                "naive-coast:    {} (min margin {:+.4} m over {} samples)",
                naive.verdict, naive.min_margin_m, naive.samples
            );
            eprintln!(
                "active-aligned: {} (min margin {:+.4} m over {} samples)",
                active.verdict, active.min_margin_m, active.samples
            );
            serde_json::to_string_pretty(&serde_json::json!({
                "bench": "envelope-4.2",
                "naive_coast": naive,
                "active_aligned": active,
            }))
            .unwrap()
        }
        Some("dyno") => {
            let report = arena_bench::dyno_bench();
            serde_json::to_string_pretty(&serde_json::json!({
                "bench": "dyno-4.1",
                "report": report,
            }))
            .unwrap()
        }
        _ => die("usage: arena bench <envelope|dyno> [--out F]"),
    };
    if let Some(path) = flag_value(args, "--out") {
        std::fs::write(&path, &json)
            .unwrap_or_else(|e| die(&format!("writing {path}: {e}")));
        println!("wrote {path}");
    } else {
        println!("{json}");
    }
}

fn cmd_diff(args: &[String]) {
    let report = arena_diff::differential_report();
    for s in &report.scenarios {
        eprintln!(
            "{}: {} — {}",
            s.id,
            if s.status == "RUN" {
                let adjudicated = s.pass && s.max_divergence > s.tolerance;
                format!(
                    "{}{} (max divergence {:.2e} {} vs tol {:.0e})",
                    if s.pass { "PASS" } else { "FAIL" },
                    if adjudicated {
                        " via exact oracle [see notes]"
                    } else {
                        ""
                    },
                    s.max_divergence,
                    s.unit,
                    s.tolerance
                )
            } else {
                s.status.clone()
            },
            s.description
        );
    }
    eprintln!("kill criterion (§2.2): {}", report.kill_criterion);
    let json = serde_json::to_string_pretty(&report).unwrap();
    if let Some(path) = flag_value(args, "--out") {
        std::fs::write(&path, &json)
            .unwrap_or_else(|e| die(&format!("writing {path}: {e}")));
        println!("wrote {path}");
    } else {
        println!("{json}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[1..]),
        Some("replay") => cmd_replay(&args[1..]),
        Some("fuzz") => cmd_fuzz(&args[1..]),
        Some("ablate") => cmd_ablate(&args[1..]),
        Some("bench") => cmd_bench(&args[1..]),
        Some("diff") => cmd_diff(&args[1..]),
        _ => {
            eprintln!("usage: arena <run|replay|fuzz|ablate|bench|diff> [flags]");
            std::process::exit(2);
        }
    }
}
