//! robowire CLI command implementations (split from main.rs per review:
//! one file per concern; main() is a dispatcher only).

use robowire::catalogue::{sha256_hex, ElecCatalogue};
use robowire::{run_checks, Netlist};
use std::path::PathBuf;

pub(crate) fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2)
}

pub(crate) fn cmd_render(args: &[String]) {
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

pub(crate) fn cmd_view(args: &[String]) {
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

pub(crate) fn cmd_power(args: &[String]) {
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

    let derived = robotspec::derive(&robot, &cat).unwrap_or_else(|e| die(&e));
    let derived =
        robowire::power_graph::attach_power_graph(derived, &netlist, &cat).unwrap_or_else(|e| die(&e));

    println!(
        "mass_total_g  {:.2}  (chassis {:.2} + parts {:.2} + wiring {:.2})",
        derived.mass_total_g, derived.mass_chassis_g, derived.mass_parts_g, derived.mass_wiring_g
    );
    println!("budget_margin {:.2}g", derived.budget_margin_g);
    if let Some(pg) = &derived.power {
        for rail in &pg.rails {
            match rail.capacity_a {
                Some(cap) => println!(
                    "rail {}: {:.2}A worst-case vs {cap:.2}A capacity ({:+.2}A margin)",
                    rail.source, rail.worst_case_a, rail.margin_a.unwrap_or(0.0)
                ),
                None => println!("rail {}: {:.2}A worst-case (no declared capacity)", rail.source, rail.worst_case_a),
            }
        }
        for seg in &pg.segments {
            println!(
                "segment {} ({}AWG): {:.2}A worst-case vs {:.2}A ampacity",
                seg.net,
                seg.gauge_awg,
                seg.worst_case_a,
                seg.ampacity_a.unwrap_or(0.0)
            );
        }
        for chain in &pg.chains {
            println!("chain: {} -> {} -> {}", chain.source, chain.esc, chain.motor);
        }
    }
    for c in &derived.checks {
        if !c.pass {
            println!("FAIL {} {}", c.code, c.detail);
        }
    }

    if let Some(out) = flag("--out") {
        std::fs::write(&out, serde_json::to_string_pretty(&derived).unwrap())
            .unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
        println!("wrote {out}");
    }
}

pub(crate) fn cmd_design(args: &[String]) {
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let parts_dir = PathBuf::from(flag("--parts").unwrap_or_else(|| "parts".into()));
    // Parts as raw JSON values (descriptions and all) for the palette.
    let mut entries = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(&parts_dir)
        .unwrap_or_else(|e| die(&format!("{parts_dir:?}: {e}")))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let v: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path).unwrap_or_else(|e| die(&format!("{path:?}: {e}"))),
        )
        .unwrap_or_else(|e| die(&format!("{path:?}: {e}")));
        entries.push(v);
    }
    let parts_json = serde_json::to_string(&entries).unwrap();

    let netlist_json = match flag("--netlist") {
        Some(f) => {
            let v: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&f).unwrap_or_else(|e| die(&format!("{f}: {e}"))),
            )
            .unwrap_or_else(|e| die(&format!("{f}: {e}")));
            serde_json::to_string(&v).unwrap()
        }
        None => "null".to_string(),
    };

    let wasm_path = PathBuf::from(flag("--wasm").unwrap_or_else(|| {
        "robowire-wasm/target/wasm32-unknown-unknown/release/robowire_wasm.wasm".into()
    }));
    let wasm = std::fs::read(&wasm_path).unwrap_or_else(|e| {
        die(&format!(
            "{wasm_path:?}: {e} — build it with: cargo build --release --target wasm32-unknown-unknown (in robowire-wasm/)"
        ))
    });
    let b64 = {
        // minimal base64 (no new deps)
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(wasm.len() * 4 / 3 + 4);
        for chunk in wasm.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(T[(n >> 18) as usize & 63] as char);
            out.push(T[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
            out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
        }
        out
    };

    // Examples library (harness/examples/*.json), embedded for the sidebar.
    let examples_dir = PathBuf::from(flag("--examples").unwrap_or_else(|| "harness/examples".into()));
    let mut examples = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&examples_dir) {
        let mut ex_paths: Vec<_> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map_or(false, |x| x == "json"))
            .collect();
        ex_paths.sort();
        for path in ex_paths {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(&path).unwrap_or_default(),
            ) {
                examples.push(v);
            }
        }
    }
    let examples_json = serde_json::to_string(&examples).unwrap();

    // The designer ships as one self-contained file but is AUTHORED as
    // components: shell + thematic JS modules, assembled here. Edit the
    // modules, never the assembled artifact.
    const MODULES: [&str; 13] = [
        include_str!("../templates/designer/00-wasm.js"),
        include_str!("../templates/designer/01-state.js"),
        include_str!("../templates/designer/02-prose.js"),
        include_str!("../templates/designer/03-geom2d.js"),
        include_str!("../templates/designer/04-geom3d.js"),
        include_str!("../templates/designer/05-edit.js"),
        include_str!("../templates/designer/06-hittest.js"),
        include_str!("../templates/designer/07-render2d.js"),
        include_str!("../templates/designer/08-render3d.js"),
        include_str!("../templates/designer/09-panels.js"),
        include_str!("../templates/designer/10-io.js"),
        include_str!("../templates/designer/11-boot.js"),
        include_str!("../templates/designer/12-run.js"),
    ];
    let script = format!(
        "(async function () {{\n  \"use strict\";\n{}\n}})();",
        MODULES.join("\n")
    );
    let shell = include_str!("../templates/designer/shell.html");
    let template = shell.replace("//__MODULES__", &script);
    let template = template.as_str();
    // Deterministic build id: template + wasm content hash, so a screenshot
    // always identifies exactly which build rendered it.
    let build_id = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(template.as_bytes());
        h.update(&wasm);
        let d = h.finalize();
        format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
    };
    let html = template
        .replace("__BUILD__", &build_id)
        .replace("//__EXAMPLES__\n[];", &format!("{examples_json};"))
        .replace("//__PARTS__\n[];", &format!("{parts_json};"))
        .replace("//__NETLIST__\nnull;", &format!("{netlist_json};"))
        .replace("//__WASM__\n\"\";", &format!("\"{b64}\";"));
    let out = flag("--out").unwrap_or_else(|| "robowire-designer.html".into());
    std::fs::write(&out, html).unwrap_or_else(|e| die(&format!("writing {out}: {e}")));
    println!("wrote {out}");
    if !args.iter().any(|a| a == "--no-open") {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        if let Err(e) = std::process::Command::new(opener).arg(&out).spawn() {
            eprintln!("could not launch browser ({e}); open {out} manually");
        }
    }
}

