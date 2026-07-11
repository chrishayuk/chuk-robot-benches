// designer/08-render3d.js — 3D renderer
  function stroke3(pts, color, width, dashed, glow) {
    cx.beginPath();
    let started = false;
    for (const pt of pts) {
      const q = project3(pt);
      if (!q) { started = false; continue; }
      if (!started) { cx.moveTo(q[0], q[1]); started = true; }
      else cx.lineTo(q[0], q[1]);
    }
    cx.strokeStyle = color; cx.lineWidth = width;
    cx.setLineDash(dashed ? [6, 5] : []);
    if (glow) { cx.shadowColor = color; cx.shadowBlur = 9; }
    cx.stroke();
    cx.shadowBlur = 0; cx.setLineDash([]);
  }
  function draw3() {
    pc = planeCenter();
    cx.setTransform(dpr, 0, 0, dpr, 0, 0);
    cx.clearRect(0, 0, cv.clientWidth, cv.clientHeight);
    for (let g = -300; g <= 300; g += 60) {
      stroke3([[pc[0] + g, pc[1] - 300, 0], [pc[0] + g, pc[1] + 300, 0]], "#151b20", 1, false, false);
      stroke3([[pc[0] - 300, pc[1] + g, 0], [pc[0] + 300, pc[1] + g, 0]], "#151b20", 1, false, false);
    }
    nl.nets.forEach((net, i) => {
      const cls = netClass(net);
      if (!layerOn[cls]) return;
      const col = COLORS[cls] || "#999";
      const selW = i === selNet;
      for (const arc of netArcs3(net, i)) stroke3(arc, col, selW ? 3 : 1.8, false, true);
    });
    for (const bus of (layerOn.i2c ? nl.buses : [])) {
      const m = pin3(bus.sda);
      if (!m) continue;
      for (const dev of bus.devices) {
        const d = pin3(dev.inst + ".SDA");
        if (d) stroke3(arc3(m, d, CLASS_LIFT.i2c), COLORS.i2c, 1.8, true, true);
      }
    }
    const pinColors3 = computePinColors();
    for (const inst of Object.keys(nl.instances)) {
      if (!instVisible(inst)) continue;
      const g = geo3(inst);
      const isSel = inst === selInst;
      const col = isSel || (pending && splitPin(pending)[0] === inst) ? "#e8a33d" : "#5b6a73";
      const c = [];
      for (const sx of [-1, 1]) for (const sy of [-1, 1]) for (const sz of [0, 1])
        c.push([g.x + sx * g.W / 2, g.y + sy * g.D / 2, sz * g.h]);
      const edges = [[0,1],[2,3],[4,5],[6,7],[0,2],[1,3],[4,6],[5,7],[0,4],[1,5],[2,6],[3,7]];
      for (const [a, b] of edges) stroke3([c[a], c[b]], col, isSel ? 2 : 1.2, false, false);
      const q = project3([g.x, g.y, g.h + 10]);
      if (q) {
        cx.fillStyle = isSel ? "#e8a33d" : "#c7ced2";
        cx.font = "11px ui-monospace, Menlo, monospace";
        cx.fillText(inst, q[0] - 12, q[1]);
      }
      for (const [pin, pp] of Object.entries(g.pinPos3)) {
        const ep = inst + "." + pin;
        const q2 = project3(pp);
        if (!q2) continue;
        const active = pending === ep;
        const wired = pinColors3[ep];
        if (active) {
          cx.fillStyle = "#e8a33d";
          cx.beginPath(); cx.arc(q2[0], q2[1], 5, 0, Math.PI * 2); cx.fill();
        } else if (wired) {
          cx.fillStyle = wired;
          cx.beginPath(); cx.arc(q2[0], q2[1], 3.6, 0, Math.PI * 2); cx.fill();
        } else {
          cx.strokeStyle = "#6b787f"; cx.lineWidth = 1;
          cx.beginPath(); cx.arc(q2[0], q2[1], 2.8, 0, Math.PI * 2); cx.stroke();
        }
      }
    }
    if (wireDrag && wireDrag.moved) {
      const wp = pin3(wireDrag.from);
      const sp = wp && project3(wp);
      if (sp) {
        cx.strokeStyle = "#e8a33d"; cx.lineWidth = 1.6; cx.setLineDash([6, 4]);
        cx.beginPath(); cx.moveTo(sp[0], sp[1]); cx.lineTo(wireDrag.cur[0], wireDrag.cur[1]); cx.stroke();
        cx.setLineDash([]);
      }
    }
    hint(wireDrag && wireDrag.moved ? `release on a pin to connect ${wireDrag.from}` :
      pending ? `wiring from ${pending} — click another pin, or drag from a pin (esc cancels)` :
      pickHandler ? "pick a pin…" :
      "3D — drag empty space to orbit, wheel to zoom; switch to 2D to rearrange");
  }
