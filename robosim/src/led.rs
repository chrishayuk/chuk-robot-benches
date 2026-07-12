//! LED component behavior — what "lit", "current-limited", "burned", and
//! "why is it dark" mean for an LED specifically. Kept out of
//! `simulate.rs`'s orchestration match (which only walks the graph and
//! dispatches by kind) so this one component's own rules live in one place,
//! not as a growing inline arm alongside every other kind's — the same
//! separation `robowire::checks`/`robowire::teach` already draw between
//! "what's the rule" and "who runs it". As more kinds gain their own
//! non-trivial behavior (a servo's hold current, a sensor's noise model),
//! they get their own sibling module the same way, rather than the match in
//! `simulate.rs` growing indefinitely.

use crate::electrical::{led_series_supply, net_volts_live, pin_net_by_role};
use crate::types::{InstanceRunState, RunInputs};
use robowire::catalogue::{Elec, ElecCatalogue, ElecPart};
use robowire::checks::led_current_limited;
use robowire::schema::Netlist;
use std::collections::{BTreeMap, BTreeSet};

/// `resolved_volts` is mutated in place (a lit LED sustains its own
/// forward-voltage drop on its anode net — a resistor bridge never
/// propagates voltage, so without this a lit, current-carrying LED would
/// show 0V on its own feed net, §3a) — same ordering as when this lived
/// inline in `simulate.rs`: the injection happens before this LED's own
/// current calculation reads `resolved_volts`, in case that ever matters.
/// Returns this instance's run state, and its current sink (net, amps) if
/// any current is actually flowing.
pub fn compute(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    hot: &BTreeSet<String>,
    grounded: &BTreeSet<String>,
    resolved_volts: &mut BTreeMap<String, f64>,
    inputs: &RunInputs,
    inst: &str,
    part: &ElecPart,
    elec: &Elec,
) -> Result<(InstanceRunState, Option<(String, f64)>), String> {
    let anode = pin_net_by_role(elec, inst, net_of, "diode_a");
    let cathode = pin_net_by_role(elec, inst, net_of, "diode_k");
    let anode_hot = anode.as_ref().is_some_and(|n| hot.contains(n));
    let cathode_grounded = cathode.as_ref().is_some_and(|n| grounded.contains(n));
    let lit = anode_hot && cathode_grounded;
    let limited = led_current_limited(nl, cat, inst)?;
    let burned = lit && !limited;

    if lit {
        if let Some(anode_net) = &anode {
            resolved_volts.entry(anode_net.clone()).or_insert(part.forward_v.unwrap_or(0.0));
        }
    }

    let mut amps = 0.0;
    let mut sink = None;
    if lit && limited {
        if let Some((supply_net, ohms)) = led_series_supply(nl, cat, net_of, inputs, inst)? {
            if ohms > 0.0 {
                let v_supply = net_volts_live(resolved_volts, hot, &supply_net);
                let vf = part.forward_v.unwrap_or(0.0);
                amps = ((v_supply - vf) / ohms).max(0.0);
            }
        }
        if amps > 0.0 {
            if let Some(anode_net) = &anode {
                sink = Some((anode_net.clone(), amps));
            }
        }
    }

    let reason = if lit && !limited {
        Some("no series resistor — would burn out instantly (E33)".to_string())
    } else if !lit {
        let anode_grounded = anode.as_ref().is_some_and(|n| grounded.contains(n));
        let cathode_hot = cathode.as_ref().is_some_and(|n| hot.contains(n));
        if anode_grounded && cathode_hot {
            Some("reverse polarity — anode is grounded, cathode is hot".to_string())
        } else if !anode_hot {
            Some("no power reaching the anode".to_string())
        } else {
            Some("cathode not returned to ground".to_string())
        }
    } else {
        None
    };

    let mut state = InstanceRunState::default();
    state.lit = Some(lit);
    state.current_limited = Some(limited);
    state.burned = Some(burned);
    state.current_a = Some(amps);
    state.reason = reason;

    Ok((state, sink))
}
