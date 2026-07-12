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
  function callDescribe(netlistObj) {
    const nlB = enc.encode(JSON.stringify(netlistObj));
    const psB = enc.encode(JSON.stringify(PARTS));
    const p1 = W.wasm_alloc(nlB.length), p2 = W.wasm_alloc(psB.length);
    new Uint8Array(W.memory.buffer, p1, nlB.length).set(nlB);
    new Uint8Array(W.memory.buffer, p2, psB.length).set(psB);
    W.describe_json(p1, nlB.length, p2, psB.length);
    W.wasm_free(p1, nlB.length); W.wasm_free(p2, psB.length);
    const out = new Uint8Array(W.memory.buffer, W.out_ptr(), W.out_len());
    return JSON.parse(dec.decode(out));
  }
  function callExplainError(code) {
    const cb = enc.encode(code);
    const p1 = W.wasm_alloc(cb.length);
    new Uint8Array(W.memory.buffer, p1, cb.length).set(cb);
    W.explain_error_json(p1, cb.length);
    W.wasm_free(p1, cb.length);
    const out = new Uint8Array(W.memory.buffer, W.out_ptr(), W.out_len());
    return JSON.parse(dec.decode(out));
  }
  function callRunState(netlistObj, inputsObj) {
    const nl = enc.encode(JSON.stringify(netlistObj));
    const ps = enc.encode(JSON.stringify(PARTS));
    const is = enc.encode(JSON.stringify(inputsObj));
    const p1 = W.wasm_alloc(nl.length), p2 = W.wasm_alloc(ps.length), p3 = W.wasm_alloc(is.length);
    new Uint8Array(W.memory.buffer, p1, nl.length).set(nl);
    new Uint8Array(W.memory.buffer, p2, ps.length).set(ps);
    new Uint8Array(W.memory.buffer, p3, is.length).set(is);
    W.run_state_json(p1, nl.length, p2, ps.length, p3, is.length);
    W.wasm_free(p1, nl.length); W.wasm_free(p2, ps.length); W.wasm_free(p3, is.length);
    const out = new Uint8Array(W.memory.buffer, W.out_ptr(), W.out_len());
    return JSON.parse(dec.decode(out));
  }
