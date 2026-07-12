//! robowire M0.5 acceptance (specs/robowire.md §3a): interactive run mode's
//! event-driven propagation — switch/button gate continuity, passthrough
//! parts relay power only when grounded, and LEDs/motors/sensors project the
//! right state. Mirrors robowire's `mvp_harness.rs` load-from-`harness/`
//! pattern. Current assertions hand-compute the expected Ohm's-law figure
//! from the same catalogue-declared component data the engine uses — never a
//! hardcoded "fixed" number — so these tests double as proof the model
//! really is live component math, not a lookup table.

use robowire::catalogue::ElecCatalogue;
use robowire::Netlist;
use robosim::{run_state, RunInputs};
use std::collections::BTreeMap;
use std::path::PathBuf;

const EPS: f64 = 1e-6;
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn load(harness_file: &str) -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist =
        serde_json::from_slice(&std::fs::read(root.join(harness_file)).unwrap()).unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

fn demo() -> (Netlist, ElecCatalogue) {
    load("harness/examples/example-run-mode-demo.json")
}

fn dimmer() -> (Netlist, ElecCatalogue) {
    load("harness/examples/example-dial-dimmer.json")
}

fn wedge() -> (Netlist, ElecCatalogue) {
    load("harness/mvp-wedge-harness.json")
}

fn sensors_demo() -> (Netlist, ElecCatalogue) {
    load("harness/examples/example-sensors-demo.json")
}

fn inputs_with_switch(closed: bool) -> RunInputs {
    let mut inputs = RunInputs::default();
    inputs.switches.insert("sw".to_string(), closed);
    inputs
}

fn set_net_volts(nl: &mut Netlist, net_id: &str, volts: f64) {
    for net in &mut nl.nets {
        if net.id == net_id {
            net.volts = Some(volts);
        }
    }
}

fn set_net_wire(nl: &mut Netlist, net_id: &str, gauge_awg: u32, length_mm: f64) {
    for net in &mut nl.nets {
        if net.id == net_id {
            net.gauge_awg = Some(gauge_awg);
            net.length_mm = Some(length_mm);
        }
    }
}

#[test]
fn switch_open_everything_dark() {
    let (nl, cat) = demo();
    let st = run_state(&nl, &cat, &RunInputs::default()).unwrap();
    assert!(!st.nets["vbat"].hot);
    assert_eq!(st.instances["led1"].lit, Some(false));
    assert_eq!(st.instances["led2"].lit, Some(false));
    assert_eq!(st.instances["esc"].powered, Some(false));
    assert_eq!(st.instances["esc"].current_a, Some(0.0));
    assert_eq!(st.instances["m1"].spin, Some(0.0));
    assert_eq!(st.instances["m1"].current_a, Some(0.0));
    assert_eq!(st.instances["tof1"].powered, Some(false));
}

#[test]
fn led_anode_net_shows_forward_voltage_not_zero_while_lit() {
    let (nl, cat) = demo();
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    // led1_feed is downstream of a resistor (a real voltage boundary, so it
    // never inherits v33's 3.3V by propagation) — but the LED is lit and
    // conducting, so its own net should read its forward_v (2.0V), not 0V
    // (which would be current-flowing-at-zero-volts, the same bug class
    // already fixed for switch/button-gated nets).
    assert_eq!(st.instances["led1"].lit, Some(true));
    assert!(st.nets["led1_feed"].amps > 0.0);
    assert_eq!(st.nets["led1_feed"].volts, 2.0);
}

#[test]
fn switch_closed_led_current_is_ohms_law_across_its_series_resistor() {
    let (nl, cat) = demo();
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    assert_eq!(st.instances["led1"].lit, Some(true));
    assert_eq!(st.instances["led1"].current_limited, Some(true));
    assert_eq!(st.instances["tof1"].powered, Some(true));

    // led-red-5mm forward_v=2.0, resistor-330r ohms=330, fed from v33 (3.3V).
    // I = (V - Vf) / R.
    let expected = (3.3 - 2.0) / 330.0;
    let actual = st.instances["led1"].current_a.unwrap();
    assert!(close(actual, expected), "led1 current_a = {actual}, expected {expected}");
}

#[test]
fn led_current_scales_when_supply_voltage_changes() {
    let (mut nl, cat) = demo();
    let before = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    let before_amps = before.instances["led1"].current_a.unwrap();

    // Same LED, same series resistor — a different supply voltage on v33.
    set_net_volts(&mut nl, "v33", 5.0);
    let after = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    let after_amps = after.instances["led1"].current_a.unwrap();

    let expected_after = (5.0 - 2.0) / 330.0;
    assert!(close(after_amps, expected_after), "led1 current_a = {after_amps}, expected {expected_after}");
    // The whole point: it MOVED when the voltage did, not a fixed number.
    assert!((after_amps - before_amps).abs() > 1e-4, "current didn't change when voltage did");
}

#[test]
fn motor_current_is_ohms_law_and_scales_with_supply_voltage() {
    let (nl, cat) = demo();
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 2000.0); // full forward (2000µs = +1.0 throttle)
    let st = run_state(&nl, &cat, &inputs).unwrap();

    // n20-motor-6v: stall_current_a=1.6, nominal_v=6.0 -> r_winding = 3.75Ω.
    // Fed from vbat (7.4V, declared). current = throttle * V / r_winding.
    let r_winding = 6.0_f64 / 1.6;
    let expected = 1.0_f64 * 7.4 / r_winding;
    let actual = st.instances["m1"].current_a.unwrap();
    assert!(close(actual, expected), "m1 current_a = {actual}, expected {expected}");
    // NOT the naive fixed stall_current_a (1.6A regardless of voltage).
    assert!((actual - 1.6).abs() > 1e-3);

    // Same throttle, a lower battery voltage -> current follows it down.
    let mut nl2 = nl.clone();
    set_net_volts(&mut nl2, "vbat_sw", 6.0);
    set_net_volts(&mut nl2, "vbat", 6.0);
    let st2 = run_state(&nl2, &cat, &inputs).unwrap();
    let expected2 = 1.0_f64 * 6.0 / r_winding;
    let actual2 = st2.instances["m1"].current_a.unwrap();
    assert!(close(actual2, expected2), "m1 current_a = {actual2}, expected {expected2}");
    assert!(actual2 < actual, "lower supply voltage should draw less current");
}

#[test]
fn motor_reports_powered_independent_of_throttle() {
    // A motor's ESC channel can be powered while sitting at zero throttle —
    // a distinct, visible state from "no power reaching it at all" (the
    // same distinction switch/LED already draw via `closed`/`lit`), not
    // something only a spin animation (which needs nonzero throttle) shows.
    let (nl, cat) = demo();

    let mut idle = inputs_with_switch(true);
    idle.pwm_signals.insert("mcu.GP2".to_string(), 1500.0); // neutral (0.0 throttle)
    let st = run_state(&nl, &cat, &idle).unwrap();
    assert_eq!(st.instances["m1"].powered, Some(true), "powered rail, zero throttle -> still powered");
    assert_eq!(st.instances["m1"].spin, Some(0.0));

    let off = inputs_with_switch(false);
    let st_off = run_state(&nl, &cat, &off).unwrap();
    assert_eq!(st_off.instances["m1"].powered, Some(false), "switch open -> motor's own channel unpowered");
    assert_eq!(st_off.instances["m1"].reason.as_deref(), Some("driver channel unpowered"));
}

#[test]
fn motor_current_also_shows_on_its_own_terminal_wires() {
    let (nl, cat) = demo();
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 1850.0); // 1850µs = 0.7 throttle
    let st = run_state(&nl, &cat, &inputs).unwrap();

    // The current visibly flowing INTO the motor (m1_a/m1_b, its own two
    // terminal wires) must match what the motor itself reports — not just
    // show up upstream at the battery. This is what a flow-animation on the
    // wires would otherwise miss: the motor's own leads reading "no flow"
    // while it's visibly spinning.
    let motor_amps = st.instances["m1"].current_a.unwrap();
    assert!(motor_amps > 0.0);
    assert!(close(st.nets["m1_a"].amps, motor_amps));
    assert!(close(st.nets["m1_b"].amps, motor_amps));
}

#[test]
fn fixed_power_device_current_matches_its_equivalent_resistance() {
    let (nl, cat) = demo();
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    // vl53l0x-breakout: current_ma=19, nominal_v=3.3; v33 is declared 3.3V.
    let expected = 19.0_f64 / 1000.0 * (3.3 / 3.3);
    let actual = st.instances["tof1"].current_a.unwrap();
    assert!(close(actual, expected), "tof1 current_a = {actual}, expected {expected}");
}

#[test]
fn battery_current_is_the_sum_of_every_live_load() {
    let (nl, cat) = demo();
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 1850.0); // 1850µs = 0.7 throttle
    let st = run_state(&nl, &cat, &inputs).unwrap();

    let esc_amps = 15.0_f64 / 1000.0 * (7.4 / 7.4);
    let mcu_amps = 40.0_f64 / 1000.0 * (5.0 / 5.0);
    let tof_amps = 19.0_f64 / 1000.0 * (3.3 / 3.3);
    let led1_amps = (3.3_f64 - 2.0) / 330.0;
    let motor_amps = 0.7_f64 * 7.4 / (6.0 / 1.6);
    let expected_total = esc_amps + mcu_amps + tof_amps + led1_amps + motor_amps;

    let batt_amps = st.instances["batt"].current_a.unwrap();
    assert!(close(batt_amps, expected_total), "batt current_a = {batt_amps}, expected {expected_total}");
    assert!(close(st.nets["vbat_sw"].amps, expected_total));
    assert!(close(st.nets["vbat"].amps, expected_total));
    // v33 only carries what's downstream of it (tof1 + led1), not the motor.
    assert!(close(st.nets["v33"].amps, tof_amps + led1_amps));
}

#[test]
fn battery_terminal_voltage_sags_proportional_to_its_own_current() {
    let (nl, cat) = demo();
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 1850.0); // 1850µs = 0.7 throttle
    let st = run_state(&nl, &cat, &inputs).unwrap();

    // lipo-2s-260 declares r_internal_ohm = 0.18.
    let batt_amps = st.instances["batt"].current_a.unwrap();
    let expected_sag = batt_amps * 0.18;
    let actual_sag = st.instances["batt"].sag_v.unwrap();
    assert!(close(actual_sag, expected_sag), "sag_v = {actual_sag}, expected {expected_sag}");

    // Switch off -> zero current -> zero sag, not a fixed number.
    let idle = run_state(&nl, &cat, &inputs_with_switch(false)).unwrap();
    assert!(close(idle.instances["batt"].sag_v.unwrap(), 0.0), "no current flowing must mean no sag");

    // One-shot: the battery's OWN positive net still shows its undropped
    // declared voltage (7.4V) even though sag_v is computed and available —
    // the sag is a separate, additional number, not an overwrite (see
    // `InstanceRunState.sag_v`'s doc comment for why feeding it back even
    // one hop was deliberately rejected).
    assert!(actual_sag > 0.0, "expect nonzero sag under this load to make the next assertion meaningful");
    assert!(close(st.nets["vbat_sw"].volts, 7.4), "sag must not overwrite the net's own declared voltage");
}

#[test]
fn button_gates_second_led_behind_switch() {
    let (nl, cat) = demo();

    // switch alone: led2 still dark (button not held).
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    assert_eq!(st.instances["led2"].lit, Some(false));

    // switch + button held: led2 lights.
    let mut inputs = inputs_with_switch(true);
    inputs.buttons.insert("btn".to_string(), true);
    let st = run_state(&nl, &cat, &inputs).unwrap();
    assert_eq!(st.instances["led2"].lit, Some(true));

    // led2's supply passes through an UNDECLARED intermediate net (btn_feed
    // has no "volts" in the harness JSON — it's just a wire between the
    // button and the resistor). It must still resolve to vbat's 7.4V (an
    // ideal switch/button carries voltage across unchanged), giving a real,
    // nonzero current — not the "lit LED, 0.00A" bug this model exists to
    // avoid. led-green-5mm forward_v=2.2, resistor-330r ohms=330.
    let expected = (7.4 - 2.2) / 330.0;
    let actual = st.instances["led2"].current_a.unwrap();
    assert!(close(actual, expected), "led2 current_a = {actual}, expected {expected}");

    // button held, switch open: still dark.
    let mut inputs2 = RunInputs::default();
    inputs2.buttons.insert("btn".to_string(), true);
    let st2 = run_state(&nl, &cat, &inputs2).unwrap();
    assert_eq!(st2.instances["led2"].lit, Some(false));
}

#[test]
fn switch_open_voltage_present_at_battery_terminal_only() {
    let (nl, cat) = demo();
    let st = run_state(&nl, &cat, &RunInputs::default()).unwrap();
    // The battery's own terminal net is always at its declared voltage,
    // switch or no switch — only what's downstream of the (open) switch is dark.
    assert_eq!(st.nets["vbat_sw"].volts, 7.4);
    assert_eq!(st.nets["vbat"].volts, 0.0);
    assert_eq!(st.nets["v5"].volts, 0.0);
    assert_eq!(st.nets["v33"].volts, 0.0);
    assert_eq!(st.nets["vbat_sw"].amps, 0.0);
}

#[test]
fn mvp_wedge_motors_spin_independently_by_channel() {
    let (nl, cat) = wedge();
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 1750.0); // 1750µs = 0.5 throttle
    inputs.pwm_signals.insert("mcu.GP3".to_string(), 1350.0); // 1350µs = -0.3 throttle (reverse)
    let st = run_state(&nl, &cat, &inputs).unwrap();
    assert_eq!(st.instances["m_l"].spin, Some(0.5));
    assert_eq!(st.instances["m_r"].spin, Some(-0.3));
    // Independent Ohm's-law currents too (same winding, opposite direction
    // uses abs() so both draw current).
    assert!(st.instances["m_l"].current_a.unwrap() > 0.0);
    assert!(st.instances["m_r"].current_a.unwrap() > 0.0);
}

#[test]
fn mvp_wedge_signal_is_pinned_to_the_mcu_pin_not_the_motor() {
    // Only GP2 gets a value — GP3 (m_r's own channel) never does. If this
    // were still keyed by motor instance, both would independently take
    // whatever was set for them; keyed by MCU pin, m_r must stay at rest
    // since nothing actually drives esc.S2.
    let (nl, cat) = wedge();
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 1900.0); // 1900µs = 0.8 throttle
    let st = run_state(&nl, &cat, &inputs).unwrap();
    assert_eq!(st.instances["m_l"].spin, Some(0.8));
    assert_eq!(st.instances["m_r"].spin, Some(0.0));

    // The MCU itself reports which of its own pins are wired to drive a
    // motor, and which — same run's mcu.GP2 -> m_l, mcu.GP3 -> m_r.
    let channels = st.instances["mcu"].pwm_channels.as_ref().unwrap();
    assert!(channels.iter().any(|c| c.pin == "GP2" && c.drives.as_deref() == Some("m_l")));
    assert!(channels.iter().any(|c| c.pin == "GP3" && c.drives.as_deref() == Some("m_r")));
}

#[test]
fn stage3_motor_driver_never_spins_without_a_brain_wired_up() {
    // harness/lessons/03-motor-driver.json has a real, powered ESC+motor but
    // no MCU at all — esc.S1 sits on a dummy single-pin net. Setting a pwm
    // signal for a pin that isn't even in this netlist must have no effect:
    // the motor is powered (the rail reaches it) but never spins, and says
    // exactly why, rather than quietly taking whatever value is handed to it.
    let (nl, cat) = load("harness/lessons/03-motor-driver.json");
    let mut inputs = inputs_with_switch(true);
    inputs.pwm_signals.insert("mcu.GP2".to_string(), 2000.0); // full forward — still irrelevant, nothing is wired to it here
    let st = run_state(&nl, &cat, &inputs).unwrap();
    assert_eq!(st.instances["m1"].powered, Some(true));
    assert_eq!(st.instances["m1"].spin, Some(0.0));
    assert_eq!(st.instances["m1"].reason.as_deref(), Some("no signal source wired to this channel"));
}

#[test]
fn mvp_wedge_bus_conflict_false_normally() {
    let (nl, cat) = wedge();
    let st = run_state(&nl, &cat, &RunInputs::default()).unwrap();
    assert_eq!(st.instances["tof_l"].bus_conflict, Some(false));
    assert_eq!(st.instances["tof_r"].bus_conflict, Some(false));
    assert_eq!(st.instances["imu"].bus_conflict, Some(false));
}

#[test]
fn mvp_wedge_planted_dual_0x29_yields_bus_conflict() {
    let (mut nl, cat) = wedge();
    for bus in &mut nl.buses {
        for dev in &mut bus.devices {
            if dev.inst == "tof_l" {
                dev.reassign_to = None;
            }
        }
    }
    let st = run_state(&nl, &cat, &RunInputs::default()).unwrap();
    assert_eq!(st.instances["tof_l"].bus_conflict, Some(true));
    assert_eq!(st.instances["tof_r"].bus_conflict, Some(true));
}

#[test]
fn dial_alone_satisfies_e33_current_limiting() {
    let (nl, cat) = dimmer();
    let mut inputs = RunInputs::default();
    inputs.switches.insert("sw".to_string(), true);
    let st = run_state(&nl, &cat, &inputs).unwrap();
    // No fixed resistor anywhere in this harness — only the potentiometer.
    assert_eq!(st.instances["led1"].lit, Some(true));
    assert_eq!(st.instances["led1"].current_limited, Some(true));
}

#[test]
fn twisting_the_dial_changes_led_current_live() {
    let (nl, cat) = dimmer();
    let mut inputs = RunInputs::default();
    inputs.switches.insert("sw".to_string(), true);

    // potentiometer-1k: ohms_min=100, ohms_max=1000. led-red-5mm forward_v=2.0.
    // Fed from "feed" (7.4V, declared). I = (V - Vf) / R.
    let expected = |dial: f64| -> f64 {
        let ohms = 100.0 + (1000.0 - 100.0) * dial;
        (7.4 - 2.0) / ohms
    };

    for dial in [0.0, 0.25, 0.5, 0.75, 1.0] {
        inputs.dial_positions.insert("pot".to_string(), dial);
        let st = run_state(&nl, &cat, &inputs).unwrap();
        let actual = st.instances["led1"].current_a.unwrap();
        let want = expected(dial);
        assert!(close(actual, want), "dial={dial}: current_a = {actual}, expected {want}");
    }

    // Turning the dial toward max resistance must strictly DIM the LED
    // (lower current) — the whole point of a dimmer.
    inputs.dial_positions.insert("pot".to_string(), 0.1);
    let bright = run_state(&nl, &cat, &inputs).unwrap().instances["led1"].current_a.unwrap();
    inputs.dial_positions.insert("pot".to_string(), 0.9);
    let dim = run_state(&nl, &cat, &inputs).unwrap().instances["led1"].current_a.unwrap();
    assert!(dim < bright, "higher dial position should draw less current (dimmer): dim={dim} bright={bright}");
}

#[test]
fn dial_default_position_is_midway_when_untouched() {
    let (nl, cat) = dimmer();
    let mut inputs = RunInputs::default();
    inputs.switches.insert("sw".to_string(), true);
    // No dial_positions entry at all — should default to 0.5, not 0.
    let st = run_state(&nl, &cat, &inputs).unwrap();
    let expected = (7.4 - 2.0) / (100.0 + (1000.0 - 100.0) * 0.5);
    let actual = st.instances["led1"].current_a.unwrap();
    assert!(close(actual, expected), "default dial current_a = {actual}, expected {expected}");
}

#[test]
fn gauge_declared_net_shows_a_live_wire_drop() {
    let (mut nl, cat) = demo();
    set_net_wire(&mut nl, "v33", 26, 150.0); // 26AWG, 150mm
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();

    let v33 = &st.nets["v33"];
    assert!(v33.amps > 0.0, "v33 should be carrying current for this to mean anything");
    let expected_ohms = robowire::wire::awg_resistance_ohms_per_m(26).unwrap() * (150.0 / 1000.0);
    let expected_drop = v33.amps * expected_ohms;
    let actual_drop = v33.wire_drop_v.unwrap();
    assert!(close(actual_drop, expected_drop), "wire_drop_v = {actual_drop}, expected {expected_drop}");

    // One-shot: the drop must not have fed back into anything else. v33's
    // OWN volts field, and led1's current, are unaffected by the drop.
    let without = run_state(&{ let (nl, _) = demo(); nl }, &cat, &inputs_with_switch(true)).unwrap();
    assert!(close(v33.volts, without.nets["v33"].volts), "volts must not be adjusted by wire_drop_v");
    assert!(
        close(st.instances["led1"].current_a.unwrap(), without.instances["led1"].current_a.unwrap()),
        "downstream current must not be affected by the wire drop (one-shot, not iterative)"
    );
}

#[test]
fn net_with_no_gauge_has_no_wire_drop() {
    let (nl, cat) = demo();
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    assert!(st.nets["v33"].amps > 0.0);
    assert!(st.nets["v33"].wire_drop_v.is_none(), "no gauge/length declared — must not fabricate a drop");
}

#[test]
fn bare_led_reports_burned_a_protected_led_does_not() {
    // burned == lit && !current_limited (crate::led) — a real consequence
    // of E33's own failure condition, not a rendering-layer inference.
    let (nl, cat) = load("harness/lessons/01-basics-broken.json");
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    assert_eq!(st.instances["led1"].lit, Some(true));
    assert_eq!(st.instances["led1"].current_limited, Some(false));
    assert_eq!(st.instances["led1"].burned, Some(true), "an unprotected, powered LED must report burned");

    let (nl, cat) = load("harness/lessons/01-basics.json");
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();
    assert_eq!(st.instances["led1"].lit, Some(true));
    assert_eq!(st.instances["led1"].current_limited, Some(true));
    assert_eq!(st.instances["led1"].burned, Some(false), "a properly current-limited LED must not report burned");

    // Switch open: not lit, so not burned either, regardless of protection.
    let (nl, cat) = load("harness/lessons/01-basics-broken.json");
    let st = run_state(&nl, &cat, &inputs_with_switch(false)).unwrap();
    assert_eq!(st.instances["led1"].lit, Some(false));
    assert_eq!(st.instances["led1"].burned, Some(false), "an unpowered LED isn't burning, protected or not");
}

#[test]
fn light_and_env_sensors_share_tof_imu_shape_with_no_new_component_module() {
    // line-sensor-analog (kind "light", not on any bus) and env-bme280
    // (kind "env", on the I2C bus alongside tof-longrange) both go through
    // the exact same `sensor::compute` path as tof/imu — proving that
    // reusing the dispatch arm (no new component module) actually works,
    // not just compiles.
    let (nl, cat) = sensors_demo();
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();

    // light1 has no declared `readings` — single-value shape, same as
    // tof/imu; defaults to 0 (no range_mm declared).
    assert_eq!(st.instances["light1"].value, Some(0.0));
    assert_eq!(st.instances["light1"].readings, None);

    // env-bme280 declares three named readings — one physical part, three
    // independent numbers, not one collapsed value.
    assert_eq!(st.instances["env1"].value, None);
    let env_readings = st.instances["env1"].readings.as_ref().unwrap();
    assert_eq!(env_readings.len(), 3);
    assert_eq!(env_readings.get("temp_c"), Some(&0.0));
    assert_eq!(env_readings.get("humidity_pct"), Some(&0.0));
    assert_eq!(env_readings.get("pressure_hpa"), Some(&0.0));

    // Current draw is real Ohm's-law-equivalent math against each part's
    // OWN declared rail, not a shared/fixed number: light1 off v33 (3.3V),
    // env1 off v33 (3.3V), tof1 off v5 (5.0V) — a genuinely different rail.
    assert!(close(st.instances["light1"].current_a.unwrap(), 10.0 / 1000.0));
    assert!(close(st.instances["env1"].current_a.unwrap(), 1.0 / 1000.0));
    assert!(close(st.instances["tof1"].current_a.unwrap(), 80.0 / 1000.0));

    // tof1/env1 share the I2C bus at different addresses (0x10 vs 0x76) —
    // no conflict. light1 isn't on any bus at all, so bus_conflict is
    // untouched (None), not a spurious `Some(false)`.
    assert_eq!(st.instances["tof1"].bus_conflict, Some(false));
    assert_eq!(st.instances["env1"].bus_conflict, Some(false));
    assert_eq!(st.instances["light1"].bus_conflict, None);

    // A user-set fake reading round-trips through inputs.sensor_values same
    // as any tof/imu instance.
    let mut inputs = inputs_with_switch(true);
    inputs.sensor_values.insert("light1".to_string(), 42.0);
    let st2 = run_state(&nl, &cat, &inputs).unwrap();
    assert_eq!(st2.instances["light1"].value, Some(42.0));

    // Each of env1's named readings is independently settable, and the
    // other two stay at their own default when only one is set.
    let mut inputs2 = inputs_with_switch(true);
    let mut env_set = BTreeMap::new();
    env_set.insert("temp_c".to_string(), 21.5);
    inputs2.sensor_readings.insert("env1".to_string(), env_set);
    let st3 = run_state(&nl, &cat, &inputs2).unwrap();
    let readings3 = st3.instances["env1"].readings.as_ref().unwrap();
    assert_eq!(readings3.get("temp_c"), Some(&21.5));
    assert_eq!(readings3.get("humidity_pct"), Some(&0.0));
    assert_eq!(readings3.get("pressure_hpa"), Some(&0.0));
}

#[test]
fn stage1_basics_potentiometer_is_playable_live() {
    // 01-basics uses a potentiometer instead of a fixed resistor precisely
    // so the very first lesson is something to drag and watch change, not
    // just a single static data point — same math as example-dial-dimmer's
    // own dial test, proving the substitution didn't just compile but
    // actually behaves like a real variable resistor in run mode.
    let (nl, cat) = load("harness/lessons/01-basics.json");
    let mut inputs = inputs_with_switch(true);

    // potentiometer-1k: ohms_min=100, ohms_max=1000. led-red-5mm forward_v=2.0.
    // Fed from "feed" (7.4V, declared). I = (V - Vf) / R.
    let expected = |dial: f64| -> f64 {
        let ohms = 100.0 + (1000.0 - 100.0) * dial;
        (7.4 - 2.0) / ohms
    };

    for dial in [0.0, 0.5, 1.0] {
        inputs.dial_positions.insert("pot".to_string(), dial);
        let st = run_state(&nl, &cat, &inputs).unwrap();
        let actual = st.instances["led1"].current_a.unwrap();
        let want = expected(dial);
        assert!(close(actual, want), "dial={dial}: current_a = {actual}, expected {want}");
        assert_eq!(st.instances["led1"].current_limited, Some(true), "still E33-legal at every dial position");
    }
}

#[test]
fn solar_panel_never_seeds_the_hot_graph_by_design() {
    // energy-sim.md §2.1: a solar-panel has no `elec.source` and no power_in
    // pin at all, so it deliberately never becomes a battery-style seed or a
    // passthrough candidate in robosim's boolean hot/grounded graph — its
    // whole electrical presence here is a single power_out pin, checked
    // statically by robowire (E02/E30) but otherwise inert in run mode until
    // a real time-domain energy model exists. This proves that inertness is
    // honest (no crash, no spurious "powered") rather than accidental.
    let (nl, cat) = load("harness/examples/example-solar-charging-demo.json");
    let st = run_state(&nl, &cat, &inputs_with_switch(true)).unwrap();

    // The LED load still works normally off the real battery seed — the
    // solar/charge-controller side existing at all doesn't disturb it.
    assert_eq!(st.instances["led1"].lit, Some(true));
    assert_eq!(st.instances["led1"].current_limited, Some(true));

    // charge-controller's own "powered" reflects its INPUT (panel) side,
    // which nothing seeds — honestly unpowered, not a stale/wrong true.
    assert_eq!(st.instances["cc"].powered, Some(false));

    // The battery itself is unaffected by the charge controller feeding
    // (electrically) the same net as its own terminal — still just the
    // normal battery projection.
    assert!(st.instances["batt"].current_a.unwrap() > 0.0);
}
