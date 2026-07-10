//! Parts catalogue: content-hashed entries shared with robowire (§2.1).

use crate::identity::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Part {
    pub id: String,
    pub kind: String,
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
    pub provisional: bool,
    #[serde(default)]
    pub notes: String,
}

pub struct Catalogue {
    pub parts: BTreeMap<String, (Part, String)>, // id -> (part, content hash)
}

impl Catalogue {
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

    pub fn get(&self, id: &str) -> Result<&(Part, String), String> {
        self.parts
            .get(id)
            .ok_or_else(|| format!("part '{id}' not in catalogue"))
    }
}

