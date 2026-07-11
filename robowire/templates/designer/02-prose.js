// designer/02-prose.js — prose comes from the REAL Rust generator via WASM
// (describe_json); this module is just the cache. The old JS mirror is gone.
  let PROSE = { pins: {}, nets: {} };
  function updateProse() {
    try {
      const res = callDescribe(nl);
      if (!res.error) PROSE = res;
    } catch (e) { /* keep last good prose */ }
  }
  function pinProse(ep) {
    return PROSE.pins[ep] || "";
  }
  function netProse(netId) {
    return PROSE.nets[netId] || { about: "", ends: [] };
  }
