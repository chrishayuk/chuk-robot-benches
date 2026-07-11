// designer/06-hittest.js — screen-space picking for pins, wires, components
  function wireAt(mx, my) {
    for (let i = nl.nets.length - 1; i >= 0; i--) {
      const net = nl.nets[i];
      if (!layerOn[netClass(net)]) continue;
      if (mode === "3d") {
        for (const arc of netArcs3(net, i)) {
          let prev = null;
          for (const p of arc) {
            const q = project3(p);
            if (q && prev) {
              if (distSeg(mx, my, prev[0], prev[1], q[0], q[1]) < 6) return i;
            }
            prev = q;
          }
        }
      } else {
        const { legs, segs } = netSegments(net);
        for (const [a, b] of legs) {
          if (distSeg(mx, my, a[0], a[1], b[0], b[1]) < 6) return i;
        }
        for (const seg of segs) {
          let prev = seg[0];
          for (let k = 1; k <= 12; k++) {
            const q = cubicAt(seg, k / 12);
            const d = distSeg(mx, my, prev[0], prev[1], q[0], q[1]);
            if (d < 6) return i;
            prev = q;
          }
        }
      }
    }
    return -1;
  }
  function pinAt(mx, my) {
    for (const inst of Object.keys(nl.instances)) {
      if (!instVisible(inst)) continue;
      if (mode === "3d") {
        const g = geo3(inst);
        for (const [pin, pp] of Object.entries(g.pinPos3)) {
          const q = project3(pp);
          if (q && Math.hypot(mx - q[0], my - q[1]) < 8) return inst + "." + pin;
        }
      } else {
        const g = boxGeo(inst);
        for (const [pin, [px, py]] of Object.entries(g.pinPos)) {
          if (Math.hypot(mx - px, my - py) < 8) return inst + "." + pin;
        }
      }
    }
    return null;
  }
  function instAt(mx, my) {
    for (const inst of Object.keys(nl.instances).reverse()) {
      if (!instVisible(inst)) continue;
      if (mode === "3d") {
        const g = geo3(inst);
        const q = project3([g.x, g.y, g.h / 2]);
        if (q && Math.hypot(mx - q[0], my - q[1]) < 34) return inst;
      } else {
        const g = boxGeo(inst);
        if (Math.abs(mx - g.x) < g.w / 2 && Math.abs(my - g.y) < g.h / 2) return inst;
      }
    }
    return null;
  }
