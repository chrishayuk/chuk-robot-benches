// designer/04-geom3d.js — 3D loom geometry: camera, boxes, class-altitude arcs
  let yaw = -0.7, pitch = 0.5, dist = 900;
  const KIND_H = { battery: 30, esc: 16, mcu: 12, motor: 26, switch: 14, tof: 10, imu: 10, radio: 10, wiring: 8 };
  function planeCenter() {
    const insts = Object.keys(nl.instances).filter(instVisible);
    if (!insts.length) return [0, 0];
    let sx = 0, sy = 0;
    for (const i of insts) { const g = boxGeo(i); sx += g.x; sy += g.y; }
    return [sx / insts.length, sy / insts.length];
  }
  let pc = [0, 0];
  function project3(pt) {
    // world: x right, y "south" on the floor plane (from 2D layout), z up.
    const dx = pt[0] - pc[0], dy = pt[1] - pc[1], dz = pt[2] - 0;
    const cy = Math.cos(yaw), sy = Math.sin(yaw);
    const x1 = dx * cy - dy * sy, y1 = dx * sy + dy * cy;
    const cp = Math.cos(pitch), sp = Math.sin(pitch);
    const y2 = y1 * cp - dz * sp;
    const z2 = y1 * sp + dz * cp;
    const depth = dist + y2;
    if (depth < 40) return null;
    const f = 900 / depth;
    return [cv.clientWidth / 2 + x1 * f, cv.clientHeight / 2 - z2 * f + 80];
  }
  function geo3(inst) {
    const g = boxGeo(inst);
    const kind = kindOf(inst);
    const h = KIND_H[kind] || 12;
    const W = 74, D = 52;
    const pins = pinsOf(inst);
    const pinPos3 = {};
    pins.forEach((pin, i) => {
      const side = i % 2 === 0 ? -1 : 1;
      const row = Math.floor(i / 2);
      const rows = Math.ceil(pins.length / 2);
      const t = rows > 1 ? row / (rows - 1) : 0.5;
      pinPos3[pin] = [g.x + side * W / 2, g.y - D / 2 + t * D, h];
    });
    return { x: g.x, y: g.y, W, D, h, pinPos3 };
  }
  function pin3(ep) {
    const [inst, pin] = splitPin(ep);
    if (!(inst in nl.instances) || !instVisible(inst)) return null;
    return geo3(inst).pinPos3[pin] || null;
  }
  function arc3(a, b, lift) {
    const mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, Math.max(a[2], b[2]) + lift];
    const pts = [];
    for (let i = 0; i <= 12; i++) {
      const t = i / 12, u = 1 - t;
      pts.push([
        u * u * a[0] + 2 * u * t * mid[0] + t * t * b[0],
        u * u * a[1] + 2 * u * t * mid[1] + t * t * b[1],
        u * u * a[2] + 2 * u * t * mid[2] + t * t * b[2],
      ]);
    }
    return pts;
  }
  const CLASS_LIFT = { gnd: 12, vbat: 24, v5: 36, v33: 48, motor: 30, pwm: 62, uart: 72, sig: 66, i2c: 84 };
  function netArcs3(net, i) {
    const ends = net.pins.map(pin3).filter(Boolean);
    const lift = (CLASS_LIFT[netClass(net)] || 40) + (i % 3) * 4;
    if (ends.length === 2) return [arc3(ends[0], ends[1], lift)];
    if (ends.length > 2) {
      const n = ends.length;
      const hub = [
        ends.reduce((s, p) => s + p[0], 0) / n,
        ends.reduce((s, p) => s + p[1], 0) / n,
        ends.reduce((s, p) => s + p[2], 0) / n + lift,
      ];
      return ends.map(e => arc3(e, hub, 8));
    }
    return [];
  }
