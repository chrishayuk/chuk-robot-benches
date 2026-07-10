//! Authored schema — data types only, no behaviour beyond serde.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RobotSpec {
    pub identity: Identity,
    pub sources: Sources,
    pub drive: Drive,
    pub sensors: Vec<SensorFit>,
    pub components: Vec<Placement>,
    /// Ground-contact skid points (part of the chassis, not parts).
    pub skids: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub name: String,
    pub revision: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Sources {
    pub mech: MechSource,
    pub elec: ElecSource,
    pub models: BTreeMap<String, String>,
    pub kernel: KernelRef,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode")]
pub enum MechSource {
    /// The design-search-native representation (spec §2: first-class, not a
    /// fallback). CAD mode arrives at robotspec M2.
    #[serde(rename = "parametric")]
    Parametric { chassis: WedgeChassis },
}

/// Parametric wedge, v0 plate model: base plate, two side plates following
/// the wedge profile, rear wall, wedge face plate. Origin: footprint centre,
/// z = 0 at the ground plane. +x forward (toward the wedge nose).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WedgeChassis {
    pub length_mm: f64,
    pub width_mm: f64,
    pub rear_height_mm: f64,
    pub nose_height_mm: f64,
    /// Length of the sloped front section.
    pub wedge_length_mm: f64,
    pub wall_mm: f64,
    pub material: String,
    /// Material -> density g/cm^3. Declared with the source per spec §2.
    pub density_map: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ElecSource {
    /// robowire netlist ref+hash; "PENDING" until robowire M0 lands.
    pub r#ref: String,
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KernelRef {
    pub family_hash: String,
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Drive {
    pub wheels: Vec<Wheel>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Wheel {
    pub part: String,
    /// Axle position, mm.
    pub pos_mm: [f64; 3],
    pub driven: bool,
    pub motor_part: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SensorFit {
    pub id: String,
    pub part: String,
    pub pos_mm: [f64; 3],
    /// Unit-ish pointing vector; normalised on load.
    pub dir: [f64; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Placement {
    pub id: String,
    pub part: String,
    pub pos_mm: [f64; 3],
}

