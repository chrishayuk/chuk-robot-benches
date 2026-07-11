// designer/00-wasm.js — the real E-check engine, in-browser (buffer ABI)
  const bytes = Uint8Array.from(atob(WASM_B64), c => c.charCodeAt(0));
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const W = instance.exports;
  const enc = new TextEncoder(), dec = new TextDecoder();
  function callChecks(netlistObj) {
    const nl = enc.encode(JSON.stringify(netlistObj));
    const ps = enc.encode(JSON.stringify(PARTS));
    const p1 = W.wasm_alloc(nl.length), p2 = W.wasm_alloc(ps.length);
    new Uint8Array(W.memory.buffer, p1, nl.length).set(nl);
    new Uint8Array(W.memory.buffer, p2, ps.length).set(ps);
    W.run_checks_json(p1, nl.length, p2, ps.length);
    W.wasm_free(p1, nl.length); W.wasm_free(p2, ps.length);
    const out = new Uint8Array(W.memory.buffer, W.out_ptr(), W.out_len());
    return JSON.parse(dec.decode(out));
  }

