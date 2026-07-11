//! roboparts — the shared parts catalogue: ONE Part schema carrying every
//! view (mass for robotspec's roll-up, drive models for arena binding,
//! electrical personality for robowire's checks), one content-hash rule.
//! Replaces the drift-prone twin structs robotspec::Part / robowire::ElecPart
//! (review finding: same files parsed through two diverging schemas).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ---------------------------------------------------------------------------
// Drive / physical model refs (arena binding)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MotorProps {
    pub stall_torque_mnm: f64,
    pub no_load_rpm: f64,
    pub stall_current_a: f64,
    /// Winding voltage `stall_current_a` was rated at — lets robowire's run
    /// mode derive an equivalent winding resistance (`nominal_v /
    /// stall_current_a`) and compute live current from the ACTUAL supply
    /// voltage and throttle, Ohm's law, rather than a fixed figure.
    #[serde(default)]
    pub nominal_v: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TyreProps {
    pub mu_min: f64,
    pub mu_max: f64,
    pub mu_kinetic_ratio: f64,
}

// ---------------------------------------------------------------------------
// Electrical personality (robowire's checks + designer)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Elec {
    #[serde(default)]
    pub pins: BTreeMap<String, PinDecl>,
    #[serde(default)]
    pub bus: Option<BusDecl>,
    #[serde(default)]
    pub source: Option<SourceDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct PinDecl {
    /// pos | gnd | power_in | power_out | switch_in | switch_out |
    /// motor_in | motor_out | signal_in | signal_out | gpio | mcu_io |
    /// bus_sda | bus_scl | gpio_in | diode_a | diode_k | passive
    pub role: String,
    #[serde(default)]
    pub v_range: Option<[f64; 2]>,
    #[serde(default)]
    pub volts: Option<f64>,
    #[serde(default)]
    pub signal: Option<String>,
    #[serde(default)]
    pub caps: Option<Vec<String>>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub max_a: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BusDecl {
    pub kind: String,
    pub default_addr: String,
    #[serde(default)]
    pub addr_reassignable: bool,
    #[serde(default)]
    pub requires_xshut: bool,
    #[serde(default)]
    pub io_v: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SourceDecl {
    pub volts: f64,
    #[serde(default)]
    pub c_rating: Option<f64>,
    #[serde(default)]
    pub capacity_mah: Option<f64>,
    #[serde(default)]
    pub r_internal_ohm: Option<f64>,
}

// ---------------------------------------------------------------------------
// The Part
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Part {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub mass_g: f64,
    #[serde(default)]
    pub wheel_radius_mm: Option<f64>,
    #[serde(default)]
    pub wheel_width_mm: Option<f64>,
    #[serde(default)]
    pub fov_deg: Option<f64>,
    #[serde(default)]
    pub range_mm: Option<f64>,
    #[serde(default)]
    pub motor: Option<MotorProps>,
    #[serde(default)]
    pub tyre: Option<TyreProps>,
    #[serde(default)]
    pub elec: Option<Elec>,
    /// Rated current draw in mA at `nominal_v` — together they let robowire's
    /// run mode derive an equivalent resistance (`nominal_v / (current_ma/1000)`)
    /// and compute LIVE current via Ohm's law against whatever voltage the
    /// part actually sees, instead of reporting a fixed number. Applies to
    /// fixed-power kinds (tof/imu/mcu/esc-quiescent/regulator-quiescent/
    /// radio/buzzer). NOT the same as robowire M1's forthcoming idle/active/
    /// peak power-budget fields (E30-32); a single representative operating
    /// point, datasheet-typical like the rest of a `provisional` entry.
    #[serde(default)]
    pub current_ma: Option<f64>,
    #[serde(default)]
    pub nominal_v: Option<f64>,
    /// Resistance in ohms — `resistor`-kind parts only.
    #[serde(default)]
    pub ohms: Option<f64>,
    /// End-to-end resistance range for a `potentiometer`-kind part (a
    /// 2-terminal variable resistor / rheostat): its live resistance is
    /// `ohms_min + (ohms_max - ohms_min) * dial_position` (run mode's
    /// `RunInputs.dial_positions`, 0.0-1.0) — so turning the dial changes the
    /// resistance, which changes the current, live, exactly like a fixed
    /// resistor except the value moves. Accepted anywhere a `resistor` is
    /// (E33's current-limiter check, run mode's series-supply resolution).
    #[serde(default)]
    pub ohms_min: Option<f64>,
    #[serde(default)]
    pub ohms_max: Option<f64>,
    /// Diode forward-voltage drop — `led`-kind parts only. Combined with a
    /// series resistor's `ohms` and the ACTUAL supply voltage, run mode
    /// solves I = (V − forward_v) / ohms: the standard hand-calculation for
    /// an LED+resistor circuit (a fixed-Vf diode approximation, not an
    /// iterative nonlinear SPICE solve).
    #[serde(default)]
    pub forward_v: Option<f64>,
    #[serde(default)]
    pub provisional: bool,
    #[serde(default)]
    pub notes: String,
    /// Plain-English "what this is and what it does".
    #[serde(default)]
    pub description: String,
}

pub struct Catalogue {
    pub parts: BTreeMap<String, (Part, String)>, // id -> (part, content hash)
}

impl Catalogue {
    /// Load from a directory of JSON files. Hash = file bytes, so either
    /// consumer's citation pins the exact same content.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let mut parts = BTreeMap::new();
        let entries =
            std::fs::read_dir(dir).map_err(|e| format!("parts dir {dir:?}: {e}"))?;
        let mut paths: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map_or(false, |x| x == "json"))
            .collect();
        paths.sort();
        for p in paths {
            let bytes = std::fs::read(&p).map_err(|e| format!("{p:?}: {e}"))?;
            let part: Part =
                serde_json::from_slice(&bytes).map_err(|e| format!("{p:?}: {e}"))?;
            let hash = sha256_hex(&bytes);
            parts.insert(part.id.clone(), (part, hash));
        }
        Ok(Catalogue { parts })
    }

    /// Build from in-memory values (WASM path — no filesystem). Hashes are
    /// over canonical serialization; identity-grade hashing stays file-based.
    pub fn from_values(parts: Vec<Part>) -> Self {
        let mut map = BTreeMap::new();
        for p in parts {
            let hash = sha256_hex(&serde_json::to_vec(&p).unwrap_or_default());
            map.insert(p.id.clone(), (p, hash));
        }
        Catalogue { parts: map }
    }

    pub fn get(&self, id: &str) -> Result<&(Part, String), String> {
        self.parts
            .get(id)
            .ok_or_else(|| format!("part '{id}' not in catalogue"))
    }
}
