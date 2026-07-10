//! Content addressing: canonical hashing and the body ⊂ robot nesting (§4).

use crate::schema::RobotSpec;
use crate::SCHEMA_VERSION;
use std::collections::BTreeMap;

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// body_hash covers everything physical; robot_hash = body + models + kernel.
/// Kernel-only changes keep body_hash stable ("same body, different brain").
pub fn identity_hashes(
    spec: &RobotSpec,
    resolved_parts: &BTreeMap<String, String>,
) -> (String, String) {
    let body_payload = serde_json::json!({
        "schema": SCHEMA_VERSION,
        "mech": &spec.sources.mech,
        "elec": &spec.sources.elec,
        "drive": &spec.drive,
        "sensors": &spec.sensors,
        "components": &spec.components,
        "skids": &spec.skids,
        "resolved_parts": resolved_parts,
    });
    let body_hash = sha256_hex(&serde_json::to_vec(&body_payload).unwrap());
    let robot_payload = serde_json::json!({
        "schema": SCHEMA_VERSION,
        "body_hash": &body_hash,
        "models": &spec.sources.models,
        "kernel": &spec.sources.kernel,
    });
    let robot_hash = sha256_hex(&serde_json::to_vec(&robot_payload).unwrap());
    (body_hash, robot_hash)
}
