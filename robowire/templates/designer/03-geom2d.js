// designer/03-geom2d.js — 2D bench-top geometry: boxes, pins, wire routing
  const BOXW = 128;
  function pinsOf(inst) {
    const part = partById[nl.instances[inst]];
    return Object.keys(part?.elec?.pins || {});
  }
  function boxGeo(inst) {
    const pins = pinsOf(inst);
    const rows = Math.ceil(pins.length / 2);
    const bh = 34 + Math.max(rows, 1) * 15; // upright height
    if (!layout[inst]) {
      const n = Object.keys(layout).length;
      layout[inst] = [130 + (n % 5) * 180, 120 + Math.floor(n / 5) * 165, 0];
    }
    const L = layout[inst];
    const [x, y] = L;
    const rot = ((L[2] || 0) % 360 + 360) % 360;
    const rad = rot * Math.PI / 180;
    const cr = Math.cos(rad), sr = Math.sin(rad);
    const pinPos = {};
    pins.forEach((p, i) => {
      const side = i % 2 === 0 ? -1 : 1;
      const row = Math.floor(i / 2);
      const rx = side * BOXW / 2, ry = -bh / 2 + 40 + row * 15;
      const ox = side * cr, oy = side * sr;
      pinPos[p] = [x + rx * cr - ry * sr, y + rx * sr + ry * cr, ox, oy];
    });
    const w = rot % 180 === 0 ? BOXW : bh;
    const h = rot % 180 === 0 ? bh : BOXW;
    return { x, y, w, h, rot, pinPos };
  }
  function pinXY(ep) {
    const [inst, pin] = splitPin(ep);
    if (!(inst in nl.instances)) return null;
    if (!instVisible(inst)) return null;
    const g = boxGeo(inst);
    return g.pinPos[pin] || null;
  }

  const STUB = 13;
  // A user-dragged bend point for a 2-pin net, stored alongside component
  // positions (same `layout` object, same localStorage persistence) under a
  // "wire:" key so it can never collide with an instance name.
  function wireBendOf(netId) {
    const v = layout["wire:" + netId];
    return Array.isArray(v) ? v : null;
  }
  // 2D drag: x/y only, preserving any 3D height already set (so tweaking a
  // wire's schematic position doesn't reset how it was routed in 3D).
  function setWireBend2D(netId, x, y) {
    const prev = wireBendOf(netId);
    layout["wire:" + netId] = [x, y, prev ? prev[2] : undefined];
  }
  // 3D drag: a full world-space point (dragging happens on a fixed-height
  // plane — see unproject3 in 04-geom3d.js).
  function setWireBend3D(netId, x, y, z) { layout["wire:" + netId] = [x, y, z]; }
  function clearWireBend(netId) { delete layout["wire:" + netId]; }

  function netSegments(net) {
    const ends = [];
    for (const ep of net.pins) {
      const q = pinXY(ep);
      if (q) ends.push({
        pin: [q[0], q[1]],
        out: [q[2] || 0, q[3] || 0],
        stub: [q[0] + (q[2] || 0) * STUB, q[1] + (q[3] || 0) * STUB],
      });
    }
    if (ends.length < 2) return { legs: [], segs: [], hub: null, bendHandle: null };
    const legs = ends.map(e => [e.pin, e.stub]);
    const bend = ends.length === 2 ? wireBendOf(net.id) : null;
    // Control points extend along each pin's exit direction, so the wire
    // keeps leaving the pin before it sweeps toward the target — instead of
    // cutting straight back across its own component's body.
    if (ends.length === 2 && !bend) {
      const [A, B] = ends;
      const d = Math.hypot(A.stub[0] - B.stub[0], A.stub[1] - B.stub[1]);
      const kFor = (from, to) => {
        let k = Math.min(46, Math.max(16, d * 0.22));
        // If the pin exits away from the target, keep the excursion short —
        // enough to read which pin it leaves, no antenna spike.
        const dx = to.stub[0] - from.stub[0], dy = to.stub[1] - from.stub[1];
        const dot = from.out[0] * dx + from.out[1] * dy;
        if (dot < 0) k *= 0.45;
        return k;
      };
      const ka = kFor(A, B), kb = kFor(B, A);
      return {
        legs,
        segs: [[A.stub, [A.stub[0] + A.out[0] * ka, A.stub[1] + A.out[1] * ka],
                [B.stub[0] + B.out[0] * kb, B.stub[1] + B.out[1] * kb], B.stub]],
        hub: null,
        bendHandle: null,
      };
    }
    // A user bend point routes the wire through it exactly like a genuine
    // multi-pin junction — each end draws its own cubic into the shared
    // point — but it's not a real junction, so no hub dot is drawn for it
    // (that would misleadingly read as "3+ wires meet here").
    const hub = bend || [
      ends.reduce((s, e) => s + e.stub[0], 0) / ends.length,
      ends.reduce((s, e) => s + e.stub[1], 0) / ends.length,
    ];
    return {
      legs,
      segs: ends.map(e => {
        const d = Math.hypot(e.stub[0] - hub[0], e.stub[1] - hub[1]);
        let k = Math.min(30, Math.max(12, d * 0.22));
        const dot = e.out[0] * (hub[0] - e.stub[0]) + e.out[1] * (hub[1] - e.stub[1]);
        if (dot < 0) k *= 0.45;
        return [e.stub, [e.stub[0] + e.out[0] * k, e.stub[1] + e.out[1] * k], hub, hub];
      }),
      hub: bend ? null : hub,
      bendHandle: bend ? hub : null,
    };
  }
  function cubicAt(seg, t) {
    const [a, c1, c2, b] = seg;
    const u = 1 - t;
    return [
      u*u*u*a[0] + 3*u*u*t*c1[0] + 3*u*t*t*c2[0] + t*t*t*b[0],
      u*u*u*a[1] + 3*u*u*t*c1[1] + 3*u*t*t*c2[1] + t*t*t*b[1],
    ];
  }
  function computePinColors() {
    const m = {};
    nl.nets.forEach(net => {
      const cls = netClass(net);
      if (!layerOn[cls]) return;
      for (const ep of net.pins) m[ep] = COLORS[cls] || "#999";
    });
    if (layerOn.i2c) {
      for (const bus of nl.buses) {
        m[bus.sda] = COLORS.i2c; m[bus.scl] = COLORS.i2c;
        for (const dev of bus.devices) {
          m[dev.inst + ".SDA"] = COLORS.i2c;
          m[dev.inst + ".SCL"] = COLORS.i2c;
          if (dev.xshut) { m[dev.xshut] = COLORS.i2c; m[dev.inst + ".XSHUT"] = COLORS.i2c; }
        }
      }
    }
    return m;
  }
  function distSeg(px, py, x1, y1, x2, y2) {
    const dx = x2 - x1, dy = y2 - y1;
    const len2 = dx * dx + dy * dy;
    const t = len2 ? Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / len2)) : 0;
    return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
  }
  // Human-language mirrors of robowire's Rust prose (the canonical
  // generator lives in view.rs; the editor mirrors it because it edits live).
  function autoNetId(a, b) {
    const base = (splitPin(a)[1] + "_" + splitPin(b)[0]).toLowerCase().replace(/[^a-z0-9]+/g, "_");
    let id = base, k = 2;
    while (nl.nets.some(n => n.id === id)) id = base + "_" + k++;
    return id;
  }

