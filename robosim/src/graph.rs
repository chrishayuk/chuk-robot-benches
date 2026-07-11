//! Generic reachability engine — net-to-net adjacency and BFS. Nothing in
//! this module knows what a switch or a resistor IS; `electrical.rs` and
//! `simulate.rs` decide what gets linked and why. Kept separate so the graph
//! traversal itself stays simple, testable, and reusable independent of any
//! particular kind of component.

use robowire::Netlist;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// "inst.PIN" -> net id. If an endpoint is (illegally) wired into more than
/// one net, the last one wins — a malformed-netlist edge case the E-checks
/// are responsible for catching, not the simulator.
pub fn endpoint_net_index(nl: &Netlist) -> BTreeMap<String, String> {
    let mut idx = BTreeMap::new();
    for net in &nl.nets {
        for p in &net.pins {
            idx.insert(p.clone(), net.id.clone());
        }
    }
    idx
}

/// Undirected edge — current/energy can flow either way (a closed switch, a
/// resistor, a length of wire).
pub fn link(adj: &mut BTreeMap<String, BTreeSet<String>>, a: &str, b: &str) {
    if a == b {
        return;
    }
    adj.entry(a.to_string()).or_default().insert(b.to_string());
    adj.entry(b.to_string()).or_default().insert(a.to_string());
}

/// Directed edge — a regulator/BEC/3V3-out only passes power from its input
/// side to its output side, never the reverse.
pub fn link_forward(adj: &mut BTreeMap<String, BTreeSet<String>>, from: &str, to: &str) {
    adj.entry(from.to_string()).or_default().insert(to.to_string());
}

/// BFS reachability from `seeds` over `undirected` edges, plus `forward`
/// (directed) edges when given.
pub fn bfs(
    seeds: &BTreeSet<String>,
    undirected: &BTreeMap<String, BTreeSet<String>>,
    forward: Option<&BTreeMap<String, BTreeSet<String>>>,
) -> BTreeSet<String> {
    let mut visited: BTreeSet<String> = seeds.clone();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    while let Some(n) = queue.pop_front() {
        if let Some(neighbors) = undirected.get(&n) {
            for nb in neighbors {
                if visited.insert(nb.clone()) {
                    queue.push_back(nb.clone());
                }
            }
        }
        if let Some(fwd) = forward.and_then(|f| f.get(&n)) {
            for nb in fwd {
                if visited.insert(nb.clone()) {
                    queue.push_back(nb.clone());
                }
            }
        }
    }
    visited
}

/// Reachability from a single net — used to attribute a current sink's draw
/// to every net upstream of it.
pub fn reach_from(
    net_id: &str,
    undirected: &BTreeMap<String, BTreeSet<String>>,
    forward: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut seed = BTreeSet::new();
    seed.insert(net_id.to_string());
    bfs(&seed, undirected, Some(forward))
}
