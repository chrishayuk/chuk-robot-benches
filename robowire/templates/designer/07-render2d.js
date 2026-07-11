// designer/07-render2d.js — 2D renderer + canvas sizing (syncCanvas gates every paint)
  function syncCanvas() {
    dpr = window.devicePixelRatio || 1;
    const w = cv.clientWidth, h = cv.clientHeight;
    if (!w || !h) return false;
    const bw = Math.round(w * dpr), bh = Math.round(h * dpr);
    if (cv.width !== bw) cv.width = bw;
    if (cv.height !== bh) cv.height = bh;
    document.getElementById("dbg").textContent =
      `build __BUILD__ · dpr ${dpr} · css ${w}x${h} · backing ${cv.width}x${cv.height}`;
    return true;
  }
  function resize() { if (syncCanvas()) draw(); }
  function draw() {
    currentFocus = focusSet();
    if (!syncCanvas()) { requestAnimationFrame(draw); return; }
    if (mode === "3d") { draw3(); return; }
    cx.setTransform(dpr, 0, 0, dpr, 0, 0);
    cx.clearRect(0, 0, cv.clientWidth, cv.clientHeight);
    // grid
    cx.strokeStyle = "#151b20"; cx.lineWidth = 1;
    for (let g = 0; g < cv.clientWidth; g += 24) { cx.beginPath(); cx.moveTo(g, 0); cx.lineTo(g, cv.clientHeight); cx.stroke(); }
    for (let g = 0; g < cv.clientHeight; g += 24) { cx.beginPath(); cx.moveTo(0, g); cx.lineTo(cv.clientWidth, g); cx.stroke(); }

    const pinColors = computePinColors();

    // 1) boxes (bodies + titles + badge) — under the wires, so a crossing
    // wire visibly crosses instead of appearing to terminate on the body.
    for (const inst of Object.keys(nl.instances)) {
      if (!instVisible(inst)) continue;
      const g = boxGeo(inst);
      const isSel = inst === selInst;
      cx.fillStyle = "#1a2126";
      cx.strokeStyle = isSel || (pending && splitPin(pending)[0] === inst) ? "#e8a33d" : "#4a575f";
      cx.lineWidth = isSel ? 2.2 : 1.4;
      roundRect(g.x - g.w / 2, g.y - g.h / 2, g.w, g.h, 6);
      cx.fill(); cx.stroke();
      cx.fillStyle = "#e6e9e4";
      cx.font = "bold 11px ui-monospace, Menlo, monospace";
      cx.fillText(inst, g.x - g.w / 2 + 8, g.y - g.h / 2 + 15);
      cx.fillStyle = "#7d8b93";
      cx.font = "9px ui-monospace, Menlo, monospace";
      cx.fillText(nl.instances[inst], g.x - g.w / 2 + 8, g.y - g.h / 2 + 27);
      if (isSel) {
        cx.fillStyle = "#e05c50";
        cx.beginPath(); cx.arc(g.x + g.w / 2 - 2, g.y - g.h / 2 + 2, 8, 0, Math.PI * 2); cx.fill();
        cx.fillStyle = "#0d1114";
        cx.font = "bold 11px ui-monospace, Menlo, monospace";
        cx.textAlign = "center";
        cx.fillText("✕", g.x + g.w / 2 - 2, g.y - g.h / 2 + 6);
        cx.textAlign = "left";
      }
      if (runMode) drawRunOverlay2d(inst, g);
    }

    // 2) wires (lead stubs + exit-directed cubics), over the boxes
    nl.nets.forEach((net, i) => {
      const cls = netClass(net);
      if (!layerOn[cls]) return;
      let col = COLORS[cls] || "#999";
      let flowing = false;
      if (runMode) {
        const ns = runState.nets && runState.nets[net.id];
        flowing = !!(ns && ns.amps > 0.001);
        if (flowing) col = "#57b48f"; // green — current is actually flowing here
      }
      const { legs, segs, hub, bendHandle } = netSegments(net);
      cx.strokeStyle = col;
      cx.lineWidth = i === selNet ? 3 : 1.8;
      cx.shadowColor = col; cx.shadowBlur = i === selNet ? 12 : 5;
      if (flowing) { cx.setLineDash([8, 6]); cx.lineDashOffset = -spinPhase * 14; }
      for (const [a, b] of legs) {
        cx.beginPath(); cx.moveTo(a[0], a[1]); cx.lineTo(b[0], b[1]); cx.stroke();
      }
      for (const seg of segs) {
        cx.beginPath();
        cx.moveTo(seg[0][0], seg[0][1]);
        cx.bezierCurveTo(seg[1][0], seg[1][1], seg[2][0], seg[2][1], seg[3][0], seg[3][1]);
        cx.stroke();
      }
      if (flowing) { cx.setLineDash([]); cx.lineDashOffset = 0; }
      cx.shadowBlur = 0;
      if (hub) { cx.fillStyle = col; cx.beginPath(); cx.arc(hub[0], hub[1], 3.4, 0, Math.PI * 2); cx.fill(); }
      if (bendHandle && !runMode) {
        // A user-placed bend point, draggable — a small diamond (distinct
        // from the round hub dot, which means "genuine multi-pin junction").
        const s = i === selNet || dragWireNet === i ? 5 : 3.6;
        cx.fillStyle = i === selNet || dragWireNet === i ? "#e8a33d" : col;
        cx.beginPath();
        cx.moveTo(bendHandle[0], bendHandle[1] - s); cx.lineTo(bendHandle[0] + s, bendHandle[1]);
        cx.lineTo(bendHandle[0], bendHandle[1] + s); cx.lineTo(bendHandle[0] - s, bendHandle[1]);
        cx.closePath(); cx.fill();
      }
      if (runMode) {
        const ns = runState.nets && runState.nets[net.id];
        if (ns && (ns.hot || ns.amps > 0.001)) {
          const lp = hub || bendHandle || netLabelPos(net);
          if (lp) {
            const label = `${ns.volts.toFixed(1)}V · ${ns.amps.toFixed(2)}A`;
            cx.font = "9px ui-monospace, Menlo, monospace";
            const lw = cx.measureText(label).width;
            cx.fillStyle = "#0d1114cc";
            cx.fillRect(lp[0] - lw / 2 - 3, lp[1] - 18, lw + 6, 12);
            cx.fillStyle = col;
            cx.textAlign = "center";
            cx.fillText(label, lp[0], lp[1] - 9);
            cx.textAlign = "left";
          }
        }
      }
      if (i === selNet) {
        cx.font = "bold 9px ui-monospace, Menlo, monospace";
        for (const ep of net.pins) {
          const q = pinXY(ep);
          if (!q) continue;
          const lx = q[0] + (q[2] || 0) * (STUB + 6), ly = q[1] + (q[3] || 0) * (STUB + 6);
          cx.fillStyle = "#0d1114";
          const wdt = cx.measureText(ep).width;
          cx.fillRect(lx - (q[2] < 0 ? wdt : 0) - 2, ly - 8, wdt + 4, 11);
          cx.fillStyle = col;
          cx.textAlign = q[2] < 0 ? "right" : "left";
          cx.fillText(ep, lx, ly);
          cx.textAlign = "left";
        }
      }
    });
    // bus wires
    for (const bus of (layerOn.i2c ? nl.buses : [])) {
      const m = pinXY(bus.sda);
      if (!m) continue;
      for (const dev of bus.devices) {
        const dpin = pinXY(dev.inst + ".SDA");
        if (!dpin) continue;
        cx.strokeStyle = COLORS.i2c; cx.lineWidth = 1.8; cx.setLineDash([7, 4]);
        cx.beginPath();
        cx.moveTo(m[0], m[1]);
        cx.bezierCurveTo(
          m[0] + (m[2] || 0) * 50, m[1] + (m[3] || 0) * 50,
          dpin[0] + (dpin[2] || 0) * 50, dpin[1] + (dpin[3] || 0) * 50,
          dpin[0], dpin[1]);
        cx.stroke(); cx.setLineDash([]);
      }
    }

    // 3) pins — always-on-top plugs (colored = wired, hollow = free)
    for (const inst of Object.keys(nl.instances)) {
      if (!instVisible(inst)) continue;
      const g = boxGeo(inst);
      for (const [pin, [px, py, ox, oy]] of Object.entries(g.pinPos)) {
        const ep = inst + "." + pin;
        const active = pending === ep;
        const wired = pinColors[ep];
        if (active) {
          cx.fillStyle = "#e8a33d";
          cx.beginPath(); cx.arc(px, py, 5, 0, Math.PI * 2); cx.fill();
        } else if (wired) {
          cx.fillStyle = wired;
          cx.beginPath(); cx.arc(px, py, 4, 0, Math.PI * 2); cx.fill();
        } else {
          cx.strokeStyle = "#6b787f"; cx.lineWidth = 1.2;
          cx.beginPath(); cx.arc(px, py, 3, 0, Math.PI * 2); cx.stroke();
        }
        cx.fillStyle = "#8b969b";
        cx.font = "8.5px ui-monospace, Menlo, monospace";
        cx.textAlign = ox < -0.3 ? "right" : ox > 0.3 ? "left" : "center";
        cx.fillText(pin, px + ox * 9, py + oy * 9 + 3);
        cx.textAlign = "left";
      }
    }

    if (wireDrag && wireDrag.moved) {
      const sp = pinXY(wireDrag.from);
      if (sp) {
        cx.strokeStyle = "#e8a33d"; cx.lineWidth = 1.6; cx.setLineDash([6, 4]);
        cx.beginPath(); cx.moveTo(sp[0], sp[1]); cx.lineTo(wireDrag.cur[0], wireDrag.cur[1]); cx.stroke();
        cx.setLineDash([]);
      }
    }
    hint(wireDrag && wireDrag.moved ? `release on a pin to connect ${wireDrag.from}` :
      dragWireNet != null ? "dragging wire bend — release to drop, double-click it to straighten" :
      pending ? `wiring from ${pending} — click another pin, or drag from a pin (esc cancels)` :
      pickHandler ? "pick a pin…" :
      selInst ? `${selInst} selected — drag to move, R rotates, ✕ or Delete removes` :
      selNet >= 0 ? "net selected — drag its path to bend it, double-click to straighten, Delete removes it" : "");
  }
  function roundRect(x, y, w, h, r) {
    cx.beginPath();
    cx.moveTo(x + r, y); cx.arcTo(x + w, y, x + w, y + h, r); cx.arcTo(x + w, y + h, x, y + h, r);
    cx.arcTo(x, y + h, x, y, r); cx.arcTo(x, y, x + w, y, r); cx.closePath();
  }
  function hint(t) { document.getElementById("hint").textContent = t; }

