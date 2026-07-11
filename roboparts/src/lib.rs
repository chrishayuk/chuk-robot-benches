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
