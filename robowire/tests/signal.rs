//! `robowire::signal` — resolving which `mcu_io` pin actually drives a motor
//! channel (and, in reverse, which motor an MCU pin drives), so robosim's
//! run-mode throttle input can be pinned to a real MCU pin instead of the
//! motor instance itself.

use robowire::catalogue::ElecCatalogue;
use robowire::signal::{mcu_drivable_pins, motor_signal_source_pin};
use robowire::Netlist;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn load(harness_file: &str) -> (Netlist, ElecCatalogue) {
    let root = repo_root();
    let nl: Netlist = serde_json::from_slice(&std::fs::read(root.join(harness_file)).unwrap()).unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    (nl, cat)
}

#[test]
fn wedge_motors_resolve_to_their_own_mcu_pin() {
    let (nl, cat) = load("harness/mvp-wedge-harness.json");
    assert_eq!(motor_signal_source_pin(&nl, &cat, "m_l.M+").unwrap().as_deref(), Some("mcu.GP2"));
    assert_eq!(motor_signal_source_pin(&nl, &cat, "m_r.M+").unwrap().as_deref(), Some("mcu.GP3"));
}

#[test]
fn wedge_mcu_drivable_pins_reports_both_channels_and_their_motors() {
    let (nl, cat) = load("harness/mvp-wedge-harness.json");
    let pins = mcu_drivable_pins(&nl, &cat, "mcu").unwrap();
    assert_eq!(
        pins,
        vec![("GP2".to_string(), Some("m_l".to_string())), ("GP3".to_string(), Some("m_r".to_string()))]
    );
}

#[test]
fn stage3_motor_driver_has_no_signal_source_yet() {
    // harness/lessons/03-motor-driver.json deliberately has no MCU — S1 sits
    // on a dummy single-pin net. A real, legal circuit state, not an error.
    let (nl, cat) = load("harness/lessons/03-motor-driver.json");
    assert_eq!(motor_signal_source_pin(&nl, &cat, "m1.M+").unwrap(), None);
}

#[test]
fn stage4_brain_and_radio_wires_the_signal_through() {
    let (nl, cat) = load("harness/lessons/04-brain-and-radio.json");
    assert_eq!(motor_signal_source_pin(&nl, &cat, "m1.M+").unwrap().as_deref(), Some("mcu.GP2"));
}

#[test]
fn a_servo_with_no_channel_is_labeled_by_its_own_instance_not_unconnected() {
    // A servo's SIG pin has no `channel` (unlike an ESC's S1/S2, one per
    // motor channel) — the previous version of `driven_inst` only knew how
    // to trace through a channel to a motor, so a servo's own drivable pin
    // fell through to `None` ("unconnected downstream") even though it
    // plainly IS connected, just not to a motor. Stage 5 introduces the
    // lifter servo on mcu.GP3.
    let (nl, cat) = load("harness/lessons/05-shared-5v-rail.json");
    let pins = mcu_drivable_pins(&nl, &cat, "mcu").unwrap();
    assert_eq!(
        pins,
        vec![("GP2".to_string(), Some("m1".to_string())), ("GP3".to_string(), Some("lifter".to_string()))]
    );
}

#[test]
fn stage7_two_wheel_drive_resolves_both_motors_and_the_servo() {
    let (nl, cat) = load("harness/lessons/07-two-wheel-drive.json");
    let pins = mcu_drivable_pins(&nl, &cat, "mcu").unwrap();
    assert_eq!(
        pins,
        vec![
            ("GP2".to_string(), Some("m1".to_string())),
            ("GP3".to_string(), Some("lifter".to_string())),
            ("GP7".to_string(), Some("m2".to_string())),
        ]
    );
}

#[test]
fn run_mode_demo_resolves_m1_to_gp2() {
    let (nl, cat) = load("harness/examples/example-run-mode-demo.json");
    assert_eq!(motor_signal_source_pin(&nl, &cat, "m1.M+").unwrap().as_deref(), Some("mcu.GP2"));
    let pins = mcu_drivable_pins(&nl, &cat, "mcu").unwrap();
    // GP3 -> esc.S2 is wired (channel M2), but no motor is actually connected
    // to esc's M2+/M2- in this harness — still a drivable pin, just one with
    // nothing downstream to label.
    assert_eq!(
        pins,
        vec![("GP2".to_string(), Some("m1".to_string())), ("GP3".to_string(), None)]
    );
}
