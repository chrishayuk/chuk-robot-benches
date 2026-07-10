//! The electrical view of the shared parts/ catalogue (robowire spec §2.1).
//! Same JSON files robotspec reads for mass; robowire reads the `elec`
//! personality. Content hash covers the whole file, so either consumer's
//! citation pins the same bytes.

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ElecPart {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub elec: Option<Elec>,
}

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
    /// motor_in | motor_out | signal_in | signal_out | gpio | mcu_io
    pub role: String,
    #[serde(default)]
    pub v_range: Option<[f64; 2]>,
    #[serde(default)]
    pub volts: Option<f64>,
    /// Signal class this pin emits/accepts ("pwm", "uart", "crsf"...).
    #[serde(default)]
    pub signal: Option<String>,
    /// Capabilities of an MCU io pin ("pwm", "uart_rx", "i2c_sda", "gpio"...).
    #[serde(default)]
    pub caps: Option<Vec<String>>,
    #[serde(default)]
    pub required: bool,
    /// Driver channel grouping for motor_out pins ("M1"/"M2").
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
}

pub struct ElecCatalogue {
    pub parts: BTreeMap<String, (ElecPart, String)>,
}

impl ElecCatalogue {
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
            let part: ElecPart =
                serde_json::from_slice(&bytes).map_err(|e| format!("{p:?}: {e}"))?;
            let hash = sha256_hex(&bytes);
            parts.insert(part.id.clone(), (part, hash));
        }
        Ok(ElecCatalogue { parts })
    }

    pub fn get(&self, id: &str) -> Result<&(ElecPart, String), String> {
        self.parts
            .get(id)
            .ok_or_else(|| format!("part '{id}' not in catalogue"))
    }
}
