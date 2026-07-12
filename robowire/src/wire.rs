//! AWG (American Wire Gauge) reference tables — standard copper wire
//! resistance and a general "chassis wiring" ampacity rating (30°C rise,
//! still air; NOT temperature/insulation-derated for a specific jacket or
//! bundling). Adequate for E31's static worst-case check; not a substitute
//! for a datasheet on a specific cable.

use crate::schema::Net;

/// (awg, ohms_per_meter, chassis_wiring_ampacity_a), ascending by gauge.
/// Covers the range relevant to small-robot wiring (8-30 AWG; 8 AWG for
/// weapon-motor leads whose stall current outruns what 10 AWG can carry) —
/// an undeclared or out-of-range gauge yields `None` rather than a guess.
const AWG_TABLE: &[(u32, f64, f64)] = &[
    (8, 0.002061, 24.0),
    (10, 0.003277, 15.0),
    (12, 0.005210, 9.3),
    (14, 0.008284, 5.9),
    (16, 0.013176, 3.7),
    (18, 0.020951, 2.3),
    (20, 0.033301, 1.5),
    (22, 0.052953, 0.92),
    (24, 0.084219, 0.577),
    (26, 0.133891, 0.361),
    (28, 0.212927, 0.226),
    (30, 0.338583, 0.142),
];

pub fn awg_resistance_ohms_per_m(awg: u32) -> Option<f64> {
    AWG_TABLE.iter().find(|(g, _, _)| *g == awg).map(|(_, r, _)| *r)
}

pub fn awg_ampacity(awg: u32) -> Option<f64> {
    AWG_TABLE.iter().find(|(g, _, _)| *g == awg).map(|(_, _, a)| *a)
}

/// Total resistance for a net's declared wire — `None` if gauge or length
/// is missing (no guessed defaults) or the gauge isn't in the reference
/// table above.
pub fn net_resistance_ohms(net: &Net) -> Option<f64> {
    let awg = net.gauge_awg?;
    let len_m = net.length_mm? / 1000.0;
    awg_resistance_ohms_per_m(awg).map(|r| r * len_m)
}

/// Standard copper resistivity at 20°C (Ω·m) and density (kg/m³) — the two
/// physical constants `copper_mass_g_per_m` derives conductor mass from,
/// rather than a second independent (and independently error-prone) mass
/// table alongside `AWG_TABLE`'s resistance column.
const COPPER_RESISTIVITY_OHM_M: f64 = 1.68e-8;
const COPPER_DENSITY_KG_PER_M3: f64 = 8960.0;

/// Bare-copper-conductor mass per metre for a gauge already in `AWG_TABLE`,
/// derived from its resistance (`R = ρL/A` ⇒ `A = ρ/R` for a 1m length) and
/// copper's density — self-consistent with the resistance table rather than
/// a second, independently-sourced reference. Excludes insulation jacket
/// mass (a real wire weighs somewhat more than this) — a documented
/// simplification, consistent with the "provisional/datasheet-approximate"
/// convention used throughout the catalogue.
pub fn copper_mass_g_per_m(awg: u32) -> Option<f64> {
    let r_per_m = awg_resistance_ohms_per_m(awg)?;
    let area_m2 = COPPER_RESISTIVITY_OHM_M / r_per_m;
    let mass_per_m_kg = area_m2 * COPPER_DENSITY_KG_PER_M3;
    Some(mass_per_m_kg * 1000.0)
}
