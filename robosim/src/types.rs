//! Public data types: what you send in, what you get back.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunInputs {
    /// switch instance -> closed?
    #[serde(default)]
    pub switches: BTreeMap<String, bool>,
    /// button instance -> held?
    #[serde(default)]
    pub buttons: BTreeMap<String, bool>,
    /// MCU pin endpoint (e.g. "mcu.GP2") -> commanded signal in [-1.0, 1.0].
    /// Keyed by the pin that would really be carrying this PWM signal, not
    /// by whatever motor it happens to end up driving — a stand-in for the
    /// signal generator/RC receiver you'd hook up on a real bench before any
    /// firmware exists, not for the firmware itself (see `robowire::signal`
    /// for how this reaches a motor's own throttle).
    #[serde(default)]
    pub pwm_signals: BTreeMap<String, f64>,
    /// potentiometer instance -> dial position in [0.0, 1.0], scaling its
    /// live resistance between the part's declared ohms_min and ohms_max.
    #[serde(default)]
    pub dial_positions: BTreeMap<String, f64>,
    /// tof/imu/light instance -> user-set fake reading, for sensors that
    /// report exactly one value (see `roboparts::Part::readings` — `None`
    /// there means this map is what's used).
    #[serde(default)]
    pub sensor_values: BTreeMap<String, f64>,
    /// sensor instance -> named reading -> user-set fake value, for a part
    /// that reports SEVERAL simultaneous readings from one physical device
    /// (`roboparts::Part::readings`, e.g. a BME280's own
    /// `["temp_c", "humidity_pct", "pressure_hpa"]`) — kept as a distinct
    /// map from `sensor_values` rather than compound-keying the same one,
    /// so a single-reading sensor's plain instance-name key can never
    /// collide with a multi-reading sensor's own reading names.
    #[serde(default)]
    pub sensor_readings: BTreeMap<String, BTreeMap<String, f64>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NetRunState {
    pub hot: bool,
    pub grounded: bool,
    /// The net's declared voltage (schema `Net.volts`) when hot, else 0.0 —
    /// a real, already-authored number, not a derived estimate.
    pub volts: f64,
    /// Σ of every component's Ohm's-law current reachable downstream of this
    /// net over the same bridge/passthrough graph used for `hot` — real
    /// component math (resistor + LED forward-voltage, motor winding
    /// resistance, fixed-power equivalent resistance), summed the way a
    /// worst-case power budget already sums loads (still not a
    /// current-divider/Kirchhoff solve, since nothing here branches current
    /// unequally across parallel paths). 0.0 through the ground plane
    /// (return current isn't attributed in v1).
    pub amps: f64,
    /// Estimated IR drop along this net's own declared wire (`Net.gauge_awg`
    /// + `Net.length_mm`, `robowire::wire::net_resistance_ohms`) at its
    /// current `amps` — `amps * resistance`. One-shot and NOT propagated:
    /// every other net's `volts`/`amps` here were computed as if this drop
    /// were zero, so this is a display annotation, not a re-solved node
    /// voltage (feeding it back even one hop would require redoing every
    /// downstream current calculation against the reduced voltage, which is
    /// the iterative solve this whole model deliberately avoids). `None`
    /// when the net declares no gauge/length, or carries no current.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_drop_v: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InstanceRunState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub powered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_limited: Option<bool>,
    /// LED-kind only: `lit && !current_limited` — a real consequence of
    /// this circuit's wiring (E33: no series resistor means it burns out
    /// instantly under power), not a rendering-layer inference. Computed
    /// here rather than left for the designer to reconstruct from `lit`/
    /// `current_limited` itself, so there's exactly one place that decides
    /// what "burned" means.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spin: Option<f64>,
    /// A single-reading sensor's (tof/imu/light) live value — `None` for a
    /// multi-reading sensor, which reports through `readings` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// A multi-reading sensor's (`roboparts::Part::readings` non-empty,
    /// e.g. `env`) named live values — one physical part, several
    /// independent numbers at once, rather than one collapsed reading.
    /// `None` for a single-reading sensor, which reports through `value`
    /// instead; exactly one of the two is ever populated for a given
    /// instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readings: Option<BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_conflict: Option<bool>,
    /// Live current draw in amps, Ohm's law against the actual voltage this
    /// instance sees (see `electrical` module docs) — populated for battery,
    /// LED, motor, and any fixed-power kind with catalogue current/voltage
    /// data; absent where the catalogue doesn't declare enough to compute
    /// it, rather than fabricated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_a: Option<f64>,
    /// A battery's own terminal-voltage sag under its own `current_a`
    /// (`robowire::checks::battery_sag_v`, using the part's declared
    /// `r_internal_ohm`) — battery instances only. One-shot, terminal-net
    /// display only: does NOT feed back into any other net's already-computed
    /// current (see `simulate`'s Phase 6 doc comment). `None` when the part
    /// declares no `r_internal_ohm`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sag_v: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// MCU-kind only: every `mcu_io` pin actually wired to drive something
    /// (a `signal_in`-role pin), with the motor instance it resolves to when
    /// determinable (`robowire::signal::mcu_drivable_pins`) — the run panel
    /// renders one slider per entry here, on the MCU's own row, rather than
    /// the UI independently guessing which pins are "drivable".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pwm_channels: Option<Vec<PwmChannel>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PwmChannel {
    pub pin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drives: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunState {
    pub nets: BTreeMap<String, NetRunState>,
    pub instances: BTreeMap<String, InstanceRunState>,
}
