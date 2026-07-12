//! Battery finalization — terminal current and its one-shot terminal-
//! voltage sag. Not a per-instance `compute()` like the other component
//! modules: a battery's own current isn't knowable until every net's Σ is
//! built (the whole graph has to be walked first), so this runs as a
//! distinct finalization pass over already-built `nets`/`instances`, not
//! from inside `simulate::run_state`'s main per-instance loop.

use crate::electrical::pin_net_by_role;
use crate::types::{InstanceRunState, NetRunState};
use robowire::catalogue::ElecCatalogue;
use robowire::schema::Netlist;
use std::collections::BTreeMap;

/// One-shot, non-iterative: every other net's `volts`/`amps` were already
/// computed as if the battery's voltage were its undropped nominal figure
/// (`InstanceRunState.sag_v`'s doc comment) — feeding the sag back even one
/// hop would show a number less consistent with the rest of the model than
/// not showing it at all.
pub fn finalize(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    nets: &BTreeMap<String, NetRunState>,
    instances: &mut BTreeMap<String, InstanceRunState>,
) -> Result<(), String> {
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if part.kind != "battery" {
            continue;
        }
        let Some(elec) = &part.elec else { continue };
        let pos_net = pin_net_by_role(elec, inst, net_of, "pos");
        let amps = pos_net.and_then(|n| nets.get(&n)).map(|ns| ns.amps).unwrap_or(0.0);
        let sag_v = elec.source.as_ref().and_then(|s| robowire::checks::battery_sag_v(s, amps));
        if let Some(st) = instances.get_mut(inst) {
            st.current_a = Some(amps);
            st.sag_v = sag_v;
        }
    }
    Ok(())
}
