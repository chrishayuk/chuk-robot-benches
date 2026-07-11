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
    /// motor instance -> throttle in [-1.0, 1.0]
    #[serde(default)]
    pub throttles: BTreeMap<String, f64>,
    /// tof/imu instance -> user-set fake reading
    #[serde(default)]
    pub sensor_values: BTreeMap<String, f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_conflict: Option<bool>,
    /// Live current draw in amps, Ohm's law against the actual voltage this
    /// instance sees (see `electrical` module docs) — populated for battery,
    /// LED, motor, and any fixed-power kind with catalogue current/voltage
    /// data; absent where the catalogue doesn't declare enough to compute
    /// it, rather than fabricated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_a: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunState {
    pub nets: BTreeMap<String, NetRunState>,
    pub instances: BTreeMap<String, InstanceRunState>,
}
