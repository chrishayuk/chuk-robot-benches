//! E30/E31/E32 acceptance (specs/robowire.md §3 "power budget", M1). Same
//! discipline as `mvp_harness.rs`: the checker is verified by its ability to
//! catch planted faults (undersized battery, undersized wire, an unbuffered
//! motor/MCU rail), not merely by agreeing with a correct design.

use robowire::catalogue::ElecCatalogue;
use robowire::{run_checks, Netlist, Tier};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn load() -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist =
        serde_json::from_slice(&std::fs::read(root.join("harness/mvp-wedge-harness.json")).unwrap()).unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

fn check(nl: &Netlist, cat: &ElecCatalogue, code: &str) -> robowire::CheckResult {
    run_checks(nl, cat).unwrap().into_iter().find(|c| c.code == code).unwrap_or_else(|| panic!("no check {code}"))
}

#[test]
fn mvp_wedge_e30_e31_e32_all_pass_or_not_applicable() {
    // The MVP wedge is already a correctly-buffered, well-within-budget
    // design (battery 30C x 260mAh = 7.8A, dual N20 stall ~1.6A each; MCU's
    // 5V rail comes through the ESC's own BEC, a genuine regulator hop).
    // No net declares a gauge yet, so E31 is vacuously "every gauge-declared
    // net" (none) — legal. Confirms the checks don't false-positive on a
    // real, correct design before testing that they DO catch planted faults.
    let (nl, cat) = load();
    let e30 = check(&nl, &cat, "E30");
    let e31 = check(&nl, &cat, "E31");
    let e32 = check(&nl, &cat, "E32");
    assert!(e30.pass, "{e30:?}");
    assert!(e31.pass, "{e31:?}");
    assert!(e32.pass && e32.tier == Tier::Fail, "MCU is properly BEC-buffered, expected no warn: {e32:?}");
}

#[test]
fn planted_undersized_battery_fails_e30() {
    let (nl, mut cat) = load();
    // Shrink the battery's declared capacity until even the wedge's own
    // (legitimate) worst-case draw exceeds it.
    let (batt, _) = cat.parts.get_mut("lipo-2s-260").unwrap();
    let source = batt.elec.as_mut().unwrap().source.as_mut().unwrap();
    source.c_rating = Some(1.0);
    source.capacity_mah = Some(1.0); // 1C x 1mAh = 0.001A cap
    let e30 = check(&nl, &cat, "E30");
    assert!(!e30.pass, "undersized battery must fail E30");
    assert!(e30.detail.contains("batt"), "{e30:?}");
}

#[test]
fn planted_undersized_regulator_fails_e30() {
    let root = repo_root();
    let nl: Netlist = serde_json::from_slice(
        &std::fs::read(root.join("harness/examples/example-led-from-brain.json")).unwrap(),
    )
    .unwrap();
    let mut cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    let (reg, _) = cat.parts.get_mut("reg-5v-mini").unwrap();
    let out = reg.elec.as_mut().unwrap().pins.get_mut("OUT").unwrap();
    out.max_a = Some(0.0001);
    let e30 = check(&nl, &cat, "E30");
    assert!(!e30.pass, "undersized regulator output must fail E30");
    assert!(e30.detail.contains("OUT"), "{e30:?}");
}

#[test]
fn planted_thin_gauge_fails_e31() {
    let (mut nl, cat) = load();
    // 30AWG (0.142A rated) on the net carrying the dual-motor ESC's full
    // stall current (~3.2A worst case) — nowhere close.
    for net in nl.nets.iter_mut() {
        if net.id == "vbat" {
            net.gauge_awg = Some(30);
        }
    }
    let e31 = check(&nl, &cat, "E31");
    assert!(!e31.pass, "30AWG on the main motor rail must fail E31");
    assert!(e31.detail.contains("30AWG"), "{e31:?}");
}

#[test]
fn planted_thin_motor_leg_gauge_fails_e31() {
    // A motor's own terminal wires carry its full stall current too, not
    // just the ESC's supply net — regression test for that attribution
    // (mirrors robosim::simulate's identical choice for the live model).
    let (mut nl, cat) = load();
    for net in nl.nets.iter_mut() {
        if net.id == "m_l_a" {
            net.gauge_awg = Some(30); // 0.142A rated vs m_l's 1.6A stall current
        }
    }
    let e31 = check(&nl, &cat, "E31");
    assert!(!e31.pass, "30AWG on a motor terminal leg must fail E31 too, not just the supply-side net");
    assert!(e31.detail.contains("m_l_a"), "{e31:?}");
}

#[test]
fn adequate_gauge_passes_e31() {
    let (mut nl, cat) = load();
    // 12AWG (9.3A rated) comfortably covers the same rail.
    for net in nl.nets.iter_mut() {
        if net.id == "vbat" {
            net.gauge_awg = Some(12);
        }
    }
    let e31 = check(&nl, &cat, "E31");
    assert!(e31.pass, "{e31:?}");
}

#[test]
fn no_mcu_present_e32_not_applicable() {
    let root = repo_root();
    let nl: Netlist =
        serde_json::from_slice(&std::fs::read(root.join("harness/examples/example-blink-basic.json")).unwrap())
            .unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    let e32 = check(&nl, &cat, "E32");
    assert!(e32.pass && e32.tier == Tier::Fail, "no MCU at all must be a clean ok, not a warn: {e32:?}");
}

#[test]
fn planted_unbuffered_mcu_rail_warns_e32_without_blocking_verdict() {
    // MCU wired directly onto the same net as the motor-driving ESC's own
    // VIN — no BEC/regulator hop between them, the exact brownout-exposure
    // topology E32 exists to catch.
    let nl: Netlist = serde_json::from_value(serde_json::json!({
        "name": "unbuffered-mcu-rail",
        "instances": {
            "batt": "lipo-2s-260", "sw": "power-switch-slide",
            "esc": "bbb-dual-esc", "mcu": "rp2350-zero", "m1": "n20-motor-6v"
        },
        "nets": [
            { "id": "vbat_sw", "pins": ["batt.+", "sw.in"], "volts": 7.4 },
            { "id": "rail", "pins": ["sw.out", "esc.VIN", "mcu.5V"], "volts": 7.4 },
            { "id": "gnd", "pins": ["batt.-", "esc.GND", "mcu.GND"] },
            { "id": "m1_a", "pins": ["esc.M1+", "m1.M+"] },
            { "id": "m1_b", "pins": ["esc.M1-", "m1.M-"] }
        ],
        "buses": [],
        "failsafe": null
    }))
    .unwrap();
    let root = repo_root();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();

    let e32 = check(&nl, &cat, "E32");
    assert!(e32.tier == Tier::Warn, "{e32:?}");
    assert!(e32.pass, "warn must not block (pass stays true): {e32:?}");
    assert!(e32.detail.contains("mcu"), "{e32:?}");

    // The warn must not flip run_checks' overall picture: nothing else in
    // this deliberately-minimal netlist should fail either (this harness
    // has no bus/failsafe/etc. to trip other codes), and no result may have
    // pass == false because of E32.
    let all = run_checks(&nl, &cat).unwrap();
    let e32_in_all = all.iter().find(|c| c.code == "E32").unwrap();
    assert!(e32_in_all.pass);
}

#[test]
fn isolated_battery_domains_are_not_cross_contaminated_in_e30() {
    // Two electrically-isolated battery domains (no shared nets at all,
    // not even ground): domain A is deliberately undersized so its OWN tiny
    // load alone already exceeds its own capacity; domain B carries a much
    // heavier (but, for B, perfectly legal) dual-motor load. If E30 ever
    // unioned battery seeds (the mistake robosim's live hot-BFS deliberately
    // accepts for its own boolean "powered at all" projection, but which
    // E30 must NOT repeat), domain A's reported worst-case draw would
    // include domain B's motors too. Assert on the actual number in the
    // failure detail, not just pass/fail, so a reintroduced union bug is
    // caught even if it wouldn't otherwise flip the pass/fail outcome.
    let nl: Netlist = serde_json::from_value(serde_json::json!({
        "name": "two-isolated-domains",
        "instances": {
            "battA": "lipo-2s-260", "buzzerA": "buzzer-active-5v",
            "battB": "lipo-2s-260", "swB": "power-switch-slide",
            "escB": "bbb-dual-esc", "m1B": "n20-motor-6v", "m2B": "n20-motor-6v"
        },
        "nets": [
            { "id": "railA", "pins": ["battA.+", "buzzerA.+"], "volts": 7.4 },
            { "id": "gndA", "pins": ["battA.-", "buzzerA.-"] },
            { "id": "vbat_swB", "pins": ["battB.+", "swB.in"], "volts": 7.4 },
            { "id": "vbatB", "pins": ["swB.out", "escB.VIN"], "volts": 7.4 },
            { "id": "gndB", "pins": ["battB.-", "escB.GND"] },
            { "id": "m1_aB", "pins": ["escB.M1+", "m1B.M+"] },
            { "id": "m1_bB", "pins": ["escB.M1-", "m1B.M-"] },
            { "id": "m2_aB", "pins": ["escB.M2+", "m2B.M+"] },
            { "id": "m2_bB", "pins": ["escB.M2-", "m2B.M-"] }
        ],
        "buses": [],
        "failsafe": null
    }))
    .unwrap();
    let root = repo_root();
    let mut cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    // Shrink ONLY domain A's battery so its own 30mA buzzer alone exceeds
    // it, while domain B keeps its normal 30C x 260mAh = 7.8A capacity
    // (comfortably covering its own ~3.2A dual-motor stall draw).
    //
    // NOTE: both instances resolve to the SAME catalogue entry
    // ("lipo-2s-260"), so this mutation applies to both domains' battery —
    // that's fine here since domain B's own real load (~3.2A) still fits
    // the ORIGINAL 7.8A capacity, not the shrunk one; the point of this test
    // is domain A's reported number, not B's pass/fail.
    let (batt, _) = cat.parts.get_mut("lipo-2s-260").unwrap();
    let source = batt.elec.as_mut().unwrap().source.as_mut().unwrap();
    source.c_rating = Some(1.0);
    source.capacity_mah = Some(20.0); // 1C x 20mAh = 0.02A cap — below the 0.03A buzzer alone

    let e30 = check(&nl, &cat, "E30");
    assert!(!e30.pass, "domain A's own tiny load must already exceed its shrunk cap: {e30:?}");
    assert!(e30.detail.contains("battA"), "expected the FIRST offending battery to be battA: {e30:?}");
    // The reported worst-case draw must be domain A's OWN ~0.03A buzzer
    // draw, not that plus domain B's ~3.2A of motors — proof E30 scoped
    // reachability per-battery rather than unioning all battery seeds.
    assert!(
        e30.detail.contains("0.03A") || e30.detail.contains("0.030A"),
        "expected only domain A's own current in the detail, got: {e30:?}"
    );
}
