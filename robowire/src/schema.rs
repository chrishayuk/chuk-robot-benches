//! Netlist schema (specs/robowire.md §2.2): structured data canonical
//! (spec Q1 lean), authored as JSON, content-hashed into RobotSpec's
//! `elec` source.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Netlist {
    pub name: String,
    /// instance id -> part id (resolved against the shared parts/ catalogue).
    pub instances: BTreeMap<String, String>,
    pub nets: Vec<Net>,
    #[serde(default)]
    pub buses: Vec<Bus>,
    #[serde(default)]
    pub failsafe: Option<Failsafe>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Net {
    pub id: String,
    /// "instance.PIN" endpoints.
    pub pins: Vec<String>,
    /// Declared rail voltage (power nets).
    #[serde(default)]
    pub volts: Option<f64>,
    /// Signal class carried (signal nets): "pwm", "uart", ...
    #[serde(default)]
    pub signal: Option<String>,
    /// Wire gauge for this net (American Wire Gauge, e.g. `26` for 26AWG),
    /// for E31's ampacity check and robosim's live wire-drop display
    /// (`robowire::wire`). A net's pins are still one equipotential node —
    /// this is a lumped approximation covering the whole net (including any
    /// fan-out to multiple pins), not a per-segment topology. `None` when
    /// undeclared: no gauge is guessed, matching the catalogue's existing
    /// "missing field ⇒ not computed, never fabricated" convention.
    #[serde(default)]
    pub gauge_awg: Option<u32>,
    /// Total wire length for this net in millimetres, paired with
    /// `gauge_awg` to derive resistance (`robowire::wire::net_resistance_ohms`).
    #[serde(default)]
    pub length_mm: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Bus {
    pub id: String,
    pub kind: String, // "i2c" in v0
    pub sda: String,
    pub scl: String,
    pub devices: Vec<BusDevice>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BusDevice {
    pub inst: String,
    pub addr: String,
    #[serde(default)]
    pub reassign_to: Option<String>,
    /// "instance.PIN" of the GPIO driving this device's XSHUT, if wired.
    #[serde(default)]
    pub xshut: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Failsafe {
    /// Human-auditable description of the loss-of-signal behaviour chain.
    pub rx_loss: String,
    /// The pins through which the stop is actuated (must be MCU-reachable).
    pub stop_pins: Vec<String>,
}

/// Split "inst.PIN" into (inst, pin).
pub fn split_pin(endpoint: &str) -> Result<(&str, &str), String> {
    endpoint
        .split_once('.')
        .ok_or_else(|| format!("endpoint '{endpoint}' is not 'instance.PIN'"))
}
